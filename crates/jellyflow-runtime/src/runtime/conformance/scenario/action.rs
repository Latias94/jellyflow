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
use crate::runtime::events::{ConnectStart, NodeGraphGestureEvent};
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
    ApplyNodeDragSession {
        node: NodeId,
        start: CanvasPoint,
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
    ApplyConnectEdgeSession {
        start: ConnectStart,
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
    ApplyViewportDragPanSession {
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
        graph::kind(self)
            .or_else(|| node::kind(self))
            .or_else(|| selection::kind(self))
            .or_else(|| connection::kind(self))
            .or_else(|| viewport::kind(self))
            .or_else(|| rendering::kind(self))
            .expect("all conformance action variants must have a kind")
    }
}
