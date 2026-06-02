use crate::runtime::connection::{
    ClosestConnectionHandleInput, ConnectionDragActivationInput, ConnectionHandleCandidate,
    ConnectionHandleConnection, ConnectionHandleIndicatorInput, ConnectionHandleRef,
    ConnectionHandleValidity, ConnectionTargetHandle, ConnectionTargetInput,
    closest_connection_handle, connection_drag_threshold_met, connection_handle_validity,
    resolve_connection_handle_indicator, resolve_connection_target,
};
use crate::runtime::geometry::{HandleBounds, HandlePosition};
use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize, NodeId, PortDirection, PortId};
use jellyflow_core::interaction::NodeGraphConnectionMode;

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

#[test]
fn connection_handle_validity_matches_xyflow_true_false_null_shape() {
    assert_eq!(
        connection_handle_validity(false, true),
        ConnectionHandleValidity::Valid
    );
    assert_eq!(
        connection_handle_validity(true, false),
        ConnectionHandleValidity::Invalid
    );
    assert_eq!(
        connection_handle_validity(false, false),
        ConnectionHandleValidity::NoHandle
    );
}

#[test]
fn resolve_connection_target_orders_endpoints_like_xyflow() {
    let source = handle_ref(PortDirection::Out);
    let target = target_handle(PortDirection::In);

    let result = resolve_connection_target(ConnectionTargetInput::new(
        source,
        Some(target),
        NodeGraphConnectionMode::Strict,
        true,
    ));

    assert!(result.is_handle_valid);
    assert_eq!(result.feedback, ConnectionHandleValidity::Valid);
    assert_eq!(
        result.connection,
        Some(ConnectionHandleConnection {
            source,
            target: target.handle,
        })
    );

    let target_start = handle_ref(PortDirection::In);
    let source_target = target_handle(PortDirection::Out);
    let reversed = resolve_connection_target(ConnectionTargetInput::new(
        target_start,
        Some(source_target),
        NodeGraphConnectionMode::Strict,
        true,
    ));

    assert_eq!(
        reversed.connection,
        Some(ConnectionHandleConnection {
            source: source_target.handle,
            target: target_start,
        })
    );
}

#[test]
fn resolve_connection_target_matches_strict_and_loose_mode_rules() {
    let from = handle_ref(PortDirection::Out);
    let same_direction_target = target_handle(PortDirection::Out);

    let strict = resolve_connection_target(ConnectionTargetInput::new(
        from,
        Some(same_direction_target),
        NodeGraphConnectionMode::Strict,
        true,
    ));
    assert!(!strict.is_handle_valid);
    assert_eq!(strict.feedback, ConnectionHandleValidity::Invalid);
    assert!(strict.connection.is_some());

    let loose = resolve_connection_target(ConnectionTargetInput::new(
        from,
        Some(same_direction_target),
        NodeGraphConnectionMode::Loose,
        true,
    ));
    assert!(loose.is_handle_valid);
    assert_eq!(loose.feedback, ConnectionHandleValidity::Valid);

    let same_handle = ConnectionTargetHandle::new(from, true, true);
    let same_handle_loose = resolve_connection_target(ConnectionTargetInput::new(
        from,
        Some(same_handle),
        NodeGraphConnectionMode::Loose,
        true,
    ));
    assert!(!same_handle_loose.is_handle_valid);
    assert_eq!(
        same_handle_loose.feedback,
        ConnectionHandleValidity::Invalid
    );
}

#[test]
fn resolve_connection_target_applies_target_connectability_and_custom_validity() {
    let from = handle_ref(PortDirection::Out);
    let blocked_target = ConnectionTargetHandle::new(handle_ref(PortDirection::In), true, false);
    let blocked = resolve_connection_target(ConnectionTargetInput::new(
        from,
        Some(blocked_target),
        NodeGraphConnectionMode::Strict,
        true,
    ));
    assert!(!blocked.is_handle_valid);
    assert_eq!(blocked.feedback, ConnectionHandleValidity::Invalid);

    let custom_rejected = resolve_connection_target(
        ConnectionTargetInput::new(
            from,
            Some(target_handle(PortDirection::In)),
            NodeGraphConnectionMode::Strict,
            true,
        )
        .with_connection_validity(false),
    );
    assert!(!custom_rejected.is_handle_valid);
    assert_eq!(custom_rejected.feedback, ConnectionHandleValidity::Invalid);
}

