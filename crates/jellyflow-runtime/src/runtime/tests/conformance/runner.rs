use super::super::fixtures::make_graph;

use crate::runtime::auto_pan::{AutoPanActivation, AutoPanRequest};
use crate::runtime::conformance::{
    ConformanceAction, ConformanceCallbackEvent, ConformanceScenario, ConformanceSuite,
    ConformanceTraceConfig, ConformanceTraceEvent, ConformanceViewChange, run_conformance_scenario,
    run_conformance_suite,
};
use crate::runtime::drag::NODE_DRAG_TRANSACTION_LABEL;
use crate::runtime::events::{
    NodeDragStart, NodeDragUpdate, NodeGraphGestureEvent, ViewportMove, ViewportMoveEnd,
    ViewportMoveEndOutcome, ViewportMoveKind, ViewportMoveStart,
};
use crate::runtime::viewport::{ViewportPanRequest, ViewportZoomRequest};
use jellyflow_core::core::{CanvasPoint, CanvasSize};

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

#[test]
fn conformance_runner_records_viewport_pan_zoom_fixture_and_callbacks() {
    let (graph, _node_id, _b, _out_port, _in_port, _edge_id) = make_graph();

    let pan_start = ViewportMoveStart {
        kind: ViewportMoveKind::PanDrag,
        pan: CanvasPoint::default(),
        zoom: 1.0,
    };
    let pan_start_event = NodeGraphGestureEvent::ViewportMoveStart(pan_start);
    let pan = CanvasPoint { x: 40.0, y: -10.0 };
    let pan_update = ViewportMove {
        kind: ViewportMoveKind::PanDrag,
        pan,
        zoom: 1.0,
    };
    let pan_update_event = NodeGraphGestureEvent::ViewportMove(pan_update);
    let pan_end = ViewportMoveEnd {
        kind: ViewportMoveKind::PanDrag,
        pan,
        zoom: 1.0,
        outcome: ViewportMoveEndOutcome::Ended,
    };
    let pan_end_event = NodeGraphGestureEvent::ViewportMoveEnd(pan_end);

    let zoom_start = ViewportMoveStart {
        kind: ViewportMoveKind::ZoomWheel,
        pan,
        zoom: 1.0,
    };
    let zoom_start_event = NodeGraphGestureEvent::ViewportMoveStart(zoom_start);
    let zoomed_pan = CanvasPoint { x: -10.0, y: -35.0 };
    let zoom_update = ViewportMove {
        kind: ViewportMoveKind::ZoomWheel,
        pan: zoomed_pan,
        zoom: 2.0,
    };
    let zoom_update_event = NodeGraphGestureEvent::ViewportMove(zoom_update);
    let zoom_end = ViewportMoveEnd {
        kind: ViewportMoveKind::ZoomWheel,
        pan: zoomed_pan,
        zoom: 2.0,
        outcome: ViewportMoveEndOutcome::Ended,
    };
    let zoom_end_event = NodeGraphGestureEvent::ViewportMoveEnd(zoom_end);

    let scenario = ConformanceScenario::new("viewport pan zoom fixture", graph)
        .with_trace_config(ConformanceTraceConfig::with_xyflow_callbacks())
        .with_actions([
            ConformanceAction::emit_gesture(pan_start_event.clone()),
            ConformanceAction::apply_viewport_pan(ViewportPanRequest::new(CanvasPoint {
                x: 40.0,
                y: -10.0,
            })),
            ConformanceAction::emit_gesture(pan_update_event.clone()),
            ConformanceAction::emit_gesture(pan_end_event.clone()),
            ConformanceAction::emit_gesture(zoom_start_event.clone()),
            ConformanceAction::apply_viewport_zoom(ViewportZoomRequest::new(
                CanvasPoint { x: 100.0, y: 50.0 },
                2.0,
                0.5,
                4.0,
            )),
            ConformanceAction::emit_gesture(zoom_update_event.clone()),
            ConformanceAction::emit_gesture(zoom_end_event.clone()),
        ])
        .with_expected_trace([
            ConformanceTraceEvent::gesture(pan_start_event),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewportMoveStart(pan_start)),
            ConformanceTraceEvent::viewport(pan, 1.0),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewChange {
                changes: vec![ConformanceViewChange::Viewport { pan, zoom: 1.0 }],
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewportChange {
                pan,
                zoom: 1.0,
            }),
            ConformanceTraceEvent::gesture(pan_update_event),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewportMove(pan_update)),
            ConformanceTraceEvent::gesture(pan_end_event),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewportMoveEnd(pan_end)),
            ConformanceTraceEvent::gesture(zoom_start_event),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewportMoveStart(
                zoom_start,
            )),
            ConformanceTraceEvent::viewport(zoomed_pan, 2.0),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewChange {
                changes: vec![ConformanceViewChange::Viewport {
                    pan: zoomed_pan,
                    zoom: 2.0,
                }],
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewportChange {
                pan: zoomed_pan,
                zoom: 2.0,
            }),
            ConformanceTraceEvent::gesture(zoom_update_event),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewportMove(zoom_update)),
            ConformanceTraceEvent::gesture(zoom_end_event),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewportMoveEnd(zoom_end)),
        ]);

    let report = run_conformance_scenario(&scenario).expect("fixture should run");

    assert!(report.is_match(), "{report}");
}

