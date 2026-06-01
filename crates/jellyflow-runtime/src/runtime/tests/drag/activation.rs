use crate::runtime::drag::{NodeDragActivationInput, node_drag_threshold_met};
use jellyflow_core::core::CanvasPoint;

#[test]
fn node_drag_threshold_uses_xyflow_screen_distance_semantics() {
    assert!(
        node_drag_threshold_met(NodeDragActivationInput::new(CanvasPoint::default(), 0.0)),
        "threshold zero starts immediately"
    );
    assert!(
        !node_drag_threshold_met(NodeDragActivationInput::new(
            CanvasPoint { x: 3.0, y: 4.0 },
            5.0,
        )),
        "distance equal to threshold does not start drag"
    );
    assert!(
        node_drag_threshold_met(NodeDragActivationInput::new(
            CanvasPoint { x: 3.0, y: 4.0 },
            4.99,
        )),
        "distance greater than threshold starts drag"
    );
    assert!(
        node_drag_threshold_met(NodeDragActivationInput::new(CanvasPoint::default(), -1.0)),
        "negative thresholds are already exceeded by any finite distance"
    );
}

#[test]
fn node_drag_threshold_rejects_non_finite_inputs() {
    assert!(!node_drag_threshold_met(NodeDragActivationInput::new(
        CanvasPoint {
            x: f32::INFINITY,
            y: 0.0,
        },
        1.0,
    )));
    assert!(!node_drag_threshold_met(NodeDragActivationInput::new(
        CanvasPoint { x: 1.0, y: 0.0 },
        f32::NAN,
    )));
}
