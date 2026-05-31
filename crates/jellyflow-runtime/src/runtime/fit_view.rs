//! Fit-view helpers (XyFlow-style viewport framing).
//!
//! This module is intentionally headless-safe and does not depend on `fret-ui`.

mod compute;
mod geometry;
mod options;
mod types;

pub use compute::{compute_fit_view_target, compute_fit_view_target_for_canvas_rect};
pub use options::FitViewComputeOptions;
pub use types::FitViewNodeInfo;

#[cfg(test)]
mod tests {
    use super::{
        FitViewComputeOptions, FitViewNodeInfo, compute_fit_view_target,
        compute_fit_view_target_for_canvas_rect,
    };
    use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize};

    #[test]
    fn compute_fit_view_target_returns_valid_viewport() {
        let nodes = [
            FitViewNodeInfo {
                pos: CanvasPoint { x: 0.0, y: 0.0 },
                size_px: (200.0, 100.0),
            },
            FitViewNodeInfo {
                pos: CanvasPoint { x: 1000.0, y: 0.0 },
                size_px: (200.0, 100.0),
            },
        ];

        let (pan, zoom) = compute_fit_view_target(
            &nodes,
            FitViewComputeOptions {
                viewport_width_px: 800.0,
                viewport_height_px: 600.0,
                node_origin: (0.0, 0.0),
                padding: 0.1,
                margin_px_fallback: 48.0,
                min_zoom: 0.1,
                max_zoom: 4.0,
            },
        )
        .expect("target");

        assert!(pan.x.is_finite() && pan.y.is_finite());
        assert!(zoom.is_finite() && zoom > 0.0);
    }

    #[test]
    fn compute_fit_view_target_for_canvas_rect_returns_valid_viewport() {
        let (pan, zoom) = compute_fit_view_target_for_canvas_rect(
            CanvasRect {
                origin: CanvasPoint { x: 100.0, y: 50.0 },
                size: CanvasSize {
                    width: 400.0,
                    height: 200.0,
                },
            },
            FitViewComputeOptions {
                viewport_width_px: 800.0,
                viewport_height_px: 600.0,
                node_origin: (0.0, 0.0),
                padding: 0.0,
                margin_px_fallback: 24.0,
                min_zoom: 0.1,
                max_zoom: 4.0,
            },
        )
        .expect("target");

        assert!((zoom - 1.88).abs() <= 1.0e-6);
        assert!((pan.x - (-87.23404)).abs() <= 1.0e-4);
        assert!((pan.y - (9.574471)).abs() <= 1.0e-4);
    }
}
