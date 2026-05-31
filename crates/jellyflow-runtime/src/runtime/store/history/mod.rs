//! Undo/redo history replay for `NodeGraphStore`.

mod replay;

use crate::profile::{ApplyPipelineError, GraphProfile};

use self::replay::{HistoryReplayDirection, HistoryReplayPipeline};
use super::dispatch_profile::DispatchProfile;
use super::{DispatchError, DispatchOutcome, NodeGraphStore};

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
