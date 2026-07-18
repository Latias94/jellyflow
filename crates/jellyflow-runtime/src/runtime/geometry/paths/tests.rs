use super::{
    BezierEdgeOptions, OrthogonalEdgeOptions, PathCommand, SmoothStepEdgeOptions, bezier_edge_path,
    orthogonal_edge_path, smoothstep_edge_path, straight_edge_path,
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
fn orthogonal_and_smoothstep_share_route_points_but_not_corner_commands() {
    let source = EdgeEndpointPosition {
        point: CanvasPoint { x: 0.0, y: 0.0 },
        position: HandlePosition::Right,
    };
    let target = EdgeEndpointPosition {
        point: CanvasPoint { x: 100.0, y: 40.0 },
        position: HandlePosition::Left,
    };

    let orthogonal =
        orthogonal_edge_path(source, target, OrthogonalEdgeOptions::default()).expect("path");
    let smooth =
        smoothstep_edge_path(source, target, SmoothStepEdgeOptions::default()).expect("path");
    assert!(
        orthogonal
            .commands
            .iter()
            .all(|command| !matches!(command, PathCommand::CubicTo { .. }))
    );
    assert!(
        smooth
            .commands
            .iter()
            .any(|command| matches!(command, PathCommand::CubicTo { .. }))
    );
    assert_ne!(orthogonal.commands, smooth.commands);
    assert_eq!(orthogonal.label, smooth.label);
    assert!((smooth.label.point.x - 50.0).abs() <= 1.0e-6);
    assert!((smooth.label.point.y - 20.0).abs() <= 1.0e-6);
}

#[test]
fn orthogonal_route_handles_every_endpoint_direction_and_short_reversed_paths() {
    let positions = [
        HandlePosition::Top,
        HandlePosition::Right,
        HandlePosition::Bottom,
        HandlePosition::Left,
    ];
    let point_pairs = [
        (
            CanvasPoint { x: 0.0, y: 0.0 },
            CanvasPoint { x: 100.0, y: 60.0 },
        ),
        (
            CanvasPoint { x: 10.0, y: 10.0 },
            CanvasPoint { x: 4.0, y: 6.0 },
        ),
    ];

    for (source_point, target_point) in point_pairs {
        for source_position in positions {
            for target_position in positions {
                let source = EdgeEndpointPosition {
                    point: source_point,
                    position: source_position,
                };
                let target = EdgeEndpointPosition {
                    point: target_point,
                    position: target_position,
                };
                let orthogonal =
                    orthogonal_edge_path(source, target, OrthogonalEdgeOptions::default())
                        .expect("finite orthogonal route");
                let smooth = smoothstep_edge_path(source, target, SmoothStepEdgeOptions::default())
                    .expect("finite smoothstep route");

                assert_path_endpoints(&orthogonal.commands, source_point, target_point);
                assert_path_endpoints(&smooth.commands, source_point, target_point);
                assert_orthogonal_lines(&orthogonal.commands);
                assert!(path_is_finite(&orthogonal.commands));
                assert!(path_is_finite(&smooth.commands));
            }
        }
    }
}

fn assert_path_endpoints(
    commands: &[PathCommand],
    expected_start: CanvasPoint,
    expected_end: CanvasPoint,
) {
    assert_eq!(commands.first(), Some(&PathCommand::MoveTo(expected_start)));
    let end = commands.last().map(|command| match command {
        PathCommand::MoveTo(point) | PathCommand::LineTo(point) => *point,
        PathCommand::CubicTo { to, .. } => *to,
    });
    assert_eq!(end, Some(expected_end));
}

fn assert_orthogonal_lines(commands: &[PathCommand]) {
    let mut current = None;
    for command in commands {
        match *command {
            PathCommand::MoveTo(point) => current = Some(point),
            PathCommand::LineTo(point) => {
                let from = current.expect("line follows move");
                assert!(
                    (from.x - point.x).abs() <= 1.0e-6 || (from.y - point.y).abs() <= 1.0e-6,
                    "non-orthogonal segment {from:?} -> {point:?} in {commands:?}"
                );
                current = Some(point);
            }
            PathCommand::CubicTo { .. } => panic!("orthogonal route contains a curve"),
        }
    }
}

fn path_is_finite(commands: &[PathCommand]) -> bool {
    commands.iter().all(|command| match command {
        PathCommand::MoveTo(point) | PathCommand::LineTo(point) => point.is_finite(),
        PathCommand::CubicTo {
            control1,
            control2,
            to,
        } => control1.is_finite() && control2.is_finite() && to.is_finite(),
    })
}
