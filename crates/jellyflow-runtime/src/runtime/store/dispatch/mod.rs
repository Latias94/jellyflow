//! Transaction dispatch, undo, redo, middleware, and commit publication.

mod gate;
mod pipeline;

use crate::profile::{ApplyPipelineError, GraphProfile, apply_transaction_with_profile};
use crate::runtime::commit::NodeGraphPatch;
use crate::runtime::events::NodeGraphStoreSnapshot;
use crate::runtime::middleware::NodeGraphStoreMiddleware;
use jellyflow_core::core::Graph;
use jellyflow_core::ops::GraphTransaction;

use super::dispatch_profile::DispatchProfile;
use super::snapshot::StoreSnapshotParts;
use super::{DispatchError, DispatchOutcome, NodeGraphStore};

use self::pipeline::{DispatchPipeline, DispatchPipelineResult};

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
        self.complete_committed_patch(patch)
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
            Ok(tx.clone())
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

    pub(super) fn complete_committed_patch(&mut self, patch: NodeGraphPatch) -> DispatchOutcome {
        self.run_after_dispatch_middleware(&patch);
        self.publish_graph_commit(&patch);
        DispatchOutcome::new(patch)
    }
}
