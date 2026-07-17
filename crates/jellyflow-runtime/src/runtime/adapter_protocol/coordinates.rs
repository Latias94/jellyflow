//! Framework logical-pixel coordinates reported by native layout systems.

use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize};

use crate::runtime::geometry::HandlePosition;
use crate::runtime::viewport::ViewportTransform;

/// A point in framework view-space logical pixels.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ViewPoint {
    pub x: f32,
    pub y: f32,
}

impl ViewPoint {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }

    pub(crate) fn to_canvas_point(self, transform: ViewportTransform) -> Option<CanvasPoint> {
        if !self.is_finite() || !transform.is_valid() {
            return None;
        }
        let point = transform.canvas_point_at_screen(CanvasPoint {
            x: self.x,
            y: self.y,
        });
        point.is_finite().then_some(point)
    }
}

/// A size in framework view-space logical pixels.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ViewSize {
    pub width: f32,
    pub height: f32,
}

impl ViewSize {
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }

    pub fn is_positive_finite(self) -> bool {
        self.width.is_finite() && self.height.is_finite() && self.width > 0.0 && self.height > 0.0
    }
}

/// A rectangle in framework view-space logical pixels.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ViewRect {
    pub origin: ViewPoint,
    pub size: ViewSize,
}

impl ViewRect {
    pub const fn new(origin: ViewPoint, size: ViewSize) -> Self {
        Self { origin, size }
    }

    pub fn is_positive_finite(self) -> bool {
        self.origin.is_finite() && self.size.is_positive_finite()
    }

    /// Converts a framework-local logical-pixel rectangle into canvas space.
    pub fn to_canvas_rect(self, transform: ViewportTransform) -> Option<CanvasRect> {
        if !self.is_positive_finite() || !transform.is_valid() {
            return None;
        }
        let rect = CanvasRect {
            origin: self.origin.to_canvas_point(transform)?,
            size: CanvasSize {
                width: self.size.width / transform.zoom,
                height: self.size.height / transform.zoom,
            },
        };
        rect.is_positive_finite().then_some(rect)
    }
}

/// Returns the midpoint of one side of a valid framework view-space rectangle.
///
/// Invalid or non-finite bounds do not produce an anchor.
#[must_use]
pub fn side_anchor(bounds: ViewRect, outward: HandlePosition) -> Option<ViewPoint> {
    if !bounds.is_positive_finite() {
        return None;
    }
    let point = match outward {
        HandlePosition::Top => {
            ViewPoint::new(bounds.origin.x + bounds.size.width * 0.5, bounds.origin.y)
        }
        HandlePosition::Right => ViewPoint::new(
            bounds.origin.x + bounds.size.width,
            bounds.origin.y + bounds.size.height * 0.5,
        ),
        HandlePosition::Bottom => ViewPoint::new(
            bounds.origin.x + bounds.size.width * 0.5,
            bounds.origin.y + bounds.size.height,
        ),
        HandlePosition::Left => {
            ViewPoint::new(bounds.origin.x, bounds.origin.y + bounds.size.height * 0.5)
        }
    };

    point.is_finite().then_some(point)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn side_anchor_returns_each_cardinal_midpoint() {
        let bounds = ViewRect::new(ViewPoint::new(10.0, 20.0), ViewSize::new(40.0, 60.0));

        assert_eq!(
            side_anchor(bounds, HandlePosition::Top),
            Some(ViewPoint::new(30.0, 20.0))
        );
        assert_eq!(
            side_anchor(bounds, HandlePosition::Right),
            Some(ViewPoint::new(50.0, 50.0))
        );
        assert_eq!(
            side_anchor(bounds, HandlePosition::Bottom),
            Some(ViewPoint::new(30.0, 80.0))
        );
        assert_eq!(
            side_anchor(bounds, HandlePosition::Left),
            Some(ViewPoint::new(10.0, 50.0))
        );
    }

    #[test]
    fn side_anchor_rejects_invalid_bounds() {
        let non_finite_origin =
            ViewRect::new(ViewPoint::new(f32::NAN, 0.0), ViewSize::new(10.0, 10.0));
        let non_finite_size =
            ViewRect::new(ViewPoint::new(0.0, 0.0), ViewSize::new(f32::INFINITY, 10.0));
        let empty = ViewRect::new(ViewPoint::new(0.0, 0.0), ViewSize::new(0.0, 10.0));

        assert_eq!(side_anchor(non_finite_origin, HandlePosition::Top), None);
        assert_eq!(side_anchor(non_finite_size, HandlePosition::Right), None);
        assert_eq!(side_anchor(empty, HandlePosition::Bottom), None);
    }
}
