use crate::io::{
    NodeGraphEditorConfig, NodeGraphInteractionConfig, NodeGraphRuntimeTuning, NodeGraphViewState,
};
use jellyflow_core::core::Graph;
use jellyflow_core::ops::GraphHistory;

/// Immutable snapshot of store state for selector subscriptions.
#[derive(Debug, Clone, Copy)]
pub struct NodeGraphStoreSnapshot<'a> {
    pub graph: &'a Graph,
    pub graph_revision: u64,
    pub layout_facts_revision: u64,
    pub view_state: &'a NodeGraphViewState,
    pub interaction: &'a NodeGraphInteractionConfig,
    pub runtime_tuning: &'a NodeGraphRuntimeTuning,
    pub history: &'a GraphHistory,
}

impl<'a> NodeGraphStoreSnapshot<'a> {
    pub(crate) fn new(
        graph: &'a Graph,
        graph_revision: u64,
        layout_facts_revision: u64,
        view_state: &'a NodeGraphViewState,
        interaction: &'a NodeGraphInteractionConfig,
        runtime_tuning: &'a NodeGraphRuntimeTuning,
        history: &'a GraphHistory,
    ) -> Self {
        Self {
            graph,
            graph_revision,
            layout_facts_revision,
            view_state,
            interaction,
            runtime_tuning,
            history,
        }
    }
}

/// Atomic document replacement snapshot.
#[derive(Debug, Clone, Copy)]
pub struct NodeGraphDocumentSnapshot<'a> {
    pub graph: &'a Graph,
    pub graph_revision: u64,
    pub view_state: &'a NodeGraphViewState,
    pub editor_config: &'a NodeGraphEditorConfig,
}
