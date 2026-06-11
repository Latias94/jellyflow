use crate::runtime::drag::{NodeNudgeRequest, PointerGestureClaim};
use crate::runtime::resize::{NodePointerResizeRequest, NodeResizeRequest};
use crate::runtime::selection::NodePointerDownInput;
use jellyflow_core::core::{CanvasPoint, NodeId};

use super::ConformanceAction;

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

impl ConformanceAction {
    pub fn apply_node_drag(node: NodeId, to: CanvasPoint) -> Self {
        Self::ApplyNodeDrag { node, to }
    }

    pub fn apply_node_drag_session(node: NodeId, start: CanvasPoint, to: CanvasPoint) -> Self {
        Self::ApplyNodeDragSession { node, start, to }
    }

    pub fn apply_node_resize(request: NodeResizeRequest) -> Self {
        Self::ApplyNodeResize { request }
    }

    pub fn apply_node_pointer_resize(request: NodePointerResizeRequest) -> Self {
        Self::ApplyNodePointerResize { request }
    }

    pub fn apply_node_pointer_resize_session(request: NodePointerResizeRequest) -> Self {
        Self::ApplyNodePointerResizeSession { request }
    }

    pub fn apply_node_pointer_down(
        node: NodeId,
        multi_selection_active: bool,
        screen_delta: CanvasPoint,
    ) -> Self {
        Self::ApplyNodePointerDown {
            input: NodePointerDownInput {
                node,
                multi_selection_active,
                screen_delta,
            },
            expected_claim: None,
        }
    }

    pub fn apply_node_pointer_down_expect_claim(
        node: NodeId,
        multi_selection_active: bool,
        screen_delta: CanvasPoint,
        expected_claim: PointerGestureClaim,
    ) -> Self {
        Self::ApplyNodePointerDown {
            input: NodePointerDownInput {
                node,
                multi_selection_active,
                screen_delta,
            },
            expected_claim: Some(expected_claim),
        }
    }

    pub fn apply_node_nudge(request: NodeNudgeRequest) -> Self {
        Self::ApplyNodeNudge { request }
    }
}
