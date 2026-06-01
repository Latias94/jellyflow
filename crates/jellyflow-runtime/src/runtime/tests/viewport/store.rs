use super::*;

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
