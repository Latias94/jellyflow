use std::collections::HashSet;

use crate::io::NodeGraphViewState;
use crate::runtime::lookups::NodeGraphLookups;
use crate::runtime::utils::{GetNodesInsideOptions, NodeInclusion, get_nodes_inside};
use crate::runtime::viewport::ViewportTransform;
use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize, Graph, NodeId};

use super::order::{NodeRenderOrderOptions, resolve_node_render_order};

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
