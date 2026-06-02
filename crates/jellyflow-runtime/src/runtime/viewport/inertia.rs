use serde::{Deserialize, Serialize};

use jellyflow_core::core::CanvasPoint;

use crate::io::NodeGraphPanInertiaTuning;

use super::transform::ViewportTransform;

/// Request to plan inertial panning from an adapter-provided release velocity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ViewportPanInertiaRequest {
    pub current: ViewportTransform,
    /// Initial pan velocity in logical screen pixels per second.
    pub initial_velocity_screen: CanvasPoint,
    pub tuning: NodeGraphPanInertiaTuning,
}

impl ViewportPanInertiaRequest {
    pub fn new(
        current: ViewportTransform,
        initial_velocity_screen: CanvasPoint,
        tuning: NodeGraphPanInertiaTuning,
    ) -> Self {
        Self {
            current,
            initial_velocity_screen,
            tuning,
        }
    }
}

/// Deterministic pan inertia plan.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ViewportPanInertiaPlan {
    pub from: ViewportTransform,
    pub initial_velocity_screen: CanvasPoint,
    pub duration_seconds: f32,
    pub decay_per_s: f32,
    pub min_speed: f32,
}

impl ViewportPanInertiaPlan {
    pub fn frame_at(self, elapsed_seconds: f32) -> Option<ViewportPanInertiaFrame> {
        if !elapsed_seconds.is_finite() {
            return None;
        }

        let elapsed_seconds = elapsed_seconds.max(0.0);
        let sample_elapsed = elapsed_seconds.min(self.duration_seconds);
        let decay_factor = (-self.decay_per_s * sample_elapsed).exp();
        let velocity_screen = scale_point(self.initial_velocity_screen, decay_factor);
        let speed_screen = point_speed(velocity_screen);
        let displacement_screen = scale_point(
            self.initial_velocity_screen,
            (1.0 - decay_factor) / self.decay_per_s,
        );
        let transform = ViewportTransform::new(
            CanvasPoint {
                x: self.from.pan.x + displacement_screen.x / self.from.zoom,
                y: self.from.pan.y + displacement_screen.y / self.from.zoom,
            },
            self.from.zoom,
        )?;

        Some(ViewportPanInertiaFrame {
            elapsed_seconds,
            progress: if self.duration_seconds <= 0.0 {
                1.0
            } else {
                (sample_elapsed / self.duration_seconds).clamp(0.0, 1.0)
            },
            speed_screen,
            velocity_screen,
            transform,
            done: elapsed_seconds >= self.duration_seconds || speed_screen <= self.min_speed,
        })
    }

    pub fn terminal_frame(self) -> Option<ViewportPanInertiaFrame> {
        self.frame_at(self.duration_seconds)
    }
}

/// Sampled inertial pan frame.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ViewportPanInertiaFrame {
    pub elapsed_seconds: f32,
    pub progress: f32,
    pub speed_screen: f32,
    pub velocity_screen: CanvasPoint,
    pub transform: ViewportTransform,
    pub done: bool,
}

pub fn plan_viewport_pan_inertia(
    request: ViewportPanInertiaRequest,
) -> Option<ViewportPanInertiaPlan> {
    if !request.current.is_valid()
        || !request.initial_velocity_screen.is_finite()
        || !request.tuning.enabled
        || !valid_positive(request.tuning.decay_per_s)
        || !valid_positive(request.tuning.min_speed)
        || !valid_positive(request.tuning.max_speed)
        || request.tuning.max_speed <= request.tuning.min_speed
    {
        return None;
    }

    let initial_speed = point_speed(request.initial_velocity_screen);
    if !initial_speed.is_finite() || initial_speed <= request.tuning.min_speed {
        return None;
    }

    let initial_velocity_screen = clamp_velocity(
        request.initial_velocity_screen,
        initial_speed,
        request.tuning.max_speed,
    )?;
    let clamped_speed = point_speed(initial_velocity_screen);
    if clamped_speed <= request.tuning.min_speed {
        return None;
    }

    let duration_seconds =
        (clamped_speed / request.tuning.min_speed).ln() / request.tuning.decay_per_s;
    if !duration_seconds.is_finite() || duration_seconds <= 0.0 {
        return None;
    }

    Some(ViewportPanInertiaPlan {
        from: request.current,
        initial_velocity_screen,
        duration_seconds,
        decay_per_s: request.tuning.decay_per_s,
        min_speed: request.tuning.min_speed,
    })
}

fn clamp_velocity(velocity: CanvasPoint, speed: f32, max_speed: f32) -> Option<CanvasPoint> {
    if speed <= max_speed {
        return Some(velocity);
    }

    let scale = max_speed / speed;
    let clamped = scale_point(velocity, scale);
    clamped.is_finite().then_some(clamped)
}

fn scale_point(point: CanvasPoint, scale: f32) -> CanvasPoint {
    CanvasPoint {
        x: point.x * scale,
        y: point.y * scale,
    }
}

fn point_speed(point: CanvasPoint) -> f32 {
    point.x.hypot(point.y)
}

fn valid_positive(value: f32) -> bool {
    value.is_finite() && value > 0.0
}
