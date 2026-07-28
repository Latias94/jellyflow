use super::*;

fn assert_runtime_rejects_graph_with(
    graph: Graph,
    predicate: impl Fn(&jellyflow_core::core::GraphValidationError) -> bool,
) {
    let error = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect_err("structurally invalid graph must not publish");
    let GraphRuntimeError::Canonicalization(NodeCatalogMigrationError::InvalidResult { errors }) =
        error
    else {
        panic!("expected canonical publication to reject structural graph errors, got {error:?}");
    };
    assert!(
        errors.iter().any(predicate),
        "missing expected error: {errors:?}"
    );
}

fn routed_graph_builder(graph: &Graph) -> GraphBuilder {
    let mut builder = GraphBuilder::new(graph.graph_id());
    for (id, node) in graph.nodes() {
        builder.insert_node(*id, node.clone());
    }
    for (id, port) in graph.ports() {
        builder.insert_port(*id, port.clone());
    }
    for (id, edge) in graph.edges() {
        builder.insert_edge(*id, edge.clone());
    }
    builder
}

#[test]
fn runtime_rejects_structural_connection_errors_before_publication() {
    let (graph, ids) = graph_with_routed_edge();
    let mut duplicate = routed_graph_builder(&graph);
    duplicate.insert_edge(
        EdgeId::from_u128(0xd001),
        graph.edge(&ids.edge).unwrap().clone(),
    );
    assert_runtime_rejects_graph_with(duplicate.build_unchecked(), |error| {
        matches!(
            error,
            jellyflow_core::core::GraphValidationError::DuplicateEdge { .. }
        )
    });

    let mut kind_mismatch = graph.clone();
    kind_mismatch
        .with_detached_builder(|builder| {
            builder
                .port_mut(&ids.target_port)
                .map(|port| port.kind = PortKind::Exec)
        })
        .expect("target port");
    assert_runtime_rejects_graph_with(kind_mismatch, |error| {
        matches!(
            error,
            jellyflow_core::core::GraphValidationError::EdgeKindMismatch { .. }
        )
    });

    let mut capacity_source = graph.clone();
    capacity_source
        .with_detached_builder(|builder| {
            builder
                .port_mut(&ids.target_port)
                .map(|port| port.capacity = PortCapacity::Single)
        })
        .expect("target port");
    let mut capacity = routed_graph_builder(&capacity_source);
    capacity.insert_edge(
        EdgeId::from_u128(0xd002),
        Edge::new(EdgeKind::Data, ids.alternate_source_port, ids.target_port),
    );
    assert_runtime_rejects_graph_with(capacity.build_unchecked(), |error| {
        matches!(
            error,
            jellyflow_core::core::GraphValidationError::PortCapacityExceeded { .. }
        )
    });
}

