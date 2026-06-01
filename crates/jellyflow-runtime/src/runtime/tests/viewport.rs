use super::fixtures::make_graph;
use super::harness::{HarnessCallbackEvent, HarnessEvent, InteractionHarness};
use crate::io::{NodeGraphInteractionState, NodeGraphPanOnDragButtons, NodeGraphPanOnScrollMode};
use crate::runtime::events::{
    NodeGraphGestureEvent, ViewportMove, ViewportMoveEnd, ViewportMoveEndOutcome, ViewportMoveKind,
    ViewportMoveStart,
};
use crate::runtime::viewport::{
    ViewportDragPanInput, ViewportGestureContext, ViewportGestureIntent, ViewportGestureRejection,
    ViewportPanRequest, ViewportPointerButton, ViewportScrollInput, ViewportTransform,
    ViewportZoomRequest, pan_viewport, resolve_viewport_drag_pan_gesture,
    resolve_viewport_scroll_gesture, zoom_viewport,
};
use jellyflow_core::core::CanvasPoint;

#[test]
fn viewport_pan_request_translates_screen_delta_by_zoom() {
    let current = ViewportTransform::new(CanvasPoint { x: 10.0, y: 20.0 }, 2.0).unwrap();

    let next = pan_viewport(
        current,
        ViewportPanRequest::new(CanvasPoint { x: 40.0, y: -10.0 }),
    )
    .expect("valid pan");

    assert_eq!(next.zoom, 2.0);
    assert_eq!(next.pan, CanvasPoint { x: 30.0, y: 15.0 });
}

#[test]
fn viewport_zoom_request_keeps_anchor_canvas_point_stable_and_clamps() {
    let current = ViewportTransform::new(CanvasPoint { x: 10.0, y: 20.0 }, 2.0).unwrap();

    let next = zoom_viewport(
        current,
        ViewportZoomRequest::new(CanvasPoint { x: 200.0, y: 120.0 }, 4.0, 0.5, 3.0),
    )
    .expect("valid zoom");

    assert_eq!(next.zoom, 3.0);
    assert!((next.pan.x - (-23.333332)).abs() <= 1.0e-5);
    assert!((next.pan.y - 0.0).abs() <= 1.0e-6);
    assert_eq!(
        current.canvas_point_at_screen(CanvasPoint { x: 200.0, y: 120.0 }),
        next.canvas_point_at_screen(CanvasPoint { x: 200.0, y: 120.0 }),
    );
}

#[test]
fn viewport_transform_rejects_non_finite_or_non_positive_values() {
    let non_finite_pan = CanvasPoint {
        x: f32::NAN,
        y: 0.0,
    };
    assert!(ViewportTransform::new(non_finite_pan, 1.0).is_none());
    assert!(ViewportTransform::new(CanvasPoint { x: 0.0, y: 0.0 }, 0.0).is_none());

    let current = ViewportTransform::new(CanvasPoint::default(), 1.0).unwrap();
    assert!(
        pan_viewport(
            current,
            ViewportPanRequest::new(CanvasPoint {
                x: f32::INFINITY,
                y: 0.0,
            }),
        )
        .is_none()
    );
    assert!(
        zoom_viewport(
            current,
            ViewportZoomRequest::new(CanvasPoint::default(), f32::NAN, 0.1, 4.0),
        )
        .is_none()
    );

    let invalid_current = ViewportTransform {
        pan: CanvasPoint::default(),
        zoom: f32::INFINITY,
    };
    let no_delta = ViewportPanRequest::new(CanvasPoint::default());
    assert!(pan_viewport(invalid_current, no_delta).is_none());
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

#[test]
fn store_viewport_pan_and_zoom_helpers_publish_view_changes() {
    let (graph, _a, _b, _out_port, _in_port, _eid) = make_graph();
    let mut harness = InteractionHarness::new("viewport pan and zoom helpers", graph);

    let panned = harness
        .store_mut()
        .apply_viewport_pan(ViewportPanRequest::new(CanvasPoint { x: 40.0, y: -10.0 }))
        .expect("pan");
    assert_eq!(panned.pan, CanvasPoint { x: 40.0, y: -10.0 });
    assert_eq!(panned.zoom, 1.0);

    let zoomed = harness
        .store_mut()
        .apply_viewport_zoom(ViewportZoomRequest::new(
            CanvasPoint { x: 100.0, y: 50.0 },
            2.0,
            0.5,
            4.0,
        ))
        .expect("zoom");
    assert_eq!(zoomed.zoom, 2.0);
    assert_eq!(zoomed.pan, CanvasPoint { x: -10.0, y: -35.0 });

    harness.assert_events(&[
        HarnessEvent::viewport(CanvasPoint { x: 40.0, y: -10.0 }, 1.0),
        HarnessEvent::viewport(CanvasPoint { x: -10.0, y: -35.0 }, 2.0),
    ]);
}

#[test]
fn viewport_move_gesture_lifecycle_dispatches_xyflow_callbacks_in_order() {
    let (graph, _a, _b, _out_port, _in_port, _eid) = make_graph();
    let mut harness = InteractionHarness::new("viewport move callback order", graph);
    harness.install_callback_trace();

    let start = ViewportMoveStart {
        kind: ViewportMoveKind::PanDrag,
        pan: CanvasPoint::default(),
        zoom: 1.0,
    };
    harness.emit_gesture(NodeGraphGestureEvent::ViewportMoveStart(start));

    let moved = harness
        .store_mut()
        .apply_viewport_pan(ViewportPanRequest::new(CanvasPoint { x: 12.0, y: -6.0 }))
        .expect("pan");
    let update = ViewportMove {
        kind: ViewportMoveKind::PanDrag,
        pan: moved.pan,
        zoom: moved.zoom,
    };
    harness.emit_gesture(NodeGraphGestureEvent::ViewportMove(update));

    let end = ViewportMoveEnd {
        kind: ViewportMoveKind::PanDrag,
        pan: moved.pan,
        zoom: moved.zoom,
        outcome: ViewportMoveEndOutcome::Ended,
    };
    harness.emit_gesture(NodeGraphGestureEvent::ViewportMoveEnd(end));

    harness.assert_events(&[
        HarnessEvent::gesture(NodeGraphGestureEvent::ViewportMoveStart(start)),
        HarnessEvent::callback(HarnessCallbackEvent::ViewportMoveStart(start)),
        HarnessEvent::viewport(CanvasPoint { x: 12.0, y: -6.0 }, 1.0),
        HarnessEvent::gesture(NodeGraphGestureEvent::ViewportMove(update)),
        HarnessEvent::callback(HarnessCallbackEvent::ViewportMove(update)),
        HarnessEvent::gesture(NodeGraphGestureEvent::ViewportMoveEnd(end)),
        HarnessEvent::callback(HarnessCallbackEvent::ViewportMoveEnd(end)),
    ]);
}
