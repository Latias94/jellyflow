//! Undo/redo history replay for `NodeGraphStore`.

use crate::profile::{ApplyPipelineError, GraphProfile, apply_transaction_with_profile};
use jellyflow_core::core::Graph;
use jellyflow_core::ops::{GraphHistory, GraphTransaction};

use super::{DispatchError, DispatchOutcome, DispatchProfile, NodeGraphStore};

#[derive(Debug, Clone, Copy)]
enum HistoryReplayDirection {
    Undo,
    Redo,
}

struct HistoryReplayPipeline<'store, 'profile> {
    store: &'store mut NodeGraphStore,
    direction: HistoryReplayDirection,
    dispatch_profile: DispatchProfile<'profile>,
}

struct HistoryReplayResult {
    graph: Graph,
    committed: GraphTransaction,
}

impl HistoryReplayDirection {
    fn replay<E>(
        self,
        history: &mut GraphHistory,
        apply: impl FnMut(&GraphTransaction) -> Result<GraphTransaction, E>,
    ) -> Result<bool, E> {
        match self {
            Self::Undo => history.undo(apply),
            Self::Redo => history.redo(apply),
        }
    }
}

impl<'store, 'profile> HistoryReplayPipeline<'store, 'profile> {
    fn new(
        store: &'store mut NodeGraphStore,
        direction: HistoryReplayDirection,
        dispatch_profile: DispatchProfile<'profile>,
    ) -> Self {
        Self {
            store,
            direction,
            dispatch_profile,
        }
    }

    fn run(mut self) -> Result<Option<HistoryReplayResult>, ApplyPipelineError> {
        let mut scratch = self.store.graph.clone();
        let mut committed: Option<GraphTransaction> = None;

        let mut history = std::mem::take(&mut self.store.history);
        let did: Result<bool, ApplyPipelineError> = self.direction.replay(&mut history, |tx| {
            let committed_tx = self.apply_transaction(&mut scratch, tx)?;
            committed = Some(committed_tx.clone());
            Ok(committed_tx)
        });
        self.store.history = history;
        let did = did?;
        if !did {
            return Ok(None);
        }

        let committed =
            committed.expect("history replay must apply a transaction when it reports progress");
        Ok(Some(HistoryReplayResult {
            graph: scratch,
            committed,
        }))
    }

    fn apply_transaction(
        &mut self,
        graph: &mut Graph,
        tx: &GraphTransaction,
    ) -> Result<GraphTransaction, ApplyPipelineError> {
        match &mut self.dispatch_profile {
            DispatchProfile::StoreProfile => self.store.apply_to_graph(graph, tx),
            DispatchProfile::External(profile) => {
                apply_transaction_with_profile(graph, &mut **profile, tx)
            }
        }
    }
}

impl NodeGraphStore {
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
        dispatch_profile: DispatchProfile<'_>,
    ) -> Result<Option<DispatchOutcome>, ApplyPipelineError> {
        let Some(replayed) = HistoryReplayPipeline::new(self, direction, dispatch_profile).run()?
        else {
            return Ok(None);
        };

        let patch = self.prepare_committed_graph_patch(replayed.graph, replayed.committed);
        self.run_after_dispatch_middleware(&patch);
        Ok(Some(self.publish_dispatch_outcome(patch)))
    }
}
