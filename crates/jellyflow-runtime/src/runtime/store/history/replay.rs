use crate::profile::ApplyPipelineError;
use jellyflow_core::core::Graph;
use jellyflow_core::ops::{GraphHistory, GraphTransaction};

use super::super::{DispatchProfile, NodeGraphStore};

#[derive(Debug, Clone, Copy)]
pub(super) enum HistoryReplayDirection {
    Undo,
    Redo,
}

pub(super) struct HistoryReplayPipeline<'store, 'profile> {
    store: &'store mut NodeGraphStore,
    direction: HistoryReplayDirection,
    dispatch_profile: DispatchProfile<'profile>,
}

pub(super) struct HistoryReplayResult {
    pub(super) graph: Graph,
    pub(super) committed: GraphTransaction,
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
    pub(super) fn new(
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

    pub(super) fn run(mut self) -> Result<Option<HistoryReplayResult>, ApplyPipelineError> {
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
        self.dispatch_profile.apply_to_graph(self.store, graph, tx)
    }
}
