use super::*;
use jellyflow_core::core::{EdgeId, NodeId};

fn node_pointer_down_selection_trace(node_id: NodeId) -> Vec<ConformanceTraceEvent> {
    selection_trace(vec![node_id], Vec::new())
}

fn selection_trace(nodes: Vec<NodeId>, edges: Vec<EdgeId>) -> Vec<ConformanceTraceEvent> {
    vec![
        ConformanceTraceEvent::selection(nodes.clone(), edges.clone(), Vec::new()),
        ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewChange {
            changes: vec![ConformanceViewChange::Selection {
                nodes: nodes.clone(),
                edges: edges.clone(),
                groups: Vec::new(),
            }],
        }),
        ConformanceTraceEvent::callback(ConformanceCallbackEvent::SelectionChange {
            nodes,
            edges,
            groups: Vec::new(),
        }),
    ]
}

fn node_pointer_down_view_state(other: NodeId, edge_id: EdgeId) -> crate::io::NodeGraphViewState {
    let mut view_state = crate::io::NodeGraphViewState::default();
    view_state.set_selection(vec![other], vec![edge_id], Vec::new());
    view_state
}

#[test]
fn conformance_runner_executes_keyboard_nudge_fixture_and_matches_trace() {
    let (graph, node_id, _b, _out_port, _in_port, _edge_id) = make_graph();
    let mut view_state = crate::io::NodeGraphViewState::default();
    view_state.set_selection(vec![node_id], Vec::new(), Vec::new());
    view_state.zoom = 2.0;

    let scenario = ConformanceScenario::new("keyboard nudge runner", graph)
        .with_view_state(view_state)
        .with_trace_config(ConformanceTraceConfig::with_xyflow_callbacks())
        .with_actions([ConformanceAction::apply_node_nudge(NodeNudgeRequest {
            direction: NodeNudgeDirection::Right,
            fast: false,
        })])
        .with_expected_trace([
            ConformanceTraceEvent::graph_commit(
                Some(NODE_NUDGE_TRANSACTION_LABEL),
                ["set_node_pos"],
            ),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::GraphCommit {
                label: Some(NODE_NUDGE_TRANSACTION_LABEL.to_owned()),
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeEdgeChanges {
                nodes: 1,
                edges: 0,
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodesChange { count: 1 }),
        ]);

    let report = run_conformance_scenario(&scenario).expect("fixture should run");

    assert!(report.is_match(), "{report}");
    assert_eq!(report.actual_trace(), scenario.expected_trace.as_slice());
}

#[test]
fn conformance_runner_executes_delete_selection_fixture_and_matches_trace() {
    let (graph, node_id, _b, out_port, in_port, edge_id) = make_graph();
    let mut view_state = crate::io::NodeGraphViewState::default();
    view_state.set_selection(vec![node_id], vec![edge_id], Vec::new());
    let disconnected = EdgeConnection::new(edge_id, out_port, in_port, EdgeKind::Data);

    let scenario = ConformanceScenario::new("delete selection runner", graph)
        .with_view_state(view_state)
        .with_trace_config(ConformanceTraceConfig::with_xyflow_callbacks())
        .with_actions([ConformanceAction::apply_delete_selection_for_key(
            keyboard_types::Code::Backspace,
        )])
        .with_expected_trace([
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
        ]);

    let report = run_conformance_scenario(&scenario).expect("fixture should run");

    assert!(report.is_match(), "{report}");
    assert_eq!(report.actual_trace(), scenario.expected_trace.as_slice());
}

#[test]
fn conformance_runner_executes_node_drag_fixture_and_matches_trace() {
    let (graph, node_id, _b, _out_port, _in_port, _edge_id) = make_graph();
    let start = NodeDragStart {
        primary: node_id,
        nodes: vec![node_id],
        pointer: CanvasPoint { x: 1.0, y: 2.0 },
    };
    let target = CanvasPoint { x: 32.0, y: 16.0 };
    let update = NodeDragUpdate {
        primary: node_id,
        nodes: vec![node_id],
        pointer: target,
    };
    let start_event = NodeGraphGestureEvent::NodeDragStart(start.clone());
    let update_event = NodeGraphGestureEvent::NodeDragUpdate(update.clone());

    let scenario = ConformanceScenario::new("node drag runner", graph)
        .with_trace_config(ConformanceTraceConfig::with_xyflow_callbacks())
        .with_actions([
            ConformanceAction::emit_gesture(start_event.clone()),
            ConformanceAction::apply_node_drag(node_id, target),
            ConformanceAction::emit_gesture(update_event.clone()),
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
        ]);

    let report = run_conformance_scenario(&scenario).expect("fixture should run");

    assert!(report.is_match(), "{report}");
    assert_eq!(report.actual_trace(), scenario.expected_trace.as_slice());
    assert!(report.mismatches().is_empty());
}

#[test]
fn conformance_runner_executes_selection_box_fixture_and_matches_trace() {
    let (graph, node_id, other, _out_port, _in_port, edge_id) = make_graph();
    let view_state = node_pointer_down_view_state(other, edge_id);
    let rect = CanvasRect {
        origin: CanvasPoint { x: -5.0, y: -5.0 },
        size: CanvasSize {
            width: 20.0,
            height: 20.0,
        },
    };

    let scenario = ConformanceScenario::new("selection box runner", graph)
        .with_view_state(view_state)
        .with_trace_config(ConformanceTraceConfig::with_xyflow_callbacks())
        .with_actions([ConformanceAction::apply_selection_box(
            SelectionBoxInput::new(
                rect,
                SelectionBoxOptions {
                    fallback_size: Some(CanvasSize {
                        width: 10.0,
                        height: 10.0,
                    }),
                    ..SelectionBoxOptions::default()
                },
            ),
        )])
        .with_expected_trace(selection_trace(vec![node_id], vec![edge_id]));

    let report = run_conformance_scenario(&scenario).expect("fixture should run");

    assert!(report.is_match(), "{report}");
    assert_eq!(report.actual_trace(), scenario.expected_trace.as_slice());
}

#[test]
fn conformance_runner_reports_compact_trace_mismatches() {
    let (graph, node_id, _b, _out_port, _in_port, _edge_id) = make_graph();
    let target = CanvasPoint { x: 32.0, y: 16.0 };
    let scenario = ConformanceScenario::new("node drag mismatch", graph)
        .with_actions([ConformanceAction::apply_node_drag(node_id, target)])
        .with_expected_trace([ConformanceTraceEvent::graph_commit(
            Some(NODE_DRAG_TRANSACTION_LABEL),
            ["wrong_op_kind"],
        )]);

    let report = run_conformance_scenario(&scenario).expect("fixture should run");
    let rendered = report.to_string();

    assert!(!report.is_match());
    assert_eq!(report.mismatches().len(), 1);
    assert_eq!(report.mismatches()[0].index, 0);
    assert!(rendered.contains("node drag mismatch"));
    assert!(rendered.contains("wrong_op_kind"));
    assert!(rendered.contains("set_node_pos"));
}

#[test]
fn conformance_runner_errors_when_mutating_action_produces_no_commit() {
    let (graph, _node_id, _b, _out_port, _in_port, _edge_id) = make_graph();
    let scenario = ConformanceScenario::new("empty delete runner", graph)
        .with_actions([ConformanceAction::apply_delete_selection()]);

    let err = run_conformance_scenario(&scenario).expect_err("empty delete should error");

    assert_eq!(err.scenario, "empty delete runner");
    assert_eq!(err.action_index, 0);
    assert_eq!(err.action_kind, "apply_delete_selection");
    assert!(err.message.contains("produced no commit"));
}

#[test]
fn conformance_runner_executes_node_pointer_down_fixture_and_matches_selection_trace() {
    let (graph, node_id, other, _out_port, _in_port, edge_id) = make_graph();
    let view_state = node_pointer_down_view_state(other, edge_id);

    let scenario = ConformanceScenario::new("node pointer down runner", graph)
        .with_view_state(view_state)
        .with_trace_config(ConformanceTraceConfig::with_xyflow_callbacks())
        .with_actions([ConformanceAction::apply_node_pointer_down(
            node_id,
            false,
            CanvasPoint { x: 3.0, y: 4.0 },
        )])
        .with_expected_trace(node_pointer_down_selection_trace(node_id));

    let report = run_conformance_scenario(&scenario).expect("fixture should run");

    assert!(report.is_match(), "{report}");
    assert_eq!(report.actual_trace(), scenario.expected_trace.as_slice());
}

#[test]
fn conformance_runner_executes_node_pointer_down_then_drag_chain() {
    let (graph, node_id, other, _out_port, _in_port, edge_id) = make_graph();
    let view_state = node_pointer_down_view_state(other, edge_id);

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

    let mut expected_trace = node_pointer_down_selection_trace(node_id);
    expected_trace.extend([
        ConformanceTraceEvent::gesture(start_event.clone()),
        ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeDragStart(start.clone())),
        ConformanceTraceEvent::graph_commit(Some(NODE_DRAG_TRANSACTION_LABEL), ["set_node_pos"]),
        ConformanceTraceEvent::callback(ConformanceCallbackEvent::GraphCommit {
            label: Some(NODE_DRAG_TRANSACTION_LABEL.to_owned()),
        }),
        ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeEdgeChanges {
            nodes: 1,
            edges: 0,
        }),
        ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodesChange { count: 1 }),
        ConformanceTraceEvent::gesture(update_event.clone()),
        ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeDrag(update.clone())),
    ]);

    let scenario = ConformanceScenario::new("node pointer down drag chain", graph)
        .with_view_state(view_state)
        .with_trace_config(ConformanceTraceConfig::with_xyflow_callbacks())
        .with_actions([
            ConformanceAction::apply_node_pointer_down(
                node_id,
                false,
                CanvasPoint { x: 3.0, y: 4.0 },
            ),
            ConformanceAction::emit_gesture(start_event.clone()),
            ConformanceAction::apply_node_drag(node_id, target),
            ConformanceAction::emit_gesture(update_event.clone()),
        ])
        .with_expected_trace(expected_trace);

    let report = run_conformance_scenario(&scenario).expect("fixture should run");

    assert!(report.is_match(), "{report}");
    assert_eq!(report.actual_trace(), scenario.expected_trace.as_slice());
}
