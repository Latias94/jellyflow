use crate::io::NodeGraphInteractionState;
use crate::rules::{ConnectPlan, EdgeEndpoint};
use jellyflow_core::core::{Edge, EdgeId, Graph, Port, PortDirection, PortId};
use jellyflow_core::interaction::NodeGraphConnectionMode;
use jellyflow_core::ops::EdgeEndpoints;

use super::super::common::{
    ConnectionCapacity, ConnectionOpBuilder, connection_exists, connection_ports,
    reject_duplicate_connection, reject_edge_kind_incompatible_with_ports,
    reject_if_connection_policy_disallows, reject_incompatible_port_kinds, reject_missing_edge,
    reject_reconnect_directions_required, reject_self_connection,
};

/// Plans reconnecting one endpoint of an existing edge to a new port.
///
/// This is used for "yank and reattach" workflows where edge identity should be preserved.
pub fn plan_reconnect_edge_with_mode_and_policy(
    graph: &Graph,
    edge_id: EdgeId,
    endpoint: EdgeEndpoint,
    new_port: PortId,
    mode: NodeGraphConnectionMode,
    state: &NodeGraphInteractionState,
) -> ConnectPlan {
    let Some(edge) = graph.edges.get(&edge_id) else {
        return reject_missing_edge(edge_id);
    };

    if let Some(reject) = reconnect_endpoint_policy_rejection(edge, endpoint, state) {
        return reject;
    }

    let old = EdgeEndpoints::from_edge(edge);
    let candidate = reconnect_candidate(edge, endpoint, new_port);

    if candidate.from == candidate.to {
        return reject_self_connection();
    }

    if candidate == old {
        return ConnectPlan::accept();
    }

    let (from, to) = match connection_ports(graph, candidate.from, candidate.to) {
        Ok(ports) => ports,
        Err(reject) => return reject,
    };

    if let Some(reject) = strict_mode_rejection(mode, from, to) {
        return reject;
    }

    if let Some(reject) =
        reject_if_connection_policy_disallows(graph, candidate.from, candidate.to, state)
    {
        return reject;
    }

    if let Some(reject) = port_kind_rejection(from, to) {
        return reject;
    }

    let expected_edge_kind = from.kind.edge_kind();

    if edge.kind != expected_edge_kind {
        return reject_edge_kind_incompatible_with_ports(edge.kind, expected_edge_kind);
    }

    if connection_exists(
        graph,
        edge.kind,
        candidate.from,
        candidate.to,
        Some(edge_id),
    ) {
        return reject_duplicate_connection();
    }

    let mut ops = ConnectionOpBuilder::with_capacity_disconnects(
        graph,
        ConnectionCapacity::new(
            edge.kind,
            candidate.from,
            from.capacity,
            candidate.to,
            to.capacity,
        ),
        Some(edge_id),
    );

    ops.push_set_edge_endpoints(edge_id, old, candidate);

    ConnectPlan::from_ops(ops.into_ops())
}

fn reconnect_endpoint_policy_rejection(
    edge: &Edge,
    endpoint: EdgeEndpoint,
    state: &NodeGraphInteractionState,
) -> Option<ConnectPlan> {
    let edge_policy = state.edge_interaction_policy(edge);
    match endpoint {
        EdgeEndpoint::From if !edge_policy.can_reconnect_source() => Some(ConnectPlan::reject(
            "edge source endpoint is not reconnectable",
        )),
        EdgeEndpoint::To if !edge_policy.can_reconnect_target() => Some(ConnectPlan::reject(
            "edge target endpoint is not reconnectable",
        )),
        _ => None,
    }
}

fn reconnect_candidate(edge: &Edge, endpoint: EdgeEndpoint, new_port: PortId) -> EdgeEndpoints {
    match endpoint {
        EdgeEndpoint::From => EdgeEndpoints::new(new_port, edge.to),
        EdgeEndpoint::To => EdgeEndpoints::new(edge.from, new_port),
    }
}

fn strict_mode_rejection(
    mode: NodeGraphConnectionMode,
    from: &Port,
    to: &Port,
) -> Option<ConnectPlan> {
    (mode == NodeGraphConnectionMode::Strict
        && (from.dir != PortDirection::Out || to.dir != PortDirection::In))
        .then(reject_reconnect_directions_required)
}

fn port_kind_rejection(from: &Port, to: &Port) -> Option<ConnectPlan> {
    (from.kind != to.kind).then(|| reject_incompatible_port_kinds(from.kind, to.kind))
}
