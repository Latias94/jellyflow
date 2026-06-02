use crate::runtime::store::NodeGraphStore;
use crate::runtime::viewport::ViewportTransform;
use jellyflow_core::core::{CanvasSize, EdgeId, GroupId, NodeId};

use super::order::{
    EdgeRenderOrderOptions, GroupRenderOrderOptions, NodeRenderOrderOptions,
    resolve_edge_render_order, resolve_group_render_order, resolve_node_render_order,
};
use super::visibility::{
    VisibleNodeIdsRequest, resolve_visible_node_ids, resolve_visible_node_render_order,
};

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

    /// Resolves node ids visible in the given logical viewport size using current store tuning.
    pub fn visible_node_ids(&self, viewport_size: CanvasSize) -> Vec<NodeId> {
        let Some(request) = self.visible_node_ids_request(viewport_size) else {
            return Vec::new();
        };

        resolve_visible_node_ids(self.lookups(), request)
    }

    /// Resolves visible node ids in the current node paint order using current store tuning.
    pub fn visible_node_render_order(&self, viewport_size: CanvasSize) -> Vec<NodeId> {
        let Some(request) = self.visible_node_ids_request(viewport_size) else {
            return Vec::new();
        };
        let interaction = self.resolved_interaction_state();
        resolve_visible_node_render_order(
            self.graph(),
            self.lookups(),
            self.view_state(),
            request,
            NodeRenderOrderOptions::from_interaction(&interaction),
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
