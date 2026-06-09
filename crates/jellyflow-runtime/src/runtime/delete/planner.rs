use keyboard_types::Code as KeyCode;

use crate::io::{NodeGraphInteractionState, NodeGraphViewState};
use crate::rules::{DeletePlan, plan_delete_elements_with_policy};
use jellyflow_core::core::Graph;
use jellyflow_core::ops::{GraphOp, GraphTransaction};

use super::types::{
    DELETE_SELECTION_TRANSACTION_LABEL, DeleteElements, DeleteSelectionError, PreDeleteRequest,
};

/// Plans deletion for the current node/edge selection without mutating the graph.
pub fn plan_delete_selection(
    graph: &Graph,
    view_state: &NodeGraphViewState,
    interaction: &NodeGraphInteractionState,
) -> DeletePlan {
    let elements = delete_selection_elements(view_state);
    plan_delete_elements(graph, interaction, &elements)
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

/// Returns the direct node/edge selection before cascaded delete planning.
pub fn delete_selection_elements(view_state: &NodeGraphViewState) -> DeleteElements {
    DeleteElements::new(
        view_state.selected_nodes.iter().copied(),
        view_state.selected_edges.iter().copied(),
    )
}

/// Plans deleting explicit node/edge ids through normal delete policy.
pub fn plan_delete_elements(
    graph: &Graph,
    interaction: &NodeGraphInteractionState,
    elements: &DeleteElements,
) -> DeletePlan {
    plan_delete_elements_with_policy(
        graph,
        elements.nodes().iter().copied(),
        elements.edges().iter().copied(),
        interaction,
    )
}

/// Builds a pre-delete request for adapter-owned `onBeforeDelete` hooks.
pub fn prepare_delete_selection(
    graph: &Graph,
    view_state: &NodeGraphViewState,
    interaction: &NodeGraphInteractionState,
) -> Result<Option<PreDeleteRequest>, DeleteSelectionError> {
    let requested = delete_selection_elements(view_state);
    prepare_delete_elements(graph, interaction, requested)
}

/// Builds a key-gated pre-delete request when the configured delete key matches.
pub fn prepare_delete_selection_for_key(
    graph: &Graph,
    view_state: &NodeGraphViewState,
    interaction: &NodeGraphInteractionState,
    key: KeyCode,
) -> Result<Option<PreDeleteRequest>, DeleteSelectionError> {
    let keyboard = interaction.keyboard_interaction();
    if keyboard.disable_keyboard_a11y || !keyboard.delete_key.matches(key) {
        return Ok(None);
    }

    prepare_delete_selection(graph, view_state, interaction)
}

/// Builds a pre-delete request for explicit node/edge ids.
pub fn prepare_delete_elements(
    graph: &Graph,
    interaction: &NodeGraphInteractionState,
    requested: DeleteElements,
) -> Result<Option<PreDeleteRequest>, DeleteSelectionError> {
    let plan = plan_delete_elements(graph, interaction, &requested);
    if plan.is_reject() {
        return Err(DeleteSelectionError::Rejected {
            diagnostics: plan.diagnostics,
        });
    }

    let planned = delete_elements_from_plan(&plan);
    if planned.is_empty() {
        return Ok(None);
    }

    Ok(Some(PreDeleteRequest::new(requested, planned)))
}

/// Extracts the actual node/edge ids removed by a delete plan, including cascaded edges.
pub fn delete_elements_from_plan(plan: &DeletePlan) -> DeleteElements {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    for op in plan.ops() {
        match op {
            GraphOp::RemoveNode {
                id,
                edges: removed_edges,
                ..
            } => {
                nodes.push(*id);
                edges.extend(removed_edges.iter().map(|(edge_id, _)| *edge_id));
            }
            GraphOp::RemovePort {
                edges: removed_edges,
                ..
            } => {
                edges.extend(removed_edges.iter().map(|(edge_id, _)| *edge_id));
            }
            GraphOp::RemoveEdge { id, .. } => edges.push(*id),
            _ => {}
        }
    }

    DeleteElements::new(nodes, edges)
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
