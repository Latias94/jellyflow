use serde::{Deserialize, Serialize};

use jellyflow_core::core::{CanvasPoint, CanvasRect};

use crate::runtime::adapter_protocol::{ViewDelta, ViewPoint, ViewSize};

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

    /// Projects a canvas point into logical screen pixels.
    ///
    /// Canvas and view coordinates are intentionally not interchangeable.
    ///
    /// ```compile_fail
    /// use jellyflow_core::CanvasPoint;
    /// use jellyflow_runtime::viewport::ViewportTransform;
    ///
    /// let transform = ViewportTransform::new(CanvasPoint::default(), 1.0).unwrap();
    /// let projected: CanvasPoint = transform
    ///     .screen_point_for_canvas(CanvasPoint::default())
    ///     .unwrap();
    /// ```
    pub fn screen_point_for_canvas(self, canvas: CanvasPoint) -> Option<ViewPoint> {
        if !self.is_valid() || !canvas.is_finite() {
            return None;
        }

        let screen = ViewPoint {
            x: (canvas.x + self.pan.x) * self.zoom,
            y: (canvas.y + self.pan.y) * self.zoom,
        };
        screen.is_finite().then_some(screen)
    }

    /// Converts a logical screen-pixel point to canvas space.
    ///
    /// ```compile_fail
    /// use jellyflow_core::CanvasPoint;
    /// use jellyflow_runtime::viewport::ViewportTransform;
    ///
    /// let transform = ViewportTransform::new(CanvasPoint::default(), 1.0).unwrap();
    /// let _ = transform.canvas_point_at_screen(CanvasPoint::default());
    /// ```
    pub fn canvas_point_at_screen(self, screen: ViewPoint) -> Option<CanvasPoint> {
        if !self.is_valid() || !screen.is_finite() {
            return None;
        }

        let canvas = CanvasPoint {
            x: screen.x / self.zoom - self.pan.x,
            y: screen.y / self.zoom - self.pan.y,
        };
        canvas.is_finite().then_some(canvas)
    }
}

/// Optional pan constraints for a viewport transform.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct ViewportConstraints {
    pub viewport_size: Option<ViewSize>,
    pub translate_extent: Option<CanvasRect>,
}

impl ViewportConstraints {
    pub fn unconstrained() -> Self {
        Self::default()
    }

    pub fn with_translate_extent(viewport_size: ViewSize, translate_extent: CanvasRect) -> Self {
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
    pub screen_delta: ViewDelta,
}

impl ViewportPanRequest {
    pub fn new(screen_delta: ViewDelta) -> Self {
        Self { screen_delta }
    }
}

/// Renderer-neutral zoom request anchored at a logical screen-pixel point.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ViewportZoomRequest {
    /// Logical screen-pixel point that should keep the same canvas coordinate while zooming.
    pub anchor_screen: ViewPoint,
    /// Desired zoom before clamping.
    pub target_zoom: f32,
    /// Inclusive minimum zoom clamp.
    pub min_zoom: f32,
    /// Inclusive maximum zoom clamp.
    pub max_zoom: f32,
}

impl ViewportZoomRequest {
    pub fn new(anchor_screen: ViewPoint, target_zoom: f32, min_zoom: f32, max_zoom: f32) -> Self {
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

#[cfg(test)]
mod tests {
    use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize};

    use crate::runtime::adapter_protocol::{ViewPoint, ViewSize};

    use super::{
        ViewportConstraints, ViewportTransform, ViewportZoomRequest, constrain_viewport,
        zoom_viewport,
    };

    #[test]
    fn canvas_and_view_points_round_trip_across_pans_and_zooms() {
        let canvas = CanvasPoint { x: 37.5, y: -12.25 };

        for (pan, zoom) in [
            (CanvasPoint { x: 0.0, y: 0.0 }, 0.5),
            (CanvasPoint { x: -7.0, y: 19.0 }, 1.0),
            (CanvasPoint { x: 11.5, y: -3.25 }, 2.0),
        ] {
            let transform = ViewportTransform::new(pan, zoom).expect("valid transform");
            let view = transform
                .screen_point_for_canvas(canvas)
                .expect("finite canvas point projects into view space");
            let restored = transform
                .canvas_point_at_screen(view)
                .expect("finite view point projects into canvas space");

            assert!((restored.x - canvas.x).abs() <= f32::EPSILON);
            assert!((restored.y - canvas.y).abs() <= f32::EPSILON);
        }
    }

    #[test]
    fn zoom_preserves_canvas_coordinate_under_view_anchor() {
        let current = ViewportTransform::new(CanvasPoint { x: 14.0, y: -8.0 }, 1.25)
            .expect("valid transform");
        let anchor = ViewPoint::new(160.0, 90.0);
        let before = current
            .canvas_point_at_screen(anchor)
            .expect("finite anchor projects into canvas space");
        let next = zoom_viewport(current, ViewportZoomRequest::new(anchor, 2.0, 0.25, 4.0))
            .expect("valid anchored zoom");
        let after = next
            .canvas_point_at_screen(anchor)
            .expect("finite anchor projects into canvas space");

        assert!((after.x - before.x).abs() <= f32::EPSILON);
        assert!((after.y - before.y).abs() <= f32::EPSILON);
    }

    #[test]
    fn translate_extent_centers_small_viewport_and_clamps_equal_and_large_viewports() {
        let extent = CanvasRect {
            origin: CanvasPoint { x: 10.0, y: 20.0 },
            size: CanvasSize {
                width: 100.0,
                height: 80.0,
            },
        };
        let transform = ViewportTransform::new(
            CanvasPoint {
                x: 500.0,
                y: -500.0,
            },
            1.0,
        )
        .expect("valid transform");

        let small = constrain_viewport(
            transform,
            ViewportConstraints::with_translate_extent(ViewSize::new(50.0, 40.0), extent),
        )
        .expect("small viewport is constrained");
        assert_eq!(small.pan, CanvasPoint { x: -10.0, y: -60.0 });

        let equal = constrain_viewport(
            transform,
            ViewportConstraints::with_translate_extent(ViewSize::new(100.0, 80.0), extent),
        )
        .expect("equal viewport is constrained");
        assert_eq!(equal.pan, CanvasPoint { x: -10.0, y: -20.0 });

        let large = constrain_viewport(
            transform,
            ViewportConstraints::with_translate_extent(ViewSize::new(140.0, 120.0), extent),
        )
        .expect("large viewport is constrained");
        assert_eq!(large.pan, CanvasPoint { x: 10.0, y: 0.0 });
    }

    #[test]
    fn projections_and_constraints_reject_non_finite_or_invalid_inputs() {
        let transform = ViewportTransform::new(CanvasPoint::default(), 1.0).expect("valid");

        assert_eq!(
            transform.screen_point_for_canvas(CanvasPoint {
                x: f32::NAN,
                y: 0.0,
            }),
            None
        );
        assert_eq!(
            transform.canvas_point_at_screen(ViewPoint::new(f32::INFINITY, 0.0)),
            None
        );
        assert!(ViewportTransform::new(CanvasPoint::default(), 0.0).is_none());

        let extent = CanvasRect {
            origin: CanvasPoint::default(),
            size: CanvasSize {
                width: 10.0,
                height: 10.0,
            },
        };
        assert!(
            constrain_viewport(
                transform,
                ViewportConstraints::with_translate_extent(ViewSize::new(0.0, 10.0), extent),
            )
            .is_none()
        );
    }
}
