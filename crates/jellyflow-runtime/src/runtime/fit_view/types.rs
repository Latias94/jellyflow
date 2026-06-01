use crate::node_origin::resolve_node_origin;
use jellyflow_core::core::{CanvasPoint, CanvasSize, NodeOrigin};

#[derive(Debug, Clone, Copy)]
pub struct FitViewNodeInfo {
    pub pos: CanvasPoint,
    /// Optional node origin override (XyFlow `node.origin`).
    ///
    /// When omitted, `FitViewComputeOptions.node_origin` is used.
    pub origin: Option<NodeOrigin>,
    /// Node size in logical px at zoom=1 (semantic zoom sizing).
    pub size_px: (f32, f32),
}

impl FitViewNodeInfo {
    pub fn new(pos: CanvasPoint, size_px: (f32, f32)) -> Self {
        Self {
            pos,
            origin: None,
            size_px,
        }
    }

    pub fn with_origin(mut self, origin: Option<NodeOrigin>) -> Self {
        self.origin = origin;
        self
    }

    pub fn pixel_size(&self) -> CanvasSize {
        let (width, height) = self.size_px;
        CanvasSize { width, height }
    }

    pub fn canvas_size_at_zoom(&self, zoom: f32) -> Option<CanvasSize> {
        if !zoom.is_finite() || zoom <= 0.0 {
            return None;
        }

        let size_px = self.pixel_size();
        if !size_px.is_positive_finite() {
            return None;
        }

        Some(CanvasSize {
            width: size_px.width / zoom,
            height: size_px.height / zoom,
        })
    }

    pub fn top_left_at_zoom(&self, node_origin: (f32, f32), zoom: f32) -> Option<CanvasPoint> {
        if !self.pos.is_finite() {
            return None;
        }

        let size_canvas = self.canvas_size_at_zoom(zoom)?;
        let (origin_x, origin_y) = resolve_node_origin(self.origin, node_origin);
        Some(CanvasPoint {
            x: self.pos.x - origin_x * size_canvas.width,
            y: self.pos.y - origin_y * size_canvas.height,
        })
    }
}
