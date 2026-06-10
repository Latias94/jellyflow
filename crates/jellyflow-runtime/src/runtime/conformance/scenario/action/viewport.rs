use crate::runtime::auto_pan::{AutoPanRequest, SelectionAutoPanRequest};
use crate::runtime::viewport::{
    ViewportAnimationFrame, ViewportAnimationPlan, ViewportAnimationRequest,
    ViewportDoubleClickZoomInput, ViewportDragPanInput, ViewportGestureContext,
    ViewportGestureRejection, ViewportPanInertiaFrame, ViewportPanInertiaRequest,
    ViewportPanRequest, ViewportScrollInput, ViewportZoomRequest,
};
use jellyflow_core::core::CanvasSize;

use super::ConformanceAction;

pub(super) fn kind(action: &ConformanceAction) -> Option<&'static str> {
    Some(match action {
        ConformanceAction::ApplyAutoPan { .. } => "apply_auto_pan",
        ConformanceAction::ApplySelectionAutoPan { .. } => "apply_selection_auto_pan",
        ConformanceAction::ApplyViewportPan { .. } => "apply_viewport_pan",
        ConformanceAction::ApplyViewportPanConstrained { .. } => "apply_viewport_pan_constrained",
        ConformanceAction::ApplyViewportZoom { .. } => "apply_viewport_zoom",
        ConformanceAction::ApplyViewportZoomConstrained { .. } => "apply_viewport_zoom_constrained",
        ConformanceAction::ApplyViewportAnimationFrame { .. } => "apply_viewport_animation_frame",
        ConformanceAction::ApplyViewportAnimationFrames { .. } => "apply_viewport_animation_frames",
        ConformanceAction::AssertViewportAnimationFrame { .. } => "assert_viewport_animation_frame",
        ConformanceAction::ApplyViewportPanInertiaFrame { .. } => {
            "apply_viewport_pan_inertia_frame"
        }
        ConformanceAction::ApplyViewportPanInertiaFrames { .. } => {
            "apply_viewport_pan_inertia_frames"
        }
        ConformanceAction::AssertViewportPanInertiaFrame { .. } => {
            "assert_viewport_pan_inertia_frame"
        }
        ConformanceAction::ExpectViewportPanInertiaRejected { .. } => {
            "expect_viewport_pan_inertia_rejected"
        }
        ConformanceAction::AssertViewportDoubleClickZoom { .. } => {
            "assert_viewport_double_click_zoom"
        }
        ConformanceAction::ApplyViewportScrollGesture { .. } => "apply_viewport_scroll_gesture",
        ConformanceAction::ApplyViewportDragPanGesture { .. } => "apply_viewport_drag_pan_gesture",
        ConformanceAction::ApplyViewportDragPanSession { .. } => "apply_viewport_drag_pan_session",
        _ => return None,
    })
}

impl ConformanceAction {
    pub fn apply_auto_pan(request: AutoPanRequest) -> Self {
        Self::ApplyAutoPan { request }
    }

    pub fn apply_selection_auto_pan(request: SelectionAutoPanRequest) -> Self {
        Self::ApplySelectionAutoPan { request }
    }

    pub fn apply_viewport_pan(request: ViewportPanRequest) -> Self {
        Self::ApplyViewportPan { request }
    }

    pub fn apply_viewport_pan_constrained(
        request: ViewportPanRequest,
        viewport_size: CanvasSize,
    ) -> Self {
        Self::ApplyViewportPanConstrained {
            request,
            viewport_size,
        }
    }

    pub fn apply_viewport_zoom(request: ViewportZoomRequest) -> Self {
        Self::ApplyViewportZoom { request }
    }

    pub fn apply_viewport_zoom_constrained(
        request: ViewportZoomRequest,
        viewport_size: CanvasSize,
    ) -> Self {
        Self::ApplyViewportZoomConstrained {
            request,
            viewport_size,
        }
    }

    pub fn apply_viewport_animation_frame(
        request: ViewportAnimationRequest,
        elapsed_seconds: f32,
    ) -> Self {
        Self::ApplyViewportAnimationFrame {
            request,
            elapsed_seconds,
        }
    }

    pub fn apply_viewport_animation_frames(
        request: ViewportAnimationRequest,
        elapsed_seconds: impl IntoIterator<Item = f32>,
    ) -> Self {
        Self::ApplyViewportAnimationFrames {
            request,
            elapsed_seconds: elapsed_seconds.into_iter().collect(),
        }
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

    pub fn apply_viewport_pan_inertia_frame(
        request: ViewportPanInertiaRequest,
        elapsed_seconds: f32,
    ) -> Self {
        Self::ApplyViewportPanInertiaFrame {
            request,
            elapsed_seconds,
        }
    }

    pub fn apply_viewport_pan_inertia_frames(
        request: ViewportPanInertiaRequest,
        elapsed_seconds: impl IntoIterator<Item = f32>,
    ) -> Self {
        Self::ApplyViewportPanInertiaFrames {
            request,
            elapsed_seconds: elapsed_seconds.into_iter().collect(),
        }
    }

    pub fn assert_viewport_pan_inertia_frame(
        request: ViewportPanInertiaRequest,
        elapsed_seconds: f32,
        expected: ViewportPanInertiaFrame,
    ) -> Self {
        Self::AssertViewportPanInertiaFrame {
            request,
            elapsed_seconds,
            expected,
        }
    }

    pub fn expect_viewport_pan_inertia_rejected(request: ViewportPanInertiaRequest) -> Self {
        Self::ExpectViewportPanInertiaRejected { request }
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

    pub fn apply_viewport_drag_pan_session(
        context: ViewportGestureContext,
        input: ViewportDragPanInput,
    ) -> Self {
        Self::ApplyViewportDragPanSession {
            context,
            input,
            expect_rejection: None,
        }
    }

    pub fn expect_viewport_drag_pan_session_rejected(
        context: ViewportGestureContext,
        input: ViewportDragPanInput,
        rejection: ViewportGestureRejection,
    ) -> Self {
        Self::ApplyViewportDragPanSession {
            context,
            input,
            expect_rejection: Some(rejection),
        }
    }
}
