use crate::runtime::connection::{
    ClosestConnectionHandleInput, ConnectionDragActivationInput, ConnectionHandleCandidate,
    ConnectionHandleRef, closest_connection_handle, connection_drag_threshold_met,
};
use crate::runtime::geometry::{HandleBounds, HandlePosition};
use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize, NodeId, PortDirection, PortId};

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

#[test]
fn closest_connection_handle_prefers_opposite_direction_on_equal_distance() {
    let from = handle_ref(PortDirection::Out);
    let same_distance_out = candidate(PortDirection::Out, CanvasPoint { x: 10.0, y: 10.0 });
    let same_distance_in = candidate(PortDirection::In, CanvasPoint { x: 10.0, y: 10.0 });
    let far_in = candidate(PortDirection::In, CanvasPoint { x: 100.0, y: 100.0 });
    let candidates = [same_distance_out, same_distance_in, far_in];

    let result = closest_connection_handle(ClosestConnectionHandleInput::new(
        CanvasPoint { x: 15.0, y: 15.0 },
        10.0,
        from,
        &candidates,
    ))
    .expect("closest handle");

    assert_eq!(result.handle.direction, PortDirection::In);
    assert_eq!(result.center, CanvasPoint { x: 15.0, y: 15.0 });
    assert_eq!(result.distance, 0.0);
}

#[test]
fn closest_connection_handle_skips_starting_handle_and_rejects_invalid_inputs() {
    let from = handle_ref(PortDirection::Out);
    let same_handle = ConnectionHandleCandidate::new(
        from,
        node_rect(CanvasPoint { x: 0.0, y: 0.0 }),
        handle_bounds(CanvasPoint { x: 10.0, y: 10.0 }),
    );
    let outside_radius = candidate(PortDirection::In, CanvasPoint { x: 100.0, y: 100.0 });
    let candidates = [same_handle, outside_radius];

    assert!(
        closest_connection_handle(ClosestConnectionHandleInput::new(
            CanvasPoint { x: 15.0, y: 15.0 },
            10.0,
            from,
            &candidates,
        ))
        .is_none()
    );
    assert!(
        closest_connection_handle(ClosestConnectionHandleInput::new(
            CanvasPoint {
                x: f32::NAN,
                y: 15.0
            },
            10.0,
            from,
            &candidates,
        ))
        .is_none()
    );
    assert!(
        closest_connection_handle(ClosestConnectionHandleInput::new(
            CanvasPoint { x: 15.0, y: 15.0 },
            -1.0,
            from,
            &candidates,
        ))
        .is_none()
    );
}

fn handle_ref(direction: PortDirection) -> ConnectionHandleRef {
    ConnectionHandleRef::new(NodeId::new(), PortId::new(), direction)
}

fn candidate(direction: PortDirection, handle_origin: CanvasPoint) -> ConnectionHandleCandidate {
    ConnectionHandleCandidate::new(
        handle_ref(direction),
        node_rect(CanvasPoint { x: 0.0, y: 0.0 }),
        handle_bounds(handle_origin),
    )
}

fn node_rect(origin: CanvasPoint) -> CanvasRect {
    CanvasRect {
        origin,
        size: CanvasSize {
            width: 200.0,
            height: 120.0,
        },
    }
}

fn handle_bounds(origin: CanvasPoint) -> HandleBounds {
    HandleBounds {
        rect: CanvasRect {
            origin,
            size: CanvasSize {
                width: 10.0,
                height: 10.0,
            },
        },
        position: HandlePosition::Right,
    }
}
