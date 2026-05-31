use jellyflow_core::core::{CanvasPoint, CanvasRect};

use super::geometry::{compute_fit_view_target_top_left, compute_target_for_canvas_rect};
use super::{FitViewComputeOptions, FitViewNodeInfo};

/// Computes the viewport pan/zoom that frames the given nodes in view.
///
/// The returned pan/zoom matches the UI contract: `pan` is in canvas space (added in the render
/// transform) and `zoom` is a scalar.
pub fn compute_fit_view_target(
    nodes: &[FitViewNodeInfo],
    options: FitViewComputeOptions,
) -> Option<(CanvasPoint, f32)> {
    let options = options.normalized()?;
    if nodes.is_empty() {
        return None;
    }

    let (viewport_w, viewport_h) = (options.viewport_width_px, options.viewport_height_px);
    if !viewport_w.is_finite() || !viewport_h.is_finite() {
        return None;
    }

    let mut zoom_guess = options.max_zoom;
    let mut best: Option<(CanvasPoint, f32)> = None;

    for _ in 0..4 {
        if !zoom_guess.is_finite() || zoom_guess <= 0.0 {
            zoom_guess = 1.0;
        }
        zoom_guess = zoom_guess.clamp(options.min_zoom, options.max_zoom);

        let mut scratch: Vec<FitViewNodeInfo> = Vec::with_capacity(nodes.len());
        let ox = options.node_origin.0;
        let oy = options.node_origin.1;
        for n in nodes {
            let (w_px, h_px) = n.size_px;
            if !w_px.is_finite() || !h_px.is_finite() || w_px <= 0.0 || h_px <= 0.0 {
                continue;
            }
            let w_canvas = w_px / zoom_guess;
            let h_canvas = h_px / zoom_guess;
            scratch.push(FitViewNodeInfo {
                pos: CanvasPoint {
                    x: n.pos.x - ox * w_canvas,
                    y: n.pos.y - oy * h_canvas,
                },
                size_px: n.size_px,
            });
        }

        let Some((pan, zoom_next)) = compute_fit_view_target_top_left(&scratch, options) else {
            return best;
        };
        best = Some((pan, zoom_next));
        zoom_guess = zoom_next;
    }

    best
}

/// Computes the viewport pan/zoom that frames the given canvas-space rect in view.
pub fn compute_fit_view_target_for_canvas_rect(
    target_canvas: CanvasRect,
    options: FitViewComputeOptions,
) -> Option<(CanvasPoint, f32)> {
    compute_target_for_canvas_rect(target_canvas, options.normalized()?)
}
