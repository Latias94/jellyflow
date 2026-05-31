use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct CanvasBounds {
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
}

impl CanvasBounds {
    pub(super) fn from_rect(rect: CanvasRect) -> Self {
        let width = rect.size.width.max(0.0);
        let height = rect.size.height.max(0.0);
        Self {
            min_x: rect.origin.x,
            min_y: rect.origin.y,
            max_x: rect.origin.x + width,
            max_y: rect.origin.y + height,
        }
    }

    pub(super) fn from_node(
        pos: CanvasPoint,
        size: CanvasSize,
        node_origin: (f32, f32),
    ) -> Option<Self> {
        let width = size.width;
        let height = size.height;
        if !size.is_positive_finite() {
            return None;
        }
        if !pos.is_finite() {
            return None;
        }

        let (origin_x, origin_y) = node_origin;
        let min_x = pos.x - origin_x * width;
        let min_y = pos.y - origin_y * height;
        Some(Self {
            min_x,
            min_y,
            max_x: min_x + width,
            max_y: min_y + height,
        })
    }

    pub(super) fn top_left(self) -> CanvasPoint {
        CanvasPoint {
            x: self.min_x,
            y: self.min_y,
        }
    }

    pub(super) fn to_rect(self) -> CanvasRect {
        CanvasRect {
            origin: self.top_left(),
            size: CanvasSize {
                width: (self.max_x - self.min_x).max(0.0),
                height: (self.max_y - self.min_y).max(0.0),
            },
        }
    }

    pub(super) fn union(self, other: Self) -> Self {
        Self {
            min_x: self.min_x.min(other.min_x),
            min_y: self.min_y.min(other.min_y),
            max_x: self.max_x.max(other.max_x),
            max_y: self.max_y.max(other.max_y),
        }
    }

    pub(super) fn intersects(self, other: Self) -> bool {
        self.min_x < other.max_x
            && self.max_x > other.min_x
            && self.min_y < other.max_y
            && self.max_y > other.min_y
    }

    pub(super) fn contains(self, other: Self) -> bool {
        other.min_x >= self.min_x
            && other.min_y >= self.min_y
            && other.max_x <= self.max_x
            && other.max_y <= self.max_y
    }
}
