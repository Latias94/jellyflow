use crate::graph_kernel::GraphOp;
use crate::rules::{ConnectPlan, DiagnosticTarget};
use jellyflow_core::core::{Edge, EdgeId, EdgeKind, Graph, PortId};

use super::ConnectionPlanningContext;
use super::endpoints::connection_ports;
use super::rejections::reject_diagnostic;

pub(in crate::rules::connection) fn add_existing_ports_edge_op(
    graph: &Graph,
    edge_id: EdgeId,
    edge: Edge,
) -> Result<GraphOp, ConnectPlan> {
    if graph.edge(&edge_id).is_some() {
        return Err(reject_diagnostic(
            "connect.edge_already_exists",
            DiagnosticTarget::Edge { id: edge_id },
            format!("edge already exists: {edge_id:?}"),
        ));
    }
    connection_ports(graph, edge.from, edge.to)?;
    Ok(GraphOp::AddEdge { id: edge_id, edge })
}

pub(in crate::rules::connection) fn connection_exists_using(
    query: &mut dyn ConnectionPlanningContext,
    graph: &Graph,
    edge_kind: EdgeKind,
    from: PortId,
    to: PortId,
    skip_edge: Option<EdgeId>,
) -> bool {
    query
        .incident_edge_ids(graph, from)
        .into_iter()
        .any(|edge_id| {
            let Some(edge) = graph.edge(&edge_id) else {
                return false;
            };
            Some(edge_id) != skip_edge
                && edge.kind == edge_kind
                && edge.from == from
                && edge.to == to
        })
}
