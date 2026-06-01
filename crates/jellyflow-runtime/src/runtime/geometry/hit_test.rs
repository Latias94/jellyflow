use jellyflow_core::core::CanvasPoint;

use super::paths::{EdgePath, PathCommand};

/// Options for numeric edge hit testing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeHitTestOptions {
    pub interaction_width: f32,
    pub curve_samples: usize,
}

impl EdgeHitTestOptions {
    pub fn new(interaction_width: f32, curve_samples: usize) -> Self {
        let width = if interaction_width.is_finite() && interaction_width > 0.0 {
            interaction_width
        } else {
            1.0
        };
        let samples = curve_samples.max(1);
        Self {
            interaction_width: width,
            curve_samples: samples,
        }
    }

    fn radius(self) -> f32 {
        self.interaction_width * 0.5
    }
}

impl Default for EdgeHitTestOptions {
    fn default() -> Self {
        Self {
            interaction_width: 12.0,
            curve_samples: 24,
        }
    }
}

pub fn edge_path_contains_point(
    path: &EdgePath,
    point: CanvasPoint,
    options: EdgeHitTestOptions,
) -> bool {
    edge_path_distance(path, point, options).is_some_and(|distance| distance <= options.radius())
}

pub fn edge_path_distance(
    path: &EdgePath,
    point: CanvasPoint,
    options: EdgeHitTestOptions,
) -> Option<f32> {
    if !point.is_finite() {
        return None;
    }

    let mut current: Option<CanvasPoint> = None;
    let mut min_distance = f32::INFINITY;
    let samples = options.curve_samples.max(1);

    for command in &path.commands {
        match *command {
            PathCommand::MoveTo(to) => {
                if !to.is_finite() {
                    return None;
                }
                current = Some(to);
            }
            PathCommand::LineTo(to) => {
                let from = current?;
                if !to.is_finite() {
                    return None;
                }
                min_distance = min_distance.min(distance_to_segment(point, from, to)?);
                current = Some(to);
            }
            PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => {
                let from = current?;
                if !control1.is_finite() || !control2.is_finite() || !to.is_finite() {
                    return None;
                }
                let mut prev = from;
                for step in 1..=samples {
                    let t = step as f32 / samples as f32;
                    let next = cubic_point(from, control1, control2, to, t);
                    min_distance = min_distance.min(distance_to_segment(point, prev, next)?);
                    prev = next;
                }
                current = Some(to);
            }
        }
    }

    min_distance.is_finite().then_some(min_distance)
}

fn distance_to_segment(point: CanvasPoint, a: CanvasPoint, b: CanvasPoint) -> Option<f32> {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len_sq = dx * dx + dy * dy;
    if !len_sq.is_finite() {
        return None;
    }
    if len_sq <= f32::EPSILON {
        return distance(point, a);
    }

    let t = (((point.x - a.x) * dx + (point.y - a.y) * dy) / len_sq).clamp(0.0, 1.0);
    distance(
        point,
        CanvasPoint {
            x: a.x + t * dx,
            y: a.y + t * dy,
        },
    )
}

fn distance(a: CanvasPoint, b: CanvasPoint) -> Option<f32> {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let distance = (dx * dx + dy * dy).sqrt();
    distance.is_finite().then_some(distance)
}

fn cubic_point(
    start: CanvasPoint,
    control1: CanvasPoint,
    control2: CanvasPoint,
    end: CanvasPoint,
    t: f32,
) -> CanvasPoint {
    let mt = 1.0 - t;
    let mt2 = mt * mt;
    let t2 = t * t;
    CanvasPoint {
        x: start.x * mt2 * mt
            + control1.x * 3.0 * mt2 * t
            + control2.x * 3.0 * mt * t2
            + end.x * t2 * t,
        y: start.y * mt2 * mt
            + control1.y * 3.0 * mt2 * t
            + control2.y * 3.0 * mt * t2
            + end.y * t2 * t,
    }
}

#[cfg(test)]
mod tests {
    use super::{EdgeHitTestOptions, edge_path_contains_point, edge_path_distance};
    use crate::io::NodeGraphInteractionState;
    use crate::runtime::geometry::{
        BezierEdgeOptions, EdgeEndpointPosition, HandlePosition, bezier_edge_path,
        straight_edge_path,
    };
    use jellyflow_core::core::CanvasPoint;

    #[test]
    fn hit_test_uses_edge_path_distance_for_straight_and_bezier_paths() {
        let source = EdgeEndpointPosition {
            point: CanvasPoint { x: 0.0, y: 0.0 },
            position: HandlePosition::Right,
        };
        let target = EdgeEndpointPosition {
            point: CanvasPoint { x: 100.0, y: 0.0 },
            position: HandlePosition::Left,
        };
        let straight = straight_edge_path(source, target).expect("straight path");
        let options = EdgeHitTestOptions::new(12.0, 24);

        assert!(edge_path_contains_point(
            &straight,
            CanvasPoint { x: 50.0, y: 5.0 },
            options
        ));
        assert!(!edge_path_contains_point(
            &straight,
            CanvasPoint { x: 50.0, y: 7.0 },
            options
        ));

        let bezier =
            bezier_edge_path(source, target, BezierEdgeOptions::default()).expect("bezier path");
        let distance = edge_path_distance(&bezier, CanvasPoint { x: 50.0, y: 0.0 }, options)
            .expect("distance");
        assert!(distance <= 1.0e-3);
    }

    #[test]
    fn hit_test_options_are_derived_from_interaction_state() {
        let mut state = NodeGraphInteractionState::default();
        state.edge_interaction_width = 18.0;
        state.bezier_hit_test_steps = 32;

        let options = state.edge_hit_test_options();
        assert!((options.interaction_width - 18.0).abs() <= 1.0e-6);
        assert_eq!(options.curve_samples, 32);
    }
}
