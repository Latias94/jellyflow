use crate::io::NodeGraphInteractionState;
use crate::rules::{ConnectPlan, EdgeEndpoint};
use jellyflow_core::core::{Edge, EdgeId, Graph, PortDirection, PortId};
use jellyflow_core::interaction::NodeGraphConnectionMode;
use jellyflow_core::ops::EdgeEndpoints;

use super::super::common::{
    ConnectionEndpoints, ConnectionOpBuilder, connection_exists, reject_duplicate_connection,
    reject_edge_kind_incompatible_with_ports, reject_if_connection_policy_disallows,
    reject_missing_edge, reject_reconnect_directions_required, reject_self_connection,
    resolve_ordered_connection_endpoints,
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

    let endpoint_change = ReconnectEndpointChange::new(edge, endpoint, new_port);

    if let Some(reject) = endpoint_change.self_connection_rejection() {
        return reject;
    }

    if endpoint_change.is_noop() {
        return ConnectPlan::accept();
    }

    let endpoints = match endpoint_change.resolve(graph) {
        Ok(endpoints) => endpoints,
        Err(reject) => return reject,
    };

    if let Some(reject) = strict_mode_rejection(mode, &endpoints) {
        return reject;
    }

    if let Some(reject) = reject_if_connection_policy_disallows(graph, &endpoints, state) {
        return reject;
    }

    if edge.kind != endpoints.edge_kind {
        return reject_edge_kind_incompatible_with_ports(edge.kind, endpoints.edge_kind);
    }

    if connection_exists(
        graph,
        edge.kind,
        endpoints.from_id,
        endpoints.to_id,
        Some(edge_id),
    ) {
        return reject_duplicate_connection();
    }

    let mut ops =
        ConnectionOpBuilder::with_endpoint_capacity_disconnects(graph, &endpoints, Some(edge_id));

    ops.push_set_edge_endpoints(
        edge_id,
        endpoint_change.old,
        EdgeEndpoints::new(endpoints.from_id, endpoints.to_id),
    );

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

struct ReconnectEndpointChange {
    old: EdgeEndpoints,
    candidate: EdgeEndpoints,
}

impl ReconnectEndpointChange {
    fn new(edge: &Edge, endpoint: EdgeEndpoint, new_port: PortId) -> Self {
        Self {
            old: EdgeEndpoints::from_edge(edge),
            candidate: match endpoint {
                EdgeEndpoint::From => EdgeEndpoints::new(new_port, edge.to),
                EdgeEndpoint::To => EdgeEndpoints::new(edge.from, new_port),
            },
        }
    }

    fn self_connection_rejection(&self) -> Option<ConnectPlan> {
        (self.candidate.from == self.candidate.to).then(reject_self_connection)
    }

    fn is_noop(&self) -> bool {
        self.candidate == self.old
    }

    fn resolve<'a>(&self, graph: &'a Graph) -> Result<ConnectionEndpoints<'a>, ConnectPlan> {
        resolve_ordered_connection_endpoints(graph, self.candidate.from, self.candidate.to)
    }
}

fn strict_mode_rejection(
    mode: NodeGraphConnectionMode,
    endpoints: &ConnectionEndpoints<'_>,
) -> Option<ConnectPlan> {
    (mode == NodeGraphConnectionMode::Strict
        && (endpoints.from.dir != PortDirection::Out || endpoints.to.dir != PortDirection::In))
        .then(reject_reconnect_directions_required)
}
