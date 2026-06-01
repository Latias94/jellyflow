use super::*;

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
