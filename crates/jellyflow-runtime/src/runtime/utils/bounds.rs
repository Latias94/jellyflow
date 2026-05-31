use crate::runtime::lookups::NodeGraphLookups;
use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize, NodeId};

use super::geometry::{CanvasBounds, normalize_node_origin};
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
