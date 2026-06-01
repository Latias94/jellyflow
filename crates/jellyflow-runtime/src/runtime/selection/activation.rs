use jellyflow_core::core::CanvasPoint;

/// Screen-space input for deciding whether a marquee selection drag should activate.
///
/// XyFlow evaluates the pane drag threshold in client/screen coordinates so zoom does not change
/// when selection starts. When the selection key is held, `paneClickDistance` is bypassed and any
/// positive movement can start the marquee gesture.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SelectionDragActivationInput {
    pub screen_delta: CanvasPoint,
    pub pane_click_distance: f32,
    pub selection_key_pressed: bool,
}

impl SelectionDragActivationInput {
    pub fn new(
        screen_delta: CanvasPoint,
        pane_click_distance: f32,
        selection_key_pressed: bool,
    ) -> Self {
        Self {
            screen_delta,
            pane_click_distance,
            selection_key_pressed,
        }
    }
}

/// Returns whether pointer movement should start a marquee selection drag.
///
/// This mirrors XyFlow's threshold shape: the Euclidean screen-space distance must be strictly
/// greater than the required distance. Holding the selection key makes the required distance zero.
pub fn selection_drag_threshold_met(input: SelectionDragActivationInput) -> bool {
    if !input.screen_delta.is_finite() {
        return false;
    }

    let required_distance = if input.selection_key_pressed {
        0.0
    } else if input.pane_click_distance.is_finite() {
        input.pane_click_distance.max(0.0)
    } else {
        return false;
    };

    input.screen_delta.x.hypot(input.screen_delta.y) > required_distance
}
