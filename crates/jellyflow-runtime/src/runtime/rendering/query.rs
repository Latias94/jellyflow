use std::collections::HashSet;

use crate::io::NodeGraphViewState;
use crate::runtime::lookups::NodeGraphLookups;
use jellyflow_core::core::{EdgeId, Graph, GroupId, NodeId};

use super::order::{
    EdgeRenderOrderOptions, GroupRenderOrderOptions, NodeRenderOrderOptions,
    resolve_edge_render_order, resolve_group_render_order, resolve_node_render_order,
};
use super::visibility::{VisibleNodeIdsRequest, resolve_visible_node_ids};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderingQueryOptions {
    pub groups: GroupRenderOrderOptions,
    pub nodes: NodeRenderOrderOptions,
    pub edges: EdgeRenderOrderOptions,
    pub visible_nodes: Option<VisibleNodeIdsRequest>,
}

impl RenderingQueryOptions {
    pub fn new(
        groups: GroupRenderOrderOptions,
        nodes: NodeRenderOrderOptions,
        edges: EdgeRenderOrderOptions,
    ) -> Self {
        Self {
            groups,
            nodes,
            edges,
            visible_nodes: None,
        }
    }

    pub fn with_visible_nodes(mut self, request: Option<VisibleNodeIdsRequest>) -> Self {
        self.visible_nodes = request;
        self
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RenderingQueryResult {
    pub group_order: Vec<GroupId>,
    pub node_order: Vec<NodeId>,
    pub edge_order: Vec<EdgeId>,
    pub visible_node_ids: Vec<NodeId>,
    pub visible_node_render_order: Vec<NodeId>,
}

pub fn resolve_rendering_query(
    graph: &Graph,
    lookups: &NodeGraphLookups,
    view_state: &NodeGraphViewState,
    options: RenderingQueryOptions,
) -> RenderingQueryResult {
    let group_order = resolve_group_render_order(graph, view_state, options.groups);
    let node_order = resolve_node_render_order(graph, view_state, options.nodes);
    let edge_order = resolve_edge_render_order(graph, view_state, options.edges);
    let (visible_node_ids, visible_node_render_order) =
        resolve_visible_nodes(lookups, &node_order, options.visible_nodes);

    RenderingQueryResult {
        group_order,
        node_order,
        edge_order,
        visible_node_ids,
        visible_node_render_order,
    }
}

fn resolve_visible_nodes(
    lookups: &NodeGraphLookups,
    node_order: &[NodeId],
    request: Option<VisibleNodeIdsRequest>,
) -> (Vec<NodeId>, Vec<NodeId>) {
    let Some(request) = request else {
        return (Vec::new(), Vec::new());
    };

    let visible_node_ids = resolve_visible_node_ids(lookups, request);
    let visible: HashSet<NodeId> = visible_node_ids.iter().copied().collect();
    let visible_node_render_order = node_order
        .iter()
        .copied()
        .filter(|id| visible.contains(id))
        .collect();

    (visible_node_ids, visible_node_render_order)
}
