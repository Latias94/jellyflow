use super::super::fixtures::make_graph;
use super::support::{assert_conformance_trace, insert_input_port};

use crate::rules::plan_connect;
use crate::runtime::auto_pan::{AutoPanActivation, AutoPanRequest};
use crate::runtime::conformance::{
    ConformanceAction, ConformanceCallbackEvent, ConformanceScenario, ConformanceTraceConfig,
    ConformanceTraceEvent,
};
use crate::runtime::drag::NODE_DRAG_TRANSACTION_LABEL;
use crate::runtime::events::{
    ConnectDragKind, ConnectEnd, ConnectEndOutcome, ConnectStart, NodeDragEnd, NodeDragEndOutcome,
    NodeDragStart, NodeDragUpdate, NodeGraphGestureEvent,
};
use crate::runtime::xyflow::callbacks::{ConnectionChange, EdgeConnection};
use jellyflow_core::core::{CanvasPoint, CanvasSize, EdgeKind};
use jellyflow_core::interaction::NodeGraphConnectionMode;
use jellyflow_core::ops::{GraphOp, GraphTransaction};

#[test]
fn adapter_conformance_fixture_runner_records_viewport_and_selection_ordering() {
    let (graph, node_id, _b, _out_port, _in_port, edge_id) = make_graph();
    let scenario = ConformanceScenario::new("viewport and selection ordering", graph)
        .with_actions([
            ConformanceAction::set_viewport(CanvasPoint { x: 10.0, y: 20.0 }, 1.25),
            ConformanceAction::set_selection(vec![node_id], vec![edge_id], Vec::new()),
        ])
        .with_expected_trace([
            ConformanceTraceEvent::viewport(CanvasPoint { x: 10.0, y: 20.0 }, 1.25),
            ConformanceTraceEvent::selection(vec![node_id], vec![edge_id], Vec::new()),
        ]);

    assert_conformance_trace(&scenario);
}

#[test]
fn adapter_conformance_fixture_runner_records_auto_pan_frame() {
    let (graph, _node_id, _b, _out_port, _in_port, _edge_id) = make_graph();
    let mut editor_config = crate::io::NodeGraphEditorConfig::default();
    editor_config.interaction.auto_pan.speed = 100.0;
    editor_config.interaction.auto_pan.margin = 20.0;
    let pan = CanvasPoint { x: -50.0, y: 0.0 };

    let scenario = ConformanceScenario::new("auto-pan frame", graph)
        .with_editor_config(editor_config)
        .with_actions([ConformanceAction::apply_auto_pan(AutoPanRequest::new(
            AutoPanActivation::Always,
            CanvasPoint { x: 190.0, y: 50.0 },
            CanvasSize {
                width: 200.0,
                height: 100.0,
            },
            1.0,
        ))])
        .with_expected_trace([ConformanceTraceEvent::viewport(pan, 1.0)]);

    assert_conformance_trace(&scenario);
}

#[test]
fn adapter_conformance_fixture_runner_records_connect_gesture_lifecycle() {
    let (graph, _a, _b, out_port, in_port, _eid) = make_graph();
    let kind = ConnectDragKind::New {
        from: out_port,
        bundle: vec![out_port],
    };
    let start = NodeGraphGestureEvent::ConnectStart(ConnectStart {
        kind: kind.clone(),
        mode: NodeGraphConnectionMode::Strict,
    });
    let end = NodeGraphGestureEvent::ConnectEnd(ConnectEnd {
        kind,
        mode: NodeGraphConnectionMode::Strict,
        target: Some(in_port),
        outcome: ConnectEndOutcome::Committed,
    });
    let scenario = ConformanceScenario::new("connect gesture lifecycle", graph)
        .with_actions([
            ConformanceAction::emit_gesture(start.clone()),
            ConformanceAction::emit_gesture(end.clone()),
        ])
        .with_expected_trace([
            ConformanceTraceEvent::gesture(start),
            ConformanceTraceEvent::gesture(end),
        ]);

    assert_conformance_trace(&scenario);
}

#[test]
fn adapter_conformance_fixture_runner_records_connect_gesture_transaction_and_callbacks() {
    let (mut graph, _a, b, out_port, _in_port, _eid) = make_graph();
    let next_in = insert_input_port(&mut graph, b, "in2");
    let kind = ConnectDragKind::New {
        from: out_port,
        bundle: vec![out_port],
    };
    let start = ConnectStart {
        kind: kind.clone(),
        mode: NodeGraphConnectionMode::Strict,
    };
    let start_event = NodeGraphGestureEvent::ConnectStart(start.clone());

    let plan = plan_connect(&graph, out_port, next_in);
    assert!(plan.is_accept(), "connect gesture fixture should accept");
    let tx = GraphTransaction::from_ops(plan.into_ops()).with_label("connect gesture commit");
    let (edge_id, edge) = match tx.ops() {
        [GraphOp::AddEdge { id, edge }] => (*id, edge.clone()),
        other => panic!("expected single add-edge op, got {other:#?}"),
    };
    let connection = EdgeConnection::new(edge_id, out_port, next_in, EdgeKind::Data);

    let end = ConnectEnd {
        kind,
        mode: NodeGraphConnectionMode::Strict,
        target: Some(next_in),
        outcome: ConnectEndOutcome::Committed,
    };
    let end_event = NodeGraphGestureEvent::ConnectEnd(end.clone());

    assert_eq!(edge.from, out_port);
    assert_eq!(edge.to, next_in);
    let scenario = ConformanceScenario::new("connect gesture transaction callbacks", graph)
        .with_trace_config(ConformanceTraceConfig::with_xyflow_callbacks())
        .with_actions([
            ConformanceAction::emit_gesture(start_event.clone()),
            ConformanceAction::dispatch_transaction(tx),
            ConformanceAction::emit_gesture(end_event.clone()),
        ])
        .with_expected_trace([
            ConformanceTraceEvent::gesture(start_event),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ConnectStart(start)),
            ConformanceTraceEvent::graph_commit(Some("connect gesture commit"), ["add_edge"]),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::GraphCommit {
                label: Some("connect gesture commit".to_owned()),
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeEdgeChanges {
                nodes: 0,
                edges: 1,
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::EdgesChange { count: 1 }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ConnectionChange(
                ConnectionChange::Connected(connection),
            )),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::Connect(connection)),
            ConformanceTraceEvent::gesture(end_event),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ConnectEnd(end)),
        ]);

    assert_conformance_trace(&scenario);
}

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
