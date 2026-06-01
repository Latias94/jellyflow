use crate::io::NodeGraphPanOnDragButtons;

use super::types::ViewportPointerButton;

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
