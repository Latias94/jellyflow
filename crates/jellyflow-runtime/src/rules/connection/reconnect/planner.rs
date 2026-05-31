use crate::io::NodeGraphInteractionState;
use crate::rules::{ConnectPlan, EdgeEndpoint};
use crate::runtime::policy::resolve_edge_interaction_policy;
use jellyflow_core::core::{Edge, EdgeId, Graph, Port, PortDirection, PortId};
use jellyflow_core::interaction::NodeGraphConnectionMode;
use jellyflow_core::ops::{EdgeEndpoints, GraphOp};

use super::super::common::{
    ConnectionCapacity, connection_exists, connection_ports, disconnect_for_capacity,
    edge_kind_for_port_kind, reject_duplicate_connection, reject_edge_kind_incompatible_with_ports,
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

    let old = edge_endpoints(edge);
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

    let expected_edge_kind = edge_kind_for_port_kind(from.kind);

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

    let mut ops: Vec<GraphOp> = disconnect_for_capacity(
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

    ops.push(GraphOp::SetEdgeEndpoints {
        id: edge_id,
        from: old,
        to: candidate,
    });

    ConnectPlan::from_ops(ops)
}

fn reconnect_endpoint_policy_rejection(
    edge: &Edge,
    endpoint: EdgeEndpoint,
    state: &NodeGraphInteractionState,
) -> Option<ConnectPlan> {
    let edge_policy = resolve_edge_interaction_policy(edge, state);
    match endpoint {
        EdgeEndpoint::From if !edge_policy.reconnect_source => Some(ConnectPlan::reject(
            "edge source endpoint is not reconnectable",
        )),
        EdgeEndpoint::To if !edge_policy.reconnect_target => Some(ConnectPlan::reject(
            "edge target endpoint is not reconnectable",
        )),
        _ => None,
    }
}

fn edge_endpoints(edge: &Edge) -> EdgeEndpoints {
    EdgeEndpoints {
        from: edge.from,
        to: edge.to,
    }
}

fn reconnect_candidate(edge: &Edge, endpoint: EdgeEndpoint, new_port: PortId) -> EdgeEndpoints {
    match endpoint {
        EdgeEndpoint::From => EdgeEndpoints {
            from: new_port,
            to: edge.to,
        },
        EdgeEndpoint::To => EdgeEndpoints {
            from: edge.from,
            to: new_port,
        },
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
