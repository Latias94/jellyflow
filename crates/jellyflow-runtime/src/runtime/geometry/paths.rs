use jellyflow_core::core::CanvasPoint;

use super::endpoints::{EdgeEndpointPosition, HandlePosition};

/// Renderer-neutral path command.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathCommand {
    MoveTo(CanvasPoint),
    LineTo(CanvasPoint),
    CubicTo {
        control1: CanvasPoint,
        control2: CanvasPoint,
        to: CanvasPoint,
    },
}

/// Label placement derived from an edge path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgePathLabel {
    pub point: CanvasPoint,
    pub offset_x: f32,
    pub offset_y: f32,
}

/// Renderer-neutral edge path.
#[derive(Debug, Clone, PartialEq)]
pub struct EdgePath {
    pub commands: Vec<PathCommand>,
    pub label: EdgePathLabel,
}

/// Bezier edge options.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BezierEdgeOptions {
    pub curvature: f32,
}

impl Default for BezierEdgeOptions {
    fn default() -> Self {
        Self { curvature: 0.25 }
    }
}

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

pub fn straight_edge_path(
    source: EdgeEndpointPosition,
    target: EdgeEndpointPosition,
) -> Option<EdgePath> {
    let label = edge_center_label(source.point, target.point)?;
    Some(EdgePath {
        commands: vec![
            PathCommand::MoveTo(source.point),
            PathCommand::LineTo(target.point),
        ],
        label,
    })
}

