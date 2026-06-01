use super::fixtures::make_graph;

use crate::runtime::conformance::{
    ConformanceAction, ConformanceCallbackEvent, ConformanceScenario, ConformanceTraceConfig,
    ConformanceTraceEvent, run_conformance_scenario,
};
use crate::runtime::drag::NODE_DRAG_TRANSACTION_LABEL;
use crate::runtime::events::{NodeDragStart, NodeDragUpdate, NodeGraphGestureEvent};
use jellyflow_core::core::CanvasPoint;

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
