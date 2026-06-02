use crate::node_origin::normalize_node_origin;
use jellyflow_core::core::{CanvasPoint, CanvasSize, NodeId};
use jellyflow_core::ops::GraphTransaction;
use serde::{Deserialize, Serialize};

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

/// Runtime context for interpreting node resize geometry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeResizeContext {
    /// Fallback node origin used when a node has no per-node origin override.
    pub node_origin: (f32, f32),
}

impl NodeResizeContext {
    pub fn new(node_origin: (f32, f32)) -> Self {
        Self {
            node_origin: normalize_node_origin(node_origin),
        }
    }
}

impl Default for NodeResizeContext {
    fn default() -> Self {
        Self::new((0.0, 0.0))
    }
}

/// XyFlow-style resize control direction.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeResizeDirection {
    Top,
    TopRight,
    Right,
    #[default]
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
    TopLeft,
}

impl NodeResizeDirection {
    pub(super) fn is_horizontal(self) -> bool {
        matches!(
            self,
            Self::TopRight
                | Self::Right
                | Self::BottomRight
                | Self::BottomLeft
                | Self::Left
                | Self::TopLeft
        )
    }

    pub(super) fn is_vertical(self) -> bool {
        matches!(
            self,
            Self::Top
                | Self::TopRight
                | Self::BottomRight
                | Self::Bottom
                | Self::BottomLeft
                | Self::TopLeft
        )
    }

    pub(super) fn affects_x(self) -> bool {
        matches!(self, Self::BottomLeft | Self::Left | Self::TopLeft)
    }

    pub(super) fn affects_y(self) -> bool {
        matches!(self, Self::Top | Self::TopRight | Self::TopLeft)
    }
}

/// Optional resize-axis filter for pointer-derived resize geometry.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeResizeAxis {
    #[default]
    Both,
    Horizontal,
    Vertical,
}

impl NodeResizeAxis {
    pub(super) fn includes_width(self) -> bool {
        matches!(self, Self::Both | Self::Horizontal)
    }

    pub(super) fn includes_height(self) -> bool {
        matches!(self, Self::Both | Self::Vertical)
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
    /// Resize control direction that determines affected axes and position updates.
    pub direction: NodeResizeDirection,
}

impl NodeResizeRequest {
    pub fn new(node: NodeId, to: CanvasSize) -> Self {
        Self {
            node,
            to,
            constraints: NodeResizeConstraints::default(),
            direction: NodeResizeDirection::default(),
        }
    }

    pub fn with_constraints(mut self, constraints: NodeResizeConstraints) -> Self {
        self.constraints = constraints;
        self
    }

    pub fn with_direction(mut self, direction: NodeResizeDirection) -> Self {
        self.direction = direction;
        self
    }
}

/// Canvas-space request for resizing one node from pointer movement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodePointerResizeRequest {
    /// Node being resized.
    pub node: NodeId,
    /// Pointer position at resize start in canvas space.
    pub start: CanvasPoint,
    /// Current pointer position in canvas space.
    pub current: CanvasPoint,
    /// Resize control direction that determines affected axes and position updates.
    pub direction: NodeResizeDirection,
    /// Optional min/max bounds applied before planning.
    pub constraints: NodeResizeConstraints,
    /// Whether pointer-derived dimensions preserve the starting aspect ratio.
    pub keep_aspect_ratio: bool,
    /// Optional axis filter for pointer-derived dimensions.
    pub axis: NodeResizeAxis,
}

impl NodePointerResizeRequest {
    pub fn new(
        node: NodeId,
        start: CanvasPoint,
        current: CanvasPoint,
        direction: NodeResizeDirection,
    ) -> Self {
        Self {
            node,
            start,
            current,
            direction,
            constraints: NodeResizeConstraints::default(),
            keep_aspect_ratio: false,
            axis: NodeResizeAxis::default(),
        }
    }

    pub fn with_constraints(mut self, constraints: NodeResizeConstraints) -> Self {
        self.constraints = constraints;
        self
    }

    pub fn with_keep_aspect_ratio(mut self, keep_aspect_ratio: bool) -> Self {
        self.keep_aspect_ratio = keep_aspect_ratio;
        self
    }

    pub fn with_axis(mut self, axis: NodeResizeAxis) -> Self {
        self.axis = axis;
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
    /// Current node position.
    pub from_pos: CanvasPoint,
    /// Planned node position.
    pub to_pos: CanvasPoint,
    items: Vec<NodeResizeItem>,
    transaction: GraphTransaction,
}

impl NodeResizePlan {
    pub(super) fn new(
        node: NodeId,
        from: Option<CanvasSize>,
        to: CanvasSize,
        from_pos: CanvasPoint,
        to_pos: CanvasPoint,
        transaction: GraphTransaction,
    ) -> Self {
        Self {
            node,
            from,
            to,
            from_pos,
            to_pos,
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
