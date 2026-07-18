use std::cmp::Ordering;

use jellyflow_core::core::{CanvasPoint, CanvasRect, NodeId};

use crate::runtime::adapter_protocol::{GeometrySnapshot, ViewSize};
use crate::runtime::viewport::ViewportTransform;

use super::AlignmentGuide;

/// Bounded alignment result for one node-drag presentation update.
#[derive(Debug, Clone, PartialEq)]
pub struct AlignmentSnapResult {
    delta: CanvasPoint,
    guides: Box<[AlignmentGuide]>,
    candidates_examined: usize,
}

impl AlignmentSnapResult {
    fn unchanged(delta: CanvasPoint) -> Self {
        Self {
            delta,
            guides: Box::default(),
            candidates_examined: 0,
        }
    }

    /// Returns the canvas-space delta after alignment snapping.
    pub fn delta(&self) -> CanvasPoint {
        self.delta
    }

    /// Returns the deterministic alignment guides associated with the snapped axes.
    pub fn guides(&self) -> &[AlignmentGuide] {
        &self.guides
    }

    /// Returns the number of spatially admitted, non-moving nodes inspected.
    pub fn candidates_examined(&self) -> usize {
        self.candidates_examined
    }
}

#[derive(Debug, Clone, Copy)]
struct AxisSnap {
    distance: f32,
    adjustment: f32,
    coordinate: f32,
    moving: NodeId,
    candidate: NodeId,
    moving_feature: usize,
    candidate_feature: usize,
}

impl AxisSnap {
    fn is_better_than(self, current: Self) -> bool {
        self.distance
            .total_cmp(&current.distance)
            .then_with(|| self.candidate.cmp(&current.candidate))
            .then_with(|| self.moving.cmp(&current.moving))
            .then_with(|| self.candidate_feature.cmp(&current.candidate_feature))
            .then_with(|| self.moving_feature.cmp(&current.moving_feature))
            == Ordering::Less
    }
}

/// Resolves deterministic axis alignment against an adapter-supplied spatial candidate set.
///
/// `tolerance_in_canvas` must already be converted from the adapter's screen-pixel setting through
/// the current viewport zoom. The function never scans graph storage: callers decide which
/// spatially admitted candidates are eligible for this frame.
pub fn resolve_alignment_snap(
    geometry: &GeometrySnapshot,
    moving_nodes: &[NodeId],
    candidate_nodes: &[NodeId],
    delta: CanvasPoint,
    tolerance_in_canvas: f32,
) -> AlignmentSnapResult {
    if !delta.is_finite()
        || !tolerance_in_canvas.is_finite()
        || tolerance_in_canvas <= 0.0
        || moving_nodes.is_empty()
    {
        return AlignmentSnapResult::unchanged(delta);
    }

    let mut x_snap = None;
    let mut y_snap = None;
    let mut candidates_examined = 0_usize;
    for candidate in candidate_nodes.iter().copied() {
        if moving_nodes.contains(&candidate) {
            continue;
        }
        let Some(candidate_geometry) = geometry.node(candidate).filter(|node| node.is_fresh())
        else {
            continue;
        };
        candidates_examined = candidates_examined.saturating_add(1);
        let candidate_bounds = candidate_geometry.bounds();
        let candidate_x = axis_features(candidate_bounds.origin.x, candidate_bounds.size.width);
        let candidate_y = axis_features(candidate_bounds.origin.y, candidate_bounds.size.height);

        for moving in moving_nodes.iter().copied() {
            let Some(moving_geometry) = geometry.node(moving).filter(|node| node.is_fresh()) else {
                continue;
            };
            let moving_bounds = moving_geometry.bounds();
            let moving_x =
                axis_features(moving_bounds.origin.x + delta.x, moving_bounds.size.width);
            let moving_y =
                axis_features(moving_bounds.origin.y + delta.y, moving_bounds.size.height);
            x_snap = best_axis_snap(
                x_snap,
                moving,
                candidate,
                moving_x,
                candidate_x,
                tolerance_in_canvas,
            );
            y_snap = best_axis_snap(
                y_snap,
                moving,
                candidate,
                moving_y,
                candidate_y,
                tolerance_in_canvas,
            );
        }
    }

    let mut guides = Vec::with_capacity(2);
    let mut snapped = delta;
    if let Some(x) = x_snap {
        snapped.x += x.adjustment;
        guides.push(AlignmentGuide::Vertical {
            canvas_x: x.coordinate,
        });
    }
    if let Some(y) = y_snap {
        snapped.y += y.adjustment;
        guides.push(AlignmentGuide::Horizontal {
            canvas_y: y.coordinate,
        });
    }
    AlignmentSnapResult {
        delta: snapped,
        guides: guides.into_boxed_slice(),
        candidates_examined,
    }
}

