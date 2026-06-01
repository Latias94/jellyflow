use jellyflow_core::core::CanvasPoint;

use super::super::endpoints::{EdgeEndpointPosition, HandlePosition};
use super::types::{EdgePath, EdgePathLabel, PathCommand};

/// Smoothstep-like edge options.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SmoothStepEdgeOptions {
    pub offset: f32,
    pub step_position: f32,
}

impl Default for SmoothStepEdgeOptions {
    fn default() -> Self {
        Self {
            offset: 20.0,
            step_position: 0.5,
        }
    }
}

pub fn smoothstep_edge_path(
    source: EdgeEndpointPosition,
    target: EdgeEndpointPosition,
    options: SmoothStepEdgeOptions,
) -> Option<EdgePath> {
    if !source.point.is_finite() || !target.point.is_finite() {
        return None;
    }

    let options = normalized_smoothstep_options(options);
    let source_dir = handle_direction(source.position);
    let target_dir = handle_direction(target.position);
    let source_gap = translate_point(source.point, source_dir, options.offset)?;
    let target_gap = translate_point(target.point, target_dir, options.offset)?;

    let mut points = Vec::with_capacity(6);
    push_distinct_point(&mut points, source.point);
    push_distinct_point(&mut points, source_gap);

    let horizontal = matches!(
        source.position,
        HandlePosition::Left | HandlePosition::Right
    );
    let label_point = if horizontal {
        let center_x = source_gap.x + (target_gap.x - source_gap.x) * options.step_position;
        push_distinct_point(
            &mut points,
            CanvasPoint {
                x: center_x,
                y: source_gap.y,
            },
        );
        push_distinct_point(
            &mut points,
            CanvasPoint {
                x: center_x,
                y: target_gap.y,
            },
        );
        CanvasPoint {
            x: center_x,
            y: 0.5 * (source_gap.y + target_gap.y),
        }
    } else {
        let center_y = source_gap.y + (target_gap.y - source_gap.y) * options.step_position;
        push_distinct_point(
            &mut points,
            CanvasPoint {
                x: source_gap.x,
                y: center_y,
            },
        );
        push_distinct_point(
            &mut points,
            CanvasPoint {
                x: target_gap.x,
                y: center_y,
            },
        );
        CanvasPoint {
            x: 0.5 * (source_gap.x + target_gap.x),
            y: center_y,
        }
    };

    push_distinct_point(&mut points, target_gap);
    push_distinct_point(&mut points, target.point);

    let mut commands = Vec::with_capacity(points.len());
    let first = *points.first()?;
    commands.push(PathCommand::MoveTo(first));
    for point in points.into_iter().skip(1) {
        commands.push(PathCommand::LineTo(point));
    }

    Some(EdgePath {
        commands,
        label: EdgePathLabel {
            point: label_point,
            offset_x: (label_point.x - source.point.x).abs(),
            offset_y: (label_point.y - source.point.y).abs(),
        },
    })
}

fn normalized_smoothstep_options(options: SmoothStepEdgeOptions) -> SmoothStepEdgeOptions {
    SmoothStepEdgeOptions {
        offset: if options.offset.is_finite() && options.offset >= 0.0 {
            options.offset
        } else {
            SmoothStepEdgeOptions::default().offset
        },
        step_position: if options.step_position.is_finite() {
            options.step_position.clamp(0.0, 1.0)
        } else {
            SmoothStepEdgeOptions::default().step_position
        },
    }
}

fn handle_direction(position: HandlePosition) -> (f32, f32) {
    match position {
        HandlePosition::Top => (0.0, -1.0),
        HandlePosition::Right => (1.0, 0.0),
        HandlePosition::Bottom => (0.0, 1.0),
        HandlePosition::Left => (-1.0, 0.0),
    }
}

fn translate_point(
    point: CanvasPoint,
    direction: (f32, f32),
    distance: f32,
) -> Option<CanvasPoint> {
    let out = CanvasPoint {
        x: point.x + direction.0 * distance,
        y: point.y + direction.1 * distance,
    };
    out.is_finite().then_some(out)
}

fn push_distinct_point(points: &mut Vec<CanvasPoint>, point: CanvasPoint) {
    if points.last().is_some_and(|last| *last == point) {
        return;
    }
    points.push(point);
}
