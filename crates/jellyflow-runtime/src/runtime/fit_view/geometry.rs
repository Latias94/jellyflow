use jellyflow_core::core::{CanvasPoint, CanvasRect};

use super::{FitViewComputeOptions, FitViewNodeInfo};

pub(super) fn compute_fit_view_target_top_left(
    nodes: &[FitViewNodeInfo],
    options: FitViewComputeOptions,
) -> Option<(CanvasPoint, f32)> {
    let (viewport_w, viewport_h) = (options.viewport_width_px, options.viewport_height_px);
    let (margin_x, margin_y) = viewport_margins(options);

    let spread = NodePositionSpread::from_nodes(nodes)?;
    let zoom = spread.fit_zoom(options, margin_x, margin_y);
    let bounds = CanvasBounds::from_nodes(nodes, zoom)?;
    let center = bounds.center();

    let pan = CanvasPoint {
        x: 0.5 * viewport_w / zoom - center.x,
        y: 0.5 * viewport_h / zoom - center.y,
    };

    Some((pan, zoom))
}

pub(super) fn compute_target_for_canvas_rect(
    target_canvas: CanvasRect,
    options: FitViewComputeOptions,
) -> Option<(CanvasPoint, f32)> {
    if !target_canvas.is_positive_finite() {
        return None;
    }

    let (viewport_w, viewport_h) = (options.viewport_width_px, options.viewport_height_px);
    let (margin_x, margin_y) = viewport_margins(options);

    let zoom_x = (viewport_w - 2.0 * margin_x) / target_canvas.size.width;
    let zoom_y = (viewport_h - 2.0 * margin_y) / target_canvas.size.height;
    if !zoom_x.is_finite() || !zoom_y.is_finite() {
        return None;
    }

    let zoom = zoom_x.min(zoom_y).clamp(options.min_zoom, options.max_zoom);
    if !zoom.is_finite() || zoom <= 0.0 {
        return None;
    }

    let center_x = target_canvas.origin.x + 0.5 * target_canvas.size.width;
    let center_y = target_canvas.origin.y + 0.5 * target_canvas.size.height;
    let pan = CanvasPoint {
        x: 0.5 * viewport_w / zoom - center_x,
        y: 0.5 * viewport_h / zoom - center_y,
    };
    if !pan.is_finite() {
        return None;
    }

    Some((pan, zoom))
}

fn viewport_margins(options: FitViewComputeOptions) -> (f32, f32) {
    if options.padding > 0.0 {
        (
            options.viewport_width_px * options.padding,
            options.viewport_height_px * options.padding,
        )
    } else {
        (options.margin_px_fallback, options.margin_px_fallback)
    }
}

struct NodePositionSpread {
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
    max_w_px: f32,
    max_h_px: f32,
}

impl NodePositionSpread {
    fn new() -> Self {
        Self {
            min_x: f32::INFINITY,
            min_y: f32::INFINITY,
            max_x: f32::NEG_INFINITY,
            max_y: f32::NEG_INFINITY,
            max_w_px: 0.0,
            max_h_px: 0.0,
        }
    }

    fn from_nodes(nodes: &[FitViewNodeInfo]) -> Option<Self> {
        let mut spread = Self::new();
        for node in nodes {
            spread.include(node);
        }
        spread.is_valid().then_some(spread)
    }

    fn include(&mut self, node: &FitViewNodeInfo) {
        let size_px = node.pixel_size();
        if !size_px.is_positive_finite() {
            return;
        }

        self.min_x = self.min_x.min(node.pos.x);
        self.min_y = self.min_y.min(node.pos.y);
        self.max_x = self.max_x.max(node.pos.x);
        self.max_y = self.max_y.max(node.pos.y);
        self.max_w_px = self.max_w_px.max(size_px.width);
        self.max_h_px = self.max_h_px.max(size_px.height);
    }

    fn fit_zoom(&self, options: FitViewComputeOptions, margin_x: f32, margin_y: f32) -> f32 {
        let mut zoom_x = options.max_zoom;
        let mut zoom_y = options.max_zoom;

        let spread_x = (self.max_x - self.min_x).max(0.0);
        let spread_y = (self.max_y - self.min_y).max(0.0);
        if spread_x > 1.0e-3 {
            zoom_x = (options.viewport_width_px - self.max_w_px - 2.0 * margin_x) / spread_x;
        }
        if spread_y > 1.0e-3 {
            zoom_y = (options.viewport_height_px - self.max_h_px - 2.0 * margin_y) / spread_y;
        }

        let mut zoom = zoom_x.min(zoom_y);
        if !zoom.is_finite() {
            zoom = 1.0;
        }
        zoom.clamp(options.min_zoom, options.max_zoom)
    }

    fn is_valid(&self) -> bool {
        self.min_x.is_finite()
            && self.min_y.is_finite()
            && self.max_x.is_finite()
            && self.max_y.is_finite()
    }
}

struct CanvasBounds {
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
}

impl CanvasBounds {
    fn new() -> Self {
        Self {
            min_x: f32::INFINITY,
            min_y: f32::INFINITY,
            max_x: f32::NEG_INFINITY,
            max_y: f32::NEG_INFINITY,
        }
    }

    fn from_nodes(nodes: &[FitViewNodeInfo], zoom: f32) -> Option<Self> {
        let mut bounds = Self::new();
        for node in nodes {
            bounds.include(node, zoom);
        }
        bounds.is_valid().then_some(bounds)
    }

    fn include(&mut self, node: &FitViewNodeInfo, zoom: f32) {
        let Some(size_canvas) = node.canvas_size_at_zoom(zoom) else {
            return;
        };

        self.min_x = self.min_x.min(node.pos.x);
        self.min_y = self.min_y.min(node.pos.y);
        self.max_x = self.max_x.max(node.pos.x + size_canvas.width);
        self.max_y = self.max_y.max(node.pos.y + size_canvas.height);
    }

    fn center(&self) -> CanvasPoint {
        CanvasPoint {
            x: 0.5 * (self.min_x + self.max_x),
            y: 0.5 * (self.min_y + self.max_y),
        }
    }

    fn is_valid(&self) -> bool {
        self.min_x.is_finite()
            && self.min_y.is_finite()
            && self.max_x.is_finite()
            && self.max_y.is_finite()
    }
}
