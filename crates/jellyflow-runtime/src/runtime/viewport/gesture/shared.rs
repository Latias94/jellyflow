use crate::io::NodeGraphPanOnDragButtons;
use crate::runtime::selection::selection_modifier_blocks_viewport_drag;

use super::types::{ViewportGestureContext, ViewportPointerButton};

const XYFLOW_ACTIVATION_PAN_ON_DRAG: NodeGraphPanOnDragButtons = NodeGraphPanOnDragButtons {
    left: true,
    middle: true,
    right: false,
};

pub(super) fn effective_pan_on_drag_buttons(
    buttons: NodeGraphPanOnDragButtons,
    context: ViewportGestureContext,
) -> NodeGraphPanOnDragButtons {
    if context.selection_key_pressed {
        NodeGraphPanOnDragButtons::default()
    } else if context.pan_activation_key_pressed {
        XYFLOW_ACTIVATION_PAN_ON_DRAG
    } else {
        buttons
    }
}

pub(super) fn effective_pan_on_scroll_enabled(
    pan_on_scroll: bool,
    context: ViewportGestureContext,
) -> bool {
    pan_on_scroll || context.pan_activation_key_pressed
}

pub(super) fn selection_modifier_claims_drag(context: ViewportGestureContext) -> bool {
    selection_modifier_blocks_viewport_drag(
        context.selection_key_pressed,
        context.user_selection_active,
    )
}

pub(super) fn pan_on_drag_enabled(buttons: NodeGraphPanOnDragButtons) -> bool {
    buttons.left || buttons.middle || buttons.right
}

pub(super) fn pan_button_allowed(
    buttons: NodeGraphPanOnDragButtons,
    button: ViewportPointerButton,
) -> bool {
    match button {
        ViewportPointerButton::Left => buttons.left,
        ViewportPointerButton::Middle => buttons.middle,
        ViewportPointerButton::Right => buttons.right,
        ViewportPointerButton::Other => false,
    }
}
