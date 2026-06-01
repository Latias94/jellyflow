use crate::node_origin::normalize_node_origin;
use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CanvasBounds {
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
}

impl CanvasBounds {
    pub(crate) fn empty() -> Self {
        Self {
            min_x: f32::INFINITY,
            min_y: f32::INFINITY,
            max_x: f32::NEG_INFINITY,
            max_y: f32::NEG_INFINITY,
        }
    }

    pub(crate) fn from_rect(rect: CanvasRect) -> Option<Self> {
        if !rect.is_finite() {
            return None;
        }

        let width = rect.size.width.max(0.0);
        let height = rect.size.height.max(0.0);
        Some(Self {
            min_x: rect.origin.x,
            min_y: rect.origin.y,
            max_x: rect.origin.x + width,
            max_y: rect.origin.y + height,
        })
    }

    pub(crate) fn from_top_left_rect(pos: CanvasPoint, size: CanvasSize) -> Option<Self> {
        if !pos.is_finite() || !size.is_positive_finite() {
            return None;
        }

        Some(Self {
            min_x: pos.x,
            min_y: pos.y,
            max_x: pos.x + size.width,
            max_y: pos.y + size.height,
        })
    }

    pub(crate) fn from_node(
        pos: CanvasPoint,
        size: CanvasSize,
        node_origin: (f32, f32),
    ) -> Option<Self> {
        let top_left = top_left_from_node_origin(pos, size, node_origin)?;
        Self::from_top_left_rect(top_left, size)
    }

    pub(crate) fn top_left(self) -> CanvasPoint {
        CanvasPoint {
            x: self.min_x,
            y: self.min_y,
        }
    }

    pub(crate) fn center(self) -> CanvasPoint {
        CanvasPoint {
            x: 0.5 * (self.min_x + self.max_x),
            y: 0.5 * (self.min_y + self.max_y),
        }
    }

    pub(crate) fn to_rect(self) -> CanvasRect {
        CanvasRect {
            origin: self.top_left(),
            size: CanvasSize {
                width: (self.max_x - self.min_x).max(0.0),
                height: (self.max_y - self.min_y).max(0.0),
            },
        }
    }

    pub(crate) fn include(&mut self, other: Self) {
        self.min_x = self.min_x.min(other.min_x);
        self.min_y = self.min_y.min(other.min_y);
        self.max_x = self.max_x.max(other.max_x);
        self.max_y = self.max_y.max(other.max_y);
    }

    pub(crate) fn union(mut self, other: Self) -> Self {
        self.include(other);
        self
    }

    pub(crate) fn intersects(self, other: Self) -> bool {
        self.min_x < other.max_x
            && self.max_x > other.min_x
            && self.min_y < other.max_y
            && self.max_y > other.min_y
    }

    pub(crate) fn contains(self, other: Self) -> bool {
        other.min_x >= self.min_x
            && other.min_y >= self.min_y
            && other.max_x <= self.max_x
            && other.max_y <= self.max_y
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

fn top_left_from_node_origin(
    pos: CanvasPoint,
    size: CanvasSize,
    node_origin: (f32, f32),
) -> Option<CanvasPoint> {
    if !pos.is_finite() || !size.is_positive_finite() {
        return None;
    }

    let (origin_x, origin_y) = normalize_node_origin(node_origin);
    Some(CanvasPoint {
        x: pos.x - origin_x * size.width,
        y: pos.y - origin_y * size.height,
    })
}

#[cfg(test)]
mod tests {
    use super::{CanvasBounds, top_left_from_node_origin};
    use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize};

    #[test]
    fn node_origin_projection_returns_top_left_and_rejects_invalid_geometry() {
        let top_left = top_left_from_node_origin(
            CanvasPoint { x: 20.0, y: 10.0 },
            CanvasSize {
                width: 10.0,
                height: 6.0,
            },
            (0.5, 0.5),
        )
        .expect("top left");

        assert!((top_left.x - 15.0).abs() <= 1.0e-6);
        assert!((top_left.y - 7.0).abs() <= 1.0e-6);

        assert!(
            top_left_from_node_origin(
                CanvasPoint {
                    x: f32::INFINITY,
                    y: 0.0
                },
                CanvasSize {
                    width: 10.0,
                    height: 6.0,
                },
                (0.0, 0.0),
            )
            .is_none()
        );
    }

    #[test]
    fn canvas_bounds_unions_intersects_and_contains_rects() {
        let a = CanvasBounds::from_top_left_rect(
            CanvasPoint { x: 0.0, y: 0.0 },
            CanvasSize {
                width: 10.0,
                height: 10.0,
            },
        )
        .expect("bounds");
        let b = CanvasBounds::from_top_left_rect(
            CanvasPoint { x: 9.0, y: 9.0 },
            CanvasSize {
                width: 5.0,
                height: 5.0,
            },
        )
        .expect("bounds");
        let query = CanvasBounds::from_rect(CanvasRect {
            origin: CanvasPoint { x: 0.0, y: 0.0 },
            size: CanvasSize {
                width: 10.0,
                height: 10.0,
            },
        })
        .expect("query");

        assert!(query.contains(a));
        assert!(query.intersects(b));
        assert!(!query.contains(b));

        let union = a.union(b).to_rect();
        assert!((union.origin.x - 0.0).abs() <= 1.0e-6);
        assert!((union.origin.y - 0.0).abs() <= 1.0e-6);
        assert!((union.size.width - 14.0).abs() <= 1.0e-6);
        assert!((union.size.height - 14.0).abs() <= 1.0e-6);
    }
}
