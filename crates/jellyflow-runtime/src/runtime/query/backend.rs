use crate::io::{NodeGraphInteractionState, NodeGraphViewState};
use crate::runtime::binding::{BindingQueryOptions, BindingQueryResult};
use crate::runtime::lookups::NodeGraphLookups;
use crate::runtime::measurement::LayoutFactsQueryResult;
use crate::runtime::rendering::RenderingQueryResult;
use jellyflow_core::core::{CanvasSize, Graph};

/// Runtime query implementation kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum QueryBackendKind {
    Linear,
    Spatial,
}

/// Immutable facts used by runtime read-model queries.
#[derive(Debug, Clone)]
pub(crate) struct NodeGraphQuerySnapshot<'a> {
    pub(crate) graph: &'a Graph,
    pub(crate) lookups: &'a NodeGraphLookups,
    pub(crate) view_state: &'a NodeGraphViewState,
    pub(crate) interaction: NodeGraphInteractionState,
    pub(crate) layout_facts_revision: u64,
}

impl<'a> NodeGraphQuerySnapshot<'a> {
    pub(crate) fn new(
        graph: &'a Graph,
        lookups: &'a NodeGraphLookups,
        view_state: &'a NodeGraphViewState,
        interaction: NodeGraphInteractionState,
        layout_facts_revision: u64,
    ) -> Self {
        Self {
            graph,
            lookups,
            view_state,
            interaction,
            layout_facts_revision,
        }
    }

    pub(crate) fn node_origin(&self) -> (f32, f32) {
        let node_origin = self.interaction.node_origin.normalized();
        (node_origin.x, node_origin.y)
    }
}

/// Backend contract for store-level read-model composition.
pub(crate) trait QueryBackend {
    fn kind(&self) -> QueryBackendKind;

    fn rendering_query(
        &self,
        snapshot: &NodeGraphQuerySnapshot<'_>,
        viewport_size: CanvasSize,
    ) -> RenderingQueryResult;

    fn layout_facts_query(
        &self,
        snapshot: &NodeGraphQuerySnapshot<'_>,
        viewport_size: CanvasSize,
    ) -> LayoutFactsQueryResult;

    fn binding_query(
        &self,
        snapshot: &NodeGraphQuerySnapshot<'_>,
        options: BindingQueryOptions,
    ) -> BindingQueryResult;
}
