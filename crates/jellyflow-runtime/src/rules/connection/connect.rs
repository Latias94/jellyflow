use crate::rules::ConnectPlan;
use crate::runtime::policy::GraphRuntimePolicy;
use jellyflow_core::core::{Edge, EdgeId, Graph, PortId};
use jellyflow_core::interaction::NodeGraphConnectionMode;

use super::common::{
    ConnectionEndpoints, ConnectionPlanningContext, add_existing_ports_edge_op,
    connection_exists_using, disconnect_for_endpoint_capacity_using,
    resolve_policy_checked_connection,
};

pub(in crate::rules::connection) struct ResolvedConnectPlan<'a> {
    endpoints: ConnectionEndpoints<'a>,
    plan: ConnectPlan,
}

impl<'a> ResolvedConnectPlan<'a> {
    pub(in crate::rules::connection) fn endpoints(&self) -> &ConnectionEndpoints<'a> {
        &self.endpoints
    }

    pub(in crate::rules::connection) fn into_plan(self) -> ConnectPlan {
        self.plan
    }
}

pub(in crate::rules::connection) fn plan_resolved_connect_using<'a>(
    graph: &'a Graph,
    a: PortId,
    b: PortId,
    mode: NodeGraphConnectionMode,
    policy: &GraphRuntimePolicy,
    edge_id: EdgeId,
    query: &mut dyn ConnectionPlanningContext,
) -> Result<ResolvedConnectPlan<'a>, ConnectPlan> {
    let endpoints = match resolve_policy_checked_connection(graph, a, b, mode, policy) {
        Ok(endpoints) => endpoints,
        Err(plan) => return Err(plan),
    };

    if connection_exists_using(
        query,
        graph,
        endpoints.edge_kind,
        endpoints.from_id,
        endpoints.to_id,
        None,
    ) {
        return Ok(ResolvedConnectPlan {
            endpoints,
            plan: ConnectPlan::accept(),
        });
    }

    let mut ops = disconnect_for_endpoint_capacity_using(query, graph, &endpoints, None, policy)?;

    let add_edge = match add_existing_ports_edge_op(
        graph,
        edge_id,
        Edge::new(endpoints.edge_kind, endpoints.from_id, endpoints.to_id),
    ) {
        Ok(op) => op,
        Err(plan) => return Err(plan),
    };
    ops.push(add_edge);

    Ok(ResolvedConnectPlan {
        endpoints,
        plan: ConnectPlan::from_ops(ops),
    })
}
