use crate::runtime::connection::{ConnectionDragActivationInput, connection_drag_threshold_met};
use jellyflow_core::core::CanvasPoint;

#[test]
fn connection_drag_threshold_uses_xyflow_squared_screen_distance_semantics() {
    assert!(
        connection_drag_threshold_met(ConnectionDragActivationInput::new(
            CanvasPoint::default(),
            0.0,
        )),
        "threshold zero starts immediately"
    );
    assert!(
        !connection_drag_threshold_met(ConnectionDragActivationInput::new(
            CanvasPoint { x: 3.0, y: 4.0 },
            5.0,
        )),
        "distance equal to threshold does not start connection"
    );
    assert!(
        connection_drag_threshold_met(ConnectionDragActivationInput::new(
            CanvasPoint { x: 3.0, y: 4.0 },
            4.99,
        )),
        "distance greater than threshold starts connection"
    );
    assert!(
        !connection_drag_threshold_met(ConnectionDragActivationInput::new(
            CanvasPoint { x: 3.0, y: 4.0 },
            -5.0,
        )),
        "negative thresholds follow XyFlow's squared-threshold shape"
    );
}

#[test]
fn connection_drag_threshold_rejects_non_finite_inputs() {
    assert!(!connection_drag_threshold_met(
        ConnectionDragActivationInput::new(
            CanvasPoint {
                x: f32::INFINITY,
                y: 0.0,
            },
            1.0,
        ),
    ));
    assert!(!connection_drag_threshold_met(
        ConnectionDragActivationInput::new(CanvasPoint { x: 1.0, y: 0.0 }, f32::NAN),
    ));
}
