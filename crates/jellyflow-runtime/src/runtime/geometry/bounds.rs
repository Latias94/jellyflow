use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CanvasBounds {
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
}

impl CanvasBounds {
    pub(crate) fn from_rect(rect: CanvasRect) -> Option<Self> {
        if !rect.origin.is_finite() || !rect.size.is_finite() {
            return None;
        }
        let width = rect.size.width.max(0.0);
        let height = rect.size.height.max(0.0);
        let normalized = CanvasRect {
            origin: rect.origin,
            size: CanvasSize { width, height },
        };
        let max = normalized.checked_max()?;
        Some(Self {
            min_x: rect.origin.x,
            min_y: rect.origin.y,
            max_x: max.x,
            max_y: max.y,
        })
    }

    pub(crate) fn to_rect(self) -> CanvasRect {
        CanvasRect {
            origin: CanvasPoint {
                x: self.min_x,
                y: self.min_y,
            },
            size: CanvasSize {
                width: (self.max_x - self.min_x).max(0.0),
                height: (self.max_y - self.min_y).max(0.0),
            },
        }
    }

    pub(crate) fn intersects(self, other: Self) -> bool {
        self.min_x < other.max_x
            && self.max_x > other.min_x
            && self.min_y < other.max_y
            && self.max_y > other.min_y
    }

    pub(crate) fn is_valid(self) -> bool {
        self.min_x.is_finite()
            && self.min_y.is_finite()
            && self.max_x.is_finite()
            && self.max_y.is_finite()
            && self.min_x <= self.max_x
            && self.min_y <= self.max_y
    }
}

#[cfg(test)]
mod tests {
    use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize};

    use super::CanvasBounds;

    #[test]
    fn from_rect_rejects_non_finite_components_before_normalizing_negative_sizes() {
        let finite = CanvasRect {
            origin: CanvasPoint { x: 1.0, y: 2.0 },
            size: CanvasSize {
                width: -3.0,
                height: -4.0,
            },
        };
        assert_eq!(
            CanvasBounds::from_rect(finite).map(CanvasBounds::to_rect),
            Some(CanvasRect {
                origin: finite.origin,
                size: CanvasSize {
                    width: 0.0,
                    height: 0.0,
                },
            })
        );

        for non_finite in [
            CanvasRect {
                origin: CanvasPoint {
                    x: f32::NAN,
                    y: 2.0,
                },
                ..finite
            },
            CanvasRect {
                size: CanvasSize {
                    width: f32::NAN,
                    height: 4.0,
                },
                ..finite
            },
            CanvasRect {
                size: CanvasSize {
                    width: 3.0,
                    height: f32::INFINITY,
                },
                ..finite
            },
        ] {
            assert_eq!(CanvasBounds::from_rect(non_finite), None);
        }
    }
}
