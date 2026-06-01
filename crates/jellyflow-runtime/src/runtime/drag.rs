//! Renderer-neutral node dragging helpers.
//!
//! These helpers turn canvas-space drag intent into normal graph transactions without depending on
//! pointer capture, DOM state, windowing, or renderer APIs.

use crate::io::{NodeGraphInteractionState, NodeGraphViewState};
use crate::runtime::geometry::CanvasBounds;
use crate::runtime::policy::resolve_node_interaction_policy;
use crate::runtime::store::{DispatchError, DispatchOutcome, NodeGraphStore};
use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize, Graph, Node, NodeExtent, NodeId};
use jellyflow_core::ops::{GraphOp, GraphTransaction};

/// Default transaction label used for committed node drag updates.
pub const NODE_DRAG_TRANSACTION_LABEL: &str = "node drag";

/// Canvas-space request for moving a primary node to a target position.
///
/// When used through [`NodeGraphStore`], currently selected nodes are co-dragged with the primary
/// node when policy allows them to move.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeDragRequest {
    /// Primary node being moved.
    pub node: NodeId,
    /// Target primary-node position in canvas space.
    pub to: CanvasPoint,
}

/// One node movement inside a drag plan.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeDragItem {
    /// Node being moved.
    pub node: NodeId,
    /// Current node position.
    pub from: CanvasPoint,
    /// Target node position.
    pub to: CanvasPoint,
}

/// Planned node drag transaction.
#[derive(Debug, Clone)]
pub struct NodeDragPlan {
    /// Primary node being moved.
    pub node: NodeId,
    /// Current primary-node position.
    pub from: CanvasPoint,
    /// Target primary-node position.
    pub to: CanvasPoint,
    items: Vec<NodeDragItem>,
    transaction: GraphTransaction,
}

impl NodeDragPlan {
    /// Returns ordered drag items. Items are sorted by node id for deterministic transactions.
    pub fn items(&self) -> &[NodeDragItem] {
        &self.items
    }

    /// Returns the transaction that applies this drag update.
    pub fn transaction(&self) -> &GraphTransaction {
        &self.transaction
    }

    /// Consumes the plan and returns its transaction.
    pub fn into_transaction(self) -> GraphTransaction {
        self.transaction
    }
}

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
    if !node_is_draggable(primary, view_state, interaction) {
        return None;
    }

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

    Some(NodeDragPlan {
        node: request.node,
        from: primary.pos,
        to: primary_to,
        items,
        transaction,
    })
}

#[derive(Debug, Clone, Copy)]
struct DragCandidate {
    node: NodeId,
    from: CanvasPoint,
    size: CanvasSize,
    extent: Option<CanvasRect>,
    node_extent_override: bool,
}

fn drag_candidates(
    graph: &Graph,
    view_state: &NodeGraphViewState,
    interaction: &NodeGraphInteractionState,
    primary: NodeId,
) -> Vec<DragCandidate> {
    let mut nodes = view_state.selected_nodes.clone();
    nodes.push(primary);
    nodes.sort();
    nodes.dedup();

    nodes
        .into_iter()
        .filter_map(|node| {
            let graph_node = graph.nodes.get(&node)?;
            if !node_is_draggable(graph_node, view_state, interaction) {
                return None;
            }
            let policy = resolve_node_interaction_policy(graph_node, interaction);
            Some(DragCandidate {
                node,
                from: graph_node.pos,
                size: normalized_size(graph_node.size),
                extent: resolved_extent_rect(
                    graph,
                    graph_node,
                    policy.extent,
                    policy.expand_parent,
                ),
                node_extent_override: graph_node.extent.is_some(),
            })
        })
        .collect()
}

fn drag_items(
    interaction: &NodeGraphInteractionState,
    candidates: &[DragCandidate],
    delta: CanvasPoint,
) -> Vec<NodeDragItem> {
    let node_drag = interaction.node_drag_interaction();
    let node_origin = node_drag.node_origin.normalized();
    let node_origin = (node_origin.x, node_origin.y);
    let global_extent = node_drag.node_extent.and_then(normalized_rect);
    let group_bounds = (candidates.len() > 1)
        .then(|| candidate_bounds(candidates, node_origin))
        .flatten()
        .map(CanvasBounds::to_rect);

    candidates
        .iter()
        .filter_map(|candidate| {
            let desired = CanvasPoint {
                x: candidate.from.x + delta.x,
                y: candidate.from.y + delta.y,
            };
            let extent =
                adjusted_candidate_extent(*candidate, node_origin, global_extent, group_bounds);
            let to = extent
                .map(|extent| clamp_candidate_position(*candidate, desired, node_origin, extent))
                .unwrap_or(desired);
            to.is_finite().then_some(NodeDragItem {
                node: candidate.node,
                from: candidate.from,
                to,
            })
        })
        .collect()
}

fn snapped_delta(
    interaction: &NodeGraphInteractionState,
    candidates: &[DragCandidate],
    delta: CanvasPoint,
) -> CanvasPoint {
    let node_drag = interaction.node_drag_interaction();
    if !node_drag.snap_to_grid || !node_drag.snap_grid.is_positive_finite() {
        return delta;
    }

    let Some(reference) = candidates.first() else {
        return delta;
    };
    let reference_target = CanvasPoint {
        x: reference.from.x + delta.x,
        y: reference.from.y + delta.y,
    };
    let snapped = snap_point(reference_target, node_drag.snap_grid);

    CanvasPoint {
        x: snapped.x - reference.from.x,
        y: snapped.y - reference.from.y,
    }
}

fn candidate_bounds(candidates: &[DragCandidate], node_origin: (f32, f32)) -> Option<CanvasBounds> {
    candidates
        .iter()
        .filter_map(|candidate| candidate_bounds_at(*candidate, candidate.from, node_origin))
        .reduce(CanvasBounds::union)
}