/// Fits one positive canvas rectangle into a native viewport with screen-pixel padding.
pub fn fit_viewport(
    bounds: CanvasRect,
    viewport: ViewSize,
    padding_in_view: f32,
    min_zoom: f32,
    max_zoom: f32,
) -> Option<ViewportTransform> {
    if !bounds.is_positive_finite()
        || !viewport.is_positive_finite()
        || !padding_in_view.is_finite()
        || padding_in_view < 0.0
        || !min_zoom.is_finite()
        || min_zoom <= 0.0
        || !max_zoom.is_finite()
        || max_zoom <= 0.0
    {
        return None;
    }
    let available_width = viewport.width - padding_in_view * 2.0;
    let available_height = viewport.height - padding_in_view * 2.0;
    if available_width <= 0.0 || available_height <= 0.0 {
        return None;
    }
    let (minimum, maximum) = if min_zoom <= max_zoom {
        (min_zoom, max_zoom)
    } else {
        (max_zoom, min_zoom)
    };
    let zoom = (available_width / bounds.size.width)
        .min(available_height / bounds.size.height)
        .clamp(minimum, maximum);
    let center_x = bounds.origin.x + bounds.size.width * 0.5;
    let center_y = bounds.origin.y + bounds.size.height * 0.5;
    ViewportTransform::new(
        CanvasPoint {
            x: viewport.width * 0.5 / zoom - center_x,
            y: viewport.height * 0.5 / zoom - center_y,
        },
        zoom,
    )
}

fn axis_features(origin: f32, extent: f32) -> [f32; 3] {
    [origin, origin + extent * 0.5, origin + extent]
}

fn best_axis_snap(
    mut best: Option<AxisSnap>,
    moving: NodeId,
    candidate: NodeId,
    moving_features: [f32; 3],
    candidate_features: [f32; 3],
    tolerance: f32,
) -> Option<AxisSnap> {
    for (moving_feature, moving_value) in moving_features.into_iter().enumerate() {
        for (candidate_feature, candidate_value) in candidate_features.into_iter().enumerate() {
            let adjustment = candidate_value - moving_value;
            let distance = adjustment.abs();
            if distance > tolerance {
                continue;
            }
            let incoming = AxisSnap {
                distance,
                adjustment,
                coordinate: candidate_value,
                moving,
                candidate,
                moving_feature,
                candidate_feature,
            };
            if best.is_none_or(|current| incoming.is_better_than(current)) {
                best = Some(incoming);
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_viewport_centers_bounds_with_screen_pixel_padding() {
        let bounds = CanvasRect {
            origin: CanvasPoint { x: 40.0, y: 80.0 },
            size: jellyflow_core::core::CanvasSize {
                width: 500.0,
                height: 100.0,
            },
        };
        let transform = fit_viewport(bounds, ViewSize::new(800.0, 600.0), 48.0, 0.1, 4.0)
            .expect("valid fit transform");
        let top_left = transform
            .screen_point_for_canvas(bounds.origin)
            .expect("project fit origin");
        let bottom_right = transform
            .screen_point_for_canvas(CanvasPoint { x: 540.0, y: 180.0 })
            .expect("project fit extent");

        assert!((top_left.x - 48.0).abs() < 0.001);
        assert!((bottom_right.x - 752.0).abs() < 0.001);
        assert!(((top_left.y + bottom_right.y) * 0.5 - 300.0).abs() < 0.001);
    }

    #[test]
    fn fit_viewport_rejects_empty_available_space_and_clamps_zoom() {
        let bounds = CanvasRect {
            origin: CanvasPoint::default(),
            size: jellyflow_core::core::CanvasSize {
                width: 10.0,
                height: 10.0,
            },
        };
        assert!(fit_viewport(bounds, ViewSize::new(80.0, 80.0), 40.0, 0.1, 4.0,).is_none());
        assert_eq!(
            fit_viewport(bounds, ViewSize::new(800.0, 600.0), 48.0, 0.1, 2.0,)
                .expect("clamped fit")
                .zoom,
            2.0,
        );
    }
}
