use crate::io::NodeGraphInteractionState;
use crate::rules::ConnectPlan;
use jellyflow_core::core::{EdgeId, Graph, PortId};
use jellyflow_core::interaction::NodeGraphConnectionMode;
use jellyflow_core::ops::GraphOp;

use super::common::{
    ConnectionCapacity, add_existing_ports_edge_op, connection_exists, disconnect_for_capacity,
    edge_between, reject_if_connection_policy_disallows, resolve_connection_endpoints,
};

/// Plans connecting two ports.
///
/// This is a rules-driven decision point used by the UI interaction loop.
/// The returned ops are intended to be applied as part of a single transaction.
pub fn plan_connect_with_mode_and_policy(
    graph: &Graph,
    a: PortId,
    b: PortId,
    mode: NodeGraphConnectionMode,
    state: &NodeGraphInteractionState,
) -> ConnectPlan {
    let endpoints = match resolve_connection_endpoints(graph, a, b, mode) {
        Ok(endpoints) => endpoints,
        Err(plan) => return plan,
    };

    if let Some(reject) =
        reject_if_connection_policy_disallows(graph, endpoints.from_id, endpoints.to_id, state)
    {
        return reject;
    }

    if connection_exists(
        graph,
        endpoints.edge_kind,
        endpoints.from_id,
        endpoints.to_id,
        None,
    ) {
        return ConnectPlan::accept();
    }

    let mut ops: Vec<GraphOp> =
        disconnect_for_capacity(graph, ConnectionCapacity::from_endpoints(&endpoints), None);

    let add_edge = match add_existing_ports_edge_op(
        graph,
        EdgeId::new(),
        edge_between(endpoints.edge_kind, endpoints.from_id, endpoints.to_id),
    ) {
        Ok(op) => op,
        Err(plan) => return plan,
    };
    ops.push(add_edge);

    ConnectPlan::from_ops(ops)
}

/// Plans connecting two ports with default interaction policy.
pub fn plan_connect_with_mode(
    graph: &Graph,
    a: PortId,
    b: PortId,
    mode: NodeGraphConnectionMode,
) -> ConnectPlan {
    plan_connect_with_mode_and_policy(graph, a, b, mode, &NodeGraphInteractionState::default())
}

/// Plans connecting two ports (strict mode).
pub fn plan_connect(graph: &Graph, a: PortId, b: PortId) -> ConnectPlan {
    plan_connect_with_mode(graph, a, b, NodeGraphConnectionMode::Strict)
}
