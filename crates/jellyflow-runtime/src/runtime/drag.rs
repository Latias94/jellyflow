//! Renderer-neutral node dragging helpers.
//!
//! These helpers turn canvas-space drag intent into normal graph transactions without depending on
//! pointer capture, DOM state, windowing, or renderer APIs.

use crate::io::{NodeGraphInteractionState, NodeGraphViewState};
use crate::runtime::policy::resolve_node_interaction_policy;
use crate::runtime::store::{DispatchError, DispatchOutcome, NodeGraphStore};
use jellyflow_core::core::{CanvasPoint, Graph, NodeId};
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
    if !node_is_draggable(graph, view_state, interaction, request.node) {
        return None;
    }

    let delta = CanvasPoint {
        x: request.to.x - primary.pos.x,
        y: request.to.y - primary.pos.y,
    };
    if !delta.is_finite() || delta == CanvasPoint::default() {
        return None;
    }

    let items = drag_items(graph, view_state, interaction, request.node, delta);
    if !items.iter().any(|item| item.node == request.node) {
        return None;
    }
    let transaction = GraphTransaction::from_ops(items.iter().map(|item| GraphOp::SetNodePos {
        id: item.node,
        from: item.from,
        to: item.to,
    }))
    .with_label(NODE_DRAG_TRANSACTION_LABEL);

    Some(NodeDragPlan {
        node: request.node,
        from: primary.pos,
        to: request.to,
        items,
        transaction,
    })
}

fn drag_items(
    graph: &Graph,
    view_state: &NodeGraphViewState,
    interaction: &NodeGraphInteractionState,
    primary: NodeId,
    delta: CanvasPoint,
) -> Vec<NodeDragItem> {
    let mut nodes = view_state.selected_nodes.clone();
    nodes.push(primary);
    nodes.sort();
    nodes.dedup();

    nodes
        .into_iter()
        .filter(|node| node_is_draggable(graph, view_state, interaction, *node))
        .filter_map(|node| {
            let from = graph.nodes.get(&node)?.pos;
            let to = CanvasPoint {
                x: from.x + delta.x,
                y: from.y + delta.y,
            };
            to.is_finite().then_some(NodeDragItem { node, from, to })
        })
        .collect()
}

fn node_is_draggable(
    graph: &Graph,
    view_state: &NodeGraphViewState,
    interaction: &NodeGraphInteractionState,
    node: NodeId,
) -> bool {
    let Some(node) = graph.nodes.get(&node) else {
        return false;
    };
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
