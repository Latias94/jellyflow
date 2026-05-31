//! Headless helper utilities (XyFlow-style graph helpers).
//!
//! XyFlow exposes a set of convenience utilities in `@xyflow/system/src/utils/*` for common
//! graph queries (e.g. "incomers/outgoers", "connected edges", "nodes inside rect", bounds).
//!
//! In Jellyflow, the canonical document (`core::Graph`) is port-based (edges connect ports), so
//! these helpers are built on top of `runtime::lookups::NodeGraphLookups` which provides a stable,
//! headless-safe adjacency surface.

use crate::runtime::lookups::{ConnectionSide, NodeGraphLookups};
use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize, EdgeId, NodeId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeInclusion {
    /// Include nodes that intersect the query rect.
    Partial,
    /// Include nodes only when fully contained within the query rect.
    Full,
}

#[derive(Debug, Clone, Copy)]
pub struct GetNodesBoundsOptions {
    /// Node origin (anchor) used to interpret `Node.pos`.
    ///
    /// - `(0.0, 0.0)` means `pos` is top-left.
    /// - `(0.5, 0.5)` means `pos` is center.
    pub node_origin: (f32, f32),
    /// Whether to include hidden nodes.
    pub include_hidden: bool,
    /// Fallback size to use when a node has no explicit size.
    ///
    /// When `None`, nodes without a size are skipped.
    pub fallback_size: Option<CanvasSize>,
}