#[test]
fn connect_command_commits_and_replays_one_stable_edge() {
    let (graph, ids) = graph_with_routed_edge();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let edge = EdgeId::from_u128(220);

    runtime
        .execute(GraphCommand::connect(
            runtime.snapshot().unwrap().version(),
            edge,
            ids.alternate_source_port,
            ids.target_port,
            NodeGraphConnectionMode::Strict,
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("connect applies");

    let snapshot = runtime.snapshot().unwrap();
    let connected = snapshot.graph().edge(&edge).expect("stable edge id");
    assert_eq!(connected.from, ids.alternate_source_port);
    assert_eq!(connected.to, ids.target_port);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);

    undo(&runtime);
    assert!(runtime.snapshot().unwrap().graph().edge(&edge).is_none());
    redo(&runtime);
    assert!(runtime.snapshot().unwrap().graph().edge(&edge).is_some());
}

#[test]
fn strict_connect_normalizes_reversed_input_output_arguments() {
    let (graph, ids) = graph_with_routed_edge();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let edge = EdgeId::from_u128(222);

    runtime
        .execute(GraphCommand::connect(
            runtime.snapshot().unwrap().version(),
            edge,
            ids.target_port,
            ids.alternate_source_port,
            NodeGraphConnectionMode::Strict,
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("reversed arguments normalize");

    let connected = runtime
        .snapshot()
        .unwrap()
        .graph()
        .edge(&edge)
        .unwrap()
        .clone();
    assert_eq!(connected.from, ids.alternate_source_port);
    assert_eq!(connected.to, ids.target_port);
}

#[test]
fn loose_connect_preserves_same_direction_argument_order() {
    let (graph, ids) = graph_with_routed_edge();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let edge = EdgeId::from_u128(223);

    runtime
        .execute(GraphCommand::connect(
            runtime.snapshot().unwrap().version(),
            edge,
            ids.source_port,
            ids.alternate_source_port,
            NodeGraphConnectionMode::Loose,
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("loose same-direction connection applies");

    let connected = runtime
        .snapshot()
        .unwrap()
        .graph()
        .edge(&edge)
        .unwrap()
        .clone();
    assert_eq!(connected.from, ids.source_port);
    assert_eq!(connected.to, ids.alternate_source_port);
}

#[test]
fn connect_rejects_node_and_target_port_connectability_overrides() {
    let (mut graph, ids) = graph_with_routed_edge();
    graph
        .with_detached_builder(|builder| {
            builder
                .node_mut(&ids.alternate_source)
                .map(|node| node.connectable = Some(false))
        })
        .expect("alternate source node");
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    assert_rejected_without_side_effects(
        &runtime,
        GraphCommand::connect(
            runtime.snapshot().unwrap().version(),
            EdgeId::from_u128(224),
            ids.alternate_source_port,
            ids.target_port,
            NodeGraphConnectionMode::Strict,
        ),
        "connect.source_not_connectable",
    );

    let (mut graph, ids) = graph_with_routed_edge();
    graph
        .with_detached_builder(|builder| {
            builder
                .port_mut(&ids.target_port)
                .map(|port| port.connectable_end = Some(false))
        })
        .expect("target port");
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    assert_rejected_without_side_effects(
        &runtime,
        GraphCommand::connect(
            runtime.snapshot().unwrap().version(),
            EdgeId::from_u128(225),
            ids.alternate_source_port,
            ids.target_port,
            NodeGraphConnectionMode::Strict,
        ),
        "connect.target_not_connectable",
    );
}

#[test]
fn connect_replaces_occupied_single_input_as_one_history_entry() {
    let (mut graph, ids) = graph_with_routed_edge();
    graph
        .with_detached_builder(|builder| {
            builder
                .port_mut(&ids.target_port)
                .map(|port| port.capacity = PortCapacity::Single)
        })
        .expect("target port");
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let replacement = EdgeId::from_u128(221);

    runtime
        .execute(GraphCommand::connect(
            runtime.snapshot().unwrap().version(),
            replacement,
            ids.alternate_source_port,
            ids.target_port,
            NodeGraphConnectionMode::Strict,
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("single-input replacement applies");

    let snapshot = runtime.snapshot().unwrap();
    assert!(snapshot.graph().edge(&ids.edge).is_none());
    assert!(snapshot.graph().edge(&replacement).is_some());
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);

    undo(&runtime);
    let snapshot = runtime.snapshot().unwrap();
    assert!(snapshot.graph().edge(&ids.edge).is_some());
    assert!(snapshot.graph().edge(&replacement).is_none());
    redo(&runtime);
    let snapshot = runtime.snapshot().unwrap();
    assert!(snapshot.graph().edge(&ids.edge).is_none());
    assert!(snapshot.graph().edge(&replacement).is_some());
}

#[test]
fn single_input_replacement_respects_existing_edge_deletion_policy() {
    let (mut graph, ids) = graph_with_routed_edge();
    graph
        .with_detached_builder(|builder| {
            builder
                .port_mut(&ids.target_port)
                .map(|port| port.capacity = PortCapacity::Single)
        })
        .expect("target port");
    graph
        .with_detached_builder(|builder| {
            builder
                .edge_mut(&ids.edge)
                .map(|edge| edge.deletable = Some(false))
        })
        .expect("existing edge");
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let replacement = EdgeId::from_u128(221);
    let edit = ConnectionEdit::connect(
        replacement,
        ids.alternate_source_port,
        ids.target_port,
        NodeGraphConnectionMode::Strict,
    );

    let evaluation = runtime.evaluate_connection(edit).unwrap();
    assert!(!evaluation.is_accept());
    assert_eq!(evaluation.diagnostics()[0].key, "delete.edge_not_deletable");

    let outcome = assert_rejected_without_side_effects(
        &runtime,
        GraphCommand::connection(evaluation.version(), edit),
        "delete.edge_not_deletable",
    );
    assert!(matches!(
        outcome.rejection().unwrap().diagnostics()[0].target,
        DiagnosticTarget::Edge { id } if id == ids.edge
    ));
}

#[test]
fn disconnect_and_reconnect_are_revisioned_exact_history_commands() {
    let (graph, ids) = graph_with_routed_edge();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();

    runtime
        .execute(GraphCommand::reconnect(
            runtime.snapshot().unwrap().version(),
            ids.edge,
            EdgeEndpoint::From,
            ids.alternate_source_port,
            NodeGraphConnectionMode::Strict,
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("reconnect applies");
    let snapshot = runtime.snapshot().unwrap();
    let reconnected = snapshot.graph().edge(&ids.edge).expect("edge retained");
    assert_eq!(reconnected.from, ids.alternate_source_port);
    assert_eq!(reconnected.to, ids.target_port);

    runtime
        .execute(GraphCommand::disconnect(
            runtime.snapshot().unwrap().version(),
            ids.edge,
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("disconnect applies");
    assert!(
        runtime
            .snapshot()
            .unwrap()
            .graph()
            .edge(&ids.edge)
            .is_none()
    );
    assert_eq!(runtime.history_state().unwrap().undo_len(), 2);

    undo(&runtime);
    assert_eq!(
        runtime
            .snapshot()
            .unwrap()
            .graph()
            .edge(&ids.edge)
            .expect("reconnected edge restored")
            .from,
        ids.alternate_source_port
    );
    undo(&runtime);
    assert_eq!(
        runtime
            .snapshot()
            .unwrap()
            .graph()
            .edge(&ids.edge)
            .expect("original edge restored")
            .from,
        ids.source_port
    );
}

#[test]
fn reconnect_rejects_an_existing_connection_without_side_effects() {
    let (graph, ids) = graph_with_routed_edge();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let duplicate = EdgeId::from_u128(226);

    runtime
        .execute(GraphCommand::connect(
            runtime.snapshot().unwrap().version(),
            duplicate,
            ids.alternate_source_port,
            ids.target_port,
            NodeGraphConnectionMode::Strict,
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("duplicate candidate setup applies");

    assert_rejected_without_side_effects(
        &runtime,
        GraphCommand::reconnect(
            runtime.snapshot().unwrap().version(),
            ids.edge,
            EdgeEndpoint::From,
            ids.alternate_source_port,
            NodeGraphConnectionMode::Strict,
        ),
        "connect.duplicate",
    );
}

#[test]
fn reconnect_to_single_target_removes_competing_edge_and_retains_identity() {
    let (graph, ids) = graph_with_routed_edge();
    let original_target = NodeId::from_u128(227);
    let original_target_port = PortId::from_u128(228);
    let competing_edge = EdgeId::from_u128(229);
    let mut builder = routed_graph_builder(&graph);
    builder
        .port_mut(&ids.target_port)
        .map(|port| port.capacity = PortCapacity::Single)
        .expect("target port");
    let mut original_target_node = graph.node(&ids.target).unwrap().clone();
    original_target_node.ports = vec![original_target_port];
    builder.insert_node(original_target, original_target_node);
    let mut original_port = graph.port(&ids.target_port).unwrap().clone();
    original_port.node = original_target;
    original_port.key = PortKey::new("original-target");
    builder.insert_port(original_target_port, original_port);
    builder.insert_edge(
        ids.edge,
        Edge::new(EdgeKind::Data, ids.source_port, original_target_port),
    );
    builder.insert_edge(
        competing_edge,
        Edge::new(EdgeKind::Data, ids.alternate_source_port, ids.target_port),
    );
    let runtime = GraphRuntime::builder(Default::default())
        .build(
            builder
                .build()
                .expect("structurally valid reconnect fixture"),
        )
        .unwrap();

    runtime
        .execute(GraphCommand::reconnect(
            runtime.snapshot().unwrap().version(),
            ids.edge,
            EdgeEndpoint::To,
            ids.target_port,
            NodeGraphConnectionMode::Strict,
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("single target reconnect applies");

    let snapshot = runtime.snapshot().unwrap();
    assert!(snapshot.graph().edge(&competing_edge).is_none());
    let retained = snapshot.graph().edge(&ids.edge).expect("edge id retained");
    assert_eq!(retained.from, ids.source_port);
    assert_eq!(retained.to, ids.target_port);
}

#[test]
fn runtime_policy_keeps_reconnect_and_disconnect_evaluation_equal_to_commit() {
    let (mut graph, ids) = graph_with_routed_edge();
    graph
        .with_detached_builder(|builder| {
            builder.edge_mut(&ids.edge).map(|edge| {
                edge.deletable = None;
                edge.reconnectable = None;
            })
        })
        .expect("edge");
    let policy = GraphRuntimePolicy::default()
        .with_edge_deletion(false)
        .with_edge_reconnection(false);
    let runtime = GraphRuntime::builder(Default::default())
        .with_policy(policy)
        .build(graph)
        .unwrap();
    let before = runtime.snapshot().unwrap();

    let reconnect = ConnectionEdit::reconnect(
        ids.edge,
        EdgeEndpoint::From,
        ids.alternate_source_port,
        NodeGraphConnectionMode::Strict,
    );
    let reconnect_evaluation = runtime.evaluate_connection(reconnect).unwrap();
    assert!(!reconnect_evaluation.is_accept());
    assert_eq!(
        reconnect_evaluation.diagnostics()[0].key,
        "connect.edge_not_reconnectable"
    );
    let reconnect_outcome = runtime
        .execute(GraphCommand::reconnect(
            reconnect_evaluation.version(),
            ids.edge,
            EdgeEndpoint::From,
            ids.alternate_source_port,
            NodeGraphConnectionMode::Strict,
        ))
        .into_result()
        .unwrap();
    assert_eq!(
        reconnect_outcome.rejection().unwrap().diagnostics()[0].key,
        reconnect_evaluation.diagnostics()[0].key
    );

    let disconnect_evaluation = runtime
        .evaluate_connection(ConnectionEdit::disconnect(ids.edge))
        .unwrap();
    assert!(!disconnect_evaluation.is_accept());
    assert_eq!(
        disconnect_evaluation.diagnostics()[0].key,
        "delete.edge_not_deletable"
    );
    let disconnect_outcome = runtime
        .execute(GraphCommand::disconnect(
            disconnect_evaluation.version(),
            ids.edge,
        ))
        .into_result()
        .unwrap();
    assert_eq!(
        disconnect_outcome.rejection().unwrap().diagnostics()[0].key,
        disconnect_evaluation.diagnostics()[0].key
    );
    assert!(before.ptr_eq(&runtime.snapshot().unwrap()));
    assert_eq!(runtime.history_state().unwrap().undo_len(), 0);
}

#[test]
fn connection_rule_rejections_preserve_diagnostics_and_runtime_state() {
    let (graph, ids) = graph_with_routed_edge();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let before = runtime.snapshot().unwrap();

    let duplicate_id = runtime
        .execute(GraphCommand::connect(
            before.version(),
            ids.edge,
            ids.alternate_source_port,
            ids.target_port,
            NodeGraphConnectionMode::Strict,
        ))
        .into_result()
        .unwrap();
    assert_eq!(
        duplicate_id.rejection().unwrap().diagnostics()[0].key,
        "connect.edge_already_exists"
    );
    assert!(before.ptr_eq(&runtime.snapshot().unwrap()));

    let wrong_direction = runtime
        .execute(GraphCommand::connect(
            before.version(),
            EdgeId::from_u128(222),
            ids.source_port,
            ids.alternate_source_port,
            NodeGraphConnectionMode::Strict,
        ))
        .into_result()
        .unwrap();
    assert_eq!(
        wrong_direction.rejection().unwrap().diagnostics()[0].key,
        "connect.direction_mismatch"
    );
    assert!(matches!(
        wrong_direction.rejection().unwrap().diagnostics()[0].target,
        DiagnosticTarget::Port { id } if id == ids.alternate_source_port
    ));
    assert!(before.ptr_eq(&runtime.snapshot().unwrap()));
    assert_eq!(runtime.history_state().unwrap().undo_len(), 0);
    assert_eq!(runtime.stats().unwrap().patch_builds, 0);
}

#[test]
fn command_rejections_expose_stable_machine_keys_and_precise_targets() {
    let (graph, ids) = graph_with_routed_edge();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let missing_node = NodeId::from_u128(90_001);
    let missing_port = PortId::from_u128(90_002);
    let version = runtime.snapshot().unwrap().version();

    let missing_node_outcome = assert_rejected_without_side_effects(
        &runtime,
        GraphCommand::set_node_data(version, missing_node, serde_json::Value::Null),
        "topology.node_missing",
    );
    assert!(matches!(
        missing_node_outcome.rejection().unwrap().diagnostics()[0].target,
        DiagnosticTarget::Node { id } if id == missing_node
    ));

    let missing_port_outcome = assert_rejected_without_side_effects(
        &runtime,
        GraphCommand::connect(
            version,
            EdgeId::from_u128(90_003),
            ids.source_port,
            missing_port,
            NodeGraphConnectionMode::Strict,
        ),
        "connect.port_missing",
    );
    assert!(matches!(
        missing_port_outcome.rejection().unwrap().diagnostics()[0].target,
        DiagnosticTarget::Port { id } if id == missing_port
    ));

    let self_connection = assert_rejected_without_side_effects(
        &runtime,
        GraphCommand::connect(
            version,
            EdgeId::from_u128(90_004),
            ids.source_port,
            ids.source_port,
            NodeGraphConnectionMode::Strict,
        ),
        "connect.self_connection",
    );
    assert!(matches!(
        self_connection.rejection().unwrap().diagnostics()[0].target,
        DiagnosticTarget::Port { id } if id == ids.source_port
    ));
}

#[test]
fn reconnect_uses_the_same_type_compatibility_rule_as_connect() {
    let (mut graph, ids) = graph_with_routed_edge();
    graph
        .with_detached_builder(|builder| {
            builder
                .port_mut(&ids.alternate_source_port)
                .map(|port| port.ty = Some(TypeDesc::String))
        })
        .expect("alternate source port");
    graph
        .with_detached_builder(|builder| {
            builder
                .port_mut(&ids.target_port)
                .map(|port| port.ty = Some(TypeDesc::Int))
        })
        .expect("target port");
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let before = runtime.snapshot().unwrap();

    let outcome = runtime
        .execute(GraphCommand::reconnect(
            before.version(),
            ids.edge,
            EdgeEndpoint::From,
            ids.alternate_source_port,
            NodeGraphConnectionMode::Strict,
        ))
        .into_result()
        .unwrap();

    let rejection = outcome.rejection().expect("typed reconnect rejects");
    assert_eq!(rejection.diagnostics()[0].key, "connect.type_mismatch");
    assert!(matches!(
        rejection.diagnostics()[0].target,
        crate::rules::DiagnosticTarget::Port { id } if id == ids.target_port
    ));
    assert!(before.ptr_eq(&runtime.snapshot().unwrap()));
    assert_eq!(runtime.history_state().unwrap().undo_len(), 0);
}
