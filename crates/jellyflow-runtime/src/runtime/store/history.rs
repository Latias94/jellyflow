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
        mut dispatch_profile: DispatchProfile<'_>,
    ) -> Result<Option<DispatchOutcome>, ApplyPipelineError> {
        let mut scratch = self.graph.clone();
        let mut committed: Option<GraphTransaction> = None;

        let mut history = std::mem::take(&mut self.history);
        let did: Result<bool, ApplyPipelineError> = direction.replay(&mut history, |tx| {
            let committed_tx =
                self.apply_history_transaction(&mut scratch, &mut dispatch_profile, tx)?;
            committed = Some(committed_tx.clone());
            Ok(committed_tx)
        });
        self.history = history;
        let did = did?;
        if !did {
            return Ok(None);
        }

        let committed = committed.unwrap_or_default();
        let patch = self.prepare_committed_graph_patch(scratch, committed);
        Ok(Some(self.publish_dispatch_outcome(patch)))
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
}
