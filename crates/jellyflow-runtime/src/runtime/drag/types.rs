use jellyflow_core::core::{CanvasPoint, NodeId};
use jellyflow_core::ops::GraphTransaction;

/// Default transaction label used for committed node drag updates.
pub const NODE_DRAG_TRANSACTION_LABEL: &str = "node drag";

/// Default transaction label used for committed keyboard nudge updates.
pub const NODE_NUDGE_TRANSACTION_LABEL: &str = "node nudge";

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

/// Keyboard nudge direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeNudgeDirection {
    Up,
    Down,
    Left,
    Right,
}

impl NodeNudgeDirection {
    pub fn unit_delta(self) -> CanvasPoint {
        match self {
            Self::Up => CanvasPoint { x: 0.0, y: -1.0 },
            Self::Down => CanvasPoint { x: 0.0, y: 1.0 },
            Self::Left => CanvasPoint { x: -1.0, y: 0.0 },
            Self::Right => CanvasPoint { x: 1.0, y: 0.0 },
        }
    }
}

/// Request for nudging the current selected nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeNudgeRequest {
    pub direction: NodeNudgeDirection,
    pub fast: bool,
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

/// Planned keyboard nudge transaction.
#[derive(Debug, Clone)]
pub struct NodeNudgePlan {
    /// Direction requested by the adapter.
    pub direction: NodeNudgeDirection,
    /// Final canvas-space delta after step resolution and snapping.
    pub delta: CanvasPoint,
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

impl NodeNudgePlan {
    pub(super) fn new(
        direction: NodeNudgeDirection,
        delta: CanvasPoint,
        items: Vec<NodeDragItem>,
        transaction: GraphTransaction,
    ) -> Self {
        Self {
            direction,
            delta,
            items,
            transaction,
        }
    }

    /// Returns ordered nudge items. Items are sorted by node id for deterministic transactions.
    pub fn items(&self) -> &[NodeDragItem] {
        &self.items
    }

    /// Returns the transaction that applies this nudge update.
    pub fn transaction(&self) -> &GraphTransaction {
        &self.transaction
    }

    /// Consumes the plan and returns its transaction.
    pub fn into_transaction(self) -> GraphTransaction {
        self.transaction
    }
}
