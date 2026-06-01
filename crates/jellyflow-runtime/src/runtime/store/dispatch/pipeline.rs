use crate::profile::ApplyPipelineError;
use jellyflow_core::core::Graph;
use jellyflow_core::ops::GraphTransaction;

use super::super::{NodeGraphStore, dispatch_profile::DispatchProfile};
use super::gate::DispatchTransactionGate;

pub(super) struct DispatchPipeline<'store, 'profile> {
    store: &'store mut NodeGraphStore,
    dispatch_profile: DispatchProfile<'profile>,
}

pub(super) enum DispatchPipelineResult {
    Empty(GraphTransaction),
    Commit {
        graph: Graph,
        committed: GraphTransaction,
    },
}

impl<'store, 'profile> DispatchPipeline<'store, 'profile> {
    pub(super) fn new(
        store: &'store mut NodeGraphStore,
        dispatch_profile: DispatchProfile<'profile>,
    ) -> Self {
        Self {
            store,
            dispatch_profile,
        }
    }

    pub(super) fn run(
        mut self,
        tx: &GraphTransaction,
    ) -> Result<DispatchPipelineResult, ApplyPipelineError> {
        let mut tx = DispatchTransactionGate::normalize_and_validate(tx.clone())?;
        if tx.is_empty() {
            return Ok(DispatchPipelineResult::Empty(tx));
        }

        self.store.run_before_dispatch_middleware(&mut tx)?;
        tx = DispatchTransactionGate::normalize_and_validate(tx)?;
        if tx.is_empty() {
            return Ok(DispatchPipelineResult::Empty(tx));
        }

        let (graph, committed) = self.apply_to_scratch(&tx)?;
        let committed = DispatchTransactionGate::normalize_and_validate(committed)?;
        Ok(DispatchPipelineResult::Commit { graph, committed })
    }

    fn apply_to_scratch(
        &mut self,
        tx: &GraphTransaction,
    ) -> Result<(Graph, GraphTransaction), ApplyPipelineError> {
        let mut scratch = self.store.graph.clone();
        let committed = self
            .dispatch_profile
            .apply_to_graph(self.store, &mut scratch, tx)?;
        Ok((scratch, committed))
    }
}
