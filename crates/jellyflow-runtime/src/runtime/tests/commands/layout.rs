use super::*;

use jellyflow_layout::{
    DugongLayoutEngine, LayoutContext, LayoutEngine, LayoutError, LayoutNodePosition,
    LayoutOptions, LayoutRequest, LayoutResult,
};

use crate::runtime::layout::layout_result_to_command;

#[test]
fn layout_result_becomes_one_revisioned_geometry_command() {
    let (graph, nodes) = graph_with_nodes(2);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph.clone())
        .unwrap();
    let layout = LayoutResult {
        nodes: vec![
            LayoutNodePosition {
                node: nodes[0],
                pos: CanvasPoint { x: 80.0, y: 120.0 },
                center: CanvasPoint { x: 0.0, y: 0.0 },
                size: CanvasSize {
                    width: 100.0,
                    height: 80.0,
                },
            },
            LayoutNodePosition {
                node: nodes[1],
                pos: CanvasPoint { x: 320.0, y: 160.0 },
                center: CanvasPoint { x: 0.0, y: 0.0 },
                size: CanvasSize {
                    width: 100.0,
                    height: 80.0,
                },
            },
        ],
        edge_routes: Vec::new(),
        bounds: None,
    };
    let command = layout_result_to_command(0, &graph, &layout).expect("layout command");
    let outcome = runtime.execute(command).unwrap();
    let receipt = outcome.applied().expect("layout command applies");

    assert_eq!(receipt.footprint().nodes.len(), 2);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);
    assert_eq!(
        runtime
            .snapshot()
            .unwrap()
            .graph()
            .node(&nodes[0])
            .unwrap()
            .pos,
        CanvasPoint { x: 80.0, y: 120.0 }
    );
}

#[test]
fn applied_layout_preserves_engine_visual_bounds_for_nodes_without_origin() {
    let (graph, nodes) = graph_with_nodes(1);
    assert_eq!(graph.node(&nodes[0]).expect("node").origin, None);
    let request = LayoutRequest::all().with_options(LayoutOptions {
        margin: CanvasSize {
            width: 32.0,
            height: 24.0,
        },
        ..LayoutOptions::default()
    });
    let layout = DugongLayoutEngine
        .layout(&graph, &request, &LayoutContext::new())
        .expect("layout");
    let positioned = layout.node_position(nodes[0]).expect("positioned node");
    let expected_visual_bounds = layout.bounds.expect("layout bounds");
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph.clone())
        .unwrap();

    runtime
        .execute(layout_result_to_command(0, &graph, &layout).expect("layout command"))
        .unwrap()
        .applied()
        .expect("layout command applies");

    let snapshot = runtime.snapshot().unwrap();
    let applied = snapshot.graph().node(&nodes[0]).expect("applied node");
    assert_eq!(
        applied.bounds_for_size(positioned.size),
        Some(expected_visual_bounds)
    );
}

#[test]
fn layout_result_rejects_duplicate_or_missing_nodes_before_command_creation() {
    let (graph, nodes) = graph_with_nodes(1);
    let duplicate = LayoutResult {
        nodes: vec![
            LayoutNodePosition {
                node: nodes[0],
                pos: CanvasPoint { x: 1.0, y: 2.0 },
                center: CanvasPoint::default(),
                size: CanvasSize {
                    width: 10.0,
                    height: 10.0,
                },
            },
            LayoutNodePosition {
                node: nodes[0],
                pos: CanvasPoint { x: 3.0, y: 4.0 },
                center: CanvasPoint::default(),
                size: CanvasSize {
                    width: 10.0,
                    height: 10.0,
                },
            },
        ],
        edge_routes: Vec::new(),
        bounds: None,
    };
    assert!(matches!(
        layout_result_to_command(0, &graph, &duplicate),
        Err(LayoutError::DuplicateResultNode(id)) if id == nodes[0]
    ));

    let missing = LayoutResult {
        nodes: vec![LayoutNodePosition {
            node: NodeId::from_u128(999_999),
            pos: CanvasPoint::default(),
            center: CanvasPoint::default(),
            size: CanvasSize {
                width: 10.0,
                height: 10.0,
            },
        }],
        edge_routes: Vec::new(),
        bounds: None,
    };
    assert!(matches!(
        layout_result_to_command(0, &graph, &missing),
        Err(LayoutError::MissingResultNode(id)) if id == NodeId::from_u128(999_999)
    ));
}
