use std::collections::BTreeSet;

use crate::core::{Edge, EdgeId, Graph, GroupId, NodeId, Port, PortId};
use crate::ops::GraphOp;

pub(super) fn incident_edges_for_port(graph: &Graph, id: PortId) -> Vec<(EdgeId, Edge)> {
    let mut edges: Vec<(EdgeId, Edge)> = graph
        .edges
        .iter()
        .filter_map(|(edge_id, edge)| {
            if edge.from == id || edge.to == id {
                Some((*edge_id, edge.clone()))
            } else {
                None
            }
        })
        .collect();
    edges.sort_by_key(|(edge_id, _)| *edge_id);
    edges
}

pub(super) fn remove_edge_ops_for_port(graph: &Graph, id: PortId) -> Vec<GraphOp> {
    incident_edges_for_port(graph, id)
        .into_iter()
        .map(|(id, edge)| GraphOp::RemoveEdge { id, edge })
        .collect()
}

pub(super) fn ports_for_node(graph: &Graph, id: NodeId) -> Vec<(PortId, Port)> {
    let mut ports: Vec<(PortId, Port)> = graph
        .ports
        .iter()
        .filter_map(|(port_id, port)| {
            if port.node == id {
                Some((*port_id, port.clone()))
            } else {
                None
            }
        })
        .collect();
    ports.sort_by_key(|(port_id, _)| *port_id);
    ports
}

pub(super) fn incident_edges_for_ports(
    graph: &Graph,
    port_ids: &BTreeSet<PortId>,
) -> Vec<(EdgeId, Edge)> {
    let mut edges: Vec<(EdgeId, Edge)> = graph
        .edges
        .iter()
        .filter_map(|(edge_id, edge)| {
            if port_ids.contains(&edge.from) || port_ids.contains(&edge.to) {
                Some((*edge_id, edge.clone()))
            } else {
                None
            }
        })
        .collect();
    edges.sort_by_key(|(edge_id, _)| *edge_id);
    edges
}

pub(super) fn detached_nodes_for_group(
    graph: &Graph,
    id: GroupId,
) -> Vec<(NodeId, Option<GroupId>)> {
    let mut detached: Vec<(NodeId, Option<GroupId>)> = graph
        .nodes
        .iter()
        .filter_map(|(node_id, node)| (node.parent == Some(id)).then_some((*node_id, node.parent)))
        .collect();
    detached.sort_by_key(|(node_id, _)| *node_id);
    detached
}