fn candidate_bounds_at(
    candidate: DragCandidate,
    position: CanvasPoint,
    node_origin: (f32, f32),
) -> Option<CanvasBounds> {
    let origin = CanvasPoint {
        x: position.x - node_origin.0 * candidate.size.width,
        y: position.y - node_origin.1 * candidate.size.height,
    };
    CanvasBounds::from_rect(CanvasRect {
        origin,
        size: candidate.size,
    })
}

fn adjusted_candidate_extent(
    candidate: DragCandidate,
    node_origin: (f32, f32),
    global_extent: Option<CanvasRect>,
    group_bounds: Option<CanvasRect>,
) -> Option<CanvasRect> {
    if !candidate.node_extent_override
        && let (Some(global_extent), Some(group_bounds), Some(candidate_bounds)) = (
            global_extent,
            group_bounds,
            candidate_bounds_at(candidate, candidate.from, node_origin).map(CanvasBounds::to_rect),
        )
    {
        let group_max_x = group_bounds.origin.x + group_bounds.size.width;
        let group_max_y = group_bounds.origin.y + group_bounds.size.height;
        let candidate_max_x = candidate_bounds.origin.x + candidate_bounds.size.width;
        let candidate_max_y = candidate_bounds.origin.y + candidate_bounds.size.height;
        let extent_max_x = global_extent.origin.x + global_extent.size.width;
        let extent_max_y = global_extent.origin.y + global_extent.size.height;

        let min = CanvasPoint {
            x: candidate_bounds.origin.x - group_bounds.origin.x + global_extent.origin.x,
            y: candidate_bounds.origin.y - group_bounds.origin.y + global_extent.origin.y,
        };
        let max = CanvasPoint {
            x: candidate_max_x - group_max_x + extent_max_x,
            y: candidate_max_y - group_max_y + extent_max_y,
        };

        return normalized_rect(CanvasRect {
            origin: min,
            size: CanvasSize {
                width: max.x - min.x,
                height: max.y - min.y,
            },
        });
    }

    candidate.extent
}

fn clamp_candidate_position(
    candidate: DragCandidate,
    target: CanvasPoint,
    node_origin: (f32, f32),
    extent: CanvasRect,
) -> CanvasPoint {
    let Some(bounds) =
        candidate_bounds_at(candidate, target, node_origin).map(CanvasBounds::to_rect)
    else {
        return target;
    };

    let max_x = extent.origin.x + extent.size.width - bounds.size.width;
    let max_y = extent.origin.y + extent.size.height - bounds.size.height;
    let top_left = CanvasPoint {
        x: clamp(bounds.origin.x, extent.origin.x, max_x),
        y: clamp(bounds.origin.y, extent.origin.y, max_y),
    };
    CanvasPoint {
        x: top_left.x + node_origin.0 * candidate.size.width,
        y: top_left.y + node_origin.1 * candidate.size.height,
    }
}

fn resolved_extent_rect(
    graph: &Graph,
    node: &Node,
    extent: Option<NodeExtent>,
    expand_parent: bool,
) -> Option<CanvasRect> {
    match extent? {
        NodeExtent::Rect { rect } => normalized_rect(rect),
        NodeExtent::Parent if !expand_parent => node
            .parent
            .and_then(|parent| graph.groups.get(&parent))
            .and_then(|group| normalized_rect(group.rect)),
        NodeExtent::Parent => None,
    }
}

fn normalized_rect(rect: CanvasRect) -> Option<CanvasRect> {
    CanvasBounds::from_rect(rect).map(CanvasBounds::to_rect)
}

fn normalized_size(size: Option<CanvasSize>) -> CanvasSize {
    let Some(size) = size else {
        return CanvasSize::default();
    };
    if !size.is_finite() {
        return CanvasSize::default();
    }
    CanvasSize {
        width: size.width.max(0.0),
        height: size.height.max(0.0),
    }
}

fn snap_point(point: CanvasPoint, grid: jellyflow_core::core::CanvasSize) -> CanvasPoint {
    CanvasPoint {
        x: grid.width * js_round(point.x / grid.width),
        y: grid.height * js_round(point.y / grid.height),
    }
}

fn js_round(value: f32) -> f32 {
    (value + 0.5).floor()
}

fn clamp(value: f32, min: f32, max: f32) -> f32 {
    value.max(min).min(max)
}

fn node_is_draggable(
    node: &Node,
    view_state: &NodeGraphViewState,
    interaction: &NodeGraphInteractionState,
) -> bool {
    if node.hidden || !node.pos.is_finite() {
        return false;
    }
    if node
        .parent
        .is_some_and(|parent| view_state.selected_groups.contains(&parent))
    {
        return false;
    }
    resolve_node_interaction_policy(node, interaction).draggable
}

impl NodeGraphStore {
    /// Plans a node drag update against the store's current selection and interaction state.
    pub fn plan_node_drag(&self, request: NodeDragRequest) -> Option<NodeDragPlan> {
        let interaction = self.resolved_interaction_state();
        plan_node_drag(self.graph(), self.view_state(), &interaction, request)
    }

    /// Commits a node drag update through the normal store dispatch path.
    ///
    /// This records normal graph history for the committed update. Higher-level drag sessions that
    /// need preview/final-commit semantics should build on top of the planning API.
    pub fn apply_node_drag(
        &mut self,
        request: NodeDragRequest,
    ) -> Result<Option<DispatchOutcome>, DispatchError> {
        let Some(plan) = self.plan_node_drag(request) else {
            return Ok(None);
        };
        self.dispatch_transaction(plan.transaction()).map(Some)
    }
}
