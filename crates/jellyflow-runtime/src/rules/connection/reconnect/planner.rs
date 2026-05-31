use crate::io::NodeGraphInteractionState;
use crate::rules::{ConnectDecision, ConnectPlan, EdgeEndpoint};
use crate::runtime::policy::resolve_edge_interaction_policy;
use jellyflow_core::core::{Edge, EdgeId, Graph, Port, PortDirection, PortId};
use jellyflow_core::interaction::NodeGraphConnectionMode;
use jellyflow_core::ops::{EdgeEndpoints, GraphOp};

use super::super::common::{
    disconnect_for_capacity, edge_kind_for_port_kind, reject_if_connection_policy_disallows,
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
        return ConnectPlan::reject(format!("missing edge: {edge_id:?}"));
    };

    if let Some(reject) = reconnect_endpoint_policy_rejection(edge, endpoint, state) {
        return reject;
    }

    let old = edge_endpoints(edge);
    let candidate = reconnect_candidate(edge, endpoint, new_port);

    if candidate.from == candidate.to {
        return ConnectPlan::reject("cannot connect a port to itself");
    }

    if candidate == old {
        return ConnectPlan::accept();
    }

    let (from, to) = match candidate_ports(graph, candidate) {
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

    let Some(expected_edge_kind) = edge_kind_for_port_kind(from.kind) else {
        return ConnectPlan::reject("port kinds are incompatible");
    };

    if edge.kind != expected_edge_kind {
        return ConnectPlan::reject(format!(
            "edge kind is incompatible with ports: edge={:?} expected={:?}",
            edge.kind, expected_edge_kind
        ));
    }

    if duplicate_connection_exists(graph, edge_id, edge, candidate) {
        return ConnectPlan::reject("duplicate connection already exists");
    }

    let mut ops: Vec<GraphOp> = disconnect_for_capacity(
        graph,
        edge.kind,
        candidate.from,
        from.capacity,
        candidate.to,
        to.capacity,
        Some(edge_id),
    );

    ops.push(GraphOp::SetEdgeEndpoints {
        id: edge_id,
        from: old,
        to: candidate,
    });

    ConnectPlan {
        decision: ConnectDecision::Accept,
        diagnostics: Vec::new(),
        ops,
    }
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

fn candidate_ports(graph: &Graph, candidate: EdgeEndpoints) -> Result<(&Port, &Port), ConnectPlan> {
    let Some(from) = graph.ports.get(&candidate.from) else {
        return Err(ConnectPlan::reject(format!(
            "missing port: {:?}",
            candidate.from
        )));
    };
    let Some(to) = graph.ports.get(&candidate.to) else {
        return Err(ConnectPlan::reject(format!(
            "missing port: {:?}",
            candidate.to
        )));
    };
    Ok((from, to))
}

fn strict_mode_rejection(
    mode: NodeGraphConnectionMode,
    from: &Port,
    to: &Port,
) -> Option<ConnectPlan> {
    (mode == NodeGraphConnectionMode::Strict
        && (from.dir != PortDirection::Out || to.dir != PortDirection::In))
        .then(|| ConnectPlan::reject("ports must be out -> in for reconnection"))
}

fn port_kind_rejection(from: &Port, to: &Port) -> Option<ConnectPlan> {
    (from.kind != to.kind).then(|| {
        ConnectPlan::reject(format!(
            "port kinds are incompatible: from={:?} to={:?}",
            from.kind, to.kind
        ))
    })
}

fn duplicate_connection_exists(
    graph: &Graph,
    edge_id: EdgeId,
    edge: &Edge,
    candidate: EdgeEndpoints,
) -> bool {
    graph.edges.iter().any(|(other_id, other)| {
        *other_id != edge_id
            && other.kind == edge.kind
            && other.from == candidate.from
            && other.to == candidate.to
    })
}
