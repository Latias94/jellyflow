use crate::runtime::viewport::{
    ViewportPanRequest, ViewportTransform, ViewportZoomRequest, pan_viewport, zoom_viewport,
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
