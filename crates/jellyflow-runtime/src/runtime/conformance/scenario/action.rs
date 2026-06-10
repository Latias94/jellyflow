use serde::{Deserialize, Serialize};

mod connection;
mod graph;
mod node;
mod rendering;
mod selection;
mod viewport;

pub use connection::ConformanceConnectionTargetFromHandlesInput;
pub use node::{
    ConformanceNodeNudgeRequest, ConformanceNodePointerDownInput,
    ConformanceNodePointerResizeRequest, ConformanceNodeResizeRequest,
};

use crate::io::NodeGraphKeyCode;
use crate::runtime::auto_pan::{AutoPanRequest, SelectionAutoPanRequest};
use crate::runtime::connection::{
    ConnectEdgeRequest, ConnectionTargetInput, ReconnectEdgeRequest, ResolvedConnectionTarget,
};
use crate::runtime::events::NodeGraphGestureEvent;
use crate::runtime::selection::SelectionBoxInput;
use crate::runtime::viewport::{
    ViewportAnimationFrame, ViewportAnimationPlan, ViewportAnimationRequest,
    ViewportDoubleClickZoomInput, ViewportDragPanInput, ViewportGestureContext,
    ViewportGestureRejection, ViewportPanInertiaFrame, ViewportPanInertiaRequest,
    ViewportPanRequest, ViewportScrollInput, ViewportZoomRequest,
};
use jellyflow_core::core::{CanvasPoint, CanvasSize, EdgeId, GroupId, NodeId};
use jellyflow_core::ops::GraphTransaction;

pub(super) fn is_false(value: &bool) -> bool {
    !*value
}

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
    AssertNodePosition {
        node: NodeId,
        expected: CanvasPoint,
    },
    ApplyNodeResize {
        request: ConformanceNodeResizeRequest,
    },
    ApplyNodePointerResize {
        request: ConformanceNodePointerResizeRequest,
    },
    ApplyNodePointerResizeSession {
        request: ConformanceNodePointerResizeRequest,
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
    AssertConnectionTargetFromHandles {
        input: ConformanceConnectionTargetFromHandlesInput,
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
    ApplySelectionAutoPan {
        request: SelectionAutoPanRequest,
    },
    ApplyViewportPan {
        request: ViewportPanRequest,
    },
    ApplyViewportPanConstrained {
        request: ViewportPanRequest,
        viewport_size: CanvasSize,
    },
    ApplyViewportZoom {
        request: ViewportZoomRequest,
    },
    ApplyViewportZoomConstrained {
        request: ViewportZoomRequest,
        viewport_size: CanvasSize,
    },
    ApplyViewportAnimationFrame {
        request: ViewportAnimationRequest,
        elapsed_seconds: f32,
    },
    ApplyViewportAnimationFrames {
        request: ViewportAnimationRequest,
        elapsed_seconds: Vec<f32>,
    },
    AssertViewportAnimationFrame {
        request: ViewportAnimationRequest,
        elapsed_seconds: f32,
        expected: ViewportAnimationFrame,
    },
    ApplyViewportPanInertiaFrame {
        request: ViewportPanInertiaRequest,
        elapsed_seconds: f32,
    },
    ApplyViewportPanInertiaFrames {
        request: ViewportPanInertiaRequest,
        elapsed_seconds: Vec<f32>,
    },
    AssertViewportPanInertiaFrame {
        request: ViewportPanInertiaRequest,
        elapsed_seconds: f32,
        expected: ViewportPanInertiaFrame,
    },
    ExpectViewportPanInertiaRejected {
        request: ViewportPanInertiaRequest,
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
    AssertVisibleNodeIds {
        viewport_size: CanvasSize,
        expected: Vec<NodeId>,
    },
    AssertVisibleNodeRenderOrder {
        viewport_size: CanvasSize,
        expected: Vec<NodeId>,
    },
    AssertVisibleEdgeIds {
        viewport_size: CanvasSize,
        expected: Vec<EdgeId>,
    },
    AssertVisibleEdgeRenderOrder {
        viewport_size: CanvasSize,
        expected: Vec<EdgeId>,
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
            Self::AssertNodePosition { .. } => "assert_node_position",
            Self::ApplyNodeResize { .. } => "apply_node_resize",
            Self::ApplyNodePointerResize { .. } => "apply_node_pointer_resize",
            Self::ApplyNodePointerResizeSession { .. } => "apply_node_pointer_resize_session",
            Self::ApplyNodePointerDown { .. } => "apply_node_pointer_down",
            Self::ApplySelectionBox { .. } => "apply_selection_box",
            Self::AssertConnectionTarget { .. } => "assert_connection_target",
            Self::AssertConnectionTargetFromHandles { .. } => {
                "assert_connection_target_from_handles"
            }
            Self::ApplyConnectEdge { .. } => "apply_connect_edge",
            Self::ApplyReconnectEdge { .. } => "apply_reconnect_edge",
            Self::ApplyNodeNudge { .. } => "apply_node_nudge",
            Self::ApplyDeleteSelection => "apply_delete_selection",
            Self::ApplyDeleteSelectionForKey { .. } => "apply_delete_selection_for_key",
            Self::ApplyAutoPan { .. } => "apply_auto_pan",
            Self::ApplySelectionAutoPan { .. } => "apply_selection_auto_pan",
            Self::ApplyViewportPan { .. } => "apply_viewport_pan",
            Self::ApplyViewportPanConstrained { .. } => "apply_viewport_pan_constrained",
            Self::ApplyViewportZoom { .. } => "apply_viewport_zoom",
            Self::ApplyViewportZoomConstrained { .. } => "apply_viewport_zoom_constrained",
            Self::ApplyViewportAnimationFrame { .. } => "apply_viewport_animation_frame",
            Self::ApplyViewportAnimationFrames { .. } => "apply_viewport_animation_frames",
            Self::AssertViewportAnimationFrame { .. } => "assert_viewport_animation_frame",
            Self::ApplyViewportPanInertiaFrame { .. } => "apply_viewport_pan_inertia_frame",
            Self::ApplyViewportPanInertiaFrames { .. } => "apply_viewport_pan_inertia_frames",
            Self::AssertViewportPanInertiaFrame { .. } => "assert_viewport_pan_inertia_frame",
            Self::ExpectViewportPanInertiaRejected { .. } => "expect_viewport_pan_inertia_rejected",
            Self::AssertViewportDoubleClickZoom { .. } => "assert_viewport_double_click_zoom",
            Self::ApplyViewportScrollGesture { .. } => "apply_viewport_scroll_gesture",
            Self::ApplyViewportDragPanGesture { .. } => "apply_viewport_drag_pan_gesture",
            Self::SetViewport { .. } => "set_viewport",
            Self::AssertVisibleNodeIds { .. } => "assert_visible_node_ids",
            Self::AssertVisibleNodeRenderOrder { .. } => "assert_visible_node_render_order",
            Self::AssertVisibleEdgeIds { .. } => "assert_visible_edge_ids",
            Self::AssertVisibleEdgeRenderOrder { .. } => "assert_visible_edge_render_order",
            Self::SetSelection { .. } => "set_selection",
            Self::EmitGesture { .. } => "emit_gesture",
        }
    }
}
