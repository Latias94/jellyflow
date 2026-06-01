use crate::rules::ConnectPlan;
use jellyflow_core::core::{Edge, EdgeId, EdgeKind, Graph, PortCapacity, PortId};
use jellyflow_core::ops::{EdgeEndpoints, GraphMutationError, GraphMutationPlanner, GraphOp};
use std::collections::BTreeSet;

use super::endpoints::ConnectionEndpoints;

pub(in crate::rules::connection) fn reject_mutation_error(
    error: GraphMutationError,
) -> ConnectPlan {
    ConnectPlan::reject(error.to_string())
}

pub(in crate::rules::connection) fn ensure_edge_id_available(
    graph: &Graph,
    edge_id: EdgeId,
) -> Result<(), ConnectPlan> {
    if graph.edges.contains_key(&edge_id) {
        return Err(ConnectPlan::reject(format!(
            "edge already exists: {edge_id:?}"
        )));
    }
    Ok(())
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

pub(in crate::rules::connection) struct ConnectionOpBuilder {
    ops: Vec<GraphOp>,
}

impl ConnectionOpBuilder {
    pub(in crate::rules::connection) fn with_capacity_disconnects(
        graph: &Graph,
        connection: ConnectionCapacity,
        skip_edge: Option<EdgeId>,
    ) -> Self {
        Self {
            ops: disconnect_for_capacity(graph, connection, skip_edge),
        }
    }

    pub(in crate::rules::connection) fn with_endpoint_capacity_disconnects(
        graph: &Graph,
        endpoints: &ConnectionEndpoints<'_>,
        skip_edge: Option<EdgeId>,
    ) -> Self {
        Self::with_capacity_disconnects(
            graph,
            ConnectionCapacity::from_endpoints(endpoints),
            skip_edge,
        )
    }

    pub(in crate::rules::connection) fn push(&mut self, op: GraphOp) {
        self.ops.push(op);
    }

    pub(in crate::rules::connection) fn extend(&mut self, ops: impl IntoIterator<Item = GraphOp>) {
        self.ops.extend(ops);
    }

    pub(in crate::rules::connection) fn push_set_edge_endpoints(
        &mut self,
        id: EdgeId,
        from: EdgeEndpoints,
        to: EdgeEndpoints,
    ) {
        self.push(GraphOp::SetEdgeEndpoints { id, from, to });
    }

    pub(in crate::rules::connection) fn into_ops(self) -> Vec<GraphOp> {
        self.ops
    }
}

#[derive(Clone, Copy)]
pub(in crate::rules::connection) struct ConnectionCapacity {
    edge_kind: EdgeKind,
    from_id: PortId,
    from_capacity: PortCapacity,
    to_id: PortId,
    to_capacity: PortCapacity,
}

impl ConnectionCapacity {
    pub(in crate::rules::connection) fn new(
        edge_kind: EdgeKind,
        from_id: PortId,
        from_capacity: PortCapacity,
        to_id: PortId,
        to_capacity: PortCapacity,
    ) -> Self {
        Self {
            edge_kind,
            from_id,
            from_capacity,
            to_id,
            to_capacity,
        }
    }

    pub(in crate::rules::connection) fn from_endpoints(
        endpoints: &ConnectionEndpoints<'_>,
    ) -> Self {
        Self::new(
            endpoints.edge_kind,
            endpoints.from_id,
            endpoints.from.capacity,
            endpoints.to_id,
            endpoints.to.capacity,
        )
    }
}

fn disconnect_for_capacity(
    graph: &Graph,
    connection: ConnectionCapacity,
    skip_edge: Option<EdgeId>,
) -> Vec<GraphOp> {
    let mut plan = CapacityDisconnectPlan::new();
    plan.extend_endpoint(
        graph,
        connection.edge_kind,
        CapacityEndpoint::Source(connection.from_id),
        connection.from_capacity,
        skip_edge,
    );
    plan.extend_endpoint(
        graph,
        connection.edge_kind,
        CapacityEndpoint::Target(connection.to_id),
        connection.to_capacity,
        skip_edge,
    );

    plan.into_ops()
}

#[derive(Clone, Copy)]
enum CapacityEndpoint {
    Source(PortId),
    Target(PortId),
}

struct CapacityDisconnectPlan {
    removed_edges: BTreeSet<EdgeId>,
    ops: Vec<GraphOp>,
}

impl CapacityDisconnectPlan {
    fn new() -> Self {
        Self {
            removed_edges: BTreeSet::new(),
            ops: Vec::new(),
        }
    }

    fn extend_endpoint(
        &mut self,
        graph: &Graph,
        edge_kind: EdgeKind,
        endpoint: CapacityEndpoint,
        capacity: PortCapacity,
        skip_edge: Option<EdgeId>,
    ) {
        if capacity != PortCapacity::Single {
            return;
        }

        for (edge_id, edge) in graph.edges.iter() {
            if Some(*edge_id) == skip_edge {
                continue;
            }
            if endpoint.matches(edge_kind, edge) {
                self.push_remove_edge(graph, *edge_id);
            }
        }
    }

    fn push_remove_edge(&mut self, graph: &Graph, edge_id: EdgeId) {
        if self.removed_edges.insert(edge_id) {
            self.ops.push(remove_edge_op(graph, edge_id));
        }
    }

    fn into_ops(self) -> Vec<GraphOp> {
        self.ops
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capacity_disconnects_deduplicate_edges_matching_both_endpoints() {
        let mut graph = Graph::default();
        let port_id = PortId::from_u128(1);
        let edge_id = EdgeId::from_u128(2);
        graph
            .edges
            .insert(edge_id, edge_between(EdgeKind::Data, port_id, port_id));

        let ops = disconnect_for_capacity(
            &graph,
            ConnectionCapacity::new(
                EdgeKind::Data,
                port_id,
                PortCapacity::Single,
                port_id,
                PortCapacity::Single,
            ),
            None,
        );

        assert_eq!(ops.len(), 1);
        assert!(matches!(ops[0], GraphOp::RemoveEdge { id, .. } if id == edge_id));
    }
}
