mod diagnostics;
mod planner;
mod selection;
mod validation;

use crate::io::NodeGraphInteractionState;
use jellyflow_core::core::{EdgeId, Graph, NodeId};

use super::DeletePlan;
use planner::DeletePlanner;

/// Plans deleting a node with default interaction policy.
pub fn plan_delete_node(graph: &Graph, node: NodeId) -> DeletePlan {
    plan_delete_node_with_policy(graph, node, &NodeGraphInteractionState::default())
}

/// Plans deleting a node with explicit interaction policy.
pub fn plan_delete_node_with_policy(
    graph: &Graph,
    node: NodeId,
    state: &NodeGraphInteractionState,
) -> DeletePlan {
    plan_delete_elements_with_policy(graph, [node], std::iter::empty::<EdgeId>(), state)
}

/// Plans deleting an edge with default interaction policy.
pub fn plan_delete_edge(graph: &Graph, edge: EdgeId) -> DeletePlan {
    plan_delete_edge_with_policy(graph, edge, &NodeGraphInteractionState::default())
}

/// Plans deleting an edge with explicit interaction policy.
pub fn plan_delete_edge_with_policy(
    graph: &Graph,
    edge: EdgeId,
    state: &NodeGraphInteractionState,
) -> DeletePlan {
    plan_delete_elements_with_policy(graph, std::iter::empty::<NodeId>(), [edge], state)
}

/// Plans deleting nodes and edges with default interaction policy.
pub fn plan_delete_elements(
    graph: &Graph,
    nodes: impl IntoIterator<Item = NodeId>,
    edges: impl IntoIterator<Item = EdgeId>,
) -> DeletePlan {
    plan_delete_elements_with_policy(graph, nodes, edges, &NodeGraphInteractionState::default())
}

/// Plans deleting nodes and edges with explicit interaction policy.
pub fn plan_delete_elements_with_policy(
    graph: &Graph,
    nodes: impl IntoIterator<Item = NodeId>,
    edges: impl IntoIterator<Item = EdgeId>,
    state: &NodeGraphInteractionState,
) -> DeletePlan {
    DeletePlanner::new(graph, state).plan(nodes, edges)
}
