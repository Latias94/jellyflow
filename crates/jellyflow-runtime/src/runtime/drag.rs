//! Renderer-neutral node dragging helpers.
//!
//! These helpers turn canvas-space drag intent into normal graph transactions without depending on
//! pointer capture, DOM state, windowing, or renderer APIs.

use crate::io::NodeGraphInteractionState;
use crate::runtime::policy::resolve_node_interaction_policy;
use crate::runtime::store::{DispatchError, DispatchOutcome, NodeGraphStore};
use jellyflow_core::core::{CanvasPoint, Graph, NodeId};
use jellyflow_core::ops::{GraphOp, GraphTransaction};

/// Default transaction label used for committed node drag updates.
pub const NODE_DRAG_TRANSACTION_LABEL: &str = "node drag";

/// Canvas-space request for moving one node to a target position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeDragRequest {
    /// Node being moved.
    pub node: NodeId,
    /// Target node position in canvas space.
    pub to: CanvasPoint,
}

/// Planned node drag transaction.
#[derive(Debug, Clone)]
pub struct NodeDragPlan {
    /// Node being moved.
    pub node: NodeId,
    /// Current node position.
    pub from: CanvasPoint,
    /// Target node position.
    pub to: CanvasPoint,
    transaction: GraphTransaction,
}

impl NodeDragPlan {
    /// Returns the transaction that applies this drag update.
    pub fn transaction(&self) -> &GraphTransaction {
        &self.transaction
    }

    /// Consumes the plan and returns its transaction.
    pub fn into_transaction(self) -> GraphTransaction {
        self.transaction
    }
}

/// Plans a single-node drag update without mutating the graph.
pub fn plan_node_drag(
    graph: &Graph,
    interaction: &NodeGraphInteractionState,
    request: NodeDragRequest,
) -> Option<NodeDragPlan> {
    if !request.to.is_finite() {
        return None;
    }

    let node = graph.nodes.get(&request.node)?;
    if node.hidden || !node.pos.is_finite() {
        return None;
    }

    let policy = resolve_node_interaction_policy(node, interaction);
    if !policy.draggable || node.pos == request.to {
        return None;
    }

    let transaction = GraphTransaction::from_ops([GraphOp::SetNodePos {
        id: request.node,
        from: node.pos,
        to: request.to,
    }])
    .with_label(NODE_DRAG_TRANSACTION_LABEL);

    Some(NodeDragPlan {
        node: request.node,
        from: node.pos,
        to: request.to,
        transaction,
    })
}

impl NodeGraphStore {
    /// Plans a single-node drag update against the store's resolved interaction state.
    pub fn plan_node_drag(&self, request: NodeDragRequest) -> Option<NodeDragPlan> {
        let interaction = self.resolved_interaction_state();
        plan_node_drag(self.graph(), &interaction, request)
    }

    /// Commits a single-node drag update through the normal store dispatch path.
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
