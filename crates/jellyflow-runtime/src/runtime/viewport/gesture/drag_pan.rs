use crate::io::NodeGraphPanInteraction;
use crate::runtime::events::ViewportMoveKind;

use super::super::transform::ViewportPanRequest;
use super::shared::{pan_button_allowed, pan_on_drag_enabled};
use super::types::{
    ViewportDragPanInput, ViewportGestureContext, ViewportGestureIntent, ViewportGestureRejection,
};

/// Resolves normalized pointer-drag input into a viewport drag-pan intent.
pub fn resolve_viewport_drag_pan_gesture(
    pan: &NodeGraphPanInteraction<'_>,
    context: ViewportGestureContext,
    input: ViewportDragPanInput,
) -> Result<ViewportGestureIntent, ViewportGestureRejection> {
    if context.user_selection_active {
        return Err(ViewportGestureRejection::UserSelectionActive);
    }
    if context.connection_in_progress {
        return Err(ViewportGestureRejection::ConnectionInProgress);
    }
    if !input.screen_delta.is_finite() {
        return Err(ViewportGestureRejection::InvalidInput);
    }
    if !pan_on_drag_enabled(pan.pan_on_drag) {
        return Err(ViewportGestureRejection::PanOnDragDisabled);
    }
    if !pan_button_allowed(pan.pan_on_drag, input.button) {
        return Err(ViewportGestureRejection::PanOnDragButtonDisabled);
    }

    Ok(ViewportGestureIntent::Pan {
        kind: ViewportMoveKind::PanDrag,
        request: ViewportPanRequest::new(input.screen_delta),
    })
}
