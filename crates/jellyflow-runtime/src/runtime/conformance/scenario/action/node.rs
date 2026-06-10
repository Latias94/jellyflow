use serde::{Deserialize, Serialize};

use crate::runtime::drag::{NodeNudgeDirection, NodeNudgeRequest};
use crate::runtime::resize::{
    NodePointerResizeRequest, NodeResizeAxis, NodeResizeConstraints, NodeResizeDirection,
    NodeResizeRequest, NodeResizeSession, NodeResizeSessionUpdateRequest,
};
use crate::runtime::selection::{NodeDragStartSelectionInput, NodePointerDownInput};
use jellyflow_core::core::{CanvasPoint, CanvasSize, NodeId};

use super::{ConformanceAction, is_false};

pub(super) fn kind(action: &ConformanceAction) -> Option<&'static str> {
    Some(match action {
        ConformanceAction::ApplyNodeDrag { .. } => "apply_node_drag",
        ConformanceAction::ApplyNodeDragSession { .. } => "apply_node_drag_session",
        ConformanceAction::ApplyNodeResize { .. } => "apply_node_resize",
        ConformanceAction::ApplyNodePointerResize { .. } => "apply_node_pointer_resize",
        ConformanceAction::ApplyNodePointerResizeSession { .. } => {
            "apply_node_pointer_resize_session"
        }
        ConformanceAction::ApplyNodePointerDown { .. } => "apply_node_pointer_down",
        ConformanceAction::ApplyNodeNudge { .. } => "apply_node_nudge",
        _ => return None,
    })
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ConformanceNodeResizeConstraints {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<CanvasSize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<CanvasSize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ConformanceNodeResizeRequest {
    pub node: NodeId,
    pub to: CanvasSize,
    #[serde(default)]
    pub constraints: ConformanceNodeResizeConstraints,
    #[serde(default)]
    pub direction: ConformanceNodeResizeDirection,
}

impl ConformanceNodeResizeRequest {
    pub fn into_runtime(self) -> NodeResizeRequest {
        NodeResizeRequest::new(self.node, self.to)
            .with_constraints(NodeResizeConstraints::new(
                self.constraints.min,
                self.constraints.max,
            ))
            .with_direction(self.direction.into_runtime())
    }

    pub fn from_runtime(request: NodeResizeRequest) -> Self {
        Self {
            node: request.node,
            to: request.to,
            constraints: ConformanceNodeResizeConstraints {
                min: request.constraints.min,
                max: request.constraints.max,
            },
            direction: ConformanceNodeResizeDirection::from_runtime(request.direction),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ConformanceNodePointerResizeRequest {
    pub node: NodeId,
    pub start: CanvasPoint,
    pub current: CanvasPoint,
    #[serde(default)]
    pub constraints: ConformanceNodeResizeConstraints,
    #[serde(default)]
    pub direction: ConformanceNodeResizeDirection,
    #[serde(default, skip_serializing_if = "is_false")]
    pub keep_aspect_ratio: bool,
    #[serde(default)]
    pub axis: ConformanceNodeResizeAxis,
}

impl ConformanceNodePointerResizeRequest {
    pub fn into_runtime(self) -> NodePointerResizeRequest {
        NodePointerResizeRequest::new(
            self.node,
            self.start,
            self.current,
            self.direction.into_runtime(),
        )
        .with_constraints(NodeResizeConstraints::new(
            self.constraints.min,
            self.constraints.max,
        ))
        .with_keep_aspect_ratio(self.keep_aspect_ratio)
        .with_axis(self.axis.into_runtime())
    }

    pub fn into_runtime_session(self) -> (NodeResizeSession, NodeResizeSessionUpdateRequest) {
        (
            NodeResizeSession::new(self.node, self.start, self.direction.into_runtime()),
            NodeResizeSessionUpdateRequest::new(self.current)
                .with_constraints(NodeResizeConstraints::new(
                    self.constraints.min,
                    self.constraints.max,
                ))
                .with_keep_aspect_ratio(self.keep_aspect_ratio)
                .with_axis(self.axis.into_runtime()),
        )
    }

    pub fn from_runtime(request: NodePointerResizeRequest) -> Self {
        Self {
            node: request.node,
            start: request.start,
            current: request.current,
            constraints: ConformanceNodeResizeConstraints {
                min: request.constraints.min,
                max: request.constraints.max,
            },
            direction: ConformanceNodeResizeDirection::from_runtime(request.direction),
            keep_aspect_ratio: request.keep_aspect_ratio,
            axis: ConformanceNodeResizeAxis::from_runtime(request.axis),
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConformanceNodeResizeDirection {
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

impl ConformanceNodeResizeDirection {
    fn into_runtime(self) -> NodeResizeDirection {
        match self {
            Self::Top => NodeResizeDirection::Top,
            Self::TopRight => NodeResizeDirection::TopRight,
            Self::Right => NodeResizeDirection::Right,
            Self::BottomRight => NodeResizeDirection::BottomRight,
            Self::Bottom => NodeResizeDirection::Bottom,
            Self::BottomLeft => NodeResizeDirection::BottomLeft,
            Self::Left => NodeResizeDirection::Left,
            Self::TopLeft => NodeResizeDirection::TopLeft,
        }
    }

    fn from_runtime(direction: NodeResizeDirection) -> Self {
        match direction {
            NodeResizeDirection::Top => Self::Top,
            NodeResizeDirection::TopRight => Self::TopRight,
            NodeResizeDirection::Right => Self::Right,
            NodeResizeDirection::BottomRight => Self::BottomRight,
            NodeResizeDirection::Bottom => Self::Bottom,
            NodeResizeDirection::BottomLeft => Self::BottomLeft,
            NodeResizeDirection::Left => Self::Left,
            NodeResizeDirection::TopLeft => Self::TopLeft,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConformanceNodeResizeAxis {
    #[default]
    Both,
    Horizontal,
    Vertical,
}

impl ConformanceNodeResizeAxis {
    fn into_runtime(self) -> NodeResizeAxis {
        match self {
            Self::Both => NodeResizeAxis::Both,
            Self::Horizontal => NodeResizeAxis::Horizontal,
            Self::Vertical => NodeResizeAxis::Vertical,
        }
    }

    fn from_runtime(axis: NodeResizeAxis) -> Self {
        match axis {
            NodeResizeAxis::Both => Self::Both,
            NodeResizeAxis::Horizontal => Self::Horizontal,
            NodeResizeAxis::Vertical => Self::Vertical,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConformanceNodeNudgeRequest {
    pub direction: ConformanceNodeNudgeDirection,
    #[serde(default)]
    pub fast: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ConformanceNodePointerDownInput {
    pub node: NodeId,
    #[serde(default)]
    pub multi_selection_active: bool,
    pub screen_delta: CanvasPoint,
}

impl ConformanceNodePointerDownInput {
    pub fn into_runtime(self) -> NodePointerDownInput {
        NodePointerDownInput::new(
            NodeDragStartSelectionInput::new(self.node, self.multi_selection_active),
            self.screen_delta,
        )
    }
}

impl ConformanceNodeNudgeRequest {
    pub fn into_runtime(self) -> NodeNudgeRequest {
        NodeNudgeRequest {
            direction: self.direction.into_runtime(),
            fast: self.fast,
        }
    }

    pub fn from_runtime(request: NodeNudgeRequest) -> Self {
        Self {
            direction: ConformanceNodeNudgeDirection::from_runtime(request.direction),
            fast: request.fast,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConformanceNodeNudgeDirection {
    Up,
    Down,
    Left,
    Right,
}

impl ConformanceNodeNudgeDirection {
    fn into_runtime(self) -> NodeNudgeDirection {
        match self {
            Self::Up => NodeNudgeDirection::Up,
            Self::Down => NodeNudgeDirection::Down,
            Self::Left => NodeNudgeDirection::Left,
            Self::Right => NodeNudgeDirection::Right,
        }
    }

    fn from_runtime(direction: NodeNudgeDirection) -> Self {
        match direction {
            NodeNudgeDirection::Up => Self::Up,
            NodeNudgeDirection::Down => Self::Down,
            NodeNudgeDirection::Left => Self::Left,
            NodeNudgeDirection::Right => Self::Right,
        }
    }
}

impl ConformanceAction {
    pub fn apply_node_drag(node: NodeId, to: CanvasPoint) -> Self {
        Self::ApplyNodeDrag { node, to }
    }

    pub fn apply_node_drag_session(node: NodeId, start: CanvasPoint, to: CanvasPoint) -> Self {
        Self::ApplyNodeDragSession { node, start, to }
    }

    pub fn apply_node_resize(request: NodeResizeRequest) -> Self {
        Self::ApplyNodeResize {
            request: ConformanceNodeResizeRequest::from_runtime(request),
        }
    }

    pub fn apply_node_pointer_resize(request: NodePointerResizeRequest) -> Self {
        Self::ApplyNodePointerResize {
            request: ConformanceNodePointerResizeRequest::from_runtime(request),
        }
    }

    pub fn apply_node_pointer_resize_session(request: NodePointerResizeRequest) -> Self {
        Self::ApplyNodePointerResizeSession {
            request: ConformanceNodePointerResizeRequest::from_runtime(request),
        }
    }

    pub fn apply_node_pointer_down(
        node: NodeId,
        multi_selection_active: bool,
        screen_delta: CanvasPoint,
    ) -> Self {
        Self::ApplyNodePointerDown {
            input: ConformanceNodePointerDownInput {
                node,
                multi_selection_active,
                screen_delta,
            },
        }
    }

    pub fn apply_node_nudge(request: NodeNudgeRequest) -> Self {
        Self::ApplyNodeNudge {
            request: ConformanceNodeNudgeRequest::from_runtime(request),
        }
    }
}
