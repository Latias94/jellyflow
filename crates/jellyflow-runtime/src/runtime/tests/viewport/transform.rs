use super::*;

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
fn viewport_constraints_clamp_pan_to_translate_extent() {
    let transform = ViewportTransform::new(
        CanvasPoint {
            x: 400.0,
            y: -300.0,
        },
        1.0,
    )
    .unwrap();

    let constrained = constrain_viewport(
        transform,
        ViewportConstraints::with_translate_extent(
            CanvasSize {
                width: 50.0,
                height: 50.0,
            },
            CanvasRect {
                origin: CanvasPoint { x: 0.0, y: 0.0 },
                size: CanvasSize {
                    width: 100.0,
                    height: 100.0,
                },
            },
        ),
    )
    .expect("constrained viewport");

    assert_eq!(constrained.zoom, 1.0);
    assert_eq!(constrained.pan, CanvasPoint { x: 0.0, y: -50.0 });
}

#[test]
fn viewport_constraints_center_when_visible_area_exceeds_translate_extent() {
    let transform = ViewportTransform::new(CanvasPoint::default(), 0.5).unwrap();

    let constrained = constrain_viewport(
        transform,
        ViewportConstraints::with_translate_extent(
            CanvasSize {
                width: 100.0,
                height: 100.0,
            },
            CanvasRect {
                origin: CanvasPoint { x: 0.0, y: 0.0 },
                size: CanvasSize {
                    width: 80.0,
                    height: 60.0,
                },
            },
        ),
    )
    .expect("constrained viewport");

    assert_eq!(constrained.pan, CanvasPoint { x: 60.0, y: 70.0 },);
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
