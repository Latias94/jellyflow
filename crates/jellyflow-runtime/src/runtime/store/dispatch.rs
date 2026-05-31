//! Transaction dispatch, undo, redo, middleware, and commit publication.

use crate::profile::{ApplyPipelineError, GraphProfile, apply_transaction_with_profile};
use crate::rules::{Diagnostic, DiagnosticSeverity, DiagnosticTarget};
use crate::runtime::commit::NodeGraphPatch;
use crate::runtime::events::NodeGraphStoreSnapshot;
use crate::runtime::middleware::NodeGraphStoreMiddleware;
use jellyflow_core::core::Graph;
use jellyflow_core::ops::{GraphTransaction, normalize_transaction};

use super::snapshot::StoreSnapshotParts;
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

impl<'store, 'profile> DispatchPipeline<'store, 'profile> {
    fn new(store: &'store mut NodeGraphStore, dispatch_profile: DispatchProfile<'profile>) -> Self {
        Self {
            store,
            dispatch_profile,
        }
    }

    fn run(mut self, tx: &GraphTransaction) -> Result<DispatchPipelineResult, ApplyPipelineError> {
        let mut tx = Self::normalize_and_validate(tx.clone())?;
        if tx.is_empty() {
            return Ok(DispatchPipelineResult::Empty(tx));
        }

        self.store.run_before_dispatch_middleware(&mut tx)?;
        tx = Self::normalize_and_validate(tx)?;
        if tx.is_empty() {
            return Ok(DispatchPipelineResult::Empty(tx));
        }

        let (graph, committed) = self.apply_to_scratch(&tx)?;
        let committed = Self::normalize_and_validate(committed)?;
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

    fn normalize_and_validate(
        tx: GraphTransaction,
    ) -> Result<GraphTransaction, ApplyPipelineError> {
        let tx = normalize_transaction(tx);
        Self::validate_transaction(&tx)?;
        Ok(tx)
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
        let committed_for_history = committed.clone();
        let patch = self.prepare_committed_graph_patch(graph, committed);
        self.history.record(committed_for_history);
        self.run_after_dispatch_middleware(&patch);
        self.publish_dispatch_outcome(patch)
    }

    fn run_before_dispatch_middleware(
        &mut self,
        tx: &mut GraphTransaction,
    ) -> Result<(), ApplyPipelineError> {
        if let Some(result) = self.with_dispatch_middleware_snapshot(|middleware, snapshot| {
            middleware.before_dispatch(snapshot, tx)
        }) {
            result?;
        }
        Ok(())
    }

    pub(super) fn run_after_dispatch_middleware(&mut self, patch: &NodeGraphPatch) {
        self.with_dispatch_middleware_snapshot(|middleware, snapshot| {
            middleware.after_dispatch(snapshot, patch);
        });
    }

    fn with_dispatch_middleware_snapshot<R>(
        &mut self,
        f: impl FnOnce(&mut dyn NodeGraphStoreMiddleware, NodeGraphStoreSnapshot<'_>) -> R,
    ) -> Option<R> {
        let snapshot_parts = StoreSnapshotParts::from_store_fields(
            &self.graph,
            self.graph_revision,
            &self.view_state,
            &self.interaction,
            &self.runtime_tuning,
            &self.history,
        );
        self.middleware
            .as_deref_mut()
            .map(|middleware| f(middleware, snapshot_parts.snapshot()))
    }

    pub(super) fn apply_to_graph(
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

    pub(super) fn prepare_committed_graph_patch(
        &mut self,
        graph: Graph,
        committed: GraphTransaction,
    ) -> NodeGraphPatch {
        self.install_committed_graph_state(graph, &committed);
        NodeGraphPatch::new(committed)
    }

    pub(super) fn publish_dispatch_outcome(&mut self, patch: NodeGraphPatch) -> DispatchOutcome {
        self.publish_graph_commit(&patch);
        DispatchOutcome::new(patch)
    }
}
