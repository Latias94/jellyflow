use serde::{Deserialize, Serialize};

use jellyflow_core::core::CanvasPoint;

use super::transform::ViewportTransform;

/// Built-in easing modes for renderer-neutral viewport animation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewportAnimationEasing {
    CubicInOut,
    Linear,
}

impl Default for ViewportAnimationEasing {
    fn default() -> Self {
        Self::CubicInOut
    }
}

impl ViewportAnimationEasing {
    fn sample(self, progress: f32) -> f32 {
        match self {
            Self::CubicInOut => cubic_in_out(progress),
            Self::Linear => progress.clamp(0.0, 1.0),
        }
    }
}

/// Renderer-neutral viewport animation options.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ViewportAnimationOptions {
    /// Animation duration in seconds.
    pub duration_seconds: f32,
    /// Easing mode used when sampling frames.
    pub easing: ViewportAnimationEasing,
}

impl ViewportAnimationOptions {
    pub fn new(duration_seconds: f32) -> Self {
        Self {
            duration_seconds,
            easing: ViewportAnimationEasing::default(),
        }
    }

    pub fn with_easing(mut self, easing: ViewportAnimationEasing) -> Self {
        self.easing = easing;
        self
    }
}

/// Request to plan a viewport animation from one transform to another.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ViewportAnimationRequest {
    pub from: ViewportTransform,
    pub to: ViewportTransform,
    pub options: ViewportAnimationOptions,
}

impl ViewportAnimationRequest {
    pub fn new(
        from: ViewportTransform,
        to: ViewportTransform,
        options: ViewportAnimationOptions,
    ) -> Self {
        Self { from, to, options }
    }
}

/// Deterministic viewport animation plan.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ViewportAnimationPlan {
    pub from: ViewportTransform,
    pub to: ViewportTransform,
    pub duration_seconds: f32,
    pub easing: ViewportAnimationEasing,
}

impl ViewportAnimationPlan {
    /// Samples this plan at an elapsed time in seconds.
    pub fn frame_at(self, elapsed_seconds: f32) -> Option<ViewportAnimationFrame> {
        if !elapsed_seconds.is_finite() {
            return None;
        }

        let elapsed_seconds = elapsed_seconds.max(0.0);
        let progress = if self.duration_seconds <= 0.0 {
            1.0
        } else {
            (elapsed_seconds / self.duration_seconds).clamp(0.0, 1.0)
        };
        let eased_progress = self.easing.sample(progress);
        let transform = interpolate_transform(self.from, self.to, eased_progress)?;

        Some(ViewportAnimationFrame {
            elapsed_seconds,
            progress,
            eased_progress,
            transform,
            done: progress >= 1.0,
        })
    }

    pub fn is_immediate(self) -> bool {
        self.duration_seconds <= 0.0
    }
}

/// Sampled viewport animation frame.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ViewportAnimationFrame {
    pub elapsed_seconds: f32,
    pub progress: f32,
    pub eased_progress: f32,
    pub transform: ViewportTransform,
    pub done: bool,
}

pub fn plan_viewport_animation(
    from: ViewportTransform,
    to: ViewportTransform,
    duration_seconds: f32,
) -> Option<ViewportAnimationPlan> {
    plan_viewport_animation_with_options(ViewportAnimationRequest::new(
        from,
        to,
        ViewportAnimationOptions::new(duration_seconds),
    ))
}

pub fn plan_viewport_animation_with_options(
    request: ViewportAnimationRequest,
) -> Option<ViewportAnimationPlan> {
    if !request.from.is_valid()
        || !request.to.is_valid()
        || !request.options.duration_seconds.is_finite()
    {
        return None;
    }

    Some(ViewportAnimationPlan {
        from: request.from,
        to: request.to,
        duration_seconds: request.options.duration_seconds.max(0.0),
        easing: request.options.easing,
    })
}

fn interpolate_transform(
    from: ViewportTransform,
    to: ViewportTransform,
    progress: f32,
) -> Option<ViewportTransform> {
    ViewportTransform::new(
        CanvasPoint {
            x: lerp(from.pan.x, to.pan.x, progress),
            y: lerp(from.pan.y, to.pan.y, progress),
        },
        lerp(from.zoom, to.zoom, progress),
    )
}

fn lerp(from: f32, to: f32, progress: f32) -> f32 {
    from + (to - from) * progress
}

fn cubic_in_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let doubled = t * 2.0;
    if doubled <= 1.0 {
        doubled * doubled * doubled / 2.0
    } else {
        let shifted = doubled - 2.0;
        (shifted * shifted * shifted + 2.0) / 2.0
    }
}
