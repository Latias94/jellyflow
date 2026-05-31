//! Store snapshot construction helpers.

use crate::io::{NodeGraphInteractionConfig, NodeGraphRuntimeTuning, NodeGraphViewState};
use crate::runtime::events::NodeGraphStoreSnapshot;
use jellyflow_core::core::Graph;
use jellyflow_core::ops::GraphHistory;

use super::NodeGraphStore;

pub(super) struct StoreSnapshotParts<'a> {
    graph: &'a Graph,
    graph_revision: u64,
    view_state: &'a NodeGraphViewState,
    interaction: &'a NodeGraphInteractionConfig,
    runtime_tuning: &'a NodeGraphRuntimeTuning,
    history: &'a GraphHistory,
}

impl<'a> StoreSnapshotParts<'a> {
    pub(super) fn from_store_fields(
        graph: &'a Graph,
        graph_revision: u64,
        view_state: &'a NodeGraphViewState,
        interaction: &'a NodeGraphInteractionConfig,
        runtime_tuning: &'a NodeGraphRuntimeTuning,
        history: &'a GraphHistory,
    ) -> Self {
        Self {
            graph,
            graph_revision,
            view_state,
            interaction,
            runtime_tuning,
            history,
        }
    }

    fn from_store(store: &'a NodeGraphStore) -> Self {
        Self::from_store_fields(
            &store.graph,
            store.graph_revision,
            &store.view_state,
            &store.interaction,
            &store.runtime_tuning,
            &store.history,
        )
    }

    pub(super) fn snapshot(&self) -> NodeGraphStoreSnapshot<'a> {
        NodeGraphStoreSnapshot::new(
            self.graph,
            self.graph_revision,
            self.view_state,
            self.interaction,
            self.runtime_tuning,
            self.history,
        )
    }
}

impl NodeGraphStore {
    pub(super) fn snapshot(&self) -> NodeGraphStoreSnapshot<'_> {
        StoreSnapshotParts::from_store(self).snapshot()
    }
}
