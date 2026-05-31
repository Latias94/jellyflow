use jellyflow_core::core::CanvasPoint;

#[derive(Debug, Clone, Copy)]
pub struct FitViewNodeInfo {
    pub pos: CanvasPoint,
    /// Node size in logical px at zoom=1 (semantic zoom sizing).
    pub size_px: (f32, f32),
}
