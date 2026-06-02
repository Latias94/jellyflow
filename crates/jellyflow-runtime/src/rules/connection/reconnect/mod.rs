mod planner;

use crate::io::NodeGraphInteractionState;
use crate::rules::{ConnectPlan, EdgeEndpoint};
use jellyflow_core::core::{EdgeId, Graph, PortId};
use jellyflow_core::interaction::NodeGraphConnectionMode;

pub use planner::plan_reconnect_edge_with_mode_and_policy;

/// Plans reconnecting one endpoint of an existing edge with default interaction policy.
pub fn plan_reconnect_edge_with_mode(
    graph: &Graph,
    edge_id: EdgeId,
    endpoint: EdgeEndpoint,
    new_port: PortId,
    mode: NodeGraphConnectionMode,
) -> ConnectPlan {
    plan_reconnect_edge_with_mode_and_policy(
        graph,
        edge_id,
        endpoint,
        new_port,
        mode,
        &NodeGraphInteractionState::default(),
    )
}

/// Plans reconnecting one endpoint of an existing edge (strict mode).
pub fn plan_reconnect_edge(
    graph: &Graph,
    edge_id: EdgeId,
    endpoint: EdgeEndpoint,
    new_port: PortId,
) -> ConnectPlan {
    plan_reconnect_edge_with_mode(
        graph,
        edge_id,
        endpoint,
        new_port,
        NodeGraphConnectionMode::Strict,
    )
}
