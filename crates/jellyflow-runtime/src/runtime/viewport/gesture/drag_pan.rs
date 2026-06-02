use crate::io::NodeGraphPanInteraction;
use crate::runtime::drag::PointerGestureClaim;
use crate::runtime::events::ViewportMoveKind;

use super::super::transform::ViewportPanRequest;
use super::shared::{
    effective_pan_on_drag_buttons, pan_button_allowed, pan_on_drag_enabled, pointer_drag_claim,
};
use super::types::{
    ViewportDragPanInput, ViewportGestureContext, ViewportGestureIntent, ViewportGestureRejection,
};

/// Resolves normalized pointer-drag input into a viewport drag-pan intent.
pub fn resolve_viewport_drag_pan_gesture(
    pan: &NodeGraphPanInteraction<'_>,
    context: ViewportGestureContext,
    input: ViewportDragPanInput,
) -> Result<ViewportGestureIntent, ViewportGestureRejection> {
    match pointer_drag_claim(context) {
        PointerGestureClaim::Selection if context.user_selection_active => {
            return Err(ViewportGestureRejection::UserSelectionActive);
        }
        PointerGestureClaim::Selection => {
            return Err(ViewportGestureRejection::PanOnDragDisabled);
        }
        PointerGestureClaim::Connection => {
            return Err(ViewportGestureRejection::ConnectionInProgress);
        }
        PointerGestureClaim::None | PointerGestureClaim::NodeDrag => {}
    }
    if !input.screen_delta.is_finite() {
        return Err(ViewportGestureRejection::InvalidInput);
    }
    let pan_on_drag = effective_pan_on_drag_buttons(pan.pan_on_drag, context);
    if !pan_on_drag_enabled(pan_on_drag) {
        return Err(ViewportGestureRejection::PanOnDragDisabled);
    }
    if !pan_button_allowed(pan_on_drag, input.button) {
        return Err(ViewportGestureRejection::PanOnDragButtonDisabled);
    }

    Ok(ViewportGestureIntent::Pan {
        kind: ViewportMoveKind::PanDrag,
        request: ViewportPanRequest::new(input.screen_delta),
    })
}
