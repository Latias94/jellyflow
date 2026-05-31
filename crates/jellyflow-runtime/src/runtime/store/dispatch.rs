//! Transaction dispatch, undo, redo, middleware, and commit publication.

use crate::profile::{ApplyPipelineError, GraphProfile, apply_transaction_with_profile};
use crate::rules::{Diagnostic, DiagnosticSeverity, DiagnosticTarget};
use crate::runtime::commit::NodeGraphPatch;
use crate::runtime::events::{NodeGraphStoreEvent, NodeGraphStoreSnapshot};
use jellyflow_core::core::Graph;
use jellyflow_core::ops::{GraphTransaction, normalize_transaction};

use super::{DispatchError, DispatchOutcome, DispatchProfile, NodeGraphStore};

struct DispatchPipeline<'store, 'profile> {
    store: &'store mut NodeGraphStore,
    dispatch_profile: DispatchProfile<'profile>,
}

enum DispatchPipelineResult {
    Empty(GraphTransaction),
    Commit {
        graph: Graph,
        committed: GraphTransaction,
    },
}

#[derive(Debug, Clone, Copy)]
enum HistoryReplayDirection {
    Undo,
    Redo,
}

impl<'store, 'profile> DispatchPipeline<'store, 'profile> {
    fn new(store: &'store mut NodeGraphStore, dispatch_profile: DispatchProfile<'profile>) -> Self {
        Self {
            store,
            dispatch_profile,
        }
    }

    fn run(mut self, tx: &GraphTransaction) -> Result<DispatchPipelineResult, ApplyPipelineError> {
        let mut tx = normalize_transaction(tx.clone());
        if tx.is_empty() {
            return Ok(DispatchPipelineResult::Empty(tx));
        }
        Self::validate_transaction(&tx)?;

        self.store.run_before_dispatch_middleware(&mut tx)?;
        tx = normalize_transaction(tx);
        if tx.is_empty() {
            return Ok(DispatchPipelineResult::Empty(tx));
        }
        Self::validate_transaction(&tx)?;

        let (graph, committed) = self.apply_to_scratch(&tx)?;
        let committed = normalize_transaction(committed);
        Self::validate_transaction(&committed)?;
        Ok(DispatchPipelineResult::Commit { graph, committed })
    }

    fn apply_to_scratch(
        &mut self,
        tx: &GraphTransaction,
    ) -> Result<(Graph, GraphTransaction), ApplyPipelineError> {
        let mut scratch = self.store.graph.clone();
        let committed = match &mut self.dispatch_profile {
            DispatchProfile::StoreProfile => self.store.apply_to_graph(&mut scratch, tx)?,
            DispatchProfile::External(profile) => {
                apply_transaction_with_profile(&mut scratch, &mut **profile, tx)?
            }
        };
        Ok((scratch, committed))
    }

    fn validate_transaction(tx: &GraphTransaction) -> Result<(), ApplyPipelineError> {
        if let Some((key, message)) = jellyflow_core::ops::find_non_finite_in_tx(tx) {
            return Err(Self::reject_tx(key, message));
        }
        if let Some((key, message)) = jellyflow_core::ops::find_invalid_size_in_tx(tx) {
            return Err(Self::reject_tx(key, message));
        }
        Ok(())
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
        match DispatchPipeline::new(self, dispatch_profile).run(tx)? {
            DispatchPipelineResult::Empty(committed) => {
                Ok(DispatchOutcome::from_committed(committed))
            }
            DispatchPipelineResult::Commit { graph, committed } => {
                Ok(self.commit_dispatch(graph, committed))
            }
        }
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

    /// Undoes the last committed transaction.
    pub fn undo(&mut self) -> Result<Option<DispatchOutcome>, DispatchError> {
        self.replay_history(HistoryReplayDirection::Undo, DispatchProfile::StoreProfile)
            .map_err(DispatchError::Apply)
    }

    /// Undoes the last committed transaction using an externally-owned profile pipeline.
    pub fn undo_with_profile(
        &mut self,
        profile: &mut dyn GraphProfile,
    ) -> Result<Option<DispatchOutcome>, ApplyPipelineError> {
        self.replay_history(
            HistoryReplayDirection::Undo,
            DispatchProfile::External(profile),
        )
    }

    /// Redoes the last undone transaction.
    pub fn redo(&mut self) -> Result<Option<DispatchOutcome>, DispatchError> {
        self.replay_history(HistoryReplayDirection::Redo, DispatchProfile::StoreProfile)
            .map_err(DispatchError::Apply)
    }

    /// Redoes the last undone transaction using an externally-owned profile pipeline.
    pub fn redo_with_profile(
        &mut self,
        profile: &mut dyn GraphProfile,
    ) -> Result<Option<DispatchOutcome>, ApplyPipelineError> {
        self.replay_history(
            HistoryReplayDirection::Redo,
            DispatchProfile::External(profile),
        )
    }

    fn replay_history(
        &mut self,
        direction: HistoryReplayDirection,
        mut dispatch_profile: DispatchProfile<'_>,
    ) -> Result<Option<DispatchOutcome>, ApplyPipelineError> {
        let mut scratch = self.graph.clone();
        let mut committed: Option<GraphTransaction> = None;

        let mut history = std::mem::take(&mut self.history);
        let did = match direction {
            HistoryReplayDirection::Undo => {
                history.undo(|tx| -> Result<GraphTransaction, ApplyPipelineError> {
                    let committed_tx =
                        self.apply_history_transaction(&mut scratch, &mut dispatch_profile, tx)?;
                    committed = Some(committed_tx.clone());
                    Ok(committed_tx)
                })
            }
            HistoryReplayDirection::Redo => {
                history.redo(|tx| -> Result<GraphTransaction, ApplyPipelineError> {
                    let committed_tx =
                        self.apply_history_transaction(&mut scratch, &mut dispatch_profile, tx)?;
                    committed = Some(committed_tx.clone());
                    Ok(committed_tx)
                })
            }
        };
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

    fn apply_history_transaction(
        &mut self,
        graph: &mut Graph,
        dispatch_profile: &mut DispatchProfile<'_>,
        tx: &GraphTransaction,
    ) -> Result<GraphTransaction, ApplyPipelineError> {
        match dispatch_profile {
            DispatchProfile::StoreProfile => self.apply_to_graph(graph, tx),
            DispatchProfile::External(profile) => {
                apply_transaction_with_profile(graph, &mut **profile, tx)
            }
        }
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
}