impl Default for GetNodesBoundsOptions {
    fn default() -> Self {
        Self {
            node_origin: (0.0, 0.0),
            include_hidden: false,
            fallback_size: None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct GetNodesInsideOptions {
    pub inclusion: NodeInclusion,
    pub node_origin: (f32, f32),
    pub include_hidden: bool,
    pub fallback_size: Option<CanvasSize>,
}

impl Default for GetNodesInsideOptions {
    fn default() -> Self {
        Self {
            inclusion: NodeInclusion::Partial,
            node_origin: (0.0, 0.0),
            include_hidden: false,
            fallback_size: None,
        }
    }
}

/// Returns the nodes connected as *targets* of the given node's outgoing edges.
pub fn get_outgoers(lookups: &NodeGraphLookups, node: NodeId) -> Vec<NodeId> {
    let Some(conns) = lookups.connections_for_node_side(node, ConnectionSide::Source) else {
        return Vec::new();
    };
    let mut out: Vec<NodeId> = conns.values().map(|c| c.target_node).collect();
    out.sort();
    out.dedup();
    out
}

/// Returns the top-left position for a node, taking node origin into account.
///
/// This mirrors XyFlow's `getNodePositionWithOrigin` utility.
pub fn get_node_position_with_origin(
    lookups: &NodeGraphLookups,
    node: NodeId,
    node_origin: (f32, f32),
    fallback_size: Option<CanvasSize>,
) -> Option<CanvasPoint> {
    let entry = lookups.node_lookup.get(&node)?;
    let (ox, oy) = normalize_node_origin(node_origin);
    let bounds = CanvasBounds::from_node(entry.pos, entry.size, (ox, oy), fallback_size)?;
    Some(bounds.top_left())
}

/// Returns the node's canvas-space bounding rect.
pub fn get_node_rect(
    lookups: &NodeGraphLookups,
    node: NodeId,
    node_origin: (f32, f32),
    fallback_size: Option<CanvasSize>,
) -> Option<CanvasRect> {
    let entry = lookups.node_lookup.get(&node)?;
    let (ox, oy) = normalize_node_origin(node_origin);
    let bounds = CanvasBounds::from_node(entry.pos, entry.size, (ox, oy), fallback_size)?;
    Some(bounds.to_rect())
}

/// Returns the nodes connected as *sources* of the given node's incoming edges.
pub fn get_incomers(lookups: &NodeGraphLookups, node: NodeId) -> Vec<NodeId> {
    let Some(conns) = lookups.connections_for_node_side(node, ConnectionSide::Target) else {
        return Vec::new();
    };
    let mut out: Vec<NodeId> = conns.values().map(|c| c.source_node).collect();
    out.sort();
    out.dedup();
    out
}

/// Returns all edges incident to the given node (both directions).
pub fn get_connected_edges(lookups: &NodeGraphLookups, node: NodeId) -> Vec<EdgeId> {
    let Some(conns) = lookups.connections_for_node(node) else {
        return Vec::new();
    };
    let mut out: Vec<EdgeId> = conns.values().map(|c| c.edge).collect();
    out.sort();
    out.dedup();
    out
}

/// Returns all edges connected to any node in the given set.
///
/// This matches the intent of XyFlow's `getConnectedEdges(nodes, edges)` helper, but uses
/// `NodeGraphLookups` rather than scanning an edge array.
pub fn get_connected_edges_for_nodes(
    lookups: &NodeGraphLookups,
    nodes: impl IntoIterator<Item = NodeId>,
) -> Vec<EdgeId> {
    let mut out: Vec<EdgeId> = Vec::new();
    for node in nodes {
        out.extend(get_connected_edges(lookups, node));
    }
    out.sort();
    out.dedup();
    out
}

/// Computes the bounding rect enclosing the given nodes.
///
/// Returns `None` when no nodes contribute a valid rect (e.g. all nodes are missing sizes and
/// no `fallback_size` is provided).
pub fn get_nodes_bounds(
    lookups: &NodeGraphLookups,
    nodes: impl IntoIterator<Item = NodeId>,
    options: GetNodesBoundsOptions,
) -> Option<CanvasRect> {
    let (ox, oy) = normalize_node_origin(options.node_origin);
    let mut bounds: Option<CanvasBounds> = None;

    for node in nodes {
        let Some(entry) = lookups.node_lookup.get(&node) else {
            continue;
        };
        if !options.include_hidden && entry.hidden {
            continue;
        }
        let Some(node_bounds) =
            CanvasBounds::from_node(entry.pos, entry.size, (ox, oy), options.fallback_size)
        else {
            continue;
        };
        bounds = Some(match bounds {
            Some(current) => current.union(node_bounds),
            None => node_bounds,
        });
    }

    bounds.map(CanvasBounds::to_rect)
}

/// Returns the nodes that are inside the given query rect.
pub fn get_nodes_inside(
    lookups: &NodeGraphLookups,
    rect: CanvasRect,
    options: GetNodesInsideOptions,
) -> Vec<NodeId> {
    let (ox, oy) = normalize_node_origin(options.node_origin);
    let query = CanvasBounds::from_rect(rect);
    if !query.is_finite() {
        return Vec::new();
    }

    let mut out: Vec<NodeId> = Vec::new();
    for (node, entry) in &lookups.node_lookup {
        if !options.include_hidden && entry.hidden {
            continue;
        }
        let Some(node_bounds) =
            CanvasBounds::from_node(entry.pos, entry.size, (ox, oy), options.fallback_size)
        else {
            continue;
        };

        let keep = match options.inclusion {
            NodeInclusion::Partial => query.intersects(node_bounds),
            NodeInclusion::Full => query.contains(node_bounds),
        };
        if keep {
            out.push(*node);
        }
    }

    out.sort();
    out
}

fn normalize_node_origin(origin: (f32, f32)) -> (f32, f32) {
    let mut ox = origin.0;
    let mut oy = origin.1;
    if !ox.is_finite() {
        ox = 0.0;
    }
    if !oy.is_finite() {
        oy = 0.0;
    }
    (ox.clamp(0.0, 1.0), oy.clamp(0.0, 1.0))
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct CanvasBounds {
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
}

impl CanvasBounds {
    fn from_rect(rect: CanvasRect) -> Self {
        let width = rect.size.width.max(0.0);
        let height = rect.size.height.max(0.0);
        Self {
            min_x: rect.origin.x,
            min_y: rect.origin.y,
            max_x: rect.origin.x + width,
            max_y: rect.origin.y + height,
        }
    }

    fn from_node(
        pos: CanvasPoint,
        size: Option<CanvasSize>,
        node_origin: (f32, f32),
        fallback_size: Option<CanvasSize>,
    ) -> Option<Self> {
        let size = size.or(fallback_size)?;
        let width = size.width;
        let height = size.height;
        if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
            return None;
        }
        if !pos.x.is_finite() || !pos.y.is_finite() {
            return None;
        }

        let (origin_x, origin_y) = node_origin;
        let min_x = pos.x - origin_x * width;
        let min_y = pos.y - origin_y * height;
        Some(Self {
            min_x,
            min_y,
            max_x: min_x + width,
            max_y: min_y + height,
        })
    }

    fn is_finite(self) -> bool {
        self.min_x.is_finite()
            && self.min_y.is_finite()
            && self.max_x.is_finite()
            && self.max_y.is_finite()
    }

    fn top_left(self) -> CanvasPoint {
        CanvasPoint {
            x: self.min_x,
            y: self.min_y,
        }
    }

    fn to_rect(self) -> CanvasRect {
        CanvasRect {
            origin: self.top_left(),
            size: CanvasSize {
                width: (self.max_x - self.min_x).max(0.0),
                height: (self.max_y - self.min_y).max(0.0),
            },
        }
    }

    fn union(self, other: Self) -> Self {
        Self {
            min_x: self.min_x.min(other.min_x),
            min_y: self.min_y.min(other.min_y),
            max_x: self.max_x.max(other.max_x),
            max_y: self.max_y.max(other.max_y),
        }
    }

    fn intersects(self, other: Self) -> bool {
        self.min_x < other.max_x
            && self.max_x > other.min_x
            && self.min_y < other.max_y
            && self.max_y > other.min_y
    }

    fn contains(self, other: Self) -> bool {
        other.min_x >= self.min_x
            && other.min_y >= self.min_y
            && other.max_x <= self.max_x
            && other.max_y <= self.max_y
    }
}
