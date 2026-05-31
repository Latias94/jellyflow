use crate::io::NodeGraphInteractionState;
use crate::rules::{ConnectDecision, ConnectPlan, EdgeEndpoint};
use crate::runtime::policy::resolve_edge_interaction_policy;
use jellyflow_core::core::{EdgeId, Graph, PortDirection, PortId};
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

    let edge_policy = resolve_edge_interaction_policy(edge, state);
    match endpoint {
        EdgeEndpoint::From if !edge_policy.reconnect_source => {
            return ConnectPlan::reject("edge source endpoint is not reconnectable");
        }
        EdgeEndpoint::To if !edge_policy.reconnect_target => {
            return ConnectPlan::reject("edge target endpoint is not reconnectable");
        }
        _ => {}
    }

    let old = EdgeEndpoints {
        from: edge.from,
        to: edge.to,
    };

    let (candidate_from, candidate_to) = match endpoint {
        EdgeEndpoint::From => (new_port, edge.to),
        EdgeEndpoint::To => (edge.from, new_port),
    };

    if candidate_from == candidate_to {
        return ConnectPlan::reject("cannot connect a port to itself");
    }

    if candidate_from == old.from && candidate_to == old.to {
        return ConnectPlan::accept();
    }

    let Some(from) = graph.ports.get(&candidate_from) else {
        return ConnectPlan::reject(format!("missing port: {candidate_from:?}"));
    };
    let Some(to) = graph.ports.get(&candidate_to) else {
        return ConnectPlan::reject(format!("missing port: {candidate_to:?}"));
    };

    if mode == NodeGraphConnectionMode::Strict
        && (from.dir != PortDirection::Out || to.dir != PortDirection::In)
    {
        return ConnectPlan::reject("ports must be out -> in for reconnection");
    }

    if let Some(reject) =
        reject_if_connection_policy_disallows(graph, candidate_from, candidate_to, state)
    {
        return reject;
    }

    if from.kind != to.kind {
        return ConnectPlan::reject(format!(
            "port kinds are incompatible: from={:?} to={:?}",
            from.kind, to.kind
        ));
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

    for (other_id, other) in &graph.edges {
        if *other_id == edge_id {
            continue;
        }
        if other.kind == edge.kind && other.from == candidate_from && other.to == candidate_to {
            return ConnectPlan::reject("duplicate connection already exists");
        }
    }

    let mut ops: Vec<GraphOp> = disconnect_for_capacity(
        graph,
        edge.kind,
        candidate_from,
        from.capacity,
        candidate_to,
        to.capacity,
        Some(edge_id),
    );

    ops.push(GraphOp::SetEdgeEndpoints {
        id: edge_id,
        from: old,
        to: EdgeEndpoints {
            from: candidate_from,
            to: candidate_to,
        },
    });

    ConnectPlan {
        decision: ConnectDecision::Accept,
        diagnostics: Vec::new(),
        ops,
    }
}
