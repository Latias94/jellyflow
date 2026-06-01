use super::{
    BezierEdgeOptions, PathCommand, SmoothStepEdgeOptions, bezier_edge_path, smoothstep_edge_path,
    straight_edge_path,
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
