use jellyflow_core::core::{CanvasSize, NodeId};
use jellyflow_core::ops::GraphTransaction;

/// Default transaction label used for committed node resize updates.
pub const NODE_RESIZE_TRANSACTION_LABEL: &str = "node resize";

/// Optional canvas-space size bounds for a node resize request.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct NodeResizeConstraints {
    pub min: Option<CanvasSize>,
    pub max: Option<CanvasSize>,
}

impl NodeResizeConstraints {
    pub fn new(min: Option<CanvasSize>, max: Option<CanvasSize>) -> Self {
        Self { min, max }
    }

    pub fn unconstrained() -> Self {
        Self::default()
    }

    pub(super) fn clamp(self, target: CanvasSize) -> Option<CanvasSize> {
        if !target.is_positive_finite() {
            return None;
        }
        let min = valid_constraint(self.min)?;
        let max = valid_constraint(self.max)?;
        if let (Some(min), Some(max)) = (min, max)
            && (min.width > max.width || min.height > max.height)
        {
            return None;
        }

        Some(CanvasSize {
            width: clamp_axis(
                target.width,
                min.map(|size| size.width),
                max.map(|size| size.width),
            ),
            height: clamp_axis(
                target.height,
                min.map(|size| size.height),
                max.map(|size| size.height),
            ),
        })
    }
}

/// Canvas-space request for resizing one node to an explicit size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeResizeRequest {
    /// Node being resized.
    pub node: NodeId,
    /// Requested explicit node size in canvas space.
    pub to: CanvasSize,
    /// Optional min/max bounds applied before planning.
    pub constraints: NodeResizeConstraints,
}

impl NodeResizeRequest {
    pub fn new(node: NodeId, to: CanvasSize) -> Self {
        Self {
            node,
            to,
            constraints: NodeResizeConstraints::default(),
        }
    }

    pub fn with_constraints(mut self, constraints: NodeResizeConstraints) -> Self {
        self.constraints = constraints;
        self
    }
}

/// One node size edit inside a resize plan.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeResizeItem {
    /// Node being resized.
    pub node: NodeId,
    /// Current explicit node size.
    pub from: Option<CanvasSize>,
    /// Planned explicit node size.
    pub to: CanvasSize,
}

/// Planned node resize transaction.
#[derive(Debug, Clone)]
pub struct NodeResizePlan {
    /// Node being resized.
    pub node: NodeId,
    /// Current explicit node size.
    pub from: Option<CanvasSize>,
    /// Planned explicit node size.
    pub to: CanvasSize,
    items: Vec<NodeResizeItem>,
    transaction: GraphTransaction,
}

impl NodeResizePlan {
    pub(super) fn new(
        node: NodeId,
        from: Option<CanvasSize>,
        to: CanvasSize,
        transaction: GraphTransaction,
    ) -> Self {
        Self {
            node,
            from,
            to,
            items: vec![NodeResizeItem { node, from, to }],
            transaction,
        }
    }

    /// Returns ordered resize items.
    pub fn items(&self) -> &[NodeResizeItem] {
        &self.items
    }

    /// Returns the transaction that applies this resize update.
    pub fn transaction(&self) -> &GraphTransaction {
        &self.transaction
    }

    /// Consumes the plan and returns its transaction.
    pub fn into_transaction(self) -> GraphTransaction {
        self.transaction
    }
}

fn valid_constraint(size: Option<CanvasSize>) -> Option<Option<CanvasSize>> {
    match size {
        Some(size) if size.is_positive_finite() => Some(Some(size)),
        Some(_) => None,
        None => Some(None),
    }
}

fn clamp_axis(value: f32, min: Option<f32>, max: Option<f32>) -> f32 {
    let mut value = value;
    if let Some(min) = min {
        value = value.max(min);
    }
    if let Some(max) = max {
        value = value.min(max);
    }
    value
}
