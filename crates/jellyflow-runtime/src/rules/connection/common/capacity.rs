use std::collections::BTreeSet;

use crate::graph_kernel::GraphOp;
use jellyflow_core::core::{Edge, EdgeId, EdgeKind, Graph, PortCapacity, PortId};

use crate::rules::validate_delete_edge_policy;
use crate::runtime::policy::GraphRuntimePolicy;

use super::ConnectionPlanningContext;
use super::endpoints::ConnectionEndpoints;
use super::rejections::reject_missing_edge;

pub(in crate::rules::connection) fn disconnect_for_endpoint_capacity_using(
    query: &mut dyn ConnectionPlanningContext,
    graph: &Graph,
    endpoints: &ConnectionEndpoints<'_>,
    skip_edge: Option<EdgeId>,
    policy: &GraphRuntimePolicy,
) -> Result<Vec<GraphOp>, crate::rules::ConnectPlan> {
    disconnect_for_capacity_using(
        query,
        graph,
        ConnectionCapacity::from_endpoints(endpoints),
        skip_edge,
        policy,
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

fn disconnect_for_capacity_using(
    query: &mut dyn ConnectionPlanningContext,
    graph: &Graph,
    connection: ConnectionCapacity,
    skip_edge: Option<EdgeId>,
    policy: &GraphRuntimePolicy,
) -> Result<Vec<GraphOp>, crate::rules::ConnectPlan> {
    let mut plan = CapacityDisconnectPlan::new();
    plan.extend_endpoint(
        query,
        graph,
        connection.edge_kind,
        CapacityEndpoint::Source(connection.from_id),
        connection.from_capacity,
        skip_edge,
        policy,
    )?;
    plan.extend_endpoint(
        query,
        graph,
        connection.edge_kind,
        CapacityEndpoint::Target(connection.to_id),
        connection.to_capacity,
        skip_edge,
        policy,
    )?;

    Ok(plan.into_ops())
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
        query: &mut dyn ConnectionPlanningContext,
        graph: &Graph,
        edge_kind: EdgeKind,
        endpoint: CapacityEndpoint,
        capacity: PortCapacity,
        skip_edge: Option<EdgeId>,
        policy: &GraphRuntimePolicy,
    ) -> Result<(), crate::rules::ConnectPlan> {
        if capacity != PortCapacity::Single {
            return Ok(());
        }

        for edge_id in query.incident_edge_ids(graph, endpoint.port()) {
            if Some(edge_id) == skip_edge {
                continue;
            }
            let Some(edge) = graph.edge(&edge_id) else {
                return Err(reject_missing_edge(edge_id));
            };
            if endpoint.matches(edge_kind, edge) {
                self.push_remove_edge(query, graph, edge_id, policy)?;
            }
        }
        Ok(())
    }

    fn push_remove_edge(
        &mut self,
        query: &mut dyn ConnectionPlanningContext,
        graph: &Graph,
        edge_id: EdgeId,
        policy: &GraphRuntimePolicy,
    ) -> Result<(), crate::rules::ConnectPlan> {
        if self.removed_edges.insert(edge_id) {
            if let Some(diagnostic) = validate_delete_edge_policy(graph, edge_id, policy) {
                return Err(crate::rules::ConnectPlan::reject_with_diagnostic(
                    diagnostic,
                ));
            }
            self.ops.push(query.remove_edge_op(graph, edge_id)?);
        }
        Ok(())
    }

    fn into_ops(self) -> Vec<GraphOp> {
        self.ops
    }
}

impl CapacityEndpoint {
    fn port(self) -> PortId {
        match self {
            Self::Source(port) | Self::Target(port) => port,
        }
    }

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
