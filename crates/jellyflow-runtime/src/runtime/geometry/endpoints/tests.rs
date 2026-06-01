use super::{
    EdgeEndpointInput, HandleBounds, HandlePosition, edge_position, handle_anchor_position,
    handle_center_position,
};
use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize};

#[test]
fn edge_position_uses_handle_rect_side_anchors() {
    let source_rect = CanvasRect {
        origin: CanvasPoint { x: 100.0, y: 50.0 },
        size: CanvasSize {
            width: 200.0,
            height: 100.0,
        },
    };
    let target_rect = CanvasRect {
        origin: CanvasPoint { x: 400.0, y: 80.0 },
        size: CanvasSize {
            width: 120.0,
            height: 80.0,
        },
    };

    let position = edge_position(
        EdgeEndpointInput {
            node_rect: source_rect,
            handle: Some(HandleBounds {
                rect: CanvasRect {
                    origin: CanvasPoint { x: 190.0, y: 40.0 },
                    size: CanvasSize {
                        width: 10.0,
                        height: 20.0,
                    },
                },
                position: HandlePosition::Right,
            }),
            fallback_position: HandlePosition::Bottom,
        },
        EdgeEndpointInput {
            node_rect: target_rect,
            handle: Some(HandleBounds {
                rect: CanvasRect {
                    origin: CanvasPoint { x: 0.0, y: 30.0 },
                    size: CanvasSize {
                        width: 12.0,
                        height: 20.0,
                    },
                },
                position: HandlePosition::Left,
            }),
            fallback_position: HandlePosition::Top,
        },
    )
    .expect("edge position");

    assert_eq!(position.source.position, HandlePosition::Right);
    assert!((position.source.point.x - 300.0).abs() <= 1.0e-6);
    assert!((position.source.point.y - 100.0).abs() <= 1.0e-6);
    assert_eq!(position.target.position, HandlePosition::Left);
    assert!((position.target.point.x - 400.0).abs() <= 1.0e-6);
    assert!((position.target.point.y - 120.0).abs() <= 1.0e-6);
}

#[test]
fn edge_position_falls_back_to_node_side_without_handle_bounds() {
    let node_rect = CanvasRect {
        origin: CanvasPoint { x: 10.0, y: 20.0 },
        size: CanvasSize {
            width: 80.0,
            height: 40.0,
        },
    };

    let source =
        handle_anchor_position(node_rect, None, HandlePosition::Bottom).expect("source anchor");
    assert_eq!(source.position, HandlePosition::Bottom);
    assert!((source.point.x - 50.0).abs() <= 1.0e-6);
    assert!((source.point.y - 60.0).abs() <= 1.0e-6);

    let target =
        handle_anchor_position(node_rect, None, HandlePosition::Top).expect("target anchor");
    assert_eq!(target.position, HandlePosition::Top);
    assert!((target.point.x - 50.0).abs() <= 1.0e-6);
    assert!((target.point.y - 20.0).abs() <= 1.0e-6);
}

#[test]
fn edge_position_can_return_handle_center_for_hit_testing() {
    let node_rect = CanvasRect {
        origin: CanvasPoint { x: 10.0, y: 20.0 },
        size: CanvasSize {
            width: 80.0,
            height: 40.0,
        },
    };
    let center = handle_center_position(
        node_rect,
        Some(HandleBounds {
            rect: CanvasRect {
                origin: CanvasPoint { x: 70.0, y: 10.0 },
                size: CanvasSize {
                    width: 10.0,
                    height: 20.0,
                },
            },
            position: HandlePosition::Right,
        }),
        HandlePosition::Left,
    )
    .expect("center");

    assert!((center.x - 85.0).abs() <= 1.0e-6);
    assert!((center.y - 40.0).abs() <= 1.0e-6);
}

#[test]
fn edge_position_rejects_invalid_node_or_handle_rects() {
    let node_rect = CanvasRect {
        origin: CanvasPoint { x: 0.0, y: 0.0 },
        size: CanvasSize {
            width: 80.0,
            height: 40.0,
        },
    };

    assert!(
        handle_anchor_position(
            CanvasRect {
                origin: CanvasPoint {
                    x: f32::INFINITY,
                    y: 0.0,
                },
                ..node_rect
            },
            None,
            HandlePosition::Left,
        )
        .is_none()
    );
    assert!(
        handle_anchor_position(
            node_rect,
            Some(HandleBounds {
                rect: CanvasRect {
                    origin: CanvasPoint { x: 0.0, y: 0.0 },
                    size: CanvasSize {
                        width: 0.0,
                        height: 10.0,
                    },
                },
                position: HandlePosition::Left,
            }),
            HandlePosition::Left,
        )
        .is_none()
    );
}
