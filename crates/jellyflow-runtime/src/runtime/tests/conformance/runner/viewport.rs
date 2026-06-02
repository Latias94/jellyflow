use super::*;
use crate::io::NodeGraphPanOnDragButtons;
use crate::runtime::viewport::{
    ViewportDragPanInput, ViewportGestureContext, ViewportGestureRejection, ViewportPointerButton,
};

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
fn conformance_runner_rejects_viewport_drag_pan_when_selection_modifier_claims_pointer() {
    let (graph, _node_id, _b, _out_port, _in_port, _edge_id) = make_graph();
    let mut editor_config = crate::io::NodeGraphEditorConfig::default();
    editor_config.interaction.pan_on_drag = NodeGraphPanOnDragButtons {
        left: true,
        middle: false,
        right: false,
    };

    let scenario =
        ConformanceScenario::new("viewport selection modifier suppresses drag pan", graph)
            .with_editor_config(editor_config)
            .with_trace_config(ConformanceTraceConfig::with_xyflow_callbacks())
            .with_actions([
                ConformanceAction::expect_viewport_drag_pan_gesture_rejected(
                    ViewportGestureContext {
                        selection_key_pressed: true,
                        ..ViewportGestureContext::idle()
                    },
                    ViewportDragPanInput::new(
                        ViewportPointerButton::Left,
                        CanvasPoint { x: 10.0, y: 4.0 },
                    ),
                    ViewportGestureRejection::PanOnDragDisabled,
                ),
            ])
            .with_expected_trace([]);

    let report = run_conformance_scenario(&scenario).expect("fixture should run");

    assert!(report.is_match(), "{report}");
    assert!(report.actual_trace().is_empty());
}
