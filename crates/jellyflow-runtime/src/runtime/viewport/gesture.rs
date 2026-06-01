use serde::{Deserialize, Serialize};

use crate::io::{
    NodeGraphPanInteraction, NodeGraphPanOnDragButtons, NodeGraphPanOnScrollMode,
    NodeGraphZoomInteraction,
};
use crate::runtime::events::ViewportMoveKind;
use jellyflow_core::core::CanvasPoint;

use super::transform::{ViewportPanRequest, ViewportZoomRequest, valid_zoom};

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

/// Resolves normalized wheel or trackpad-scroll input into a viewport gesture intent.
///
/// The policy follows XyFlow's pan/zoom priority without depending on DOM or d3 events:
/// Ctrl/pinch zoom wins first, pan-on-scroll wins when no zoom activation key is pressed, and
/// zoom-on-scroll/activation-key zoom handles the remaining accepted scroll gestures.
pub fn resolve_viewport_scroll_gesture(
    pan: &NodeGraphPanInteraction<'_>,
    zoom: &NodeGraphZoomInteraction,
    context: ViewportGestureContext,
    input: ViewportScrollInput,
) -> Result<ViewportGestureIntent, ViewportGestureRejection> {
    if context.user_selection_active {
        return Err(ViewportGestureRejection::UserSelectionActive);
    }
    if !input.delta.is_finite() || !input.anchor_screen.is_finite() {
        return Err(ViewportGestureRejection::InvalidInput);
    }
    if !any_viewport_gesture_enabled(pan, zoom) {
        return Err(ViewportGestureRejection::AllViewportGesturesDisabled);
    }

    let zoom_scroll = context.zoom_activation_key_pressed || zoom.zoom_on_scroll;
    let pinch_zoom = input.ctrl_key && zoom.zoom_on_pinch;

    if input.ctrl_key && !zoom.zoom_on_pinch {
        return Err(ViewportGestureRejection::PinchDisabled);
    }

    if pinch_zoom {
        return zoom_intent(ViewportMoveKind::ZoomPinch, input);
    }

    if pan.pan_on_scroll && !context.zoom_activation_key_pressed {
        if !pan.pan_on_scroll_speed.is_finite() {
            return Err(ViewportGestureRejection::InvalidInput);
        }
        return Ok(ViewportGestureIntent::Pan {
            kind: ViewportMoveKind::PanScroll,
            request: ViewportPanRequest::new(scroll_pan_delta(pan, input.delta)),
        });
    }

    if zoom_scroll {
        return zoom_intent(ViewportMoveKind::ZoomWheel, input);
    }

    Err(ViewportGestureRejection::WheelDisabled)
}

/// Resolves normalized pointer-drag input into a viewport drag-pan intent.
pub fn resolve_viewport_drag_pan_gesture(
    pan: &NodeGraphPanInteraction<'_>,
    context: ViewportGestureContext,
    input: ViewportDragPanInput,
) -> Result<ViewportGestureIntent, ViewportGestureRejection> {
    if context.user_selection_active {
        return Err(ViewportGestureRejection::UserSelectionActive);
    }
    if context.connection_in_progress {
        return Err(ViewportGestureRejection::ConnectionInProgress);
    }
    if !input.screen_delta.is_finite() {
        return Err(ViewportGestureRejection::InvalidInput);
    }
    if !pan_on_drag_enabled(pan.pan_on_drag) {
        return Err(ViewportGestureRejection::PanOnDragDisabled);
    }
    if !pan_button_allowed(pan.pan_on_drag, input.button) {
        return Err(ViewportGestureRejection::PanOnDragButtonDisabled);
    }

    Ok(ViewportGestureIntent::Pan {
        kind: ViewportMoveKind::PanDrag,
        request: ViewportPanRequest::new(input.screen_delta),
    })
}

fn any_viewport_gesture_enabled(
    pan: &NodeGraphPanInteraction<'_>,
    zoom: &NodeGraphZoomInteraction,
) -> bool {
    pan.pan_on_scroll
        || pan_on_drag_enabled(pan.pan_on_drag)
        || zoom.zoom_on_scroll
        || zoom.zoom_on_pinch
        || zoom.zoom_on_double_click
}

fn pan_on_drag_enabled(buttons: NodeGraphPanOnDragButtons) -> bool {
    buttons.left || buttons.middle || buttons.right
}

fn pan_button_allowed(buttons: NodeGraphPanOnDragButtons, button: ViewportPointerButton) -> bool {
    match button {
        ViewportPointerButton::Left => buttons.left,
        ViewportPointerButton::Middle => buttons.middle,
        ViewportPointerButton::Right => buttons.right,
        ViewportPointerButton::Other => false,
    }
}

fn scroll_pan_delta(pan: &NodeGraphPanInteraction<'_>, delta: CanvasPoint) -> CanvasPoint {
    let delta = match pan.pan_on_scroll_mode {
        NodeGraphPanOnScrollMode::Free => delta,
        NodeGraphPanOnScrollMode::Horizontal => CanvasPoint { x: delta.x, y: 0.0 },
        NodeGraphPanOnScrollMode::Vertical => CanvasPoint { x: 0.0, y: delta.y },
    };

    CanvasPoint {
        x: -delta.x * pan.pan_on_scroll_speed,
        y: -delta.y * pan.pan_on_scroll_speed,
    }
}

fn zoom_intent(
    kind: ViewportMoveKind,
    input: ViewportScrollInput,
) -> Result<ViewportGestureIntent, ViewportGestureRejection> {
    if !valid_zoom(input.target_zoom) || !valid_zoom(input.min_zoom) || !valid_zoom(input.max_zoom)
    {
        return Err(ViewportGestureRejection::InvalidInput);
    }

    Ok(ViewportGestureIntent::Zoom {
        kind,
        request: ViewportZoomRequest::new(
            input.anchor_screen,
            input.target_zoom,
            input.min_zoom,
            input.max_zoom,
        ),
    })
}
