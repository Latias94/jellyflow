use crate::io::{NodeGraphPanInteraction, NodeGraphPanOnScrollMode, NodeGraphZoomInteraction};
use crate::runtime::events::ViewportMoveKind;
use jellyflow_core::core::CanvasPoint;

use super::super::transform::{ViewportPanRequest, ViewportZoomRequest, valid_zoom};
use super::shared::pan_on_drag_enabled;
use super::types::{
    ViewportGestureContext, ViewportGestureIntent, ViewportGestureRejection, ViewportScrollInput,
};

/// Resolves normalized wheel or trackpad-scroll input into a viewport gesture intent.
///
/// The policy follows XyFlow's pan/zoom priority without depending on DOM or d3 events:
/// Ctrl/pinch zoom wins first, pan-on-scroll wins when no zoom activation key is pressed, and
/// zoom-on-scroll/activation-key zoom handles the remaining accepted scroll gestures.
pub fn resolve_viewport_scroll_gesture(
    pan: &NodeGraphPanInteraction<'_>,
    zoom: &NodeGraphZoomInteraction,
    context: ViewportGestureContext,
    input: ViewportScrollInput,
) -> Result<ViewportGestureIntent, ViewportGestureRejection> {
    if context.user_selection_active {
        return Err(ViewportGestureRejection::UserSelectionActive);
    }
    if !input.delta.is_finite() || !input.anchor_screen.is_finite() {
        return Err(ViewportGestureRejection::InvalidInput);
    }
    if !any_viewport_gesture_enabled(pan, zoom) {
        return Err(ViewportGestureRejection::AllViewportGesturesDisabled);
    }

    let zoom_scroll = context.zoom_activation_key_pressed || zoom.zoom_on_scroll;
    let pinch_zoom = input.ctrl_key && zoom.zoom_on_pinch;

    if input.ctrl_key && !zoom.zoom_on_pinch {
        return Err(ViewportGestureRejection::PinchDisabled);
    }

    if pinch_zoom {
        return zoom_intent(ViewportMoveKind::ZoomPinch, input);
    }

    if pan.pan_on_scroll && !context.zoom_activation_key_pressed {
        if !pan.pan_on_scroll_speed.is_finite() {
            return Err(ViewportGestureRejection::InvalidInput);
        }
        return Ok(ViewportGestureIntent::Pan {
            kind: ViewportMoveKind::PanScroll,
            request: ViewportPanRequest::new(scroll_pan_delta(pan, input.delta)),
        });
    }

    if zoom_scroll {
        return zoom_intent(ViewportMoveKind::ZoomWheel, input);
    }

    Err(ViewportGestureRejection::WheelDisabled)
}

fn any_viewport_gesture_enabled(
    pan: &NodeGraphPanInteraction<'_>,
    zoom: &NodeGraphZoomInteraction,
) -> bool {
    pan.pan_on_scroll
        || pan_on_drag_enabled(pan.pan_on_drag)
        || zoom.zoom_on_scroll
        || zoom.zoom_on_pinch
        || zoom.zoom_on_double_click
}

fn scroll_pan_delta(pan: &NodeGraphPanInteraction<'_>, delta: CanvasPoint) -> CanvasPoint {
    let delta = match pan.pan_on_scroll_mode {
        NodeGraphPanOnScrollMode::Free => delta,
        NodeGraphPanOnScrollMode::Horizontal => CanvasPoint { x: delta.x, y: 0.0 },
        NodeGraphPanOnScrollMode::Vertical => CanvasPoint { x: 0.0, y: delta.y },
    };

    CanvasPoint {
        x: -delta.x * pan.pan_on_scroll_speed,
        y: -delta.y * pan.pan_on_scroll_speed,
    }
}

fn zoom_intent(
    kind: ViewportMoveKind,
    input: ViewportScrollInput,
) -> Result<ViewportGestureIntent, ViewportGestureRejection> {
    if !valid_zoom(input.target_zoom) || !valid_zoom(input.min_zoom) || !valid_zoom(input.max_zoom)
    {
        return Err(ViewportGestureRejection::InvalidInput);
    }

    Ok(ViewportGestureIntent::Zoom {
        kind,
        request: ViewportZoomRequest::new(
            input.anchor_screen,
            input.target_zoom,
            input.min_zoom,
            input.max_zoom,
        ),
    })
}
