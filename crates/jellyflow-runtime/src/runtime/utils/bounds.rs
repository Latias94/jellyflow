use crate::node_origin::normalize_node_origin;
use crate::runtime::lookups::{NodeGraphLookups, NodeLookupEntry};
use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize, NodeId};

use super::geometry::CanvasBounds;
use super::options::{GetNodesBoundsOptions, GetNodesInsideOptions, NodeInclusion};

/// Returns the top-left position for a node, taking node origin into account.
///
/// This mirrors XyFlow's `getNodePositionWithOrigin` utility.
pub fn get_node_position_with_origin(
    lookups: &NodeGraphLookups,
    node: NodeId,
    node_origin: (f32, f32),
    fallback_size: Option<CanvasSize>,
) -> Option<CanvasPoint> {
    node_bounds(lookups, node, node_origin, fallback_size).map(CanvasBounds::top_left)
}

/// Returns the node's canvas-space bounding rect.
pub fn get_node_rect(
    lookups: &NodeGraphLookups,
    node: NodeId,
    node_origin: (f32, f32),
    fallback_size: Option<CanvasSize>,
) -> Option<CanvasRect> {
    node_bounds(lookups, node, node_origin, fallback_size).map(CanvasBounds::to_rect)
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
    let resolver = NodeBoundsResolver::from_bounds_options(options);
    let mut bounds: Option<CanvasBounds> = None;

    for node in nodes {
        let Some(entry) = lookups.node_lookup.get(&node) else {
            continue;
        };
        let Some(node_bounds) = resolver.bounds_for_entry(entry) else {
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
    let resolver = NodeBoundsResolver::from_inside_options(options);
    let query = CanvasBounds::from_rect(rect);
    if !query.is_finite() {
        return Vec::new();
    }

    let mut out: Vec<NodeId> = Vec::new();
    for (node, entry) in &lookups.node_lookup {
        let Some(node_bounds) = resolver.bounds_for_entry(entry) else {
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

fn node_bounds(
    lookups: &NodeGraphLookups,
    node: NodeId,
    node_origin: (f32, f32),
    fallback_size: Option<CanvasSize>,
) -> Option<CanvasBounds> {
    let entry = lookups.node_lookup.get(&node)?;
    NodeBoundsResolver::include_hidden(node_origin, fallback_size).bounds_for_entry(entry)
}

struct NodeBoundsResolver {
    node_origin: (f32, f32),
    fallback_size: Option<CanvasSize>,
    include_hidden: bool,
}

impl NodeBoundsResolver {
    fn include_hidden(node_origin: (f32, f32), fallback_size: Option<CanvasSize>) -> Self {
        Self {
            node_origin: normalize_node_origin(node_origin),
            fallback_size,
            include_hidden: true,
        }
    }

    fn from_bounds_options(options: GetNodesBoundsOptions) -> Self {
        Self {
            node_origin: normalize_node_origin(options.node_origin),
            fallback_size: options.fallback_size,
            include_hidden: options.include_hidden,
        }
    }

    fn from_inside_options(options: GetNodesInsideOptions) -> Self {
        Self {
            node_origin: normalize_node_origin(options.node_origin),
            fallback_size: options.fallback_size,
            include_hidden: options.include_hidden,
        }
    }

    fn bounds_for_entry(&self, entry: &NodeLookupEntry) -> Option<CanvasBounds> {
        if !self.include_hidden && entry.hidden {
            return None;
        }
        CanvasBounds::from_node(entry.pos, entry.size, self.node_origin, self.fallback_size)
    }
}
