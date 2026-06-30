use super::*;

#[test]
fn adapter_conformance_fixture_runner_asserts_connection_lifecycle() {
    let (graph, source_node, target_node, out_port, in_port, _eid) = make_graph();
    let source = ConnectionHandleRef::new(source_node, out_port, PortDirection::Out);
    let target = ConnectionTargetHandle::new(
        ConnectionHandleRef::new(target_node, in_port, PortDirection::In),
        true,
        true,
    );
    let hover = ResolvedConnectionTarget {
        target: Some(target),
        connection: Some(ConnectionHandleConnection {
            source,
            target: target.handle,
        }),
        is_handle_valid: true,
        feedback: ConnectionHandleValidity::Valid,
    };
    let start = ConnectStart {
        kind: ConnectDragKind::New {
            from: out_port,
            bundle: vec![out_port],
        },
        mode: NodeGraphConnectionMode::Strict,
    };
    let expected =
        resolve_connection_lifecycle(start.clone(), Some(hover), ConnectionEndIntent::Complete);
    let scenario = ConformanceScenario::new("connection lifecycle assertion", graph)
        .with_actions([ConformanceAction::assert_connection_lifecycle(
            start,
            Some(hover),
            ConnectionEndIntent::Complete,
            expected,
        )])
        .with_expected_trace([]);

    assert_conformance_trace(&scenario);
}

#[test]
fn adapter_conformance_fixture_runner_records_connect_gesture_lifecycle() {
    let (graph, _a, _b, out_port, in_port, _eid) = make_graph();
    let kind = ConnectDragKind::New {
        from: out_port,
        bundle: vec![out_port],
    };
    let start = NodeGraphGestureEvent::ConnectStart(ConnectStart {
        kind: kind.clone(),
        mode: NodeGraphConnectionMode::Strict,
    });
    let end = NodeGraphGestureEvent::ConnectEnd(ConnectEnd {
        kind,
        mode: NodeGraphConnectionMode::Strict,
        target: Some(in_port),
        outcome: ConnectEndOutcome::Committed,
    });
    let scenario = ConformanceScenario::new("connect gesture lifecycle", graph)
        .with_actions([
            ConformanceAction::emit_gesture(start.clone()),
            ConformanceAction::emit_gesture(end.clone()),
        ])
        .with_expected_trace([
            ConformanceTraceEvent::gesture(start),
            ConformanceTraceEvent::gesture(end),
        ]);

    assert_conformance_trace(&scenario);
}

#[test]
fn adapter_conformance_fixture_runner_asserts_connection_target_policy() {
    let (graph, source_node, target_node, out_port, in_port, _eid) = make_graph();
    let source = ConnectionHandleRef::new(source_node, out_port, PortDirection::Out);
    let target = ConnectionTargetHandle::new(
        ConnectionHandleRef::new(target_node, in_port, PortDirection::In),
        true,
        true,
    );
    let input =
        ConnectionTargetInput::new(source, Some(target), NodeGraphConnectionMode::Strict, true);
    let expected = ResolvedConnectionTarget {
        target: Some(target),
        connection: Some(ConnectionHandleConnection {
            source,
            target: target.handle,
        }),
        is_handle_valid: true,
        feedback: ConnectionHandleValidity::Valid,
    };

    let scenario = ConformanceScenario::new("connection target policy assertion", graph)
        .with_actions([ConformanceAction::assert_connection_target(input, expected)])
        .with_expected_trace([]);

    assert_conformance_trace(&scenario);
}

#[test]
fn adapter_conformance_fixture_runner_asserts_connection_target_from_handle_candidates() {
    let (mut graph, source_node, target_node, out_port, in_port, _eid) = make_graph();
    let next_in = insert_input_port(&mut graph, target_node, "in2");
    let source = ConnectionHandleRef::new(source_node, out_port, PortDirection::Out);
    let blocked_near = ConnectionTargetCandidate::new(
        ConnectionTargetHandle::new(
            ConnectionHandleRef::new(target_node, in_port, PortDirection::In),
            true,
            false,
        ),
        node_rect(CanvasPoint { x: 0.0, y: 0.0 }),
        handle_bounds(CanvasPoint { x: 10.0, y: 10.0 }),
    );
    let valid_far = ConnectionTargetCandidate::new(
        ConnectionTargetHandle::new(
            ConnectionHandleRef::new(target_node, next_in, PortDirection::In),
            true,
            true,
        ),
        node_rect(CanvasPoint { x: 0.0, y: 0.0 }),
        handle_bounds(CanvasPoint { x: 80.0, y: 80.0 }),
    );
    let candidates = [valid_far, blocked_near];
    let input = ConnectionTargetFromHandlesInput::new(
        CanvasPoint { x: 15.0, y: 15.0 },
        120.0,
        source,
        &candidates,
        NodeGraphConnectionMode::Strict,
    );
    let expected = ResolvedConnectionTarget {
        target: Some(blocked_near.target),
        connection: Some(ConnectionHandleConnection {
            source,
            target: blocked_near.target.handle,
        }),
        is_handle_valid: false,
        feedback: ConnectionHandleValidity::Invalid,
    };

    let scenario = ConformanceScenario::new("connection target from handle candidates", graph)
        .with_actions([ConformanceAction::assert_connection_target_from_handles(
            input, expected,
        )])
        .with_expected_trace([]);

    assert_conformance_trace(&scenario);
}

