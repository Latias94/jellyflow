use crate::profile::{ApplyPipelineError, GraphProfile, apply_transaction_with_profile_in_place};
use jellyflow_core::core::Graph;
use jellyflow_core::ops::GraphTransaction;

use super::NodeGraphStore;

pub(in crate::runtime::store) enum DispatchProfile<'a> {
    StoreProfile,
    External(&'a mut dyn GraphProfile),
}

impl DispatchProfile<'_> {
    pub(in crate::runtime::store) fn apply_to_graph(
        &mut self,
        store: &mut NodeGraphStore,
        graph: &mut Graph,
        tx: &GraphTransaction,
    ) -> Result<GraphTransaction, ApplyPipelineError> {
        match self {
            Self::StoreProfile => store.apply_to_graph(graph, tx),
            Self::External(profile) => {
                apply_transaction_with_profile_in_place(graph, &mut **profile, tx)
            }
        }
    }
}