#[test]
fn conformance_runner_records_auto_pan_fixture_and_callbacks() {
    let (graph, _node_id, _b, _out_port, _in_port, _edge_id) = make_graph();
    let mut editor_config = crate::io::NodeGraphEditorConfig::default();
    editor_config.interaction.auto_pan.speed = 100.0;
    editor_config.interaction.auto_pan.margin = 20.0;

    let pan = CanvasPoint { x: -50.0, y: 0.0 };
    let scenario = ConformanceScenario::new("auto-pan fixture", graph)
        .with_editor_config(editor_config)
        .with_trace_config(ConformanceTraceConfig::with_xyflow_callbacks())
        .with_actions([ConformanceAction::apply_auto_pan(AutoPanRequest::new(
            AutoPanActivation::Always,
            CanvasPoint { x: 190.0, y: 50.0 },
            CanvasSize {
                width: 200.0,
                height: 100.0,
            },
            1.0,
        ))])
        .with_expected_trace([
            ConformanceTraceEvent::viewport(pan, 1.0),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewChange {
                changes: vec![ConformanceViewChange::Viewport { pan, zoom: 1.0 }],
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewportChange {
                pan,
                zoom: 1.0,
            }),
        ]);

    let report = run_conformance_scenario(&scenario).expect("fixture should run");

    assert!(report.is_match(), "{report}");
}

#[test]
fn conformance_suite_runs_all_scenarios_and_reports_mismatches() {
    let (graph, _node_id, _b, _out_port, _in_port, _edge_id) = make_graph();
    let matching = ConformanceScenario::new("matching viewport", graph.clone())
        .with_actions([ConformanceAction::set_viewport(
            CanvasPoint { x: 1.0, y: 2.0 },
            1.25,
        )])
        .with_expected_trace([ConformanceTraceEvent::viewport(
            CanvasPoint { x: 1.0, y: 2.0 },
            1.25,
        )]);
    let mismatched = ConformanceScenario::new("mismatched viewport", graph)
        .with_actions([ConformanceAction::set_viewport(
            CanvasPoint { x: 3.0, y: 4.0 },
            1.5,
        )])
        .with_expected_trace([ConformanceTraceEvent::viewport(
            CanvasPoint { x: 30.0, y: 40.0 },
            1.5,
        )]);
    let suite =
        ConformanceSuite::new("adapter viewport suite").with_scenarios([matching, mismatched]);

    let report = run_conformance_suite(&suite);

    assert!(!report.is_match(), "{report}");
    assert_eq!(report.scenario_reports.len(), 2);
    assert_eq!(report.errors.len(), 0);
    assert_eq!(report.failed_scenarios(), 1);
    assert!(report.to_string().contains("adapter viewport suite"));
    assert!(report.to_string().contains("mismatched viewport"));
}

#[test]
fn conformance_suite_captures_action_errors_without_aborting_later_scenarios() {
    let (graph, _node_id, _b, _out_port, _in_port, _edge_id) = make_graph();
    let rejected = ConformanceScenario::new("rejected pan", graph.clone()).with_actions([
        ConformanceAction::apply_viewport_pan(ViewportPanRequest::new(CanvasPoint {
            x: f32::NAN,
            y: 0.0,
        })),
    ]);
    let matching = ConformanceScenario::new("later matching viewport", graph)
        .with_actions([ConformanceAction::set_viewport(
            CanvasPoint { x: 1.0, y: 2.0 },
            1.25,
        )])
        .with_expected_trace([ConformanceTraceEvent::viewport(
            CanvasPoint { x: 1.0, y: 2.0 },
            1.25,
        )]);
    let suite = ConformanceSuite::new("adapter error suite").with_scenarios([rejected, matching]);

    let report = run_conformance_suite(&suite);

    assert!(!report.is_match(), "{report}");
    assert_eq!(report.scenario_reports.len(), 1);
    assert_eq!(report.errors.len(), 1);
    assert_eq!(report.errors[0].scenario, "rejected pan");
    assert!(report.to_string().contains("rejected pan"));
}
