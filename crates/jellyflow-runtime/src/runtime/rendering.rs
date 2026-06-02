//! Renderer-neutral ordering helpers.
//!
//! Adapters still own painting, widgets, and GPU/UI details. This module only resolves the stable
//! node order they should use when interpreting Jellyflow view-state.

use std::collections::HashSet;

use crate::io::{NodeGraphInteractionState, NodeGraphViewState};
use crate::runtime::lookups::NodeGraphLookups;
use crate::runtime::store::NodeGraphStore;
use crate::runtime::utils::{GetNodesInsideOptions, NodeInclusion, get_nodes_inside};
use crate::runtime::viewport::ViewportTransform;
use jellyflow_core::core::{
    CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeId, Graph, GroupId, NodeId,
};

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

/// Request for resolving node ids that should be considered visible to a renderer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VisibleNodeIdsRequest {
    /// Current viewport transform.
    pub transform: ViewportTransform,
    /// Logical screen-pixel size of the adapter viewport.
    pub viewport_size: CanvasSize,
    /// Whether to cull nodes outside the current viewport.
    pub only_render_visible_elements: bool,
    /// Node origin fallback used when a node does not carry its own origin.
    pub node_origin: (f32, f32),
    /// Fallback size for nodes without resolved dimensions.
    pub fallback_size: Option<CanvasSize>,
}

impl VisibleNodeIdsRequest {
    pub fn new(transform: ViewportTransform, viewport_size: CanvasSize) -> Self {
        Self {
            transform,
            viewport_size,
            only_render_visible_elements: true,
            node_origin: (0.0, 0.0),
            fallback_size: None,
        }
    }

    pub fn with_only_render_visible_elements(mut self, enabled: bool) -> Self {
        self.only_render_visible_elements = enabled;
        self
    }

    pub fn with_node_origin(mut self, node_origin: (f32, f32)) -> Self {
        self.node_origin = node_origin;
        self
    }

    pub fn with_fallback_size(mut self, fallback_size: Option<CanvasSize>) -> Self {
        self.fallback_size = fallback_size;
        self
    }
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

/// Resolves deterministic node ids visible to the current viewport.
///
/// This is the headless counterpart to XyFlow's `onlyRenderVisibleElements` node culling. It keeps
/// the v1 implementation on the existing linear lookup scan so a later spatial index can replace
/// the backend without changing adapter calls.
pub fn resolve_visible_node_ids(
    lookups: &NodeGraphLookups,
    request: VisibleNodeIdsRequest,
) -> Vec<NodeId> {
    let Some(viewport_rect) = canvas_viewport_rect(request.transform, request.viewport_size) else {
        return Vec::new();
    };

    if !request.only_render_visible_elements {
        return all_non_hidden_node_ids(lookups);
    }

    get_nodes_inside(
        lookups,
        viewport_rect,
        GetNodesInsideOptions {
            inclusion: NodeInclusion::Partial,
            node_origin: request.node_origin,
            include_hidden: false,
            fallback_size: request.fallback_size,
        },
    )
}

/// Resolves visible node ids in the order an adapter should paint them.
///
/// This composes viewport culling with the stable render-order contract so adapters do not have to
/// duplicate hidden-node, draw-order, and selected-node elevation rules before rendering.
pub fn resolve_visible_node_render_order(
    graph: &Graph,
    lookups: &NodeGraphLookups,
    view_state: &NodeGraphViewState,
    visibility_request: VisibleNodeIdsRequest,
    order_options: NodeRenderOrderOptions,
) -> Vec<NodeId> {
    let visible: HashSet<NodeId> = resolve_visible_node_ids(lookups, visibility_request)
        .into_iter()
        .collect();
    if visible.is_empty() {
        return Vec::new();
    }

    resolve_node_render_order(graph, view_state, order_options)
        .into_iter()
        .filter(|id| visible.contains(id))
        .collect()
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

fn all_non_hidden_node_ids(lookups: &NodeGraphLookups) -> Vec<NodeId> {
    let mut ids: Vec<NodeId> = lookups
        .node_lookup
        .iter()
        .filter_map(|(id, entry)| entry.is_visible_with_hidden_policy(false).then_some(*id))
        .collect();
    ids.sort();
    ids
}

fn canvas_viewport_rect(
    transform: ViewportTransform,
    viewport_size: CanvasSize,
) -> Option<CanvasRect> {
    if !transform.is_valid() || !viewport_size.is_positive_finite() {
        return None;
    }

    let origin = transform.canvas_point_at_screen(CanvasPoint::default());
    let far_corner = transform.canvas_point_at_screen(CanvasPoint {
        x: viewport_size.width,
        y: viewport_size.height,
    });
    let rect = CanvasRect {
        origin,
        size: CanvasSize {
            width: far_corner.x - origin.x,
            height: far_corner.y - origin.y,
        },
    };
    rect.is_positive_finite().then_some(rect)
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
