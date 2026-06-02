use serde::{Deserialize, Serialize};

use crate::io::NodeGraphKeyCode;
use crate::runtime::auto_pan::AutoPanRequest;
use crate::runtime::drag::{NodeNudgeDirection, NodeNudgeRequest};
use crate::runtime::events::NodeGraphGestureEvent;
use crate::runtime::viewport::{
    ViewportDragPanInput, ViewportGestureContext, ViewportGestureRejection, ViewportPanRequest,
    ViewportScrollInput, ViewportZoomRequest,
};
use jellyflow_core::core::{CanvasPoint, EdgeId, GroupId, NodeId};
use jellyflow_core::ops::GraphTransaction;
use keyboard_types::Code as KeyCode;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum ConformanceAction {
    DispatchTransaction {
        transaction: GraphTransaction,
    },
    ApplyNodeDrag {
        node: NodeId,
        to: CanvasPoint,
    },
    ApplyNodeNudge {
        request: ConformanceNodeNudgeRequest,
    },
    ApplyDeleteSelection,
    ApplyDeleteSelectionForKey {
        key: NodeGraphKeyCode,
    },
    ApplyAutoPan {
        request: AutoPanRequest,
    },
    ApplyViewportPan {
        request: ViewportPanRequest,
    },
    ApplyViewportZoom {
        request: ViewportZoomRequest,
    },
    ApplyViewportScrollGesture {
        context: ViewportGestureContext,
        input: ViewportScrollInput,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expect_rejection: Option<ViewportGestureRejection>,
    },
    ApplyViewportDragPanGesture {
        context: ViewportGestureContext,
        input: ViewportDragPanInput,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expect_rejection: Option<ViewportGestureRejection>,
    },
    SetViewport {
        pan: CanvasPoint,
        zoom: f32,
    },
    SetSelection {
        nodes: Vec<NodeId>,
        edges: Vec<EdgeId>,
        groups: Vec<GroupId>,
    },
    EmitGesture {
        event: NodeGraphGestureEvent,
    },
}

impl ConformanceAction {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::DispatchTransaction { .. } => "dispatch_transaction",
            Self::ApplyNodeDrag { .. } => "apply_node_drag",
            Self::ApplyNodeNudge { .. } => "apply_node_nudge",
            Self::ApplyDeleteSelection => "apply_delete_selection",
            Self::ApplyDeleteSelectionForKey { .. } => "apply_delete_selection_for_key",
            Self::ApplyAutoPan { .. } => "apply_auto_pan",
            Self::ApplyViewportPan { .. } => "apply_viewport_pan",
            Self::ApplyViewportZoom { .. } => "apply_viewport_zoom",
            Self::ApplyViewportScrollGesture { .. } => "apply_viewport_scroll_gesture",
            Self::ApplyViewportDragPanGesture { .. } => "apply_viewport_drag_pan_gesture",
            Self::SetViewport { .. } => "set_viewport",
            Self::SetSelection { .. } => "set_selection",
            Self::EmitGesture { .. } => "emit_gesture",
        }
    }

    pub fn dispatch_transaction(transaction: GraphTransaction) -> Self {
        Self::DispatchTransaction { transaction }
    }

    pub fn apply_node_drag(node: NodeId, to: CanvasPoint) -> Self {
        Self::ApplyNodeDrag { node, to }
    }

    pub fn apply_node_nudge(request: NodeNudgeRequest) -> Self {
        Self::ApplyNodeNudge {
            request: ConformanceNodeNudgeRequest::from_runtime(request),
        }
    }

    pub fn apply_delete_selection() -> Self {
        Self::ApplyDeleteSelection
    }

    pub fn apply_delete_selection_for_key(key: KeyCode) -> Self {
        Self::ApplyDeleteSelectionForKey {
            key: NodeGraphKeyCode(key),
        }
    }

    pub fn apply_auto_pan(request: AutoPanRequest) -> Self {
        Self::ApplyAutoPan { request }
    }

    pub fn apply_viewport_pan(request: ViewportPanRequest) -> Self {
        Self::ApplyViewportPan { request }
    }

    pub fn apply_viewport_zoom(request: ViewportZoomRequest) -> Self {
        Self::ApplyViewportZoom { request }
    }

    pub fn apply_viewport_scroll_gesture(
        context: ViewportGestureContext,
        input: ViewportScrollInput,
    ) -> Self {
        Self::ApplyViewportScrollGesture {
            context,
            input,
            expect_rejection: None,
        }
    }

    pub fn expect_viewport_scroll_gesture_rejected(
        context: ViewportGestureContext,
        input: ViewportScrollInput,
        rejection: ViewportGestureRejection,
    ) -> Self {
        Self::ApplyViewportScrollGesture {
            context,
            input,
            expect_rejection: Some(rejection),
        }
    }

    pub fn apply_viewport_drag_pan_gesture(
        context: ViewportGestureContext,
        input: ViewportDragPanInput,
    ) -> Self {
        Self::ApplyViewportDragPanGesture {
            context,
            input,
            expect_rejection: None,
        }
    }

    pub fn expect_viewport_drag_pan_gesture_rejected(
        context: ViewportGestureContext,
        input: ViewportDragPanInput,
        rejection: ViewportGestureRejection,
    ) -> Self {
        Self::ApplyViewportDragPanGesture {
            context,
            input,
            expect_rejection: Some(rejection),
        }
    }

    pub fn set_viewport(pan: CanvasPoint, zoom: f32) -> Self {
        Self::SetViewport { pan, zoom }
    }

    pub fn set_selection(
        nodes: impl IntoIterator<Item = NodeId>,
        edges: impl IntoIterator<Item = EdgeId>,
        groups: impl IntoIterator<Item = GroupId>,
    ) -> Self {
        Self::SetSelection {
            nodes: nodes.into_iter().collect(),
            edges: edges.into_iter().collect(),
            groups: groups.into_iter().collect(),
        }
    }

    pub fn emit_gesture(event: NodeGraphGestureEvent) -> Self {
        Self::EmitGesture { event }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConformanceNodeNudgeRequest {
    pub direction: ConformanceNodeNudgeDirection,
    #[serde(default)]
    pub fast: bool,
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
