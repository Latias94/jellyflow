use jellyflow_core::core::CanvasPoint;

use super::activation::{SelectionDragActivationInput, selection_drag_threshold_met};

/// Normalized pointer state for deciding whether selection should claim a drag gesture first.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SelectionPointerClaimInput {
    pub screen_delta: CanvasPoint,
    pub pane_click_distance: f32,
    pub selection_key_pressed: bool,
    pub user_selection_active: bool,
}

impl SelectionPointerClaimInput {
    pub fn new(
        screen_delta: CanvasPoint,
        pane_click_distance: f32,
        selection_key_pressed: bool,
        user_selection_active: bool,
    ) -> Self {
        Self {
            screen_delta,
            pane_click_distance,
            selection_key_pressed,
            user_selection_active,
        }
    }
}

/// Selection's current ownership status over a normalized pointer drag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionPointerClaim {
    Unclaimed,
    SelectionMayClaimDrag,
    SelectionOwnsDrag,
}

pub fn resolve_selection_pointer_claim(input: SelectionPointerClaimInput) -> SelectionPointerClaim {
    if input.user_selection_active {
        return SelectionPointerClaim::SelectionOwnsDrag;
    }

    if input.selection_key_pressed
        && selection_drag_threshold_met(SelectionDragActivationInput::new(
            input.screen_delta,
            input.pane_click_distance,
            true,
        ))
    {
        return SelectionPointerClaim::SelectionMayClaimDrag;
    }

    SelectionPointerClaim::Unclaimed
}

pub fn selection_modifier_blocks_viewport_drag(
    selection_key_pressed: bool,
    user_selection_active: bool,
) -> bool {
    resolve_selection_pointer_claim(SelectionPointerClaimInput::new(
        CanvasPoint { x: 0.1, y: 0.0 },
        0.0,
        selection_key_pressed,
        user_selection_active,
    )) == SelectionPointerClaim::SelectionMayClaimDrag
}
