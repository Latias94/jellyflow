use keyboard_types::Code as KeyCode;

use crate::io::{NodeGraphInteractionState, NodeGraphViewState};
use crate::rules::{DeletePlan, plan_delete_elements_with_policy};
use jellyflow_core::core::Graph;
use jellyflow_core::ops::GraphTransaction;

use super::types::DELETE_SELECTION_TRANSACTION_LABEL;

/// Plans deletion for the current node/edge selection without mutating the graph.
pub fn plan_delete_selection(
    graph: &Graph,
    view_state: &NodeGraphViewState,
    interaction: &NodeGraphInteractionState,
) -> DeletePlan {
    plan_delete_elements_with_policy(
        graph,
        view_state.selected_nodes.iter().copied(),
        view_state.selected_edges.iter().copied(),
        interaction,
    )
}

/// Plans deletion for a keyboard event when the configured delete key matches.
pub fn plan_delete_selection_for_key(
    graph: &Graph,
    view_state: &NodeGraphViewState,
    interaction: &NodeGraphInteractionState,
    key: KeyCode,
) -> Option<DeletePlan> {
    let keyboard = interaction.keyboard_interaction();
    if keyboard.disable_keyboard_a11y || !keyboard.delete_key.matches(key) {
        return None;
    }

    Some(plan_delete_selection(graph, view_state, interaction))
}

/// Builds a labeled transaction for an accepted, non-empty delete plan.
pub fn delete_selection_transaction(plan: &DeletePlan) -> Option<GraphTransaction> {
    if !plan.is_accept() || plan.ops().is_empty() {
        return None;
    }

    Some(
        GraphTransaction::from_ops(plan.ops().iter().cloned())
            .with_label(DELETE_SELECTION_TRANSACTION_LABEL),
    )
}

/// Consumes a delete plan and returns a labeled transaction when it has accepted ops.
pub fn delete_selection_transaction_from_plan(plan: DeletePlan) -> Option<GraphTransaction> {
    if !plan.is_accept() || plan.ops().is_empty() {
        return None;
    }

    Some(GraphTransaction::from_ops(plan.into_ops()).with_label(DELETE_SELECTION_TRANSACTION_LABEL))
}
