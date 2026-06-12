use std::collections::BTreeSet;

use crate::core::{
    Binding, BindingEndpoint, BindingId, Edge, EdgeId, Graph, GraphLocalBindingTarget, GroupId,
    NodeId, Port, PortId, StickyNoteId,
};
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
        .map(|(id, edge)| GraphOp::RemoveEdge {
            id,
            edge,
            bindings: bindings_for_edge(graph, id),
        })
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

pub(super) fn bindings_for_node(graph: &Graph, id: NodeId) -> Vec<(BindingId, Binding)> {
    bindings_for_target(
        graph,
        |target| matches!(target, GraphLocalBindingTarget::Node { id: target_id } if target_id == id),
    )
}

pub(super) fn bindings_for_node_removal(
    graph: &Graph,
    node: NodeId,
    ports: &[(PortId, Port)],
    edges: &[(EdgeId, Edge)],
) -> Vec<(BindingId, Binding)> {
    collect_binding_union([
        bindings_for_node(graph, node),
        ports
            .iter()
            .flat_map(|(port_id, _)| bindings_for_port(graph, *port_id))
            .collect(),
        edges
            .iter()
            .flat_map(|(edge_id, _)| bindings_for_edge(graph, *edge_id))
            .collect(),
    ])
}

pub(super) fn bindings_for_port(graph: &Graph, id: PortId) -> Vec<(BindingId, Binding)> {
    bindings_for_target(
        graph,
        |target| matches!(target, GraphLocalBindingTarget::Port { id: target_id } if target_id == id),
    )
}

pub(super) fn bindings_for_port_removal(
    graph: &Graph,
    port: PortId,
    edges: &[(EdgeId, Edge)],
) -> Vec<(BindingId, Binding)> {
    collect_binding_union([
        bindings_for_port(graph, port),
        edges
            .iter()
            .flat_map(|(edge_id, _)| bindings_for_edge(graph, *edge_id))
            .collect(),
    ])
}

pub(super) fn bindings_for_edge(graph: &Graph, id: EdgeId) -> Vec<(BindingId, Binding)> {
    bindings_for_target(
        graph,
        |target| matches!(target, GraphLocalBindingTarget::Edge { id: target_id } if target_id == id),
    )
}

pub(super) fn bindings_for_group(graph: &Graph, id: GroupId) -> Vec<(BindingId, Binding)> {
    bindings_for_target(
        graph,
        |target| matches!(target, GraphLocalBindingTarget::Group { id: target_id } if target_id == id),
    )
}

pub(super) fn bindings_for_sticky_note(
    graph: &Graph,
    id: StickyNoteId,
) -> Vec<(BindingId, Binding)> {
    bindings_for_target(
        graph,
        |target| matches!(target, GraphLocalBindingTarget::StickyNote { id: target_id } if target_id == id),
    )
}

fn bindings_for_target(
    graph: &Graph,
    mut matches_target: impl FnMut(GraphLocalBindingTarget) -> bool,
) -> Vec<(BindingId, Binding)> {
    let mut bindings: Vec<(BindingId, Binding)> = graph
        .bindings
        .iter()
        .filter_map(|(binding_id, binding)| {
            let subject_matches = endpoint_matches_target(&binding.subject, &mut matches_target);
            let target_matches = endpoint_matches_target(&binding.target, &mut matches_target);
            (subject_matches || target_matches).then_some((*binding_id, binding.clone()))
        })
        .collect();
    bindings.sort_by_key(|(binding_id, _)| *binding_id);
    bindings
}

fn endpoint_matches_target(
    endpoint: &BindingEndpoint,
    matches_target: &mut impl FnMut(GraphLocalBindingTarget) -> bool,
) -> bool {
    endpoint.graph_local_target().is_some_and(matches_target)
}

fn collect_binding_union<const N: usize>(
    binding_sets: [Vec<(BindingId, Binding)>; N],
) -> Vec<(BindingId, Binding)> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for (binding_id, binding) in binding_sets.into_iter().flatten() {
        if seen.insert(binding_id) {
            out.push((binding_id, binding));
        }
    }
    out.sort_by_key(|(binding_id, _)| *binding_id);
    out
}
