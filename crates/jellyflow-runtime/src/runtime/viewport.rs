//! Renderer-neutral viewport pan and zoom helpers.
//!
//! Adapters normalize platform input into these request types. The runtime owns deterministic
//! canvas/screen transform math without depending on renderer, windowing, or gesture APIs.

use serde::{Deserialize, Serialize};

use crate::io::{
    NodeGraphPanInteraction, NodeGraphPanOnDragButtons, NodeGraphPanOnScrollMode,
    NodeGraphViewState, NodeGraphZoomInteraction,
};
use crate::runtime::events::ViewportMoveKind;
use jellyflow_core::core::CanvasPoint;

/// Current viewport transform.
///
/// `pan` is stored in canvas space and `zoom` is a positive scale factor. Screen projection follows
/// `(canvas + pan) * zoom`, matching the existing fit-view helpers and persisted view-state.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ViewportTransform {
    /// Canvas-space pan.
    pub pan: CanvasPoint,
    /// Positive zoom factor.
    pub zoom: f32,
}

impl ViewportTransform {
    /// Creates a viewport transform when pan is finite and zoom is positive finite.
    pub fn new(pan: CanvasPoint, zoom: f32) -> Option<Self> {
        let transform = Self { pan, zoom };
        if !transform.is_valid() {
            return None;
        }

        Some(transform)
    }

    /// Returns true when the transform can safely participate in viewport math.
    pub fn is_valid(self) -> bool {
        self.pan.is_finite() && valid_zoom(self.zoom)
    }

    /// Reads the viewport transform from a persisted view-state.
    pub fn from_view_state(view_state: &NodeGraphViewState) -> Option<Self> {
        Self::new(view_state.pan, view_state.zoom)
    }

    /// Projects a canvas point into logical screen pixels.
    pub fn screen_point_for_canvas(self, canvas: CanvasPoint) -> Option<CanvasPoint> {
        if !self.is_valid() || !canvas.is_finite() {
            return None;
        }

        let screen = CanvasPoint {
            x: (canvas.x + self.pan.x) * self.zoom,
            y: (canvas.y + self.pan.y) * self.zoom,
        };
        screen.is_finite().then_some(screen)
    }

    /// Converts a logical screen-pixel point to canvas space.
    pub fn canvas_point_at_screen(self, screen: CanvasPoint) -> CanvasPoint {
        CanvasPoint {
            x: screen.x / self.zoom - self.pan.x,
            y: screen.y / self.zoom - self.pan.y,
        }
    }
}

/// Renderer-neutral drag-pan request.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ViewportPanRequest {
    /// Logical screen-pixel delta for the content movement.
    pub screen_delta: CanvasPoint,
}

impl ViewportPanRequest {
    pub fn new(screen_delta: CanvasPoint) -> Self {
        Self { screen_delta }
    }
}

/// Renderer-neutral zoom request anchored at a logical screen-pixel point.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ViewportZoomRequest {
    /// Logical screen-pixel point that should keep the same canvas coordinate while zooming.
    pub anchor_screen: CanvasPoint,
    /// Desired zoom before clamping.
    pub target_zoom: f32,
    /// Inclusive minimum zoom clamp.
    pub min_zoom: f32,
    /// Inclusive maximum zoom clamp.
    pub max_zoom: f32,
}

impl ViewportZoomRequest {
    pub fn new(anchor_screen: CanvasPoint, target_zoom: f32, min_zoom: f32, max_zoom: f32) -> Self {
        Self {
            anchor_screen,
            target_zoom,
            min_zoom,
            max_zoom,
        }
    }
}

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

/// Applies a drag-pan request to the current transform.
pub fn pan_viewport(
    current: ViewportTransform,
    request: ViewportPanRequest,
) -> Option<ViewportTransform> {
    if !current.is_valid() {
        return None;
    }
    if !request.screen_delta.is_finite() {
        return None;
    }

    ViewportTransform::new(
        CanvasPoint {
            x: current.pan.x + request.screen_delta.x / current.zoom,
            y: current.pan.y + request.screen_delta.y / current.zoom,
        },
        current.zoom,
    )
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

/// Applies an anchored zoom request to the current transform.
pub fn zoom_viewport(
    current: ViewportTransform,
    request: ViewportZoomRequest,
) -> Option<ViewportTransform> {
    if !current.is_valid() {
        return None;
    }
    let target_zoom = clamped_target_zoom(request)?;
    let anchor = request.anchor_screen;
    if !anchor.is_finite() {
        return None;
    }

    ViewportTransform::new(
        CanvasPoint {
            x: current.pan.x + anchor.x / target_zoom - anchor.x / current.zoom,
            y: current.pan.y + anchor.y / target_zoom - anchor.y / current.zoom,
        },
        target_zoom,
    )
}

fn clamped_target_zoom(request: ViewportZoomRequest) -> Option<f32> {
    if !valid_zoom(request.target_zoom)
        || !valid_zoom(request.min_zoom)
        || !valid_zoom(request.max_zoom)
    {
        return None;
    }

    let (min_zoom, max_zoom) = if request.min_zoom <= request.max_zoom {
        (request.min_zoom, request.max_zoom)
    } else {
        (request.max_zoom, request.min_zoom)
    };

    Some(request.target_zoom.clamp(min_zoom, max_zoom))
}

fn valid_zoom(zoom: f32) -> bool {
    zoom.is_finite() && zoom > 0.0
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
