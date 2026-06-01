use jellyflow_core::core::{CanvasPoint, CanvasRect};

use super::bounds::CanvasBounds;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ViewportFitFrame {
    width_px: f32,
    height_px: f32,
    margin_x: f32,
    margin_y: f32,
}

impl ViewportFitFrame {
    pub(crate) fn from_viewport_and_padding(
        width_px: f32,
        height_px: f32,
        padding: f32,
        margin_px_fallback: f32,
    ) -> Self {
        let (margin_x, margin_y) = if padding > 0.0 {
            (width_px * padding, height_px * padding)
        } else {
            (margin_px_fallback, margin_px_fallback)
        };

        Self {
            width_px,
            height_px,
            margin_x,
            margin_y,
        }
    }

    pub(crate) fn available_width(self) -> f32 {
        self.width_px - 2.0 * self.margin_x
    }

    pub(crate) fn available_height(self) -> f32 {
        self.height_px - 2.0 * self.margin_y
    }

    pub(crate) fn pan_for_center(self, center: CanvasPoint, zoom: f32) -> CanvasPoint {
        CanvasPoint {
            x: 0.5 * self.width_px / zoom - center.x,
            y: 0.5 * self.height_px / zoom - center.y,
        }
    }

    pub(crate) fn fit_rect(
        self,
        target_canvas: CanvasRect,
        min_zoom: f32,
        max_zoom: f32,
    ) -> Option<(CanvasPoint, f32)> {
        if !target_canvas.is_positive_finite() {
            return None;
        }

        let zoom_x = self.available_width() / target_canvas.size.width;
        let zoom_y = self.available_height() / target_canvas.size.height;
        if !zoom_x.is_finite() || !zoom_y.is_finite() {
            return None;
        }

        let zoom = zoom_x.min(zoom_y).clamp(min_zoom, max_zoom);
        if !zoom.is_finite() || zoom <= 0.0 {
            return None;
        }

        let bounds = CanvasBounds::from_rect(target_canvas)?;
        let pan = self.pan_for_center(bounds.center(), zoom);
        if !pan.is_finite() {
            return None;
        }

        Some((pan, zoom))
    }
}

#[cfg(test)]
mod tests {
    use super::ViewportFitFrame;
    use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize};

    #[test]
    fn viewport_fit_frame_targets_canvas_rect() {
        let frame = ViewportFitFrame::from_viewport_and_padding(800.0, 600.0, 0.0, 24.0);
        let (pan, zoom) = frame
            .fit_rect(
                CanvasRect {
                    origin: CanvasPoint { x: 100.0, y: 50.0 },
                    size: CanvasSize {
                        width: 400.0,
                        height: 200.0,
                    },
                },
                0.1,
                4.0,
            )
            .expect("target");

        assert!((zoom - 1.88).abs() <= 1.0e-6);
        assert!((pan.x - (-87.23404)).abs() <= 1.0e-4);
        assert!((pan.y - 9.574471).abs() <= 1.0e-4);
    }
}
