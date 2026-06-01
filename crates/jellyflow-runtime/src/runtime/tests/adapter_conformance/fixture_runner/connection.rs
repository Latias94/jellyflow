use super::*;

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
fn adapter_conformance_fixture_runner_records_connect_gesture_transaction_and_callbacks() {
    let (mut graph, _a, b, out_port, _in_port, _eid) = make_graph();
    let next_in = insert_input_port(&mut graph, b, "in2");
    let kind = ConnectDragKind::New {
        from: out_port,
        bundle: vec![out_port],
    };
    let start = ConnectStart {
        kind: kind.clone(),
        mode: NodeGraphConnectionMode::Strict,
    };
    let start_event = NodeGraphGestureEvent::ConnectStart(start.clone());

    let plan = plan_connect(&graph, out_port, next_in);
    assert!(plan.is_accept(), "connect gesture fixture should accept");
    let tx = GraphTransaction::from_ops(plan.into_ops()).with_label("connect gesture commit");
    let (edge_id, edge) = match tx.ops() {
        [GraphOp::AddEdge { id, edge }] => (*id, edge.clone()),
        other => panic!("expected single add-edge op, got {other:#?}"),
    };
    let connection = EdgeConnection::new(edge_id, out_port, next_in, EdgeKind::Data);

    let end = ConnectEnd {
        kind,
        mode: NodeGraphConnectionMode::Strict,
        target: Some(next_in),
        outcome: ConnectEndOutcome::Committed,
    };
    let end_event = NodeGraphGestureEvent::ConnectEnd(end.clone());

    assert_eq!(edge.from, out_port);
    assert_eq!(edge.to, next_in);
    let scenario = ConformanceScenario::new("connect gesture transaction callbacks", graph)
        .with_trace_config(ConformanceTraceConfig::with_xyflow_callbacks())
        .with_actions([
            ConformanceAction::emit_gesture(start_event.clone()),
            ConformanceAction::dispatch_transaction(tx),
            ConformanceAction::emit_gesture(end_event.clone()),
        ])
        .with_expected_trace([
            ConformanceTraceEvent::gesture(start_event),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ConnectStart(start)),
            ConformanceTraceEvent::graph_commit(Some("connect gesture commit"), ["add_edge"]),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::GraphCommit {
                label: Some("connect gesture commit".to_owned()),
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::NodeEdgeChanges {
                nodes: 0,
                edges: 1,
            }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::EdgesChange { count: 1 }),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ConnectionChange(
                ConnectionChange::Connected(connection),
            )),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::Connect(connection)),
            ConformanceTraceEvent::gesture(end_event),
            ConformanceTraceEvent::callback(ConformanceCallbackEvent::ConnectEnd(end)),
        ]);

    assert_conformance_trace(&scenario);
}
