use serde::{Deserialize, Serialize};

use crate::io::NodeGraphAutoPanTuning;
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
