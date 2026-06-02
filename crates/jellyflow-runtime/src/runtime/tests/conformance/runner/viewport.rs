use super::*;
use crate::io::NodeGraphPanOnDragButtons;
use crate::runtime::viewport::{
    ViewportDragPanInput, ViewportGestureContext, ViewportGestureRejection, ViewportPointerButton,
};

fn viewport_drag_rejection_scenario(
    name: &'static str,
    pan_on_drag: NodeGraphPanOnDragButtons,
    context: ViewportGestureContext,
    input: ViewportDragPanInput,
    rejection: ViewportGestureRejection,
) -> ConformanceScenario {
    let (graph, _node_id, _b, _out_port, _in_port, _edge_id) = make_graph();
    let mut editor_config = crate::io::NodeGraphEditorConfig::default();
    editor_config.interaction.pan_on_drag = pan_on_drag;

    ConformanceScenario::new(name, graph)
        .with_editor_config(editor_config)
        .with_trace_config(ConformanceTraceConfig::with_xyflow_callbacks())
        .with_actions([
            ConformanceAction::expect_viewport_drag_pan_gesture_rejected(context, input, rejection),
        ])
        .with_expected_trace([])
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
fn conformance_runner_rejects_viewport_drag_pan_when_selection_modifier_claims_pointer() {
    let scenario = viewport_drag_rejection_scenario(
        "viewport selection modifier suppresses drag pan",
        NodeGraphPanOnDragButtons {
            left: true,
            middle: false,
            right: false,
        },
        ViewportGestureContext {
            selection_key_pressed: true,
            ..ViewportGestureContext::idle()
        },
        ViewportDragPanInput::new(ViewportPointerButton::Left, CanvasPoint { x: 10.0, y: 4.0 }),
        ViewportGestureRejection::PanOnDragDisabled,
    );

    let report = run_conformance_scenario(&scenario).expect("fixture should run");

    assert!(report.is_match(), "{report}");
    assert!(report.actual_trace().is_empty());
}

#[test]
fn conformance_runner_rejects_viewport_drag_pan_when_connection_claims_pointer() {
    let scenario = viewport_drag_rejection_scenario(
        "viewport connection suppresses drag pan",
        NodeGraphPanOnDragButtons {
            left: false,
            middle: false,
            right: true,
        },
        ViewportGestureContext {
            connection_in_progress: true,
            ..ViewportGestureContext::idle()
        },
        ViewportDragPanInput::new(
            ViewportPointerButton::Right,
            CanvasPoint { x: 10.0, y: 4.0 },
        ),
        ViewportGestureRejection::ConnectionInProgress,
    );

    let report = run_conformance_scenario(&scenario).expect("fixture should run");

    assert!(report.is_match(), "{report}");
    assert!(report.actual_trace().is_empty());
}

#[test]
fn conformance_runner_asserts_viewport_animation_frame_without_trace() {
    let (graph, _node_id, _b, _out_port, _in_port, _edge_id) = make_graph();
    let from = ViewportTransform::new(CanvasPoint { x: 0.0, y: 0.0 }, 1.0).unwrap();
    let to = ViewportTransform::new(CanvasPoint { x: 100.0, y: -50.0 }, 3.0).unwrap();
    let expected = ViewportAnimationFrame {
        elapsed_seconds: 0.5,
        progress: 0.25,
        eased_progress: 0.0625,
        transform: ViewportTransform::new(CanvasPoint { x: 6.25, y: -3.125 }, 1.125).unwrap(),
        done: false,
    };

    let scenario = ConformanceScenario::new("viewport animation frame", graph)
        .with_actions([ConformanceAction::assert_viewport_animation_frame(
            ViewportAnimationRequest::new(from, to, ViewportAnimationOptions::new(2.0)),
            0.5,
            expected,
        )])
        .with_expected_trace([]);

    let report = run_conformance_scenario(&scenario).expect("fixture should run");

    assert!(report.is_match(), "{report}");
    assert!(report.actual_trace().is_empty());
}

#[test]
fn conformance_runner_applies_viewport_animation_frame_with_trace() {
    let (graph, _node_id, _b, _out_port, _in_port, _edge_id) = make_graph();
    let from = ViewportTransform::new(CanvasPoint { x: 0.0, y: 0.0 }, 1.0).unwrap();
    let to = ViewportTransform::new(CanvasPoint { x: 80.0, y: -40.0 }, 2.0).unwrap();
    let pan = CanvasPoint { x: 40.0, y: -20.0 };
    let zoom = 1.5;

    let scenario = ConformanceScenario::new("viewport animation frame apply", graph)
        .with_trace_config(ConformanceTraceConfig::with_xyflow_callbacks())
        .with_actions([ConformanceAction::apply_viewport_animation_frame(
            ViewportAnimationRequest::new(from, to, ViewportAnimationOptions::new(1.0)),
            0.5,
        )])
        .with_expected_trace([
            ConformanceTraceEvent::viewport(pan, zoom),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewChange {
                changes: vec![ConformanceViewChange::Viewport { pan, zoom }],
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ViewportChange { pan, zoom }),
        ]);

    let report = run_conformance_scenario(&scenario).expect("fixture should run");

    assert!(report.is_match(), "{report}");
}

#[test]
fn conformance_runner_asserts_double_click_zoom_plan_without_trace() {
    let (graph, _node_id, _b, _out_port, _in_port, _edge_id) = make_graph();
    let current = ViewportTransform::new(CanvasPoint { x: 10.0, y: 20.0 }, 2.0).unwrap();
    let anchor = CanvasPoint { x: 120.0, y: 60.0 };
    let input = ViewportDoubleClickZoomInput::new(
        current,
        anchor,
        2.0,
        0.5,
        3.0,
        ViewportAnimationOptions::new(0.2),
    );
    let target = ViewportTransform::new(CanvasPoint { x: -10.0, y: 10.0 }, 3.0).unwrap();
    let expected = ViewportAnimationPlan {
        from: current,
        to: target,
        duration_seconds: 0.2,
        easing: ViewportAnimationEasing::CubicInOut,
    };

    let scenario = ConformanceScenario::new("viewport double-click zoom", graph)
        .with_actions([ConformanceAction::assert_viewport_double_click_zoom(
            input, expected,
        )])
        .with_expected_trace([]);

    let report = run_conformance_scenario(&scenario).expect("fixture should run");

    assert!(report.is_match(), "{report}");
    assert!(report.actual_trace().is_empty());
}

#[test]
fn conformance_runner_asserts_double_click_zoom_rejection_without_trace() {
    let (graph, _node_id, _b, _out_port, _in_port, _edge_id) = make_graph();
    let mut editor_config = crate::io::NodeGraphEditorConfig::default();
    editor_config.interaction.zoom_on_double_click = false;
    let current = ViewportTransform::new(CanvasPoint::default(), 1.0).unwrap();

    let scenario = ConformanceScenario::new("viewport double-click zoom rejection", graph)
        .with_editor_config(editor_config)
        .with_actions([
            ConformanceAction::expect_viewport_double_click_zoom_rejected(
                ViewportDoubleClickZoomInput::new(
                    current,
                    CanvasPoint { x: 10.0, y: 10.0 },
                    2.0,
                    0.5,
                    4.0,
                    ViewportAnimationOptions::new(0.2),
                ),
                ViewportGestureRejection::DoubleClickZoomDisabled,
            ),
        ])
        .with_expected_trace([]);

    let report = run_conformance_scenario(&scenario).expect("fixture should run");

    assert!(report.is_match(), "{report}");
    assert!(report.actual_trace().is_empty());
}
