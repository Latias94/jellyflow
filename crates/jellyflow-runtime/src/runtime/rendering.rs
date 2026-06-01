//! Renderer-neutral ordering helpers.
//!
//! Adapters still own painting, widgets, and GPU/UI details. This module only resolves the stable
//! node order they should use when interpreting Jellyflow view-state.

use std::collections::HashSet;

use crate::io::{NodeGraphInteractionState, NodeGraphViewState};
use crate::runtime::store::NodeGraphStore;
use jellyflow_core::core::{Edge, EdgeId, Graph, GroupId, NodeId};

/// Options for resolving a node render order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeRenderOrderOptions {
    /// Include hidden nodes in the result.
    pub include_hidden: bool,
    /// Raise selected nodes to the front of the returned order.
    pub elevate_nodes_on_select: bool,
}

/// Options for resolving an edge render order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EdgeRenderOrderOptions {
    /// Include hidden edges in the result.
    pub include_hidden: bool,
    /// Raise selected edges and edges connected to selected nodes to the end of the returned paint
    /// order.
    pub elevate_edges_on_select: bool,
}

/// Options for resolving a group render order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GroupRenderOrderOptions {
    /// Raise selected groups to the front of the returned order.
    pub elevate_groups_on_select: bool,
}

impl EdgeRenderOrderOptions {
    pub fn from_interaction(interaction: &NodeGraphInteractionState) -> Self {
        Self {
            include_hidden: false,
            elevate_edges_on_select: interaction.rendering_interaction().elevate_edges_on_select,
        }
    }
}

impl Default for EdgeRenderOrderOptions {
    fn default() -> Self {
        Self {
            include_hidden: false,
            elevate_edges_on_select: true,
        }
    }
}

impl GroupRenderOrderOptions {
    pub fn from_interaction(interaction: &NodeGraphInteractionState) -> Self {
        Self {
            elevate_groups_on_select: interaction.rendering_interaction().elevate_nodes_on_select,
        }
    }
}

impl Default for GroupRenderOrderOptions {
    fn default() -> Self {
        Self {
            elevate_groups_on_select: true,
        }
    }
}

impl NodeRenderOrderOptions {
    pub fn from_interaction(interaction: &NodeGraphInteractionState) -> Self {
        Self {
            include_hidden: false,
            elevate_nodes_on_select: interaction.rendering_interaction().elevate_nodes_on_select,
        }
    }
}

impl Default for NodeRenderOrderOptions {
    fn default() -> Self {
        Self {
            include_hidden: false,
            elevate_nodes_on_select: true,
        }
    }
}

/// Resolves the stable group order an adapter should paint.
///
/// The base order is:
/// 1. valid IDs from `view_state.group_draw_order`,
/// 2. remaining graph groups in deterministic ID order.
///
/// Groups are Jellyflow frame resources rather than XyFlow nodes, but adapters usually paint them
/// as node-like containers. When elevation is enabled, selected groups move to the end while
/// preserving their relative order.
pub fn resolve_group_render_order(
    graph: &Graph,
    view_state: &NodeGraphViewState,
    options: GroupRenderOrderOptions,
) -> Vec<GroupId> {
    let mut seen: HashSet<GroupId> = HashSet::new();
    let mut base: Vec<GroupId> = Vec::with_capacity(graph.groups.len());

    for id in &view_state.group_draw_order {
        if seen.insert(*id) && graph.groups.contains_key(id) {
            base.push(*id);
        }
    }

    for id in graph.groups.keys() {
        if seen.insert(*id) {
            base.push(*id);
        }
    }

    if !options.elevate_groups_on_select || view_state.selected_groups.is_empty() {
        return base;
    }

    let selected: HashSet<GroupId> = view_state.selected_groups.iter().copied().collect();
    let mut normal: Vec<GroupId> = Vec::with_capacity(base.len());
    let mut elevated: Vec<GroupId> = Vec::new();

    for id in base {
        if selected.contains(&id) {
            elevated.push(id);
        } else {
            normal.push(id);
        }
    }

    normal.extend(elevated);
    normal
}

