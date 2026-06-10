use crate::runtime::store::NodeGraphStore;
use crate::runtime::viewport::ViewportTransform;
use jellyflow_core::core::{CanvasSize, EdgeId, GroupId, NodeId};

use super::order::{
    EdgeRenderOrderOptions, GroupRenderOrderOptions, NodeRenderOrderOptions,
    resolve_edge_render_order, resolve_group_render_order, resolve_node_render_order,
};
use super::query::{RenderingQueryOptions, RenderingQueryResult, resolve_rendering_query};
use super::visibility::{VisibleEdgeIdsRequest, VisibleNodeIdsRequest};

impl NodeGraphStore {
    /// Resolves the current group render order using the store's view-state and editor config.
    pub fn group_render_order(&self) -> Vec<GroupId> {
        let interaction = self.resolved_interaction_state();
        resolve_group_render_order(
            self.graph(),
            self.view_state(),
            GroupRenderOrderOptions::from_interaction(&interaction),
        )
    }

    /// Resolves the current node render order using the store's view-state and editor config.
    pub fn node_render_order(&self) -> Vec<NodeId> {
        let interaction = self.resolved_interaction_state();
        resolve_node_render_order(
            self.graph(),
            self.view_state(),
            NodeRenderOrderOptions::from_interaction(&interaction),
        )
    }

    /// Resolves the current edge render order using the store's view-state and editor config.
    pub fn edge_render_order(&self) -> Vec<EdgeId> {
        let interaction = self.resolved_interaction_state();
        resolve_edge_render_order(
            self.graph(),
            self.view_state(),
            EdgeRenderOrderOptions::from_interaction(&interaction),
        )
    }

    /// Resolves all renderer-facing order and visibility lists for the current store state.
    pub fn rendering_query(&self, viewport_size: CanvasSize) -> RenderingQueryResult {
        resolve_rendering_query(
            self.graph(),
            self.lookups(),
            self.view_state(),
            self.rendering_query_options(viewport_size),
        )
    }

    fn rendering_query_options(&self, viewport_size: CanvasSize) -> RenderingQueryOptions {
        let interaction = self.resolved_interaction_state();
        RenderingQueryOptions::new(
            GroupRenderOrderOptions::from_interaction(&interaction),
            NodeRenderOrderOptions::from_interaction(&interaction),
            EdgeRenderOrderOptions::from_interaction(&interaction),
        )
        .with_visible_nodes(self.visible_node_ids_request(viewport_size))
        .with_visible_edges(self.visible_edge_ids_request(viewport_size))
    }

    fn visible_edge_ids_request(&self, viewport_size: CanvasSize) -> Option<VisibleEdgeIdsRequest> {
        let transform = ViewportTransform::from_view_state(self.view_state())?;
        let interaction = self.resolved_interaction_state();
        let rendering = interaction.rendering_interaction();
        let node_origin = interaction.node_origin.normalized();
        Some(
            VisibleEdgeIdsRequest::new(transform, viewport_size)
                .with_only_render_visible_elements(rendering.only_render_visible_elements)
                .with_node_origin((node_origin.x, node_origin.y)),
        )
    }

    fn visible_node_ids_request(&self, viewport_size: CanvasSize) -> Option<VisibleNodeIdsRequest> {
        let transform = ViewportTransform::from_view_state(self.view_state())?;
        let interaction = self.resolved_interaction_state();
        let rendering = interaction.rendering_interaction();
        let node_origin = interaction.node_origin.normalized();
        Some(
            VisibleNodeIdsRequest::new(transform, viewport_size)
                .with_only_render_visible_elements(rendering.only_render_visible_elements)
                .with_node_origin((node_origin.x, node_origin.y)),
        )
    }
}
