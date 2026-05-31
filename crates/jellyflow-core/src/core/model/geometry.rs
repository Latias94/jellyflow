use serde::{Deserialize, Serialize};

/// A 2D point in canvas space.
#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CanvasPoint {
    /// X coordinate.
    pub x: f32,
    /// Y coordinate.
    pub y: f32,
}

/// A 2D size in canvas space.
#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CanvasSize {
    /// Width.
    pub width: f32,
    /// Height.
    pub height: f32,
}

/// A rectangle in canvas space.
#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CanvasRect {
    /// Top-left origin.
    pub origin: CanvasPoint,
    /// Size.
    pub size: CanvasSize,
}
