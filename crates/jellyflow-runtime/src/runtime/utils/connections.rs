use crate::runtime::lookups::{ConnectionSide, NodeGraphLookups};
use jellyflow_core::core::{EdgeId, NodeId};

/// Returns the nodes connected as *targets* of the given node's outgoing edges.
pub fn get_outgoers(lookups: &NodeGraphLookups, node: NodeId) -> Vec<NodeId> {
    let Some(conns) = lookups.connections_for_node_side(node, ConnectionSide::Source) else {
        return Vec::new();
    };
    sorted_unique(conns.values().map(|c| c.target_node).collect())
}

/// Returns the nodes connected as *sources* of the given node's incoming edges.
pub fn get_incomers(lookups: &NodeGraphLookups, node: NodeId) -> Vec<NodeId> {
    let Some(conns) = lookups.connections_for_node_side(node, ConnectionSide::Target) else {
        return Vec::new();
    };
    sorted_unique(conns.values().map(|c| c.source_node).collect())
}

/// Returns all edges incident to the given node (both directions).
pub fn get_connected_edges(lookups: &NodeGraphLookups, node: NodeId) -> Vec<EdgeId> {
    let Some(conns) = lookups.connections_for_node(node) else {
        return Vec::new();
    };
    sorted_unique(conns.values().map(|c| c.edge).collect())
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
    sorted_unique(out)
}

fn sorted_unique<T: Ord>(mut items: Vec<T>) -> Vec<T> {
    items.sort();
    items.dedup();
    items
}
