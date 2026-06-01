use crate::runtime::geometry::{CanvasBounds, ViewportFitFrame};
use jellyflow_core::core::{CanvasPoint, CanvasRect};

use super::{FitViewComputeOptions, FitViewNodeInfo};

pub(super) fn compute_fit_view_target_top_left(
    nodes: &[FitViewNodeInfo],
    options: FitViewComputeOptions,
) -> Option<(CanvasPoint, f32)> {
    let frame = frame_from_options(options);

    let spread = NodePositionSpread::from_nodes(nodes)?;
    let zoom = spread.fit_zoom(options, frame);
    let bounds = bounds_from_nodes(nodes, zoom)?;
    let center = bounds.center();

    Some((frame.pan_for_center(center, zoom), zoom))
}

pub(super) fn compute_target_for_canvas_rect(
    target_canvas: CanvasRect,
    options: FitViewComputeOptions,
) -> Option<(CanvasPoint, f32)> {
    frame_from_options(options).fit_rect(target_canvas, options.min_zoom, options.max_zoom)
}

fn frame_from_options(options: FitViewComputeOptions) -> ViewportFitFrame {
    ViewportFitFrame::from_viewport_and_padding(
        options.viewport_width_px,
        options.viewport_height_px,
        options.padding,
        options.margin_px_fallback,
    )
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

    fn fit_zoom(&self, options: FitViewComputeOptions, frame: ViewportFitFrame) -> f32 {
        let mut zoom_x = options.max_zoom;
        let mut zoom_y = options.max_zoom;

        let spread_x = (self.max_x - self.min_x).max(0.0);
        let spread_y = (self.max_y - self.min_y).max(0.0);
        if spread_x > 1.0e-3 {
            zoom_x = (frame.available_width() - self.max_w_px) / spread_x;
        }
        if spread_y > 1.0e-3 {
            zoom_y = (frame.available_height() - self.max_h_px) / spread_y;
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

fn bounds_from_nodes(nodes: &[FitViewNodeInfo], zoom: f32) -> Option<CanvasBounds> {
    let mut bounds = CanvasBounds::empty();
    for node in nodes {
        let Some(size_canvas) = node.canvas_size_at_zoom(zoom) else {
            continue;
        };
        let Some(node_bounds) = CanvasBounds::from_top_left_rect(node.pos, size_canvas) else {
            continue;
        };

        bounds.include(node_bounds);
    }
    bounds.is_valid().then_some(bounds)
}
