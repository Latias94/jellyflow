use serde::{Deserialize, Serialize};

use crate::io::NodeGraphViewState;
use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize};

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

/// Optional pan constraints for a viewport transform.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct ViewportConstraints {
    pub viewport_size: Option<CanvasSize>,
    pub translate_extent: Option<CanvasRect>,
}

impl ViewportConstraints {
    pub fn unconstrained() -> Self {
        Self::default()
    }

    pub fn with_translate_extent(viewport_size: CanvasSize, translate_extent: CanvasRect) -> Self {
        Self {
            viewport_size: Some(viewport_size),
            translate_extent: Some(translate_extent),
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
    .and_then(|next| constrain_viewport(next, ViewportConstraints::unconstrained()))
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
    .and_then(|next| constrain_viewport(next, ViewportConstraints::unconstrained()))
}

/// Applies optional translate-extent constraints to a viewport transform.
pub fn constrain_viewport(
    transform: ViewportTransform,
    constraints: ViewportConstraints,
) -> Option<ViewportTransform> {
    if !transform.is_valid() {
        return None;
    }
    let Some(translate_extent) = constraints.translate_extent else {
        return Some(transform);
    };
    let viewport_size = constraints.viewport_size?;
    if !viewport_size.is_positive_finite() || !translate_extent.is_positive_finite() {
        return None;
    }

    let visible_width = viewport_size.width / transform.zoom;
    let visible_height = viewport_size.height / transform.zoom;
    let extent_min_x = translate_extent.origin.x;
    let extent_min_y = translate_extent.origin.y;
    let extent_max_x = translate_extent.origin.x + translate_extent.size.width;
    let extent_max_y = translate_extent.origin.y + translate_extent.size.height;

    ViewportTransform::new(
        CanvasPoint {
            x: constrain_pan_axis(transform.pan.x, visible_width, extent_min_x, extent_max_x),
            y: constrain_pan_axis(transform.pan.y, visible_height, extent_min_y, extent_max_y),
        },
        transform.zoom,
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

fn constrain_pan_axis(pan: f32, visible_size: f32, extent_min: f32, extent_max: f32) -> f32 {
    let lower = visible_size - extent_max;
    let upper = -extent_min;
    if lower <= upper {
        pan.clamp(lower, upper)
    } else {
        (lower + upper) * 0.5
    }
}

pub(super) fn valid_zoom(zoom: f32) -> bool {
    zoom.is_finite() && zoom > 0.0
}
