use crate::rules::ConnectPlan;
use jellyflow_core::core::{Edge, EdgeId, EdgeKind, Graph, PortCapacity, PortId};
use jellyflow_core::ops::{GraphMutationError, GraphMutationPlanner, GraphOp};

pub(in crate::rules::connection) fn reject_mutation_error(
    error: GraphMutationError,
) -> ConnectPlan {
    ConnectPlan::reject(error.to_string())
}

fn remove_edge_op(graph: &Graph, edge_id: EdgeId) -> GraphOp {
    GraphMutationPlanner::new(graph)
        .remove_edge_op(edge_id)
        .expect("edge id came from the current graph")
}

pub(in crate::rules::connection) fn add_existing_ports_edge_op(
    graph: &Graph,
    edge_id: EdgeId,
    edge: Edge,
) -> Result<GraphOp, ConnectPlan> {
    GraphMutationPlanner::new(graph)
        .add_edge_op(edge_id, edge)
        .map_err(reject_mutation_error)
}

pub(in crate::rules::connection) fn edge_between(kind: EdgeKind, from: PortId, to: PortId) -> Edge {
    Edge {
        kind,
        from,
        to,
        selectable: None,
        deletable: None,
        reconnectable: None,
    }
}

pub(in crate::rules::connection) fn edge_like(edge: &Edge, from: PortId, to: PortId) -> Edge {
    Edge {
        kind: edge.kind,
        from,
        to,
        selectable: edge.selectable,
        deletable: edge.deletable,
        reconnectable: edge.reconnectable,
    }
}

pub(in crate::rules::connection) fn connection_exists(
    graph: &Graph,
    edge_kind: EdgeKind,
    from: PortId,
    to: PortId,
    skip_edge: Option<EdgeId>,
) -> bool {
    graph.edges.iter().any(|(edge_id, edge)| {
        Some(*edge_id) != skip_edge && edge.kind == edge_kind && edge.from == from && edge.to == to
    })
}

pub(in crate::rules::connection) fn disconnect_for_capacity(
    graph: &Graph,
    edge_kind: EdgeKind,
    from_id: PortId,
    from_capacity: PortCapacity,
    to_id: PortId,
    to_capacity: PortCapacity,
    skip_edge: Option<EdgeId>,
) -> Vec<GraphOp> {
    let mut ops: Vec<GraphOp> = Vec::new();

    disconnect_for_endpoint_capacity(
        graph,
        edge_kind,
        CapacityEndpoint::Source(from_id),
        from_capacity,
        skip_edge,
        &mut ops,
    );
    disconnect_for_endpoint_capacity(
        graph,
        edge_kind,
        CapacityEndpoint::Target(to_id),
        to_capacity,
        skip_edge,
        &mut ops,
    );

    ops
}

#[derive(Clone, Copy)]
enum CapacityEndpoint {
    Source(PortId),
    Target(PortId),
}

fn disconnect_for_endpoint_capacity(
    graph: &Graph,
    edge_kind: EdgeKind,
    endpoint: CapacityEndpoint,
    capacity: PortCapacity,
    skip_edge: Option<EdgeId>,
    ops: &mut Vec<GraphOp>,
) {
    if capacity != PortCapacity::Single {
        return;
    }

    for (edge_id, edge) in graph.edges.iter() {
        if Some(*edge_id) == skip_edge {
            continue;
        }
        if endpoint.matches(edge_kind, edge) {
            ops.push(remove_edge_op(graph, *edge_id));
        }
    }
}

impl CapacityEndpoint {
    fn matches(self, edge_kind: EdgeKind, edge: &Edge) -> bool {
        if edge.kind != edge_kind {
            return false;
        }

        match self {
            Self::Source(port) => edge.from == port,
            Self::Target(port) => edge.to == port,
        }
    }
}
