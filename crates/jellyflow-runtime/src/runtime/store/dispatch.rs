//! Transaction dispatch, undo, redo, middleware, and commit publication.

use crate::profile::{ApplyPipelineError, GraphProfile, apply_transaction_with_profile};
use crate::rules::{Diagnostic, DiagnosticSeverity, DiagnosticTarget};
use crate::runtime::commit::NodeGraphPatch;
use crate::runtime::events::{NodeGraphStoreEvent, NodeGraphStoreSnapshot};
use jellyflow_core::core::Graph;
use jellyflow_core::ops::GraphTransaction;

use super::{DispatchError, DispatchOutcome, DispatchProfile, NodeGraphStore};

impl NodeGraphStore {
    /// Applies a transaction and records it in history.
    ///
    /// This mirrors the UI loop contract: the store applies edits to a scratch graph first and only
    /// commits on success (so rejected profile validations do not partially mutate state).
    pub fn dispatch_transaction(
        &mut self,
        tx: &GraphTransaction,
    ) -> Result<DispatchOutcome, DispatchError> {
        self.dispatch_transaction_impl(tx, DispatchProfile::StoreProfile)
            .map_err(DispatchError::Apply)
    }

    /// Dispatches a transaction using an externally-owned profile pipeline.
    ///
    /// This is intended for UI integration where the profile is owned by the presenter layer.
    pub fn dispatch_transaction_with_profile(
        &mut self,
        tx: &GraphTransaction,
        profile: &mut dyn GraphProfile,
    ) -> Result<DispatchOutcome, ApplyPipelineError> {
        self.dispatch_transaction_impl(tx, DispatchProfile::External(profile))
    }

    fn dispatch_transaction_impl(
        &mut self,
        tx: &GraphTransaction,
        dispatch_profile: DispatchProfile<'_>,
    ) -> Result<DispatchOutcome, ApplyPipelineError> {
        let mut tx = jellyflow_core::ops::normalize_transaction(tx.clone());
        if tx.is_empty() {
            return Ok(DispatchOutcome::from_committed(tx));
        }
        Self::validate_dispatch_transaction(&tx)?;

        self.run_before_dispatch_middleware(&mut tx)?;
        tx = jellyflow_core::ops::normalize_transaction(tx);
        if tx.is_empty() {
            return Ok(DispatchOutcome::from_committed(tx));
        }
        Self::validate_dispatch_transaction(&tx)?;

        let mut scratch = self.graph.clone();
        let committed = match dispatch_profile {
            DispatchProfile::StoreProfile => self.apply_to_graph(&mut scratch, &tx)?,
            DispatchProfile::External(profile) => {
                apply_transaction_with_profile(&mut scratch, profile, &tx)?
            }
        };
        let committed = jellyflow_core::ops::normalize_transaction(committed);
        Self::validate_dispatch_transaction(&committed)?;
        Ok(self.commit_dispatch(scratch, committed))
    }

    fn commit_dispatch(&mut self, graph: Graph, committed: GraphTransaction) -> DispatchOutcome {
        self.install_committed_graph_state(graph, &committed);
        self.history.record(committed.clone());
        let patch = NodeGraphPatch::new(committed);
        self.run_after_dispatch_middleware(&patch);
        self.publish_graph_commit(&patch);
        DispatchOutcome::new(patch)
    }

    fn run_before_dispatch_middleware(
        &mut self,
        tx: &mut GraphTransaction,
    ) -> Result<(), ApplyPipelineError> {
        if let Some(middleware) = self.middleware.as_deref_mut() {
            let snapshot = NodeGraphStoreSnapshot {
                graph: &self.graph,
                graph_revision: self.graph_revision,
                view_state: &self.view_state,
                interaction: &self.interaction,
                runtime_tuning: &self.runtime_tuning,
                history: &self.history,
            };
            middleware.before_dispatch(snapshot, tx)?;
        }
        Ok(())
    }

    fn run_after_dispatch_middleware(&mut self, patch: &NodeGraphPatch) {
        if let Some(middleware) = self.middleware.as_deref_mut() {
            let snapshot = NodeGraphStoreSnapshot {
                graph: &self.graph,
                graph_revision: self.graph_revision,
                view_state: &self.view_state,
                interaction: &self.interaction,
                runtime_tuning: &self.runtime_tuning,
                history: &self.history,
            };
            middleware.after_dispatch(snapshot, patch);
        }
    }

    fn validate_dispatch_transaction(tx: &GraphTransaction) -> Result<(), ApplyPipelineError> {
        if let Some((key, message)) = jellyflow_core::ops::find_non_finite_in_tx(tx) {
            return Err(Self::reject_tx(key, message));
        }
        if let Some((key, message)) = jellyflow_core::ops::find_invalid_size_in_tx(tx) {
            return Err(Self::reject_tx(key, message));
        }
        Ok(())
    }

