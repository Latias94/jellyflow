use std::collections::BTreeSet;

use jellyflow_core::core::{Edge, EdgeId, EdgeKind, Graph, PortCapacity, PortId};
use jellyflow_core::ops::{GraphMutationPlanner, GraphOp};

use super::endpoints::ConnectionEndpoints;

pub(super) fn disconnect_for_endpoint_capacity(
    graph: &Graph,
    endpoints: &ConnectionEndpoints<'_>,
    skip_edge: Option<EdgeId>,
) -> Vec<GraphOp> {
    disconnect_for_capacity(
        graph,
        ConnectionCapacity::from_endpoints(endpoints),
        skip_edge,
    )
}

#[derive(Clone, Copy)]
struct ConnectionCapacity {
    edge_kind: EdgeKind,
    from_id: PortId,
    from_capacity: PortCapacity,
    to_id: PortId,
    to_capacity: PortCapacity,
}

impl ConnectionCapacity {
    fn new(
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

    fn from_endpoints(endpoints: &ConnectionEndpoints<'_>) -> Self {
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

fn remove_edge_op(graph: &Graph, edge_id: EdgeId) -> GraphOp {
    GraphMutationPlanner::new(graph)
        .remove_edge_op(edge_id)
        .expect("edge id came from the current graph")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::connection::common::edge_between;

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
