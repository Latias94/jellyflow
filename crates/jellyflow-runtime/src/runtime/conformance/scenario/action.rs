use serde::{Deserialize, Serialize};

use crate::io::NodeGraphKeyCode;
use crate::runtime::auto_pan::AutoPanRequest;
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
