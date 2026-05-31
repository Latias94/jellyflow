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
    pub fn is_finite(self) -> bool {
        self.origin.is_finite() && self.size.is_finite()
    }
}
