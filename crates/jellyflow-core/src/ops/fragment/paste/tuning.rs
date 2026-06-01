use crate::core::CanvasPoint;

/// Paste tuning for translating fragments into a destination graph.
#[derive(Debug, Clone, Copy)]
pub struct PasteTuning {
    /// Additional offset applied to every pasted node position.
    pub offset: CanvasPoint,
}

impl Default for PasteTuning {
    fn default() -> Self {
        Self {
            offset: CanvasPoint { x: 0.0, y: 0.0 },
        }
    }
}
