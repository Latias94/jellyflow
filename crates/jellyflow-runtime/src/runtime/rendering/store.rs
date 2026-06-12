use crate::runtime::store::NodeGraphStore;
use jellyflow_core::core::{CanvasSize, EdgeId, GroupId, NodeId};

use super::order::{
    EdgeRenderOrderOptions, GroupRenderOrderOptions, NodeRenderOrderOptions,
    resolve_edge_render_order, resolve_group_render_order, resolve_node_render_order,
};
use super::query::RenderingQueryResult;

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
        crate::runtime::query::rendering_query(self, viewport_size)
    }
}
