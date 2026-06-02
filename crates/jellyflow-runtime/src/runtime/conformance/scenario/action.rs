use serde::{Deserialize, Serialize};

use crate::io::NodeGraphKeyCode;
use crate::runtime::auto_pan::AutoPanRequest;
use crate::runtime::connection::{
    ConnectEdgeRequest, ConnectionTargetInput, ReconnectEdgeRequest, ResolvedConnectionTarget,
};
use crate::runtime::drag::{NodeNudgeDirection, NodeNudgeRequest};
use crate::runtime::events::NodeGraphGestureEvent;
use crate::runtime::selection::SelectionBoxInput;
use crate::runtime::selection::{NodeDragStartSelectionInput, NodePointerDownInput};
use crate::runtime::viewport::{
    ViewportAnimationFrame, ViewportAnimationPlan, ViewportAnimationRequest,
    ViewportDoubleClickZoomInput, ViewportDragPanInput, ViewportGestureContext,
    ViewportGestureRejection, ViewportPanRequest, ViewportScrollInput, ViewportZoomRequest,
};
use jellyflow_core::core::{CanvasPoint, EdgeId, GroupId, NodeId};
use jellyflow_core::ops::GraphTransaction;
use keyboard_types::Code as KeyCode;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum ConformanceAction {
    /// Applies a raw graph transaction through the store dispatch pipeline.
    ///
    /// Use this for low-level graph fixture setup or graph-operation conformance. Adapter feel
    /// fixtures should prefer the richer runtime actions such as `ApplyNodeDrag`,
    /// `ApplyConnectEdge`, `ApplyReconnectEdge`, and delete/viewport actions so the fixture locks
    /// the same interaction boundary an adapter should call.
    DispatchTransaction {
        transaction: GraphTransaction,
    },
    ApplyNodeDrag {
        node: NodeId,
        to: CanvasPoint,
    },
    ApplyNodePointerDown {
        input: ConformanceNodePointerDownInput,
    },
    ApplySelectionBox {
        input: SelectionBoxInput,
    },
    AssertConnectionTarget {
        input: ConnectionTargetInput,
        expected: ResolvedConnectionTarget,
    },
    ApplyConnectEdge {
        request: ConnectEdgeRequest,
    },
    ApplyReconnectEdge {
        request: ReconnectEdgeRequest,
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
    AssertViewportAnimationFrame {
        request: ViewportAnimationRequest,
        elapsed_seconds: f32,
        expected: ViewportAnimationFrame,
    },
    AssertViewportDoubleClickZoom {
        input: ViewportDoubleClickZoomInput,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected: Option<ViewportAnimationPlan>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expect_rejection: Option<ViewportGestureRejection>,
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
            Self::ApplyNodePointerDown { .. } => "apply_node_pointer_down",
            Self::ApplySelectionBox { .. } => "apply_selection_box",
            Self::AssertConnectionTarget { .. } => "assert_connection_target",
            Self::ApplyConnectEdge { .. } => "apply_connect_edge",
            Self::ApplyReconnectEdge { .. } => "apply_reconnect_edge",
            Self::ApplyNodeNudge { .. } => "apply_node_nudge",
            Self::ApplyDeleteSelection => "apply_delete_selection",
            Self::ApplyDeleteSelectionForKey { .. } => "apply_delete_selection_for_key",
            Self::ApplyAutoPan { .. } => "apply_auto_pan",
            Self::ApplyViewportPan { .. } => "apply_viewport_pan",
            Self::ApplyViewportZoom { .. } => "apply_viewport_zoom",
            Self::AssertViewportAnimationFrame { .. } => "assert_viewport_animation_frame",
            Self::AssertViewportDoubleClickZoom { .. } => "assert_viewport_double_click_zoom",
            Self::ApplyViewportScrollGesture { .. } => "apply_viewport_scroll_gesture",
            Self::ApplyViewportDragPanGesture { .. } => "apply_viewport_drag_pan_gesture",
            Self::SetViewport { .. } => "set_viewport",
            Self::SetSelection { .. } => "set_selection",
            Self::EmitGesture { .. } => "emit_gesture",
        }
    }

    /// Builds the low-level transaction fixture action.
    ///
    /// Prefer the interaction-specific constructors when checking adapter behavior.
    pub fn dispatch_transaction(transaction: GraphTransaction) -> Self {
        Self::DispatchTransaction { transaction }
    }

    pub fn apply_node_drag(node: NodeId, to: CanvasPoint) -> Self {
        Self::ApplyNodeDrag { node, to }
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

    pub fn apply_selection_box(input: SelectionBoxInput) -> Self {
        Self::ApplySelectionBox { input }
    }

    pub fn assert_connection_target(
        input: ConnectionTargetInput,
        expected: ResolvedConnectionTarget,
    ) -> Self {
        Self::AssertConnectionTarget { input, expected }
    }

    pub fn apply_connect_edge(request: ConnectEdgeRequest) -> Self {
        Self::ApplyConnectEdge { request }
    }

    pub fn apply_reconnect_edge(request: ReconnectEdgeRequest) -> Self {
        Self::ApplyReconnectEdge { request }
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

    pub fn assert_viewport_animation_frame(
        request: ViewportAnimationRequest,
        elapsed_seconds: f32,
        expected: ViewportAnimationFrame,
    ) -> Self {
        Self::AssertViewportAnimationFrame {
            request,
            elapsed_seconds,
            expected,
        }
    }

    pub fn assert_viewport_double_click_zoom(
        input: ViewportDoubleClickZoomInput,
        expected: ViewportAnimationPlan,
    ) -> Self {
        Self::AssertViewportDoubleClickZoom {
            input,
            expected: Some(expected),
            expect_rejection: None,
        }
    }

    pub fn expect_viewport_double_click_zoom_rejected(
        input: ViewportDoubleClickZoomInput,
        rejection: ViewportGestureRejection,
    ) -> Self {
        Self::AssertViewportDoubleClickZoom {
            input,
            expected: None,
            expect_rejection: Some(rejection),
        }
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