/// Resolves the stable node order an adapter should paint.
///
/// The base order is:
/// 1. valid IDs from `view_state.draw_order`,
/// 2. remaining graph nodes in deterministic ID order.
///
/// When `elevate_nodes_on_select` is enabled, selected nodes are moved to the end while preserving
/// their relative order. Adapters can treat later IDs as visually above earlier IDs, mirroring the
/// default XyFlow `elevateNodesOnSelect` feel without introducing a renderer dependency.
pub fn resolve_node_render_order(
    graph: &Graph,
    view_state: &NodeGraphViewState,
    options: NodeRenderOrderOptions,
) -> Vec<NodeId> {
    let mut seen: HashSet<NodeId> = HashSet::new();
    let mut base: Vec<NodeId> = Vec::with_capacity(graph.nodes.len());

    for id in &view_state.draw_order {
        if seen.insert(*id) && node_is_renderable(graph, *id, options.include_hidden) {
            base.push(*id);
        }
    }

    for (id, node) in &graph.nodes {
        if seen.insert(*id) && (options.include_hidden || !node.hidden) {
            base.push(*id);
        }
    }

    if !options.elevate_nodes_on_select || view_state.selected_nodes.is_empty() {
        return base;
    }

    let selected: HashSet<NodeId> = view_state.selected_nodes.iter().copied().collect();
    let mut normal: Vec<NodeId> = Vec::with_capacity(base.len());
    let mut elevated: Vec<NodeId> = Vec::new();

    for id in base {
        if selected.contains(&id) {
            elevated.push(id);
        } else {
            normal.push(id);
        }
    }

    normal.extend(elevated);
    normal
}

/// Resolves the stable edge order an adapter should paint.
///
/// The base order is:
/// 1. valid IDs from `view_state.edge_draw_order`,
/// 2. remaining graph edges in deterministic ID order.
///
/// When `elevate_edges_on_select` is enabled, selected edges and edges connected to selected nodes
/// are moved to the end while preserving their relative order. This mirrors XyFlow's selected-edge
/// elevation behavior without exposing DOM z-index details in the headless interface.
pub fn resolve_edge_render_order(
    graph: &Graph,
    view_state: &NodeGraphViewState,
    options: EdgeRenderOrderOptions,
) -> Vec<EdgeId> {
    let mut seen: HashSet<EdgeId> = HashSet::new();
    let mut base: Vec<EdgeId> = Vec::with_capacity(graph.edges.len());

    for id in &view_state.edge_draw_order {
        let Some(edge) = graph.edges.get(id) else {
            continue;
        };
        if seen.insert(*id) && edge_is_renderable(edge, options.include_hidden) {
            base.push(*id);
        }
    }

    for (id, edge) in &graph.edges {
        if seen.insert(*id) && edge_is_renderable(edge, options.include_hidden) {
            base.push(*id);
        }
    }

    if !options.elevate_edges_on_select
        || (view_state.selected_edges.is_empty() && view_state.selected_nodes.is_empty())
    {
        return base;
    }

    let selected_edges: HashSet<EdgeId> = view_state.selected_edges.iter().copied().collect();
    let selected_nodes: HashSet<NodeId> = view_state.selected_nodes.iter().copied().collect();
    let mut normal: Vec<EdgeId> = Vec::with_capacity(base.len());
    let mut elevated: Vec<EdgeId> = Vec::new();

    for id in base.drain(..) {
        let Some(edge) = graph.edges.get(&id) else {
            continue;
        };
        if edge_is_elevated(graph, id, edge, &selected_edges, &selected_nodes) {
            elevated.push(id);
        } else {
            normal.push(id);
        }
    }

    normal.extend(elevated);
    normal
}

fn node_is_renderable(graph: &Graph, id: NodeId, include_hidden: bool) -> bool {
    graph
        .nodes
        .get(&id)
        .is_some_and(|node| include_hidden || !node.hidden)
}

fn edge_is_renderable(edge: &Edge, include_hidden: bool) -> bool {
    include_hidden || !edge.hidden
}

fn edge_is_elevated(
    graph: &Graph,
    id: EdgeId,
    edge: &Edge,
    selected_edges: &HashSet<EdgeId>,
    selected_nodes: &HashSet<NodeId>,
) -> bool {
    if selected_edges.contains(&id) {
        return true;
    }
    if selected_nodes.is_empty() {
        return false;
    }

    graph
        .ports
        .get(&edge.from)
        .is_some_and(|port| selected_nodes.contains(&port.node))
        || graph
            .ports
            .get(&edge.to)
            .is_some_and(|port| selected_nodes.contains(&port.node))
}

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
}
