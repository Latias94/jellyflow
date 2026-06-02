use super::*;

#[test]
fn viewport_animation_plan_samples_cubic_eased_frames() {
    let from = ViewportTransform::new(CanvasPoint { x: 0.0, y: 0.0 }, 1.0).unwrap();
    let to = ViewportTransform::new(CanvasPoint { x: 100.0, y: -50.0 }, 3.0).unwrap();

    let plan = plan_viewport_animation(from, to, 2.0).expect("animation plan");

    let half_second = plan.frame_at(0.5).expect("sample at 0.5s");
    assert!(!half_second.done);
    assert_eq!(half_second.elapsed_seconds, 0.5);
    assert_eq!(half_second.progress, 0.25);
    assert_eq!(half_second.eased_progress, 0.0625);
    assert_eq!(
        half_second.transform.pan,
        CanvasPoint { x: 6.25, y: -3.125 }
    );
    assert_eq!(half_second.transform.zoom, 1.125);

    let finished = plan.frame_at(2.0).expect("sample at duration");
    assert!(finished.done);
    assert_eq!(finished.progress, 1.0);
    assert_eq!(finished.eased_progress, 1.0);
    assert_eq!(finished.transform, to);
}

#[test]
fn viewport_animation_zero_duration_finishes_immediately() {
    let from = ViewportTransform::new(CanvasPoint { x: 12.0, y: -4.0 }, 1.25).unwrap();
    let to = ViewportTransform::new(CanvasPoint { x: -30.0, y: 8.0 }, 0.75).unwrap();

    let plan = plan_viewport_animation(from, to, 0.0).expect("immediate plan");

    assert!(plan.is_immediate());
    let frame = plan.frame_at(0.0).expect("first frame");
    assert!(frame.done);
    assert_eq!(frame.elapsed_seconds, 0.0);
    assert_eq!(frame.progress, 1.0);
    assert_eq!(frame.eased_progress, 1.0);
    assert_eq!(frame.transform, to);
}

#[test]
fn viewport_animation_rejects_non_finite_time_inputs() {
    let from = ViewportTransform::new(CanvasPoint::default(), 1.0).unwrap();
    let to = ViewportTransform::new(CanvasPoint { x: 1.0, y: 1.0 }, 2.0).unwrap();

    assert!(plan_viewport_animation(from, to, f32::NAN).is_none());
    assert!(plan_viewport_animation(from, to, f32::INFINITY).is_none());

    let plan = plan_viewport_animation(from, to, 1.0).expect("valid plan");
    assert!(plan.frame_at(f32::NAN).is_none());
    assert!(plan.frame_at(f32::INFINITY).is_none());

    let start = plan
        .frame_at(-1.0)
        .expect("negative elapsed clamps to start");
    assert!(!start.done);
    assert_eq!(start.elapsed_seconds, 0.0);
    assert_eq!(start.progress, 0.0);
    assert_eq!(start.transform, from);
}

#[test]
fn viewport_animation_can_use_linear_easing() {
    let from = ViewportTransform::new(CanvasPoint { x: 0.0, y: 0.0 }, 1.0).unwrap();
    let to = ViewportTransform::new(CanvasPoint { x: 100.0, y: 50.0 }, 3.0).unwrap();
    let request = ViewportAnimationRequest::new(
        from,
        to,
        ViewportAnimationOptions::new(2.0).with_easing(ViewportAnimationEasing::Linear),
    );

    let plan = plan_viewport_animation_with_options(request).expect("linear animation plan");
    let frame = plan.frame_at(0.5).expect("sample at 0.5s");

    assert_eq!(frame.progress, 0.25);
    assert_eq!(frame.eased_progress, 0.25);
    assert_eq!(frame.transform.pan, CanvasPoint { x: 25.0, y: 12.5 });
    assert_eq!(frame.transform.zoom, 1.5);
}
