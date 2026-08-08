use super::{
    BezierEdgeOptions, EdgePath, EdgePathAnchor, EdgePathLabel, EdgePathSampleError,
    OrthogonalEdgeOptions, PathCommand, SmoothStepEdgeOptions, bezier_edge_path,
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
fn explicit_center_routes_reversed_steps_through_an_external_lane() {
    let source = EdgeEndpointPosition {
        point: CanvasPoint { x: 780.0, y: 300.0 },
        position: HandlePosition::Right,
    };
    let target = EdgeEndpointPosition {
        point: CanvasPoint { x: 100.0, y: 220.0 },
        position: HandlePosition::Left,
    };
    let center = CanvasPoint { x: 440.0, y: 620.0 };
    let orthogonal = orthogonal_edge_path(
        source,
        target,
        OrthogonalEdgeOptions {
            center: Some(center),
            ..OrthogonalEdgeOptions::default()
        },
    )
    .expect("orthogonal path");
    let smooth = smoothstep_edge_path(
        source,
        target,
        SmoothStepEdgeOptions {
            center: Some(center),
            ..SmoothStepEdgeOptions::default()
        },
    )
    .expect("smoothstep path");

    assert_eq!(orthogonal.label.point, center);
    assert_eq!(smooth.label.point, center);
    assert!(
        orthogonal.commands.iter().any(|command| {
            matches!(command, PathCommand::LineTo(point) if point.y == center.y)
        })
    );
    assert!(
        smooth.commands.iter().any(|command| {
            matches!(command, PathCommand::LineTo(point) if point.y == center.y)
        })
    );
    assert_path_endpoints(&orthogonal.commands, source.point, target.point);
    assert_path_endpoints(&smooth.commands, source.point, target.point);
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

#[test]
fn every_resolved_path_supports_finite_arc_length_samples() {
    let source = EdgeEndpointPosition {
        point: CanvasPoint { x: 0.0, y: 10.0 },
        position: HandlePosition::Right,
    };
    let target = EdgeEndpointPosition {
        point: CanvasPoint { x: 120.0, y: 70.0 },
        position: HandlePosition::Left,
    };
    let straight = straight_edge_path(source, target).expect("straight path");
    let bezier =
        bezier_edge_path(source, target, BezierEdgeOptions::default()).expect("bezier path");
    let orthogonal = orthogonal_edge_path(source, target, OrthogonalEdgeOptions::default())
        .expect("orthogonal path");
    let smoothstep = smoothstep_edge_path(source, target, SmoothStepEdgeOptions::default())
        .expect("smoothstep path");

    let straight_tangent = CanvasPoint {
        x: 2.0 / 5.0_f32.sqrt(),
        y: 1.0 / 5.0_f32.sqrt(),
    };
    assert_endpoint_tangents(&straight, straight_tangent, straight_tangent);
    for path in [&bezier, &orthogonal, &smoothstep] {
        assert_endpoint_tangents(
            path,
            CanvasPoint { x: 1.0, y: 0.0 },
            CanvasPoint { x: 1.0, y: 0.0 },
        );
    }

    for path in [&straight, &bezier, &orthogonal, &smoothstep] {
        let source_sample = path.sample(EdgePathAnchor::Source).expect("source sample");
        let center_sample = path.sample(EdgePathAnchor::Center).expect("center sample");
        let target_sample = path.sample(EdgePathAnchor::Target).expect("target sample");

        assert_eq!(source_sample.point, source.point);
        assert_eq!(target_sample.point, target.point);
        assert_sample_is_finite_and_normalized(center_sample);
        assert_sample_is_finite_and_normalized(source_sample);
        assert_sample_is_finite_and_normalized(target_sample);
    }
}

#[test]
fn path_sampling_uses_arc_length_and_clamps_finite_progress() {
    let path = EdgePath {
        commands: vec![
            PathCommand::MoveTo(CanvasPoint { x: 0.0, y: 0.0 }),
            PathCommand::LineTo(CanvasPoint { x: 100.0, y: 0.0 }),
            PathCommand::LineTo(CanvasPoint { x: 100.0, y: 20.0 }),
        ],
        label: EdgePathLabel {
            point: CanvasPoint { x: 100.0, y: 10.0 },
            offset_x: 100.0,
            offset_y: 10.0,
        },
    };

    let center = path.sample(EdgePathAnchor::Center).expect("center sample");
    assert_point_near(center.point, CanvasPoint { x: 60.0, y: 0.0 });
    assert_point_near(center.tangent, CanvasPoint { x: 1.0, y: 0.0 });
    assert_eq!(
        path.sample(EdgePathAnchor::Progress(-3.0))
            .expect("clamped source")
            .point,
        CanvasPoint { x: 0.0, y: 0.0 }
    );
    assert_eq!(
        path.sample(EdgePathAnchor::Progress(4.0))
            .expect("clamped target")
            .point,
        CanvasPoint { x: 100.0, y: 20.0 }
    );
}

#[test]
fn bezier_endpoint_samples_preserve_command_derivative_directions() {
    let source = EdgeEndpointPosition {
        point: CanvasPoint { x: 0.0, y: 0.0 },
        position: HandlePosition::Right,
    };
    let target = EdgeEndpointPosition {
        point: CanvasPoint { x: 80.0, y: 100.0 },
        position: HandlePosition::Top,
    };
    let path = bezier_edge_path(source, target, BezierEdgeOptions::default()).expect("path");

    assert_point_near(
        path.sample(EdgePathAnchor::Source)
            .expect("source sample")
            .tangent,
        CanvasPoint { x: 1.0, y: 0.0 },
    );
    assert_point_near(
        path.sample(EdgePathAnchor::Target)
            .expect("target sample")
            .tangent,
        CanvasPoint { x: 0.0, y: 1.0 },
    );
}

#[test]
fn degenerate_path_samples_are_finite_and_deterministic() {
    let endpoint = EdgeEndpointPosition {
        point: CanvasPoint { x: 12.0, y: -8.0 },
        position: HandlePosition::Right,
    };
    let path = straight_edge_path(endpoint, endpoint).expect("degenerate path");

    for anchor in [
        EdgePathAnchor::Source,
        EdgePathAnchor::Center,
        EdgePathAnchor::Target,
    ] {
        let sample = path.sample(anchor).expect("degenerate sample");
        assert_eq!(sample.point, endpoint.point);
        assert_eq!(sample.tangent, CanvasPoint { x: 1.0, y: 0.0 });
    }
}

#[test]
fn path_sampling_rejects_non_finite_progress_and_empty_paths() {
    let source = EdgeEndpointPosition {
        point: CanvasPoint { x: 0.0, y: 0.0 },
        position: HandlePosition::Right,
    };
    let target = EdgeEndpointPosition {
        point: CanvasPoint { x: 10.0, y: 0.0 },
        position: HandlePosition::Left,
    };
    let path = straight_edge_path(source, target).expect("path");
    assert_eq!(
        path.sample(EdgePathAnchor::Progress(f32::NAN)),
        Err(EdgePathSampleError::InvalidProgress)
    );

    let empty = EdgePath {
        commands: Vec::new(),
        label: EdgePathLabel {
            point: CanvasPoint { x: 0.0, y: 0.0 },
            offset_x: 0.0,
            offset_y: 0.0,
        },
    };
    assert_eq!(
        empty.sample(EdgePathAnchor::Center),
        Err(EdgePathSampleError::EmptyPath)
    );
}

fn assert_sample_is_finite_and_normalized(sample: super::EdgePathSample) {
    assert!(sample.point.is_finite());
    assert!(sample.tangent.is_finite());
    let length = (sample.tangent.x.powi(2) + sample.tangent.y.powi(2)).sqrt();
    assert!((length - 1.0).abs() <= 1.0e-5, "sample: {sample:?}");
}

fn assert_endpoint_tangents(
    path: &EdgePath,
    expected_source: CanvasPoint,
    expected_target: CanvasPoint,
) {
    assert_point_near(
        path.sample(EdgePathAnchor::Source)
            .expect("source sample")
            .tangent,
        expected_source,
    );
    assert_point_near(
        path.sample(EdgePathAnchor::Target)
            .expect("target sample")
            .tangent,
        expected_target,
    );
}

fn assert_point_near(actual: CanvasPoint, expected: CanvasPoint) {
    assert!((actual.x - expected.x).abs() <= 1.0e-5, "{actual:?}");
    assert!((actual.y - expected.y).abs() <= 1.0e-5, "{actual:?}");
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
