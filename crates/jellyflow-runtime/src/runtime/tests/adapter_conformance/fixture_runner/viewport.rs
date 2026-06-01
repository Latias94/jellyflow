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