#[test]
fn adapter_conformance_fixture_runner_records_connect_gesture_transaction_and_callbacks() {
    let (mut graph, _a, b, out_port, _in_port, _eid) = make_graph();
    let next_in = insert_input_port(&mut graph, b, "in2");
    let edge_id = EdgeId::from_u128(300);
    let kind = ConnectDragKind::New {
        from: out_port,
        bundle: vec![out_port],
    };
    let start = ConnectStart {
        kind: kind.clone(),
        mode: NodeGraphConnectionMode::Strict,
    };
    let connection = EdgeConnection::new(edge_id, out_port, next_in, EdgeKind::Data);
    let request = ConnectEdgeRequest::new(out_port, next_in, NodeGraphConnectionMode::Strict)
        .with_edge_id(edge_id);

    let scenario = ConformanceScenario::new("connect gesture transaction callbacks", graph)
        .with_xyflow_callbacks()
        .with_connect_edge_session_contract(ConformanceConnectEdgeSessionContract::new(
            start, request, connection,
        ));

    assert_conformance_trace(&scenario);
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

#[test]
fn adapter_conformance_fixture_runner_records_reconnect_transaction_and_callbacks() {
    let (mut graph, _a, b, out_port, in_port, edge_id) = make_graph();
    let next_in = insert_input_port(&mut graph, b, "in2");
    let kind = ConnectDragKind::Reconnect {
        edge: edge_id,
        endpoint: EdgeEndpoint::To,
        fixed: out_port,
    };
    let start = ConnectStart {
        kind: kind.clone(),
        mode: NodeGraphConnectionMode::Strict,
    };
    let start_event = NodeGraphGestureEvent::ConnectStart(start.clone());
    let from = EdgeEndpoints {
        from: out_port,
        to: in_port,
    };
    let to = EdgeEndpoints {
        from: out_port,
        to: next_in,
    };
    let end = ConnectEnd {
        kind,
        mode: NodeGraphConnectionMode::Strict,
        target: Some(next_in),
        outcome: ConnectEndOutcome::Committed,
    };
    let end_event = NodeGraphGestureEvent::ConnectEnd(end.clone());

    let scenario = ConformanceScenario::new("reconnect gesture transaction callbacks", graph)
        .with_xyflow_callbacks()
        .with_actions([
            ConformanceAction::emit_gesture(start_event.clone()),
            ConformanceAction::apply_reconnect_edge(ReconnectEdgeRequest::new(
                edge_id,
                EdgeEndpoint::To,
                next_in,
                NodeGraphConnectionMode::Strict,
            )),
            ConformanceAction::emit_gesture(end_event.clone()),
        ])
        .with_expected_trace([
            ConformanceTraceEvent::gesture(start_event),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ConnectStart(start)),
            ConformanceTraceEvent::graph_commit(
                Some(RECONNECT_EDGE_TRANSACTION_LABEL),
                ["set_edge_endpoints"],
            ),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::GraphCommit {
                label: Some(RECONNECT_EDGE_TRANSACTION_LABEL.to_owned()),
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeEdgeChanges {
                nodes: 0,
                edges: 1,
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::EdgesChange { count: 1 }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ConnectionChange(
                ConnectionChange::Reconnected {
                    edge: edge_id,
                    from,
                    to,
                },
            )),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::Reconnect {
                edge: edge_id,
                from,
                to,
            }),
            ConformanceTraceEvent::gesture(end_event),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ConnectEnd(end)),
        ]);

    assert_conformance_trace(&scenario);
}
