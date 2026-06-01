use serde::{Deserialize, Serialize};

use crate::runtime::events::ViewportMoveKind;
use jellyflow_core::core::CanvasPoint;

use super::super::transform::{ViewportPanRequest, ViewportZoomRequest};

/// Runtime context that affects whether a normalized viewport gesture is accepted.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewportGestureContext {
    /// A user selection gesture is active and should suppress viewport gestures.
    pub user_selection_active: bool,
    /// A connection gesture is active and should suppress pointer drag panning.
    pub connection_in_progress: bool,
    /// The adapter resolved the configured zoom activation key as pressed.
    pub zoom_activation_key_pressed: bool,
}

impl ViewportGestureContext {
    pub fn idle() -> Self {
        Self::default()
    }
}

/// Renderer-neutral pointer button for viewport drag-pan policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewportPointerButton {
    Left,
    Middle,
    Right,
    Other,
}

/// Normalized wheel or trackpad-scroll input for viewport gesture policy.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ViewportScrollInput {
    /// Logical screen-pixel scroll delta after adapter/platform normalization.
    pub delta: CanvasPoint,
    /// Logical screen-pixel anchor for zoom gestures.
    pub anchor_screen: CanvasPoint,
    /// Whether the input represents a Ctrl-modified wheel/pinch gesture.
    pub ctrl_key: bool,
    /// Desired zoom before clamping. Adapters own raw wheel-to-zoom normalization.
    pub target_zoom: f32,
    /// Inclusive minimum zoom clamp.
    pub min_zoom: f32,
    /// Inclusive maximum zoom clamp.
    pub max_zoom: f32,
}

impl ViewportScrollInput {
    pub fn new(
        delta: CanvasPoint,
        anchor_screen: CanvasPoint,
        ctrl_key: bool,
        target_zoom: f32,
        min_zoom: f32,
        max_zoom: f32,
    ) -> Self {
        Self {
            delta,
            anchor_screen,
            ctrl_key,
            target_zoom,
            min_zoom,
            max_zoom,
        }
    }
}

/// Normalized pointer-drag input for viewport pan policy.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ViewportDragPanInput {
    pub button: ViewportPointerButton,
    pub screen_delta: CanvasPoint,
}

impl ViewportDragPanInput {
    pub fn new(button: ViewportPointerButton, screen_delta: CanvasPoint) -> Self {
        Self {
            button,
            screen_delta,
        }
    }
}

/// Accepted viewport gesture intent.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum ViewportGestureIntent {
    Pan {
        kind: ViewportMoveKind,
        request: ViewportPanRequest,
    },
    Zoom {
        kind: ViewportMoveKind,
        request: ViewportZoomRequest,
    },
}

impl ViewportGestureIntent {
    pub fn move_kind(&self) -> ViewportMoveKind {
        match self {
            Self::Pan { kind, .. } | Self::Zoom { kind, .. } => *kind,
        }
    }
}

/// Reason a normalized viewport gesture was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewportGestureRejection {
    AllViewportGesturesDisabled,
    UserSelectionActive,
    ConnectionInProgress,
    WheelDisabled,
    PinchDisabled,
    PanOnDragDisabled,
    PanOnDragButtonDisabled,
    InvalidInput,
}
