use jellyflow_core::core::CanvasPoint;

use crate::runtime::selection::{
    SelectionPointerClaim, SelectionPointerClaimInput, resolve_selection_pointer_claim,
};

use super::activation::{NodeDragActivationInput, node_drag_threshold_met};

/// Input for resolving which headless pointer gesture should claim a drag start.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointerGestureClaimInput {
    pub screen_delta: CanvasPoint,
    pub selection_key_pressed: bool,
    pub user_selection_active: bool,
    pub pane_click_distance: f32,
    pub node_drag_threshold: f32,
}

impl PointerGestureClaimInput {
    pub fn new(
        screen_delta: CanvasPoint,
        selection_key_pressed: bool,
        user_selection_active: bool,
        pane_click_distance: f32,
        node_drag_threshold: f32,
    ) -> Self {
        Self {
            screen_delta,
            selection_key_pressed,
            user_selection_active,
            pane_click_distance,
            node_drag_threshold,
        }
    }
}

/// Which runtime gesture should own the current pointer drag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerGestureClaim {
    None,
    Selection,
    NodeDrag,
}

pub fn resolve_pointer_gesture_claim(input: PointerGestureClaimInput) -> PointerGestureClaim {
    match resolve_selection_pointer_claim(SelectionPointerClaimInput::new(
        input.screen_delta,
        input.pane_click_distance,
        input.selection_key_pressed,
        input.user_selection_active,
    )) {
        SelectionPointerClaim::SelectionOwnsDrag | SelectionPointerClaim::SelectionMayClaimDrag => {
            PointerGestureClaim::Selection
        }
        SelectionPointerClaim::Unclaimed => {
            if node_drag_threshold_met(NodeDragActivationInput::new(
                input.screen_delta,
                input.node_drag_threshold,
            )) {
                PointerGestureClaim::NodeDrag
            } else {
                PointerGestureClaim::None
            }
        }
    }
}
