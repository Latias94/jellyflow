use super::*;

fn inertia_tuning() -> NodeGraphPanInertiaTuning {
    NodeGraphPanInertiaTuning {
        enabled: true,
        decay_per_s: 2.0,
        min_speed: 100.0,
        max_speed: 1000.0,
    }
}

#[test]
fn pan_inertia_plan_samples_exponential_decay() {
    let current = ViewportTransform::new(CanvasPoint { x: 0.0, y: 0.0 }, 2.0).unwrap();
    let plan = plan_viewport_pan_inertia(ViewportPanInertiaRequest::new(
        current,
        CanvasPoint { x: 1000.0, y: 0.0 },
        inertia_tuning(),
    ))
    .expect("inertia plan");

    assert_eq!(plan.from, current);
    assert_eq!(
        plan.initial_velocity_screen,
        CanvasPoint { x: 1000.0, y: 0.0 }
    );
    assert!((plan.duration_seconds - 1.1512926).abs() <= 1.0e-6);

    let half_second = plan.frame_at(0.5).expect("sample at 0.5s");
    assert!(!half_second.done);
    assert!((half_second.progress - 0.43429446).abs() <= 1.0e-5);
    assert!((half_second.speed_screen - 367.87946).abs() <= 1.0e-4);
    assert!((half_second.velocity_screen.x - 367.87946).abs() <= 1.0e-4);
    assert_eq!(half_second.velocity_screen.y, 0.0);
    assert!((half_second.transform.pan.x - 158.03014).abs() <= 1.0e-4);
    assert_eq!(half_second.transform.pan.y, 0.0);
    assert_eq!(half_second.transform.zoom, 2.0);

    let terminal = plan.terminal_frame().expect("terminal frame");
    assert!(terminal.done);
    assert!((terminal.speed_screen - 100.0).abs() <= 1.0e-4);
    assert!((terminal.transform.pan.x - 225.0).abs() <= 1.0e-4);

    let after_done = plan.frame_at(10.0).expect("late frame");
    assert!(after_done.done);
    assert_eq!(after_done.progress, 1.0);
    assert_eq!(after_done.transform, terminal.transform);
}

#[test]
fn pan_inertia_plan_clamps_initial_speed() {
    let current = ViewportTransform::new(CanvasPoint::default(), 1.0).unwrap();
    let plan = plan_viewport_pan_inertia(ViewportPanInertiaRequest::new(
        current,
        CanvasPoint { x: 2000.0, y: 0.0 },
        inertia_tuning(),
    ))
    .expect("clamped inertia plan");

    assert_eq!(
        plan.initial_velocity_screen,
        CanvasPoint { x: 1000.0, y: 0.0 }
    );
}

#[test]
fn pan_inertia_rejects_disabled_slow_or_invalid_input() {
    let current = ViewportTransform::new(CanvasPoint::default(), 1.0).unwrap();
    let mut tuning = inertia_tuning();
    tuning.enabled = false;
    assert!(
        plan_viewport_pan_inertia(ViewportPanInertiaRequest::new(
            current,
            CanvasPoint { x: 500.0, y: 0.0 },
            tuning,
        ))
        .is_none()
    );

    assert!(
        plan_viewport_pan_inertia(ViewportPanInertiaRequest::new(
            current,
            CanvasPoint { x: 50.0, y: 0.0 },
            inertia_tuning(),
        ))
        .is_none()
    );

    let invalid_current = ViewportTransform {
        pan: CanvasPoint::default(),
        zoom: f32::INFINITY,
    };
    assert!(
        plan_viewport_pan_inertia(ViewportPanInertiaRequest::new(
            invalid_current,
            CanvasPoint { x: 500.0, y: 0.0 },
            inertia_tuning(),
        ))
        .is_none()
    );

    let mut invalid_tuning = inertia_tuning();
    invalid_tuning.decay_per_s = 0.0;
    assert!(
        plan_viewport_pan_inertia(ViewportPanInertiaRequest::new(
            current,
            CanvasPoint { x: 500.0, y: 0.0 },
            invalid_tuning,
        ))
        .is_none()
    );

    let plan = plan_viewport_pan_inertia(ViewportPanInertiaRequest::new(
        current,
        CanvasPoint { x: 500.0, y: 0.0 },
        inertia_tuning(),
    ))
    .expect("valid plan");
    assert!(plan.frame_at(f32::NAN).is_none());

    let start = plan
        .frame_at(-1.0)
        .expect("negative elapsed clamps to start");
    assert_eq!(start.elapsed_seconds, 0.0);
    assert_eq!(start.progress, 0.0);
    assert_eq!(start.transform, current);
}
