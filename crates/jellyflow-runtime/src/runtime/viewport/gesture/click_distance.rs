/// Input for resolving the effective pane click distance used by viewport adapters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PaneClickDistanceInput {
    pub pane_click_distance: f32,
    pub selection_on_drag: bool,
}

impl PaneClickDistanceInput {
    pub fn new(pane_click_distance: f32, selection_on_drag: bool) -> Self {
        Self {
            pane_click_distance,
            selection_on_drag,
        }
    }
}

/// Resolves XyFlow-compatible pane click-distance suppression.
///
/// When selection-on-drag is active, XyFlow sets the pane click distance to infinity so selection
/// gestures do not also produce pane clicks. Otherwise non-numeric or negative distances become
/// zero before reaching the viewport adapter.
pub fn resolve_pane_click_distance(input: PaneClickDistanceInput) -> f32 {
    if input.selection_on_drag {
        return f32::INFINITY;
    }
    if !input.pane_click_distance.is_finite() || input.pane_click_distance < 0.0 {
        return 0.0;
    }

    input.pane_click_distance
}
