use super::*;
use crate::io::NodeGraphViewState;

#[test]
fn adapter_conformance_fixture_runner_records_node_drag_gesture_transaction_and_callbacks() {
    let (graph, node_id, _b, _out_port, _in_port, _eid) = make_graph();

    let start = CanvasPoint { x: 1.0, y: 2.0 };
    let target = CanvasPoint { x: 32.0, y: 16.0 };
    let scenario = ConformanceScenario::new("node drag gesture transaction callbacks", graph)
        .with_xyflow_callbacks()
        .with_node_drag_session_contract(ConformanceNodeDragSessionContract::new(
            node_id, start, target,
        ));

    assert_conformance_trace(&scenario);
}

#[test]
fn adapter_conformance_fixture_runner_records_node_drag_parent_expansion_transaction() {
    let (mut graph, node_id, _b, _out_port, _in_port, _eid) = make_graph();
    let parent_id = GroupId::from_u128(200);
    graph.groups.insert(
        parent_id,
        Group {
            title: "Parent".to_owned(),
            rect: CanvasRect {
                origin: CanvasPoint { x: 0.0, y: 0.0 },
                size: CanvasSize {
                    width: 100.0,
                    height: 100.0,
                },
            },
            color: None,
        },
    );
    let node = graph.nodes.get_mut(&node_id).expect("node exists");
    node.parent = Some(parent_id);
    node.extent = Some(NodeExtent::Parent);
    node.expand_parent = Some(true);
    node.size = Some(CanvasSize {
        width: 20.0,
        height: 20.0,
    });

    let target = CanvasPoint { x: 95.0, y: 95.0 };
    let scenario = ConformanceScenario::new("node drag parent expansion", graph)
        .with_xyflow_callbacks()
        .with_actions([ConformanceAction::apply_node_drag(node_id, target)])
        .with_expected_trace([
            ConformanceTraceEvent::graph_commit(
                Some(NODE_DRAG_TRANSACTION_LABEL),
                ["set_node_pos", "set_group_rect"],
            ),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::GraphCommit {
                label: Some(NODE_DRAG_TRANSACTION_LABEL.to_owned()),
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeEdgeChanges {
                nodes: 1,
                edges: 0,
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodesChange { count: 1 }),
        ]);

    assert_conformance_trace(&scenario);
}

#[test]
fn adapter_conformance_fixture_runner_records_nested_parent_canvas_space_drag() {
    let (mut graph, node_id, _b, _out_port, _in_port, _eid) = make_graph();
    let parent_id = GroupId::from_u128(201);
    graph.groups.insert(
        parent_id,
        Group {
            title: "Nested Parent".to_owned(),
            rect: CanvasRect {
                origin: CanvasPoint { x: 200.0, y: 200.0 },
                size: CanvasSize {
                    width: 120.0,
                    height: 120.0,
                },
            },
            color: None,
        },
    );
    let node = graph.nodes.get_mut(&node_id).expect("node exists");
    node.parent = Some(parent_id);
    node.pos = CanvasPoint { x: 220.0, y: 225.0 };
    node.extent = Some(NodeExtent::Parent);
    node.expand_parent = Some(false);
    node.size = Some(CanvasSize {
        width: 20.0,
        height: 20.0,
    });

    let target = CanvasPoint { x: 260.0, y: 265.0 };
    let scenario = ConformanceScenario::new("nested parent canvas-space drag", graph)
        .with_xyflow_callbacks()
        .with_actions([
            ConformanceAction::apply_node_drag(node_id, target),
            ConformanceAction::assert_node_position(node_id, target),
        ])
        .with_expected_trace([
            ConformanceTraceEvent::graph_commit(
                Some(NODE_DRAG_TRANSACTION_LABEL),
                ["set_node_pos"],
            ),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::GraphCommit {
                label: Some(NODE_DRAG_TRANSACTION_LABEL.to_owned()),
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeEdgeChanges {
                nodes: 1,
                edges: 0,
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodesChange { count: 1 }),
        ]);

    assert_conformance_trace(&scenario);
}

#[test]
fn adapter_conformance_fixture_runner_records_delete_during_active_drag_lifecycle() {
    let (graph, node_id, _b, out_port, in_port, edge_id) = make_graph();
    let mut view_state = NodeGraphViewState::default();
    view_state.set_selection(vec![node_id], Vec::new(), Vec::new());

    let start = NodeDragStart {
        primary: node_id,
        nodes: vec![node_id],
        pointer: CanvasPoint { x: 1.0, y: 2.0 },
    };
    let end = NodeDragEnd {
        primary: node_id,
        nodes: vec![node_id],
        pointer: CanvasPoint { x: 1.0, y: 2.0 },
        outcome: NodeDragEndOutcome::Canceled,
    };
    let disconnected = EdgeConnection::new(edge_id, out_port, in_port, EdgeKind::Data);
    let scenario = ConformanceScenario::new("delete during active node drag", graph)
        .with_view_state(view_state)
        .with_xyflow_callbacks()
        .with_delete_selection_during_node_drag_contract(
            ConformanceDeleteSelectionDuringNodeDragContract::new(
                start,
                end.clone(),
                ConformanceDeleteSelectionContract::new(1, 1)
                    .for_key(keyboard_types::Code::Backspace)
                    .with_disconnected([disconnected]),
            ),
        );

    assert!(scenario.actions.is_empty());
    assert_eq!(scenario.behaviors.len(), 1);
    assert_eq!(scenario.expanded_actions().len(), 3);
    assert_conformance_trace(&scenario);
}
