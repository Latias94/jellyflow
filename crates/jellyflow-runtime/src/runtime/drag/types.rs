use jellyflow_core::core::{CanvasPoint, NodeId};
use jellyflow_core::ops::GraphTransaction;

/// Default transaction label used for committed node drag updates.
pub const NODE_DRAG_TRANSACTION_LABEL: &str = "node drag";

/// Canvas-space request for moving a primary node to a target position.
///
/// When used through [`crate::runtime::store::NodeGraphStore`], currently selected nodes are
/// co-dragged with the primary node when policy allows them to move.
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
    pub(super) fn new(
        node: NodeId,
        from: CanvasPoint,
        to: CanvasPoint,
        items: Vec<NodeDragItem>,
        transaction: GraphTransaction,
    ) -> Self {
        Self {
            node,
            from,
            to,
            items,
            transaction,
        }
    }

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
