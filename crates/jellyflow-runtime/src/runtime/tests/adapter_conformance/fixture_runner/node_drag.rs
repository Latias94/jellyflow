use super::*;
use crate::io::NodeGraphViewState;
use crate::runtime::delete::DELETE_SELECTION_TRANSACTION_LABEL;

#[test]
fn adapter_conformance_fixture_runner_records_node_drag_gesture_transaction_and_callbacks() {
    let (graph, node_id, _b, _out_port, _in_port, _eid) = make_graph();

    let start = NodeDragStart {
        primary: node_id,
        nodes: vec![node_id],
        pointer: CanvasPoint { x: 1.0, y: 2.0 },
    };
    let start_event = NodeGraphGestureEvent::NodeDragStart(start.clone());

    let target = CanvasPoint { x: 32.0, y: 16.0 };
    let update = NodeDragUpdate {
        primary: node_id,
        nodes: vec![node_id],
        pointer: target,
    };
    let update_event = NodeGraphGestureEvent::NodeDragUpdate(update.clone());

    let end = NodeDragEnd {
        primary: node_id,
        nodes: vec![node_id],
        pointer: target,
        outcome: NodeDragEndOutcome::Committed,
    };
    let end_event = NodeGraphGestureEvent::NodeDragEnd(end.clone());
    let scenario = ConformanceScenario::new("node drag gesture transaction callbacks", graph)
        .with_trace_config(ConformanceTraceConfig::with_xyflow_callbacks())
        .with_actions([
            ConformanceAction::emit_gesture(start_event.clone()),
            ConformanceAction::apply_node_drag(node_id, target),
            ConformanceAction::emit_gesture(update_event.clone()),
            ConformanceAction::emit_gesture(end_event.clone()),
        ])
        .with_expected_trace([
            ConformanceTraceEvent::gesture(start_event),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeDragStart(start)),
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
            ConformanceTraceEvent::gesture(update_event),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeDrag(update)),
            ConformanceTraceEvent::gesture(end_event),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeDragEnd(end)),
        ]);

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
        .with_trace_config(ConformanceTraceConfig::with_xyflow_callbacks())
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
        .with_trace_config(ConformanceTraceConfig::with_xyflow_callbacks())
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
    let start_event = NodeGraphGestureEvent::NodeDragStart(start.clone());
    let end = NodeDragEnd {
        primary: node_id,
        nodes: vec![node_id],
        pointer: CanvasPoint { x: 1.0, y: 2.0 },
        outcome: NodeDragEndOutcome::Canceled,
    };
    let end_event = NodeGraphGestureEvent::NodeDragEnd(end.clone());
    let disconnected = EdgeConnection::new(edge_id, out_port, in_port, EdgeKind::Data);

    let scenario = ConformanceScenario::new("delete during active node drag", graph)
        .with_view_state(view_state)
        .with_trace_config(ConformanceTraceConfig::with_xyflow_callbacks())
        .with_actions([
            ConformanceAction::emit_gesture(start_event.clone()),
            ConformanceAction::apply_delete_selection_for_key(keyboard_types::Code::Backspace),
            ConformanceAction::emit_gesture(end_event.clone()),
        ])
        .with_expected_trace([
            ConformanceTraceEvent::gesture(start_event),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeDragStart(start)),
            ConformanceTraceEvent::graph_commit(
                Some(DELETE_SELECTION_TRANSACTION_LABEL),
                ["remove_node"],
            ),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::GraphCommit {
                label: Some(DELETE_SELECTION_TRANSACTION_LABEL.to_owned()),
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeEdgeChanges {
                nodes: 1,
                edges: 1,
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodesChange { count: 1 }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::EdgesChange { count: 1 }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ConnectionChange(
                ConnectionChange::Disconnected(disconnected),
            )),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::Disconnect(disconnected)),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodesDelete { count: 1 }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::EdgesDelete { count: 1 }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::Delete {
                nodes: 1,
                edges: 1,
                groups: 0,
                sticky_notes: 0,
            }),
            ConformanceTraceEvent::selection(Vec::new(), Vec::new(), Vec::new()),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewChange {
                changes: vec![ConformanceViewChange::Selection {
                    nodes: Vec::new(),
                    edges: Vec::new(),
                    groups: Vec::new(),
                }],
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::SelectionChange {
                nodes: Vec::new(),
                edges: Vec::new(),
                groups: Vec::new(),
            }),
            ConformanceTraceEvent::gesture(end_event),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeDragEnd(end)),
        ]);

    assert_conformance_trace(&scenario);
}
