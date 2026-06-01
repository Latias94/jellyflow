use jellyflow_core::core::CanvasPoint;

/// Screen-space input for deciding whether a node drag should activate.
///
/// XyFlow evaluates `nodeDragThreshold` in client/screen coordinates so zoom does not change when
/// dragging starts. Adapters should pass the screen delta from pointer-down to the current pointer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeDragActivationInput {
    pub screen_delta: CanvasPoint,
    pub threshold: f32,
}

impl NodeDragActivationInput {
    pub fn new(screen_delta: CanvasPoint, threshold: f32) -> Self {
        Self {
            screen_delta,
            threshold,
        }
    }
}

/// Returns whether a pointer movement should start a node drag.
///
/// This mirrors XyFlow's threshold shape: threshold `0` starts immediately, otherwise the
/// Euclidean screen-space distance must be strictly greater than `nodeDragThreshold`.
pub fn node_drag_threshold_met(input: NodeDragActivationInput) -> bool {
    if !input.screen_delta.is_finite() {
        return false;
    }
    if input.threshold == 0.0 {
        return true;
    }
    if !input.threshold.is_finite() {
        return false;
    }

    input.screen_delta.x.hypot(input.screen_delta.y) > input.threshold
}
