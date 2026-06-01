use crate::io::{NodeGraphInteractionState, NodeGraphViewState};
use jellyflow_core::core::{CanvasPoint, Graph};
use jellyflow_core::ops::{GraphOp, GraphTransaction};

use super::candidates::drag_candidates;
use super::constraints::{drag_items, snapped_delta};
use super::types::{NODE_DRAG_TRANSACTION_LABEL, NodeDragPlan, NodeDragRequest};

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

    let primary = graph.nodes.get(&request.node)?;
    let delta = CanvasPoint {
        x: request.to.x - primary.pos.x,
        y: request.to.y - primary.pos.y,
    };
    if !delta.is_finite() || delta == CanvasPoint::default() {
        return None;
    }

    let candidates = drag_candidates(graph, view_state, interaction, request.node);
    if !candidates.iter().any(|item| item.node == request.node) {
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
    let primary_to = items
        .iter()
        .find(|item| item.node == request.node)
        .map(|item| item.to)?;
    let transaction = GraphTransaction::from_ops(items.iter().map(|item| GraphOp::SetNodePos {
        id: item.node,
        from: item.from,
        to: item.to,
    }))
    .with_label(NODE_DRAG_TRANSACTION_LABEL);

    Some(NodeDragPlan::new(
        request.node,
        primary.pos,
        primary_to,
        items,
        transaction,
    ))
}
