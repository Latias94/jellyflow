use crate::io::NodeGraphNudgeStepMode;
use crate::io::{NodeGraphInteractionState, NodeGraphViewState};
use jellyflow_core::core::{CanvasPoint, Graph, NodeId};
use jellyflow_core::ops::{GraphOp, GraphTransaction};

use super::candidates::drag_candidates;
use super::constraints::{drag_items, snapped_delta};
use super::parent_expansion::parent_expansion_ops;
use super::types::{
    NODE_DRAG_TRANSACTION_LABEL, NODE_NUDGE_TRANSACTION_LABEL, NodeDragItem, NodeDragPlan,
    NodeDragRequest, NodeNudgePlan, NodeNudgeRequest,
};

/// Plans a node drag update without mutating the graph.
pub fn plan_node_drag(
    graph: &Graph,
    view_state: &NodeGraphViewState,
    interaction: &NodeGraphInteractionState,
    request: NodeDragRequest,
) -> Option<NodeDragPlan> {
    if !request.to.is_finite() {
        return None;
    }

    let primary = graph.nodes().get(&request.node)?;
    let delta = CanvasPoint {
        x: request.to.x - primary.pos.x,
        y: request.to.y - primary.pos.y,
    };
    let (_delta, items, transaction) = plan_node_move_delta(
        graph,
        view_state,
        interaction,
        request.node,
        delta,
        NODE_DRAG_TRANSACTION_LABEL,
    )?;
    let primary_to = items
        .iter()
        .find(|item| item.node == request.node)
        .map(|item| item.to)?;

    Some(NodeDragPlan::new(
        request.node,
        primary.pos,
        primary_to,
        items,
        transaction,
    ))
}

/// Plans a keyboard nudge update for the currently selected nodes without mutating the graph.
pub fn plan_node_nudge(
    graph: &Graph,
    view_state: &NodeGraphViewState,
    interaction: &NodeGraphInteractionState,
    request: NodeNudgeRequest,
) -> Option<NodeNudgePlan> {
    if interaction.keyboard_interaction().disable_keyboard_a11y {
        return None;
    }

    let primary = nudge_primary(graph, view_state, interaction)?;
    let delta = nudge_delta(view_state, interaction, request)?;
    let (delta, items, transaction) = plan_node_move_delta(
        graph,
        view_state,
        interaction,
        primary,
        delta,
        NODE_NUDGE_TRANSACTION_LABEL,
    )?;

    Some(NodeNudgePlan::new(
        request.direction,
        delta,
        items,
        transaction,
    ))
}

fn plan_node_move_delta(
    graph: &Graph,
    view_state: &NodeGraphViewState,
    interaction: &NodeGraphInteractionState,
    primary: NodeId,
    delta: CanvasPoint,
    label: &'static str,
) -> Option<(CanvasPoint, Vec<NodeDragItem>, GraphTransaction)> {
    if !delta.is_finite() || delta == CanvasPoint::default() {
        return None;
    }

    let candidates = drag_candidates(graph, view_state, interaction, primary);
    if !candidates.iter().any(|item| item.node == primary) {
        return None;
    }
    let delta = snapped_delta(interaction, &candidates, delta);
    if !delta.is_finite() || delta == CanvasPoint::default() {
        return None;
    }
    let items = drag_items(interaction, &candidates, delta);
    if items.is_empty() || items.iter().all(|item| item.from == item.to) {
        return None;
    }
    let mut ops = items
        .iter()
        .map(|item| GraphOp::SetNodePos {
            id: item.node,
            from: item.from,
            to: item.to,
        })
        .collect::<Vec<_>>();
    ops.extend(parent_expansion_ops(graph, &candidates, &items));
    let transaction = GraphTransaction::from_ops(ops).with_label(label);

    Some((delta, items, transaction))
}

fn nudge_primary(
    graph: &Graph,
    view_state: &NodeGraphViewState,
    interaction: &NodeGraphInteractionState,
) -> Option<NodeId> {
    let mut nodes = view_state.selected_nodes.clone();
    nodes.sort();
    nodes.dedup();
    nodes.into_iter().find(|node| {
        drag_candidates(graph, view_state, interaction, *node)
            .iter()
            .any(|candidate| candidate.node == *node)
    })
}

fn nudge_delta(
    view_state: &NodeGraphViewState,
    interaction: &NodeGraphInteractionState,
    request: NodeNudgeRequest,
) -> Option<CanvasPoint> {
    let direction = request.direction.unit_delta();
    let keyboard = interaction.keyboard_interaction();
    let node_drag = interaction.node_drag_interaction();
    let (step_x, step_y) = match keyboard.nudge_step_mode {
        NodeGraphNudgeStepMode::ScreenPx => {
            let step_px = if request.fast {
                keyboard.nudge_fast_step_px
            } else {
                keyboard.nudge_step_px
            };
            let zoom = if view_state.zoom.is_finite() && view_state.zoom > 0.0 {
                view_state.zoom
            } else {
                1.0
            };
            let step = step_px / zoom;
            (step, step)
        }
        NodeGraphNudgeStepMode::Grid => {
            let grid = node_drag.snap_grid;
            if !grid.is_positive_finite() {
                return None;
            }
            let factor = if request.fast { 4.0 } else { 1.0 };
            (grid.width * factor, grid.height * factor)
        }
    };

    finite_positive_step(step_x, step_y)?;
    Some(CanvasPoint {
        x: direction.x * step_x,
        y: direction.y * step_y,
    })
}

fn finite_positive_step(step_x: f32, step_y: f32) -> Option<()> {
    (step_x.is_finite() && step_x > 0.0 && step_y.is_finite() && step_y > 0.0).then_some(())
}