pub fn bezier_edge_path(
    source: EdgeEndpointPosition,
    target: EdgeEndpointPosition,
    options: BezierEdgeOptions,
) -> Option<EdgePath> {
    if !source.point.is_finite() || !target.point.is_finite() {
        return None;
    }

    let curvature = if options.curvature.is_finite() && options.curvature >= 0.0 {
        options.curvature
    } else {
        BezierEdgeOptions::default().curvature
    };
    let control1 = control_with_curvature(source.position, source.point, target.point, curvature);
    let control2 = control_with_curvature(target.position, target.point, source.point, curvature);
    let label = bezier_label(source.point, control1, control2, target.point)?;

    Some(EdgePath {
        commands: vec![
            PathCommand::MoveTo(source.point),
            PathCommand::CubicTo {
                control1,
                control2,
                to: target.point,
            },
        ],
        label,
    })
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

fn edge_center_label(source: CanvasPoint, target: CanvasPoint) -> Option<EdgePathLabel> {
    if !source.is_finite() || !target.is_finite() {
        return None;
    }

    let offset_x = (target.x - source.x).abs() * 0.5;
    let offset_y = (target.y - source.y).abs() * 0.5;
    let point = CanvasPoint {
        x: if target.x < source.x {
            target.x + offset_x
        } else {
            target.x - offset_x
        },
        y: if target.y < source.y {
            target.y + offset_y
        } else {
            target.y - offset_y
        },
    };

    point.is_finite().then_some(EdgePathLabel {
        point,
        offset_x,
        offset_y,
    })
}

fn bezier_label(
    source: CanvasPoint,
    control1: CanvasPoint,
    control2: CanvasPoint,
    target: CanvasPoint,
) -> Option<EdgePathLabel> {
    let point = CanvasPoint {
        x: source.x * 0.125 + control1.x * 0.375 + control2.x * 0.375 + target.x * 0.125,
        y: source.y * 0.125 + control1.y * 0.375 + control2.y * 0.375 + target.y * 0.125,
    };

    point.is_finite().then_some(EdgePathLabel {
        point,
        offset_x: (point.x - source.x).abs(),
        offset_y: (point.y - source.y).abs(),
    })
}

fn control_with_curvature(
    position: HandlePosition,
    from: CanvasPoint,
    to: CanvasPoint,
    curvature: f32,
) -> CanvasPoint {
    match position {
        HandlePosition::Left => CanvasPoint {
            x: from.x - control_offset(from.x - to.x, curvature),
            y: from.y,
        },
        HandlePosition::Right => CanvasPoint {
            x: from.x + control_offset(to.x - from.x, curvature),
            y: from.y,
        },
        HandlePosition::Top => CanvasPoint {
            x: from.x,
            y: from.y - control_offset(from.y - to.y, curvature),
        },
        HandlePosition::Bottom => CanvasPoint {
            x: from.x,
            y: from.y + control_offset(to.y - from.y, curvature),
        },
    }
}

fn control_offset(distance: f32, curvature: f32) -> f32 {
    if distance >= 0.0 {
        0.5 * distance
    } else {
        curvature * 25.0 * (-distance).sqrt()
    }
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

#[cfg(test)]
mod tests {
    use super::{
        BezierEdgeOptions, PathCommand, SmoothStepEdgeOptions, bezier_edge_path,
        smoothstep_edge_path, straight_edge_path,
    };
    use crate::runtime::geometry::{EdgeEndpointPosition, HandlePosition};
    use jellyflow_core::core::CanvasPoint;

    #[test]
    fn edge_path_straight_commands_and_label_are_renderer_neutral() {
        let source = EdgeEndpointPosition {
            point: CanvasPoint { x: 0.0, y: 20.0 },
            position: HandlePosition::Right,
        };
        let target = EdgeEndpointPosition {
            point: CanvasPoint { x: 150.0, y: 100.0 },
            position: HandlePosition::Left,
        };

        let path = straight_edge_path(source, target).expect("path");
        assert_eq!(
            path.commands,
            vec![
                PathCommand::MoveTo(source.point),
                PathCommand::LineTo(target.point)
            ]
        );
        assert!((path.label.point.x - 75.0).abs() <= 1.0e-6);
        assert!((path.label.point.y - 60.0).abs() <= 1.0e-6);
        assert!((path.label.offset_x - 75.0).abs() <= 1.0e-6);
        assert!((path.label.offset_y - 40.0).abs() <= 1.0e-6);
    }

    #[test]
    fn edge_path_bezier_uses_side_curvature_controls() {
        let source = EdgeEndpointPosition {
            point: CanvasPoint { x: 0.0, y: 0.0 },
            position: HandlePosition::Right,
        };
        let target = EdgeEndpointPosition {
            point: CanvasPoint { x: 100.0, y: 0.0 },
            position: HandlePosition::Left,
        };

        let path = bezier_edge_path(source, target, BezierEdgeOptions::default()).expect("path");
        assert_eq!(
            path.commands,
            vec![
                PathCommand::MoveTo(source.point),
                PathCommand::CubicTo {
                    control1: CanvasPoint { x: 50.0, y: 0.0 },
                    control2: CanvasPoint { x: 50.0, y: 0.0 },
                    to: target.point,
                },
            ]
        );
        assert!((path.label.point.x - 50.0).abs() <= 1.0e-6);
        assert!((path.label.point.y - 0.0).abs() <= 1.0e-6);
    }

    #[test]
    fn edge_path_smoothstep_like_routes_orthogonal_points() {
        let source = EdgeEndpointPosition {
            point: CanvasPoint { x: 0.0, y: 0.0 },
            position: HandlePosition::Right,
        };
        let target = EdgeEndpointPosition {
            point: CanvasPoint { x: 100.0, y: 40.0 },
            position: HandlePosition::Left,
        };

        let path =
            smoothstep_edge_path(source, target, SmoothStepEdgeOptions::default()).expect("path");
        assert_eq!(
            path.commands,
            vec![
                PathCommand::MoveTo(CanvasPoint { x: 0.0, y: 0.0 }),
                PathCommand::LineTo(CanvasPoint { x: 20.0, y: 0.0 }),
                PathCommand::LineTo(CanvasPoint { x: 50.0, y: 0.0 }),
                PathCommand::LineTo(CanvasPoint { x: 50.0, y: 40.0 }),
                PathCommand::LineTo(CanvasPoint { x: 80.0, y: 40.0 }),
                PathCommand::LineTo(CanvasPoint { x: 100.0, y: 40.0 }),
            ]
        );
        assert!((path.label.point.x - 50.0).abs() <= 1.0e-6);
        assert!((path.label.point.y - 20.0).abs() <= 1.0e-6);
    }
}
