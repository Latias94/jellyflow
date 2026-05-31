use jellyflow_core::core::{CanvasPoint, CanvasRect};

use super::{FitViewComputeOptions, FitViewNodeInfo};

pub(super) fn compute_fit_view_target_top_left(
    nodes: &[FitViewNodeInfo],
    options: FitViewComputeOptions,
) -> Option<(CanvasPoint, f32)> {
    let (viewport_w, viewport_h) = (options.viewport_width_px, options.viewport_height_px);
    let (margin_x, margin_y) = viewport_margins(options);

    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    let mut max_w = 0.0f32;
    let mut max_h = 0.0f32;

    for n in nodes {
        let (w, h) = n.size_px;
        if !size_is_valid(w, h) {
            continue;
        }
        min_x = min_x.min(n.pos.x);
        min_y = min_y.min(n.pos.y);
        max_x = max_x.max(n.pos.x);
        max_y = max_y.max(n.pos.y);
        max_w = max_w.max(w);
        max_h = max_h.max(h);
    }

    if !min_x.is_finite() || !min_y.is_finite() || !max_x.is_finite() || !max_y.is_finite() {
        return None;
    }

    let spread_x = (max_x - min_x).max(0.0);
    let spread_y = (max_y - min_y).max(0.0);

    let mut zoom_x = options.max_zoom;
    let mut zoom_y = options.max_zoom;
    if spread_x > 1.0e-3 {
        zoom_x = (viewport_w - max_w - 2.0 * margin_x) / spread_x;
    }
    if spread_y > 1.0e-3 {
        zoom_y = (viewport_h - max_h - 2.0 * margin_y) / spread_y;
    }

    let mut zoom = zoom_x.min(zoom_y);
    if !zoom.is_finite() {
        zoom = 1.0;
    }
    zoom = zoom.clamp(options.min_zoom, options.max_zoom);

    let mut rect_min_x = f32::INFINITY;
    let mut rect_min_y = f32::INFINITY;
    let mut rect_max_x = f32::NEG_INFINITY;
    let mut rect_max_y = f32::NEG_INFINITY;
    for n in nodes {
        let (w_px, h_px) = n.size_px;
        if !size_is_valid(w_px, h_px) {
            continue;
        }
        let w = w_px / zoom;
        let h = h_px / zoom;
        rect_min_x = rect_min_x.min(n.pos.x);
        rect_min_y = rect_min_y.min(n.pos.y);
        rect_max_x = rect_max_x.max(n.pos.x + w);
        rect_max_y = rect_max_y.max(n.pos.y + h);
    }

    if !rect_min_x.is_finite()
        || !rect_min_y.is_finite()
        || !rect_max_x.is_finite()
        || !rect_max_y.is_finite()
    {
        return None;
    }

    let center_x = 0.5 * (rect_min_x + rect_max_x);
    let center_y = 0.5 * (rect_min_y + rect_max_y);

    let viewport_w_canvas = viewport_w / zoom;
    let viewport_h_canvas = viewport_h / zoom;
    let target_center_x = 0.5 * viewport_w_canvas;
    let target_center_y = 0.5 * viewport_h_canvas;

    let pan = CanvasPoint {
        x: target_center_x - center_x,
        y: target_center_y - center_y,
    };

    Some((pan, zoom))
}

pub(super) fn compute_target_for_canvas_rect(
    target_canvas: CanvasRect,
    options: FitViewComputeOptions,
) -> Option<(CanvasPoint, f32)> {
    if !canvas_rect_is_valid(target_canvas) {
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
    if !pan.x.is_finite() || !pan.y.is_finite() {
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

fn canvas_rect_is_valid(rect: CanvasRect) -> bool {
    rect.size.width.is_finite()
        && rect.size.height.is_finite()
        && rect.size.width > 0.0
        && rect.size.height > 0.0
        && rect.origin.x.is_finite()
        && rect.origin.y.is_finite()
}

fn size_is_valid(width: f32, height: f32) -> bool {
    width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0
}
