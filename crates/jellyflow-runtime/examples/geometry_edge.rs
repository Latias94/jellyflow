use jellyflow_core::{CanvasPoint, CanvasRect, CanvasSize};
use jellyflow_runtime::runtime::geometry::{
    BezierEdgeOptions, EdgeEndpointInput, EdgeHitTestOptions, HandleBounds, HandlePosition,
    bezier_edge_path, edge_path_contains_point, edge_position,
};

fn main() {
    let source_node = CanvasRect {
        origin: CanvasPoint { x: 0.0, y: 0.0 },
        size: CanvasSize {
            width: 120.0,
            height: 80.0,
        },
    };
    let target_node = CanvasRect {
        origin: CanvasPoint { x: 240.0, y: 40.0 },
        size: CanvasSize {
            width: 120.0,
            height: 80.0,
        },
    };

    let endpoints = edge_position(
        EdgeEndpointInput {
            node_rect: source_node,
            handle: Some(HandleBounds {
                rect: CanvasRect {
                    origin: CanvasPoint { x: 112.0, y: 32.0 },
                    size: CanvasSize {
                        width: 8.0,
                        height: 16.0,
                    },
                },
                position: HandlePosition::Right,
            }),
            fallback_position: HandlePosition::Right,
        },
        EdgeEndpointInput {
            node_rect: target_node,
            handle: Some(HandleBounds {
                rect: CanvasRect {
                    origin: CanvasPoint { x: 0.0, y: 32.0 },
                    size: CanvasSize {
                        width: 8.0,
                        height: 16.0,
                    },
                },
                position: HandlePosition::Left,
            }),
            fallback_position: HandlePosition::Left,
        },
    )
    .expect("edge endpoints");

    let path = bezier_edge_path(
        endpoints.source,
        endpoints.target,
        BezierEdgeOptions::default(),
    )
    .expect("bezier path");

    assert!(edge_path_contains_point(
        &path,
        path.label.point,
        EdgeHitTestOptions::default(),
    ));

    println!(
        "edge label at ({:.1}, {:.1}) with {} commands",
        path.label.point.x,
        path.label.point.y,
        path.commands.len()
    );
}
