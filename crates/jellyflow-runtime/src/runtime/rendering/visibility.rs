use std::collections::{BTreeSet, HashSet};

use crate::io::NodeGraphViewState;
use crate::runtime::geometry::CanvasBounds;
use crate::runtime::lookups::NodeGraphLookups;
use crate::runtime::utils::{
    GetNodesInsideOptions, NodeInclusion, get_node_rect, get_nodes_inside,
};
use crate::runtime::viewport::ViewportTransform;
use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize, EdgeId, Graph, NodeId};

use super::order::{
    EdgeRenderOrderOptions, NodeRenderOrderOptions, resolve_edge_render_order,
    resolve_node_render_order,
};

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

/// Request for resolving edge ids that should be considered visible to a renderer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VisibleEdgeIdsRequest {
    /// Current viewport transform.
    pub transform: ViewportTransform,
    /// Logical screen-pixel size of the adapter viewport.
    pub viewport_size: CanvasSize,
    /// Whether to cull edges outside the current viewport.
    pub only_render_visible_elements: bool,
    /// Node origin fallback used when an endpoint node does not carry its own origin.
    pub node_origin: (f32, f32),
    /// Fallback size for endpoint nodes without resolved dimensions.
    pub fallback_size: Option<CanvasSize>,
}

impl VisibleEdgeIdsRequest {
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

/// Resolves deterministic edge ids visible to the current viewport.
///
/// This mirrors XyFlow's visible-edge contract: an edge is visible when the union of its source and
/// target node bounds intersects the current viewport. It intentionally does not inspect renderer
/// path geometry, so adapters can keep using their own edge routing while sharing the same culling
/// decision.
pub fn resolve_visible_edge_ids(
    graph: &Graph,
    lookups: &NodeGraphLookups,
    request: VisibleEdgeIdsRequest,
) -> Vec<EdgeId> {
    let Some(viewport_rect) = canvas_viewport_rect(request.transform, request.viewport_size) else {
        return Vec::new();
    };

    if !request.only_render_visible_elements {
        return all_non_hidden_edge_ids(graph);
    }

    let Some(viewport_bounds) = CanvasBounds::from_rect(viewport_rect) else {
        return Vec::new();
    };

    graph
        .edges
        .iter()
        .filter_map(|(id, edge)| {
            if edge.hidden {
                return None;
            }
            let lookup = lookups.edge_lookup.get(id)?;
            let source = visible_node_bounds(lookups, lookup.from_node, request)?;
            let target = visible_node_bounds(lookups, lookup.to_node, request)?;
            viewport_bounds
                .intersects(source.union(target))
                .then_some(*id)
        })
        .collect()
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

/// Resolves visible edge ids in the order an adapter should paint them.
///
/// This composes viewport culling with the stable edge render-order contract so adapters do not
/// duplicate hidden-edge, draw-order, and selected-edge elevation rules before rendering.
pub fn resolve_visible_edge_render_order(
    graph: &Graph,
    lookups: &NodeGraphLookups,
    view_state: &NodeGraphViewState,
    visibility_request: VisibleEdgeIdsRequest,
    order_options: EdgeRenderOrderOptions,
) -> Vec<EdgeId> {
    let visible: HashSet<EdgeId> = resolve_visible_edge_ids(graph, lookups, visibility_request)
        .into_iter()
        .collect();
    if visible.is_empty() {
        return Vec::new();
    }

    resolve_edge_render_order(graph, view_state, order_options)
        .into_iter()
        .filter(|id| visible.contains(id))
        .collect()
}

pub(crate) fn resolve_visible_node_order_from_ids(
    visible_node_ids: Vec<NodeId>,
    node_order: &[NodeId],
) -> (Vec<NodeId>, Vec<NodeId>) {
    let visible = visible_node_ids.iter().copied().collect::<BTreeSet<_>>();
    let visible_node_render_order = node_order
        .iter()
        .copied()
        .filter(|id| visible.contains(id))
        .collect();

    (visible_node_ids, visible_node_render_order)
}

pub(crate) fn resolve_visible_edge_order_from_ids(
    visible_edge_ids: Vec<EdgeId>,
    edge_order: &[EdgeId],
) -> (Vec<EdgeId>, Vec<EdgeId>) {
    let visible = visible_edge_ids.iter().copied().collect::<BTreeSet<_>>();
    let visible_edge_render_order = edge_order
        .iter()
        .copied()
        .filter(|id| visible.contains(id))
        .collect();

    (visible_edge_ids, visible_edge_render_order)
}

pub(crate) fn all_non_hidden_node_ids(lookups: &NodeGraphLookups) -> Vec<NodeId> {
    let mut ids: Vec<NodeId> = lookups
        .node_lookup
        .iter()
        .filter_map(|(id, entry)| entry.is_visible_with_hidden_policy(false).then_some(*id))
        .collect();
    ids.sort();
    ids
}

pub(crate) fn all_non_hidden_edge_ids(graph: &Graph) -> Vec<EdgeId> {
    graph
        .edges
        .iter()
        .filter_map(|(id, edge)| (!edge.hidden).then_some(*id))
        .collect()
}

fn visible_node_bounds(
    lookups: &NodeGraphLookups,
    node: NodeId,
    request: VisibleEdgeIdsRequest,
) -> Option<CanvasBounds> {
    let entry = lookups.node_lookup.get(&node)?;
    if !entry.is_visible_with_hidden_policy(false) {
        return None;
    }
    CanvasBounds::from_rect(get_node_rect(
        lookups,
        node,
        request.node_origin,
        request.fallback_size,
    )?)
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
