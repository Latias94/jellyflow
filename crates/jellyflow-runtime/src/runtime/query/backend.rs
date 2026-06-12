use std::cell::RefCell;

use crate::io::{NodeGraphInteractionState, NodeGraphViewState};
use crate::runtime::binding::{BindingQueryOptions, BindingQueryResult};
use crate::runtime::lookups::NodeGraphLookups;
use crate::runtime::measurement::LayoutFactsQueryResult;
use crate::runtime::rendering::RenderingQueryResult;
use jellyflow_core::core::{CanvasSize, Graph};

use super::spatial::SpatialQueryCache;

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
    pub(crate) graph_revision: u64,
    pub(crate) lookups: &'a NodeGraphLookups,
    pub(crate) view_state: &'a NodeGraphViewState,
    pub(crate) interaction: NodeGraphInteractionState,
    pub(crate) layout_facts_revision: u64,
    pub(crate) spatial_cache: &'a RefCell<SpatialQueryCache>,
}

impl<'a> NodeGraphQuerySnapshot<'a> {
    pub(crate) fn new(
        graph: &'a Graph,
        graph_revision: u64,
        lookups: &'a NodeGraphLookups,
        view_state: &'a NodeGraphViewState,
        interaction: NodeGraphInteractionState,
        layout_facts_revision: u64,
        spatial_cache: &'a RefCell<SpatialQueryCache>,
    ) -> Self {
        Self {
            graph,
            graph_revision,
            lookups,
            view_state,
            interaction,
            layout_facts_revision,
            spatial_cache,
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
