use jellyflow_core::core::CanvasPoint;

/// Screen-space input for deciding whether a connection drag should activate.
///
/// XyFlow evaluates `connectionDragThreshold` in client/screen coordinates, using squared
/// distance, so zoom does not change when connection gestures start.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConnectionDragActivationInput {
    pub screen_delta: CanvasPoint,
    pub threshold: f32,
}

impl ConnectionDragActivationInput {
    pub fn new(screen_delta: CanvasPoint, threshold: f32) -> Self {
        Self {
            screen_delta,
            threshold,
        }
    }
}

/// Returns whether pointer movement should start a connection drag.
///
/// This mirrors XyFlow's threshold shape: threshold `0` starts immediately, otherwise the squared
/// screen-space distance must be strictly greater than `connectionDragThreshold^2`.
pub fn connection_drag_threshold_met(input: ConnectionDragActivationInput) -> bool {
    if !input.screen_delta.is_finite() {
        return false;
    }
    if input.threshold == 0.0 {
        return true;
    }
    if !input.threshold.is_finite() {
        return false;
    }

    let threshold = input.threshold.abs();
    let distance_squared =
        input.screen_delta.x * input.screen_delta.x + input.screen_delta.y * input.screen_delta.y;
    distance_squared > threshold * threshold
}
