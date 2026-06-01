use super::*;

#[test]
fn pane_click_distance_matches_xyflow_suppression_policy() {
    assert_eq!(
        resolve_pane_click_distance(PaneClickDistanceInput::new(7.0, false)),
        7.0
    );
    assert_eq!(
        resolve_pane_click_distance(PaneClickDistanceInput::new(-1.0, false)),
        0.0
    );
    assert_eq!(
        resolve_pane_click_distance(PaneClickDistanceInput::new(f32::NAN, false)),
        0.0
    );
    assert!(
        resolve_pane_click_distance(PaneClickDistanceInput::new(7.0, true)).is_infinite(),
        "selection-on-drag suppresses pane clicks"
    );
}

#[test]
fn viewport_scroll_policy_maps_pan_on_scroll_to_screen_delta() {
    let state = NodeGraphInteractionState {
        pan_on_scroll: true,
        pan_on_scroll_speed: 2.0,
        pan_on_scroll_mode: NodeGraphPanOnScrollMode::Horizontal,
        ..NodeGraphInteractionState::default()
    };

    let intent = resolve_viewport_scroll_gesture(
        &state.pan_interaction(),
        &state.zoom_interaction(),
        ViewportGestureContext::idle(),
        ViewportScrollInput::new(
            CanvasPoint { x: 12.0, y: -8.0 },
            CanvasPoint { x: 80.0, y: 20.0 },
            false,
            2.0,
            0.25,
            4.0,
        ),
    )
    .expect("pan-on-scroll intent");

    assert_eq!(intent.move_kind(), ViewportMoveKind::PanScroll);
    assert_eq!(
        intent,
        ViewportGestureIntent::Pan {
            kind: ViewportMoveKind::PanScroll,
            request: ViewportPanRequest::new(CanvasPoint { x: -24.0, y: 0.0 }),
        }
    );
}

#[test]
fn viewport_scroll_policy_prioritizes_zoom_activation_and_pinch() {
    let state = NodeGraphInteractionState {
        pan_on_scroll: true,
        zoom_on_scroll: false,
        zoom_on_pinch: true,
        ..NodeGraphInteractionState::default()
    };

    let zoom_by_activation = resolve_viewport_scroll_gesture(
        &state.pan_interaction(),
        &state.zoom_interaction(),
        ViewportGestureContext {
            zoom_activation_key_pressed: true,
            ..ViewportGestureContext::idle()
        },
        ViewportScrollInput::new(
            CanvasPoint { x: 0.0, y: 32.0 },
            CanvasPoint { x: 50.0, y: 25.0 },
            false,
            1.75,
            0.25,
            4.0,
        ),
    )
    .expect("activation-key zoom intent");
    assert_eq!(zoom_by_activation.move_kind(), ViewportMoveKind::ZoomWheel);

    let pinch = resolve_viewport_scroll_gesture(
        &state.pan_interaction(),
        &state.zoom_interaction(),
        ViewportGestureContext::idle(),
        ViewportScrollInput::new(
            CanvasPoint { x: 0.0, y: 32.0 },
            CanvasPoint { x: 50.0, y: 25.0 },
            true,
            2.0,
            0.25,
            4.0,
        ),
    )
    .expect("pinch zoom intent");
    assert_eq!(pinch.move_kind(), ViewportMoveKind::ZoomPinch);

    let pinch_disabled_state = NodeGraphInteractionState {
        zoom_on_pinch: false,
        ..state
    };
    let err = resolve_viewport_scroll_gesture(
        &pinch_disabled_state.pan_interaction(),
        &pinch_disabled_state.zoom_interaction(),
        ViewportGestureContext::idle(),
        ViewportScrollInput::new(
            CanvasPoint { x: 0.0, y: 32.0 },
            CanvasPoint { x: 50.0, y: 25.0 },
            true,
            2.0,
            0.25,
            4.0,
        ),
    )
    .expect_err("pinch should be rejected when disabled");
    assert_eq!(err, ViewportGestureRejection::PinchDisabled);
}

#[test]
fn viewport_drag_pan_policy_respects_buttons_and_context() {
    let state = NodeGraphInteractionState {
        pan_on_drag: NodeGraphPanOnDragButtons {
            left: false,
            middle: false,
            right: true,
        },
        ..NodeGraphInteractionState::default()
    };
    let pan = state.pan_interaction();

    let accepted = resolve_viewport_drag_pan_gesture(
        &pan,
        ViewportGestureContext::idle(),
        ViewportDragPanInput::new(
            ViewportPointerButton::Right,
            CanvasPoint { x: -3.0, y: 7.0 },
        ),
    )
    .expect("right-button pan");
    assert_eq!(
        accepted,
        ViewportGestureIntent::Pan {
            kind: ViewportMoveKind::PanDrag,
            request: ViewportPanRequest::new(CanvasPoint { x: -3.0, y: 7.0 }),
        }
    );

    let wrong_button = resolve_viewport_drag_pan_gesture(
        &pan,
        ViewportGestureContext::idle(),
        ViewportDragPanInput::new(
            ViewportPointerButton::Middle,
            CanvasPoint { x: -3.0, y: 7.0 },
        ),
    )
    .expect_err("middle button is disabled");
    assert_eq!(
        wrong_button,
        ViewportGestureRejection::PanOnDragButtonDisabled
    );

    let connection_block = resolve_viewport_drag_pan_gesture(
        &pan,
        ViewportGestureContext {
            connection_in_progress: true,
            ..ViewportGestureContext::idle()
        },
        ViewportDragPanInput::new(
            ViewportPointerButton::Right,
            CanvasPoint { x: -3.0, y: 7.0 },
        ),
    )
    .expect_err("connection gestures block drag-pan");
    assert_eq!(
        connection_block,
        ViewportGestureRejection::ConnectionInProgress
    );

    let selection_block = resolve_viewport_drag_pan_gesture(
        &pan,
        ViewportGestureContext {
            user_selection_active: true,
            ..ViewportGestureContext::idle()
        },
        ViewportDragPanInput::new(
            ViewportPointerButton::Right,
            CanvasPoint { x: -3.0, y: 7.0 },
        ),
    )
    .expect_err("selection gestures block drag-pan");
    assert_eq!(
        selection_block,
        ViewportGestureRejection::UserSelectionActive
    );
}

#[test]
fn viewport_scroll_policy_reports_disabled_and_selection_rejections() {
    let disabled = NodeGraphInteractionState {
        pan_on_scroll: false,
        pan_on_drag: NodeGraphPanOnDragButtons::default(),
        zoom_on_scroll: false,
        zoom_on_pinch: false,
        zoom_on_double_click: false,
        ..NodeGraphInteractionState::default()
    };
    let input = ViewportScrollInput::new(
        CanvasPoint { x: 1.0, y: 2.0 },
        CanvasPoint { x: 10.0, y: 10.0 },
        false,
        1.5,
        0.25,
        4.0,
    );

    let err = resolve_viewport_scroll_gesture(
        &disabled.pan_interaction(),
        &disabled.zoom_interaction(),
        ViewportGestureContext::idle(),
        input,
    )
    .expect_err("all gestures disabled");
    assert_eq!(err, ViewportGestureRejection::AllViewportGesturesDisabled);

    let state = NodeGraphInteractionState::default();
    let err = resolve_viewport_scroll_gesture(
        &state.pan_interaction(),
        &state.zoom_interaction(),
        ViewportGestureContext {
            user_selection_active: true,
            ..ViewportGestureContext::idle()
        },
        input,
    )
    .expect_err("selection active blocks scroll gestures");
    assert_eq!(err, ViewportGestureRejection::UserSelectionActive);
}