    /// Undoes the last committed transaction.
    pub fn undo(&mut self) -> Result<Option<DispatchOutcome>, DispatchError> {
        let mut scratch = self.graph.clone();
        let mut committed: Option<GraphTransaction> = None;

        let mut history = std::mem::take(&mut self.history);
        let did = history.undo(|tx| -> Result<GraphTransaction, ApplyPipelineError> {
            let committed_tx = self.apply_to_graph(&mut scratch, tx)?;
            committed = Some(committed_tx.clone());
            Ok(committed_tx)
        });
        self.history = history;
        let did = did?;

        if !did {
            return Ok(None);
        }

        let committed = committed.unwrap_or_default();
        self.install_committed_graph_state(scratch, &committed);
        let patch = NodeGraphPatch::new(committed);
        self.publish_graph_commit(&patch);
        Ok(Some(DispatchOutcome::new(patch)))
    }

    /// Undoes the last committed transaction using an externally-owned profile pipeline.
    pub fn undo_with_profile(
        &mut self,
        profile: &mut dyn GraphProfile,
    ) -> Result<Option<DispatchOutcome>, ApplyPipelineError> {
        let mut scratch = self.graph.clone();
        let mut committed: Option<GraphTransaction> = None;

        let mut history = std::mem::take(&mut self.history);
        let did = history.undo(|tx| -> Result<GraphTransaction, ApplyPipelineError> {
            let committed_tx = apply_transaction_with_profile(&mut scratch, profile, tx)?;
            committed = Some(committed_tx.clone());
            Ok(committed_tx)
        });
        self.history = history;
        let did = did?;
        if !did {
            return Ok(None);
        }

        let committed = committed.unwrap_or_default();
        self.install_committed_graph_state(scratch, &committed);
        let patch = NodeGraphPatch::new(committed);
        self.publish_graph_commit(&patch);
        Ok(Some(DispatchOutcome::new(patch)))
    }

    /// Redoes the last undone transaction.
    pub fn redo(&mut self) -> Result<Option<DispatchOutcome>, DispatchError> {
        let mut scratch = self.graph.clone();
        let mut committed: Option<GraphTransaction> = None;

        let mut history = std::mem::take(&mut self.history);
        let did = history.redo(|tx| -> Result<GraphTransaction, ApplyPipelineError> {
            let committed_tx = self.apply_to_graph(&mut scratch, tx)?;
            committed = Some(committed_tx.clone());
            Ok(committed_tx)
        });
        self.history = history;
        let did = did?;

        if !did {
            return Ok(None);
        }

        let committed = committed.unwrap_or_default();
        self.install_committed_graph_state(scratch, &committed);
        let patch = NodeGraphPatch::new(committed);
        self.publish_graph_commit(&patch);
        Ok(Some(DispatchOutcome::new(patch)))
    }

    /// Redoes the last undone transaction using an externally-owned profile pipeline.
    pub fn redo_with_profile(
        &mut self,
        profile: &mut dyn GraphProfile,
    ) -> Result<Option<DispatchOutcome>, ApplyPipelineError> {
        let mut scratch = self.graph.clone();
        let mut committed: Option<GraphTransaction> = None;

        let mut history = std::mem::take(&mut self.history);
        let did = history.redo(|tx| -> Result<GraphTransaction, ApplyPipelineError> {
            let committed_tx = apply_transaction_with_profile(&mut scratch, profile, tx)?;
            committed = Some(committed_tx.clone());
            Ok(committed_tx)
        });
        self.history = history;
        let did = did?;
        if !did {
            return Ok(None);
        }

        let committed = committed.unwrap_or_default();
        self.install_committed_graph_state(scratch, &committed);
        let patch = NodeGraphPatch::new(committed);
        self.publish_graph_commit(&patch);
        Ok(Some(DispatchOutcome::new(patch)))
    }

    fn apply_to_graph(
        &mut self,
        graph: &mut Graph,
        tx: &GraphTransaction,
    ) -> Result<GraphTransaction, ApplyPipelineError> {
        if let Some(profile) = self.profile.as_deref_mut() {
            apply_transaction_with_profile(graph, profile, tx)
        } else {
            tx.apply_to(graph)?;
            Ok(GraphTransaction {
                label: tx.label.clone(),
                ops: tx.ops.clone(),
            })
        }
    }

    fn install_committed_graph_state(&mut self, graph: Graph, committed: &GraphTransaction) {
        self.graph = graph;
        self.bump_graph_revision();
        self.lookups.apply_transaction(&self.graph, committed);
    }

    fn publish_graph_commit(&mut self, patch: &NodeGraphPatch) {
        self.emit(NodeGraphStoreEvent::GraphCommitted { patch });
        self.notify_selectors();
    }

    fn reject_tx(key: String, message: String) -> ApplyPipelineError {
        ApplyPipelineError::Rejected {
            message: message.clone(),
            diagnostics: vec![Diagnostic {
                key,
                severity: DiagnosticSeverity::Error,
                target: DiagnosticTarget::Graph,
                message,
                fixes: Vec::new(),
            }],
        }
    }
}
