use crate::rules::ConnectPlan;
use crate::runtime::policy::{GraphRuntimePolicy, ResolvedPortPolicy, resolve_port_policy};
use jellyflow_core::core::{Graph, Port, PortId};
use jellyflow_core::interaction::NodeGraphConnectionMode;

use super::endpoints::{ConnectionEndpoints, resolve_connection_endpoints};
use super::rejections::{reject_missing_port_owner_node, reject_port_not_connectable};

pub(in crate::rules::connection) fn resolve_policy_checked_connection<'a>(
    graph: &'a Graph,
    a: PortId,
    b: PortId,
    mode: NodeGraphConnectionMode,
    policy: &GraphRuntimePolicy,
) -> Result<ConnectionEndpoints<'a>, ConnectPlan> {
    let endpoints = resolve_connection_endpoints(graph, a, b, mode)?;

    if let Some(reject) = reject_if_connection_policy_disallows(graph, &endpoints, policy) {
        return Err(reject);
    }

    Ok(endpoints)
}

pub(in crate::rules::connection) fn reject_if_connection_policy_disallows(
    graph: &Graph,
    endpoints: &ConnectionEndpoints<'_>,
    policy: &GraphRuntimePolicy,
) -> Option<ConnectPlan> {
    let from_policy = match port_policy_or_reject(graph, endpoints.from, policy) {
        Ok(policy) => policy,
        Err(plan) => return Some(plan),
    };
    if !from_policy.can_start_connection() {
        return Some(reject_port_not_connectable(endpoints.from_id, true));
    }

    let to_policy = match port_policy_or_reject(graph, endpoints.to, policy) {
        Ok(policy) => policy,
        Err(plan) => return Some(plan),
    };
    if !to_policy.can_accept_connection() {
        return Some(reject_port_not_connectable(endpoints.to_id, false));
    }

    None
}

fn port_policy_or_reject(
    graph: &Graph,
    port: &Port,
    policy: &GraphRuntimePolicy,
) -> Result<ResolvedPortPolicy, ConnectPlan> {
    let Some(node) = graph.nodes().get(&port.node) else {
        return Err(reject_missing_port_owner_node(port.node));
    };
    Ok(resolve_port_policy(node, port, policy))
}