#[test]
fn connection_target_input_json_defaults_custom_validity_to_true() {
    let from = handle_ref(PortDirection::Out);
    let target = target_handle(PortDirection::In);
    let input =
        ConnectionTargetInput::new(from, Some(target), NodeGraphConnectionMode::Strict, true);
    let mut encoded = serde_json::to_value(input).expect("serialize connection target input");
    encoded
        .as_object_mut()
        .expect("connection target input object")
        .remove("is_valid_connection");

    let decoded: ConnectionTargetInput =
        serde_json::from_value(encoded).expect("deserialize connection target input");

    assert!(decoded.is_valid_connection);
    assert!(resolve_connection_target(decoded).is_handle_valid);
}

#[test]
fn resolve_connection_target_preserves_xyflow_feedback_null_when_no_handle_is_close() {
    let result = resolve_connection_target(ConnectionTargetInput::new(
        handle_ref(PortDirection::Out),
        None,
        NodeGraphConnectionMode::Strict,
        false,
    ));

    assert_eq!(result.target, None);
    assert_eq!(result.connection, None);
    assert!(!result.is_handle_valid);
    assert_eq!(result.feedback, ConnectionHandleValidity::NoHandle);
}

#[test]
fn connection_handle_indicator_shows_start_and_click_end_states_like_xyflow() {
    let handle = handle_ref(PortDirection::Out);
    let idle = resolve_connection_handle_indicator(
        ConnectionHandleIndicatorInput::new(handle, NodeGraphConnectionMode::Strict)
            .with_connectability(true, true, false),
    );
    assert!(idle.show_connection_indicator);
    assert!(!idle.connection_in_progress);

    let click_connecting = resolve_connection_handle_indicator(
        ConnectionHandleIndicatorInput::new(handle, NodeGraphConnectionMode::Strict)
            .with_click_start(Some(handle))
            .with_connectability(true, false, true),
    );
    assert!(click_connecting.click_connecting);
    assert!(click_connecting.click_connection_in_progress);
    assert!(click_connecting.show_connection_indicator);
}

#[test]
fn connection_handle_indicator_filters_strict_and_loose_end_handles() {
    let from = handle_ref(PortDirection::Out);
    let strict_target = target_handle(PortDirection::In).handle;
    let strict_same_direction = target_handle(PortDirection::Out).handle;

    let target_indicator = resolve_connection_handle_indicator(
        ConnectionHandleIndicatorInput::new(strict_target, NodeGraphConnectionMode::Strict)
            .with_connection(
                Some(from),
                Some(strict_target),
                ConnectionHandleValidity::Valid,
            ),
    );
    assert!(target_indicator.connecting_to);
    assert!(target_indicator.possible_end_handle);
    assert!(target_indicator.valid);
    assert!(target_indicator.show_connection_indicator);

    let same_direction_indicator = resolve_connection_handle_indicator(
        ConnectionHandleIndicatorInput::new(strict_same_direction, NodeGraphConnectionMode::Strict)
            .with_connection(
                Some(from),
                Some(strict_same_direction),
                ConnectionHandleValidity::Invalid,
            ),
    );
    assert!(!same_direction_indicator.possible_end_handle);
    assert!(!same_direction_indicator.valid);
    assert!(!same_direction_indicator.show_connection_indicator);

    let same_handle_loose =
        resolve_connection_handle_indicator(
            ConnectionHandleIndicatorInput::new(from, NodeGraphConnectionMode::Loose)
                .with_connection(Some(from), Some(from), ConnectionHandleValidity::Invalid),
        );
    assert!(same_handle_loose.connecting_from);
    assert!(!same_handle_loose.possible_end_handle);
    assert!(!same_handle_loose.show_connection_indicator);
}

fn handle_ref(direction: PortDirection) -> ConnectionHandleRef {
    ConnectionHandleRef::new(NodeId::new(), PortId::new(), direction)
}

fn target_handle(direction: PortDirection) -> ConnectionTargetHandle {
    ConnectionTargetHandle::new(handle_ref(direction), true, true)
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
