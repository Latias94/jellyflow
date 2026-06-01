use crate::io::NodeGraphInteractionState;
use crate::rules::ConnectPlan;
use crate::runtime::policy::NodeGraphPortInteractionPolicy;
use jellyflow_core::core::{Graph, PortId};
use jellyflow_core::interaction::NodeGraphConnectionMode;

use super::endpoints::{ConnectionEndpoints, resolve_connection_endpoints};
use super::rejections::{reject_missing_port, reject_missing_port_owner_node};

pub(in crate::rules::connection) fn resolve_policy_checked_connection<'a>(
    graph: &'a Graph,
    a: PortId,
    b: PortId,
    mode: NodeGraphConnectionMode,
    state: &NodeGraphInteractionState,
) -> Result<ConnectionEndpoints<'a>, ConnectPlan> {
    let endpoints = resolve_connection_endpoints(graph, a, b, mode)?;

    if let Some(reject) =
        reject_if_connection_policy_disallows(graph, endpoints.from_id, endpoints.to_id, state)
    {
        return Err(reject);
    }

    Ok(endpoints)
}

pub(in crate::rules::connection) fn reject_if_connection_policy_disallows(
    graph: &Graph,
    from_id: PortId,
    to_id: PortId,
    state: &NodeGraphInteractionState,
) -> Option<ConnectPlan> {
    let from_policy = match port_policy_or_reject(graph, from_id, state) {
        Ok(policy) => policy,
        Err(plan) => return Some(plan),
    };
    if !from_policy.can_start_connection() {
        return Some(ConnectPlan::reject("source port is not connectable"));
    }

    let to_policy = match port_policy_or_reject(graph, to_id, state) {
        Ok(policy) => policy,
        Err(plan) => return Some(plan),
    };
    if !to_policy.can_accept_connection() {
        return Some(ConnectPlan::reject("target port is not connectable"));
    }

    None
}

fn port_policy_or_reject(
    graph: &Graph,
    port_id: PortId,
    state: &NodeGraphInteractionState,
) -> Result<NodeGraphPortInteractionPolicy, ConnectPlan> {
    let Some(port) = graph.ports.get(&port_id) else {
        return Err(reject_missing_port(port_id));
    };
    let Some(node) = graph.nodes.get(&port.node) else {
        return Err(reject_missing_port_owner_node(port.node));
    };
    Ok(state.port_interaction_policy(node, port))
}
