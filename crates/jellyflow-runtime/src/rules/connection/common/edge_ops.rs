use crate::rules::ConnectPlan;
use jellyflow_core::core::{Edge, EdgeId, EdgeKind, Graph, PortId};
use jellyflow_core::ops::{EdgeEndpoints, GraphMutationError, GraphMutationPlanner, GraphOp};

use super::capacity::disconnect_for_endpoint_capacity;
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
    pub(in crate::rules::connection) fn with_endpoint_capacity_disconnects(
        graph: &Graph,
        endpoints: &ConnectionEndpoints<'_>,
        skip_edge: Option<EdgeId>,
    ) -> Self {
        Self {
            ops: disconnect_for_endpoint_capacity(graph, endpoints, skip_edge),
        }
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
