use super::fixtures::make_graph;
use super::harness::{HarnessEvent, InteractionHarness};
use crate::io::NodeGraphAutoPanTuning;
use crate::runtime::auto_pan::{AutoPanActivation, AutoPanRequest, compute_auto_pan};
use jellyflow_core::core::{CanvasPoint, CanvasSize};

#[test]
fn auto_pan_near_right_and_bottom_moves_rendered_content_left_and_up() {
    let tuning = NodeGraphAutoPanTuning {
        speed: 100.0,
        margin: 20.0,
        ..NodeGraphAutoPanTuning::default()
    };

    let plan = compute_auto_pan(
        &tuning,
        AutoPanRequest::new(
            AutoPanActivation::Always,
            CanvasPoint { x: 190.0, y: 95.0 },
            CanvasSize {
                width: 200.0,
                height: 100.0,
            },
            1.0,
        ),
    )
    .expect("auto-pan frame");

    assert_eq!(plan.screen_delta, CanvasPoint { x: -50.0, y: -75.0 });
}

#[test]
fn auto_pan_respects_workflow_activation_policy() {
    let tuning = NodeGraphAutoPanTuning {
        on_node_drag: false,
        speed: 100.0,
        margin: 20.0,
        ..NodeGraphAutoPanTuning::default()
    };
    let request = AutoPanRequest::new(
        AutoPanActivation::NodeDrag,
        CanvasPoint { x: 190.0, y: 50.0 },
        CanvasSize {
            width: 200.0,
            height: 100.0,
        },
        1.0,
    );

    assert!(compute_auto_pan(&tuning, request).is_none());

    let plan = compute_auto_pan(
        &tuning,
        AutoPanRequest {
            activation: AutoPanActivation::Always,
            ..request
        },
    )
    .expect("generic auto-pan bypasses workflow toggle");

    assert_eq!(plan.screen_delta, CanvasPoint { x: -50.0, y: 0.0 });
}

#[test]
fn auto_pan_rejects_invalid_or_noop_frames() {
    let tuning = NodeGraphAutoPanTuning {
        speed: 100.0,
        margin: 20.0,
        ..NodeGraphAutoPanTuning::default()
    };
    let base = AutoPanRequest::new(
        AutoPanActivation::Always,
        CanvasPoint { x: 100.0, y: 50.0 },
        CanvasSize {
            width: 200.0,
            height: 100.0,
        },
        1.0,
    );

    assert!(compute_auto_pan(&tuning, base).is_none());
    assert!(
        compute_auto_pan(
            &tuning,
            AutoPanRequest {
                elapsed_seconds: 0.0,
                pointer_screen: CanvasPoint { x: 190.0, y: 50.0 },
                ..base
            },
        )
        .is_none()
    );
    assert!(
        compute_auto_pan(
            &tuning,
            AutoPanRequest {
                viewport_size: CanvasSize {
                    width: 0.0,
                    height: 100.0,
                },
                pointer_screen: CanvasPoint { x: 190.0, y: 50.0 },
                ..base
            },
        )
        .is_none()
    );

    let invalid_tuning = NodeGraphAutoPanTuning {
        speed: f32::NAN,
        ..tuning
    };
    assert!(
        compute_auto_pan(
            &invalid_tuning,
            AutoPanRequest {
                pointer_screen: CanvasPoint { x: 190.0, y: 50.0 },
                ..base
            },
        )
        .is_none()
    );
}

#[test]
fn store_auto_pan_publishes_viewport_changes() {
    let (graph, _a, _b, _out_port, _in_port, _eid) = make_graph();
    let mut harness = InteractionHarness::new("auto-pan publishes viewport", graph);

    let outcome = harness
        .store_mut()
        .apply_auto_pan(AutoPanRequest::new(
            AutoPanActivation::NodeDrag,
            CanvasPoint { x: 188.0, y: 50.0 },
            CanvasSize {
                width: 200.0,
                height: 100.0,
            },
            1.0 / 60.0,
        ))
        .expect("auto-pan frame");

    assert!((outcome.plan.screen_delta.x - (-10.5)).abs() <= 1.0e-5);
    assert_eq!(outcome.plan.screen_delta.y, 0.0);
    assert_eq!(outcome.transform.zoom, 1.0);
    assert_eq!(outcome.transform.pan, outcome.plan.screen_delta);
    harness.assert_events(&[HarnessEvent::viewport(outcome.transform.pan, 1.0)]);
}

#[test]
fn auto_pan_plan_applies_through_store_viewport_path() {
    let (graph, _a, _b, _out_port, _in_port, _eid) = make_graph();
    let mut harness = InteractionHarness::new("auto-pan plan applies viewport", graph);
    let plan = compute_auto_pan(
        &NodeGraphAutoPanTuning {
            speed: 100.0,
            margin: 20.0,
            ..NodeGraphAutoPanTuning::default()
        },
        AutoPanRequest::new(
            AutoPanActivation::Always,
            CanvasPoint { x: 190.0, y: 50.0 },
            CanvasSize {
                width: 200.0,
                height: 100.0,
            },
            1.0,
        ),
    )
    .expect("auto-pan frame");

    let transform = plan
        .apply_to_store(harness.store_mut())
        .expect("viewport pan");

    assert_eq!(transform.pan, plan.screen_delta);
    assert_eq!(transform.zoom, 1.0);
    harness.assert_events(&[HarnessEvent::viewport(plan.screen_delta, 1.0)]);
}
