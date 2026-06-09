use crate::io::NodeGraphAutoPanTuning;
use jellyflow_core::core::CanvasPoint;

use super::types::{AutoPanPlan, AutoPanRequest, SelectionAutoPanRequest};

/// Computes one auto-pan frame without mutating store state.
pub fn compute_auto_pan(
    tuning: &NodeGraphAutoPanTuning,
    request: AutoPanRequest,
) -> Option<AutoPanPlan> {
    if !request.activation.enabled_by(tuning)
        || !request.pointer_screen.is_finite()
        || !request.viewport_size.is_positive_finite()
        || !valid_positive(request.elapsed_seconds)
        || !valid_positive(tuning.speed)
        || !valid_positive(tuning.margin)
    {
        return None;
    }

    let screen_delta = CanvasPoint {
        x: axis_screen_delta(
            request.pointer_screen.x,
            request.viewport_size.width,
            tuning.margin,
            tuning.speed,
            request.elapsed_seconds,
        ),
        y: axis_screen_delta(
            request.pointer_screen.y,
            request.viewport_size.height,
            tuning.margin,
            tuning.speed,
            request.elapsed_seconds,
        ),
    };
    if screen_delta == CanvasPoint::default() {
        return None;
    }

    Some(AutoPanPlan { screen_delta })
}

/// Computes one selection-drag auto-pan frame without mutating store state.
pub fn compute_selection_auto_pan(
    tuning: &NodeGraphAutoPanTuning,
    request: SelectionAutoPanRequest,
) -> Option<AutoPanPlan> {
    compute_auto_pan(tuning, request.auto_pan_request())
}

fn axis_screen_delta(
    position: f32,
    length: f32,
    margin: f32,
    speed: f32,
    elapsed_seconds: f32,
) -> f32 {
    let start_distance = position;
    let end_distance = length - position;
    let intensity = if start_distance < end_distance {
        edge_intensity(start_distance, margin)
    } else if end_distance < start_distance {
        -edge_intensity(end_distance, margin)
    } else {
        0.0
    };

    intensity * speed * elapsed_seconds
}

fn edge_intensity(distance: f32, margin: f32) -> f32 {
    ((margin - distance) / margin).clamp(0.0, 1.0)
}

fn valid_positive(value: f32) -> bool {
    value.is_finite() && value > 0.0
}
