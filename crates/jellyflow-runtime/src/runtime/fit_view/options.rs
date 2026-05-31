use crate::node_origin::normalize_node_origin;

const MAX_FIT_VIEW_PADDING: f32 = 0.45;

#[derive(Debug, Clone, Copy)]
pub struct FitViewComputeOptions {
    /// Viewport width in logical px.
    pub viewport_width_px: f32,
    /// Viewport height in logical px.
    pub viewport_height_px: f32,
    /// Node origin (anchor) used to interpret `FitViewNodeInfo.pos`.
    ///
    /// When `(0.0, 0.0)`, `pos` is treated as the node's top-left.
    /// When `(0.5, 0.5)`, `pos` is treated as the node's center.
    pub node_origin: (f32, f32),
    /// Extra padding as a fraction of viewport size (0.0 .. 0.45 recommended).
    ///
    /// When `0.0`, `margin_px_fallback` is used instead.
    pub padding: f32,
    /// Fixed margin in logical px used when `padding == 0.0`.
    pub margin_px_fallback: f32,
    /// Minimum zoom clamp.
    pub min_zoom: f32,
    /// Maximum zoom clamp.
    pub max_zoom: f32,
}

impl FitViewComputeOptions {
    pub fn normalized(mut self) -> Option<Self> {
        if !self.viewport_width_px.is_finite()
            || !self.viewport_height_px.is_finite()
            || self.viewport_width_px <= 1.0
            || self.viewport_height_px <= 1.0
        {
            return None;
        }

        self.node_origin = normalize_node_origin(self.node_origin);

        if !self.margin_px_fallback.is_finite() || self.margin_px_fallback < 0.0 {
            self.margin_px_fallback = 0.0;
        }

        if !self.padding.is_finite() {
            self.padding = 0.0;
        }
        self.padding = self.padding.clamp(0.0, MAX_FIT_VIEW_PADDING);

        if !self.min_zoom.is_finite() || self.min_zoom <= 0.0 {
            self.min_zoom = 1.0;
        }
        if !self.max_zoom.is_finite() || self.max_zoom <= 0.0 {
            self.max_zoom = 1.0;
        }
        if self.min_zoom > self.max_zoom {
            std::mem::swap(&mut self.min_zoom, &mut self.max_zoom);
        }

        Some(self)
    }
}
