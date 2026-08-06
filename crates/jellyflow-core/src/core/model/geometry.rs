use serde::{Deserialize, Serialize};

/// A 2D point in canvas space.
#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CanvasPoint {
    /// X coordinate.
    pub x: f32,
    /// Y coordinate.
    pub y: f32,
}

impl CanvasPoint {
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

/// A 2D size in canvas space.
#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CanvasSize {
    /// Width.
    pub width: f32,
    /// Height.
    pub height: f32,
}

impl CanvasSize {
    pub fn is_finite(self) -> bool {
        self.width.is_finite() && self.height.is_finite()
    }

    pub fn is_positive_finite(self) -> bool {
        self.is_finite() && self.width > 0.0 && self.height > 0.0
    }
}

/// A rectangle in canvas space.
#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CanvasRect {
    /// Top-left origin.
    pub origin: CanvasPoint,
    /// Size.
    pub size: CanvasSize,
}

impl CanvasRect {
    /// Returns the representable bottom-right endpoint of this rectangle.
    ///
    /// Finite components alone are insufficient because adding a large finite origin and size can
    /// overflow the `f32` canvas domain. Consumers that derive extents must cross this boundary
    /// before indexing, rendering, or hit-testing the rectangle.
    pub fn checked_max(self) -> Option<CanvasPoint> {
        if !self.origin.is_finite() || !self.size.is_finite() {
            return None;
        }
        Some(CanvasPoint {
            x: representable_f32(self.origin.x as f64 + self.size.width as f64)?,
            y: representable_f32(self.origin.y as f64 + self.size.height as f64)?,
        })
    }

    pub fn is_finite(self) -> bool {
        self.checked_max().is_some()
    }

    pub fn is_positive_finite(self) -> bool {
        self.size.is_positive_finite() && self.checked_max().is_some()
    }
}

fn representable_f32(value: f64) -> Option<f32> {
    (value.is_finite() && value >= f32::MIN as f64 && value <= f32::MAX as f64)
        .then_some(value as f32)
}

#[cfg(test)]
mod tests {
    use super::{CanvasPoint, CanvasRect, CanvasSize};

    #[test]
    fn canvas_rect_is_positive_finite_requires_finite_origin_and_positive_size() {
        let valid = CanvasRect {
            origin: CanvasPoint { x: 1.0, y: -2.0 },
            size: CanvasSize {
                width: 10.0,
                height: 20.0,
            },
        };
        assert!(valid.is_positive_finite());

        let zero_width = CanvasRect {
            size: CanvasSize {
                width: 0.0,
                height: 20.0,
            },
            ..valid
        };
        assert!(!zero_width.is_positive_finite());

        let non_finite_origin = CanvasRect {
            origin: CanvasPoint {
                x: f32::INFINITY,
                y: 0.0,
            },
            ..valid
        };
        assert!(!non_finite_origin.is_positive_finite());

        let overflowing_endpoint = CanvasRect {
            origin: CanvasPoint {
                x: f32::MAX,
                y: 0.0,
            },
            size: CanvasSize {
                width: f32::MAX,
                height: 1.0,
            },
        };
        assert!(!overflowing_endpoint.is_finite());
        assert!(!overflowing_endpoint.is_positive_finite());
        assert_eq!(overflowing_endpoint.checked_max(), None);
    }
}
