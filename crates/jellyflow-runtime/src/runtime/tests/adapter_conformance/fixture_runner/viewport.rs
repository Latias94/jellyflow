use super::*;

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
fn adapter_conformance_fixture_runner_applies_viewport_scroll_gesture_policy() {
    let (graph, _node_id, _b, _out_port, _in_port, _edge_id) = make_graph();
    let mut editor_config = crate::io::NodeGraphEditorConfig::default();
    editor_config.interaction.pan_on_scroll = true;
    editor_config.interaction.pan_on_scroll_speed = 2.0;
    editor_config.interaction.pan_on_scroll_mode = NodeGraphPanOnScrollMode::Horizontal;

    let scenario = ConformanceScenario::new("viewport scroll gesture policy", graph)
        .with_editor_config(editor_config)
        .with_actions([ConformanceAction::apply_viewport_scroll_gesture(
            ViewportGestureContext::idle(),
            ViewportScrollInput::new(
                CanvasPoint { x: 12.0, y: -8.0 },
                CanvasPoint { x: 80.0, y: 20.0 },
                false,
                2.0,
                0.25,
                4.0,
            ),
        )])
        .with_expected_trace([ConformanceTraceEvent::viewport(
            CanvasPoint { x: -24.0, y: 0.0 },
            1.0,
        )]);

    assert_conformance_trace(&scenario);
}

#[test]
fn adapter_conformance_fixture_runner_checks_viewport_gesture_rejections() {
    let (graph, _node_id, _b, _out_port, _in_port, _edge_id) = make_graph();
    let mut editor_config = crate::io::NodeGraphEditorConfig::default();
    editor_config.interaction.pan_on_drag = NodeGraphPanOnDragButtons {
        left: false,
        middle: false,
        right: true,
    };

    let scenario = ConformanceScenario::new("viewport gesture rejection policy", graph)
        .with_editor_config(editor_config)
        .with_actions([
            ConformanceAction::apply_viewport_drag_pan_gesture(
                ViewportGestureContext::idle(),
                ViewportDragPanInput::new(
                    ViewportPointerButton::Right,
                    CanvasPoint { x: 10.0, y: 4.0 },
                ),
            ),
            ConformanceAction::expect_viewport_drag_pan_gesture_rejected(
                ViewportGestureContext {
                    connection_in_progress: true,
                    ..ViewportGestureContext::idle()
                },
                ViewportDragPanInput::new(
                    ViewportPointerButton::Right,
                    CanvasPoint { x: 10.0, y: 4.0 },
                ),
                ViewportGestureRejection::ConnectionInProgress,
            ),
            ConformanceAction::expect_viewport_scroll_gesture_rejected(
                ViewportGestureContext {
                    user_selection_active: true,
                    ..ViewportGestureContext::idle()
                },
                ViewportScrollInput::new(
                    CanvasPoint { x: 0.0, y: 32.0 },
                    CanvasPoint { x: 20.0, y: 20.0 },
                    false,
                    2.0,
                    0.25,
                    4.0,
                ),
                ViewportGestureRejection::UserSelectionActive,
            ),
        ])
        .with_expected_trace([ConformanceTraceEvent::viewport(
            CanvasPoint { x: 10.0, y: 4.0 },
            1.0,
        )]);

    assert_conformance_trace(&scenario);
}

#[test]
fn adapter_conformance_fixture_runner_asserts_viewport_animation_plans() {
    let (graph, _node_id, _b, _out_port, _in_port, _edge_id) = make_graph();
    let from = ViewportTransform::new(CanvasPoint { x: 0.0, y: 0.0 }, 1.0).unwrap();
    let to = ViewportTransform::new(CanvasPoint { x: 100.0, y: -50.0 }, 3.0).unwrap();
    let expected_frame = ViewportAnimationFrame {
        elapsed_seconds: 0.5,
        progress: 0.25,
        eased_progress: 0.0625,
        transform: ViewportTransform::new(CanvasPoint { x: 6.25, y: -3.125 }, 1.125).unwrap(),
        done: false,
    };

    let double_click_current =
        ViewportTransform::new(CanvasPoint { x: 10.0, y: 20.0 }, 2.0).unwrap();
    let anchor = CanvasPoint { x: 120.0, y: 60.0 };
    let double_click_target =
        ViewportTransform::new(CanvasPoint { x: -10.0, y: 10.0 }, 3.0).unwrap();
    let expected_plan = ViewportAnimationPlan {
        from: double_click_current,
        to: double_click_target,
        duration_seconds: 0.2,
        easing: crate::runtime::viewport::ViewportAnimationEasing::CubicInOut,
    };

    let scenario = ConformanceScenario::new("viewport animation planning", graph)
        .with_actions([
            ConformanceAction::assert_viewport_animation_frame(
                ViewportAnimationRequest::new(from, to, ViewportAnimationOptions::new(2.0)),
                0.5,
                expected_frame,
            ),
            ConformanceAction::assert_viewport_double_click_zoom(
                ViewportDoubleClickZoomInput::new(
                    double_click_current,
                    anchor,
                    2.0,
                    0.5,
                    3.0,
                    ViewportAnimationOptions::new(0.2),
                ),
                expected_plan,
            ),
        ])
        .with_expected_trace([]);

    assert_conformance_trace(&scenario);
}

#[test]
fn adapter_conformance_fixture_runner_applies_viewport_animation_frames() {
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

    assert_conformance_trace(&scenario);
}
