//! Transaction dispatch, undo, redo, middleware, and commit publication.

mod gate;
mod pipeline;

use crate::io::NodeGraphViewState;
use crate::profile::{ApplyPipelineError, GraphProfile, apply_transaction_with_profile};
use crate::runtime::commit::NodeGraphPatch;
use crate::runtime::events::{NodeGraphStoreSnapshot, ViewChange};
use crate::runtime::middleware::NodeGraphStoreMiddleware;
use jellyflow_core::core::Graph;
use jellyflow_core::ops::GraphTransaction;

use super::dispatch_profile::DispatchProfile;
use super::snapshot::StoreSnapshotParts;
use super::{DispatchError, DispatchOutcome, NodeGraphStore};

use self::pipeline::{DispatchPipeline, DispatchPipelineResult};

pub(super) struct PreparedGraphCommit {
    patch: NodeGraphPatch,
    sanitized_view_state: Option<SanitizedViewState>,
}

struct SanitizedViewState {
    before: NodeGraphViewState,
    after: NodeGraphViewState,
    changes: Vec<ViewChange>,
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
        let prepared = self.prepare_committed_graph_patch(graph, committed);
        self.history.record(committed_for_history);
        self.complete_committed_patch(prepared)
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
            self.layout_facts_revision,
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
        self.view_state.sanitize_for_graph(&self.graph);
        self.lookups.apply_transaction(&self.graph, committed);
        self.bump_layout_facts_revision();
    }

    pub(super) fn prepare_committed_graph_patch(
        &mut self,
        graph: Graph,
        committed: GraphTransaction,
    ) -> PreparedGraphCommit {
        let view_before = self.view_state.clone();
        self.install_committed_graph_state(graph, &committed);
        let sanitized_view_state = self.committed_view_state_change(view_before);
        PreparedGraphCommit {
            patch: NodeGraphPatch::new(committed),
            sanitized_view_state,
        }
    }

    pub(super) fn complete_committed_patch(
        &mut self,
        prepared: PreparedGraphCommit,
    ) -> DispatchOutcome {
        let PreparedGraphCommit {
            patch,
            sanitized_view_state,
        } = prepared;
        self.run_after_dispatch_middleware(&patch);
        self.publish_graph_commit(&patch);
        if let Some(sanitized) = sanitized_view_state {
            self.publish_view_changed(&sanitized.before, &sanitized.after, &sanitized.changes);
        }
        DispatchOutcome::new(patch)
    }

    fn committed_view_state_change(
        &self,
        before: NodeGraphViewState,
    ) -> Option<SanitizedViewState> {
        let after = self.view_state.clone();
        if before == after {
            return None;
        }

        let mut changes = Vec::new();
        if before.pan != after.pan || (before.zoom - after.zoom).abs() > 1.0e-6 {
            changes.push(ViewChange::viewport(after.pan, after.zoom));
        }
        if before.selected_nodes != after.selected_nodes
            || before.selected_edges != after.selected_edges
            || before.selected_groups != after.selected_groups
        {
            changes.push(ViewChange::selection(
                after.selected_nodes.clone(),
                after.selected_edges.clone(),
                after.selected_groups.clone(),
            ));
        }

        Some(SanitizedViewState {
            before,
            after,
            changes,
        })
    }
}
