use crate::graph_kernel::{EdgeEndpoints, GraphOp};
use crate::rules::{ConnectPlan, EdgeEndpoint};
use crate::runtime::policy::{GraphRuntimePolicy, resolve_edge_policy};
use jellyflow_core::core::{Edge, EdgeId, Graph, PortId};
use jellyflow_core::interaction::NodeGraphConnectionMode;

use super::super::common::{
    ConnectionEndpoints, ConnectionPlanningContext, connection_exists_using,
    disconnect_for_endpoint_capacity_using, reject_duplicate_connection,
    reject_edge_kind_incompatible_with_ports, reject_edge_not_reconnectable,
    reject_if_connection_policy_disallows, reject_missing_edge,
    reject_reconnect_directions_required, reject_self_connection,
    resolve_ordered_connection_endpoints,
};

pub(super) struct ResolvedReconnectPlan<'a> {
    endpoints: ConnectionEndpoints<'a>,
    plan: ConnectPlan,
}

impl<'a> ResolvedReconnectPlan<'a> {
    pub(super) fn endpoints(&self) -> &ConnectionEndpoints<'a> {
        &self.endpoints
    }

    pub(super) fn into_plan(self) -> ConnectPlan {
        self.plan
    }
}

pub(super) fn plan_resolved_reconnect_using<'a>(
    graph: &'a Graph,
    edge_id: EdgeId,
    endpoint: EdgeEndpoint,
    new_port: PortId,
    mode: NodeGraphConnectionMode,
    policy: &GraphRuntimePolicy,
    query: &mut dyn ConnectionPlanningContext,
) -> Result<ResolvedReconnectPlan<'a>, ConnectPlan> {
    let Some(edge) = graph.edges().get(&edge_id) else {
        return Err(reject_missing_edge(edge_id));
    };

    if let Some(reject) = reconnect_endpoint_policy_rejection(edge_id, edge, endpoint, policy) {
        return Err(reject);
    }

    let endpoint_change = ReconnectEndpointChange::new(edge, endpoint, new_port);

    if let Some(reject) = endpoint_change.self_connection_rejection() {
        return Err(reject);
    }

    let endpoints = endpoint_change.resolve(graph)?;

    if endpoint_change.is_noop() {
        return Ok(ResolvedReconnectPlan {
            endpoints,
            plan: ConnectPlan::accept(),
        });
    }

    if let Some(reject) = strict_mode_rejection(mode, &endpoints) {
        return Err(reject);
    }

    if let Some(reject) = reject_if_connection_policy_disallows(graph, &endpoints, policy) {
        return Err(reject);
    }

    if edge.kind != endpoints.edge_kind {
        return Err(reject_edge_kind_incompatible_with_ports(
            edge.kind,
            endpoints.edge_kind,
            endpoints.to_id,
        ));
    }

    if connection_exists_using(
        query,
        graph,
        edge.kind,
        endpoints.from_id,
        endpoints.to_id,
        Some(edge_id),
    ) {
        return Err(reject_duplicate_connection(endpoints.to_id));
    }

    let mut ops =
        disconnect_for_endpoint_capacity_using(query, graph, &endpoints, Some(edge_id), policy)?;

    ops.push(GraphOp::SetEdgeEndpoints {
        id: edge_id,
        from: endpoint_change.old,
        to: endpoints.edge_endpoints(),
    });

    Ok(ResolvedReconnectPlan {
        endpoints,
        plan: ConnectPlan::from_ops(ops),
    })
}

fn reconnect_endpoint_policy_rejection(
    edge_id: EdgeId,
    edge: &Edge,
    endpoint: EdgeEndpoint,
    policy: &GraphRuntimePolicy,
) -> Option<ConnectPlan> {
    let edge_policy = resolve_edge_policy(edge, policy);
    match endpoint {
        EdgeEndpoint::From if !edge_policy.can_reconnect_source() => {
            Some(reject_edge_not_reconnectable(edge_id, true))
        }
        EdgeEndpoint::To if !edge_policy.can_reconnect_target() => {
            Some(reject_edge_not_reconnectable(edge_id, false))
        }
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
        (self.candidate.from == self.candidate.to)
            .then(|| reject_self_connection(self.candidate.from))
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
    (mode == NodeGraphConnectionMode::Strict && !endpoints.is_out_to_in())
        .then(|| reject_reconnect_directions_required(endpoints.to_id))
}
