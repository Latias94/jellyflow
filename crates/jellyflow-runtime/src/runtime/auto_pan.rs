//! Renderer-neutral auto-pan helpers.
//!
//! Adapters own frame scheduling and raw pointer capture. The runtime owns deterministic
//! screen-space edge-proximity math and feeds the existing viewport pan path.

use serde::{Deserialize, Serialize};

use crate::io::NodeGraphAutoPanTuning;
use crate::runtime::store::NodeGraphStore;
use crate::runtime::viewport::{ViewportPanRequest, ViewportTransform};
use jellyflow_core::core::{CanvasPoint, CanvasSize};

/// Auto-pan policy gate for a workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AutoPanActivation {
    /// Bypass workflow-specific toggles. Useful for adapter-owned workflows such as selection.
    Always,
    /// Respect `auto_pan.on_node_drag`.
    NodeDrag,
    /// Respect `auto_pan.on_connect`.
    Connect,
    /// Respect `auto_pan.on_node_focus`.
    NodeFocus,
}

impl AutoPanActivation {
    pub fn enabled_by(self, tuning: &NodeGraphAutoPanTuning) -> bool {
        match self {
            Self::Always => true,
            Self::NodeDrag => tuning.on_node_drag,
            Self::Connect => tuning.on_connect,
            Self::NodeFocus => tuning.on_node_focus,
        }
    }
}

/// One frame of auto-pan intent in logical screen coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AutoPanRequest {
    pub activation: AutoPanActivation,
    pub pointer_screen: CanvasPoint,
    pub viewport_size: CanvasSize,
    pub elapsed_seconds: f32,
}

impl AutoPanRequest {
    pub fn new(
        activation: AutoPanActivation,
        pointer_screen: CanvasPoint,
        viewport_size: CanvasSize,
        elapsed_seconds: f32,
    ) -> Self {
        Self {
            activation,
            pointer_screen,
            viewport_size,
            elapsed_seconds,
        }
    }
}

/// Deterministic auto-pan frame produced from edge proximity.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AutoPanPlan {
    /// Logical screen-pixel content movement for this frame.
    pub screen_delta: CanvasPoint,
}

impl AutoPanPlan {
    pub fn viewport_pan_request(self) -> ViewportPanRequest {
        ViewportPanRequest::new(self.screen_delta)
    }
}

/// Store-applied auto-pan result.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AutoPanOutcome {
    pub plan: AutoPanPlan,
    pub transform: ViewportTransform,
}

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

impl NodeGraphStore {
    /// Applies one auto-pan frame through normal viewport view-state publication.
    pub fn apply_auto_pan(&mut self, request: AutoPanRequest) -> Option<AutoPanOutcome> {
        let interaction = self.resolved_interaction_state();
        let plan = compute_auto_pan(&interaction.auto_pan, request)?;
        let transform = self.apply_viewport_pan(plan.viewport_pan_request())?;
        Some(AutoPanOutcome { plan, transform })
    }
}
