use super::*;

#[test]
fn stale_command_returns_current_snapshot_and_zero_side_effects() {
    let (graph, nodes) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let stale_version = runtime.snapshot().unwrap().version();

    runtime
        .execute(GraphCommand::set_node_data(
            stale_version,
            nodes[0],
            serde_json::json!({ "value": 1 }),
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("first command applies");
    let stats = runtime.stats().unwrap();
    let installed_impact_before = runtime.installed_impact_state_for_test();

    let outcome = runtime
        .execute(GraphCommand::set_node_data(
            stale_version,
            nodes[0],
            serde_json::json!({ "value": 2 }),
        ))
        .into_result()
        .unwrap();
    let conflict = outcome.conflict().expect("stale command conflicts");

    assert_eq!(conflict.expected_revision(), 0);
    assert_eq!(conflict.current().revision(), 1);
    assert!(conflict.footprint().nodes.contains(&nodes[0]));
    assert_eq!(
        runtime
            .snapshot()
            .unwrap()
            .graph()
            .node(&nodes[0])
            .unwrap()
            .data,
        serde_json::json!({ "value": 1 })
    );
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);
    assert_eq!(runtime.stats().unwrap(), stats);
    assert_eq!(
        runtime.installed_impact_state_for_test(),
        installed_impact_before,
        "conflicting commands must not update indexes or node identities"
    );
}

#[test]
fn stale_topology_and_connection_conflicts_include_current_dependencies() {
    let (graph, ids) = graph_with_routed_edge();
    let port_binding = BindingId::from_u128(80_001);
    let edge_binding = BindingId::from_u128(80_002);
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
    builder.insert_binding(
        port_binding,
        Binding::graph_local_to_source(
            GraphLocalBindingTarget::port(ids.source_port),
            "fixture://source-port",
            serde_json::Value::Null,
        ),
    );
    builder.insert_binding(
        edge_binding,
        Binding::graph_local_to_source(
            GraphLocalBindingTarget::edge(ids.edge),
            "fixture://edge",
            serde_json::Value::Null,
        ),
    );
    let runtime = GraphRuntime::builder(Default::default())
        .build(builder.build().expect("valid bound graph"))
        .unwrap();
    let stale_version = runtime.snapshot().unwrap().version();
    runtime
        .execute(GraphCommand::set_node_data(
            stale_version,
            ids.target,
            serde_json::json!({ "winner": true }),
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("winning command applies");
    let before = runtime.snapshot().unwrap();
    let history_before = runtime.history_state().unwrap();
    let stats_before = runtime.stats().unwrap();
    let installed_impact_before = runtime.installed_impact_state_for_test();
    let event_count = Rc::new(Cell::new(0));
    let callback_count = event_count.clone();
    runtime
        .subscribe(move |_| callback_count.set(callback_count.get() + 1))
        .unwrap();

    let mut claimed_port = before
        .graph()
        .port(&ids.source_port)
        .expect("occupied source port")
        .clone();
    claimed_port.node = ids.alternate_source;
    let replace_conflict = runtime
        .execute(GraphCommand::topology(
            stale_version,
            TopologyEdit::new().replace_node_ports(
                ids.alternate_source,
                [TopologyPort::new(ids.source_port, claimed_port)],
                IncidentEdgePolicy::Reject,
            ),
        ))
        .into_result()
        .unwrap();
    let footprint = replace_conflict
        .conflict()
        .expect("replace is stale")
        .footprint();
    assert!(footprint.nodes.contains(&ids.alternate_source));
    assert!(footprint.nodes.contains(&ids.source));
    assert!(footprint.nodes.contains(&ids.target));
    assert!(footprint.ports.contains(&ids.source_port));
    assert!(footprint.ports.contains(&ids.target_port));
    assert!(footprint.edges.contains(&ids.edge));
    assert!(footprint.bindings.contains(&port_binding));
    assert!(footprint.bindings.contains(&edge_binding));

    for command in [
        GraphCommand::connect(
            stale_version,
            ids.edge,
            ids.source_port,
            ids.target_port,
            NodeGraphConnectionMode::Strict,
        ),
        GraphCommand::disconnect(stale_version, ids.edge),
        GraphCommand::reconnect(
            stale_version,
            ids.edge,
            EdgeEndpoint::From,
            ids.alternate_source_port,
            NodeGraphConnectionMode::Strict,
        ),
    ] {
        let outcome = runtime.execute(command).into_result().unwrap();
        let footprint = outcome
            .conflict()
            .expect("connection command is stale")
            .footprint();
        assert!(footprint.edges.contains(&ids.edge));
        assert!(footprint.ports.contains(&ids.source_port));
        assert!(footprint.ports.contains(&ids.target_port));
        assert!(footprint.bindings.contains(&port_binding));
        assert!(footprint.bindings.contains(&edge_binding));
    }

    assert!(before.ptr_eq(&runtime.snapshot().unwrap()));
    assert_eq!(runtime.history_state().unwrap(), history_before);
    assert_eq!(runtime.stats().unwrap(), stats_before);
    assert_eq!(
        runtime.installed_impact_state_for_test(),
        installed_impact_before,
        "conflicting commands must not update indexes or node identities"
    );
    assert_eq!(event_count.get(), 0);
}

#[test]
fn stale_history_command_conflicts_without_consuming_history() {
    let (graph, nodes) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let stale_version = runtime.snapshot().unwrap().version();
    runtime
        .execute(GraphCommand::set_node_data(
            stale_version,
            nodes[0],
            serde_json::json!({ "value": 1 }),
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("seed command applies");
    let before = runtime.snapshot().unwrap();
    let history_before = runtime.history_state().unwrap();
    let stats_before = runtime.stats().unwrap();
    let event_count = Rc::new(Cell::new(0));
    let callback_count = event_count.clone();
    runtime
        .subscribe(move |_| callback_count.set(callback_count.get() + 1))
        .unwrap();

    let outcome = runtime
        .execute(GraphCommand::undo(stale_version))
        .into_result()
        .unwrap();
    let conflict = outcome.conflict().expect("stale undo conflicts");

    assert_eq!(conflict.expected_revision(), 0);
    assert_eq!(conflict.current().revision(), 1);
    assert!(before.ptr_eq(&runtime.snapshot().unwrap()));
    assert_eq!(runtime.history_state().unwrap(), history_before);
    assert_eq!(runtime.stats().unwrap(), stats_before);
    assert_eq!(event_count.get(), 0);
}

#[test]
fn empty_history_commands_are_zero_side_effect_noops() {
    let (graph, _) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let before = runtime.snapshot().unwrap();
    let history_before = runtime.history_state().unwrap();
    let stats_before = runtime.stats().unwrap();
    let event_count = Rc::new(Cell::new(0));
    let callback_count = event_count.clone();
    runtime
        .subscribe(move |_| callback_count.set(callback_count.get() + 1))
        .unwrap();

    for command in [
        GraphCommand::undo(before.version()),
        GraphCommand::redo(before.version()),
    ] {
        let outcome = runtime.execute(command).into_result().unwrap();
        let GraphCommandOutcome::Noop(noop) = outcome else {
            panic!("empty history command must be a no-op");
        };
        assert_eq!(noop.revision(), 0);
        assert!(noop.footprint().is_empty());
    }

    assert!(before.ptr_eq(&runtime.snapshot().unwrap()));
    assert_eq!(runtime.history_state().unwrap(), history_before);
    assert_eq!(runtime.stats().unwrap(), stats_before);
    assert_eq!(event_count.get(), 0);
}

#[test]
fn invalid_command_geometry_is_structurally_rejected_without_side_effects() {
    let (graph, nodes) = graph_with_nodes(1);
    let template = graph.node(&nodes[0]).expect("template node").clone();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let version = runtime.snapshot().unwrap().version();

    assert_rejected_without_side_effects(
        &runtime,
        GraphCommand::set_node_position(
            version,
            nodes[0],
            CanvasPoint {
                x: f32::NAN,
                y: 0.0,
            },
        ),
        "tx.non_finite",
    );
    assert_rejected_without_side_effects(
        &runtime,
        GraphCommand::set_node_size(
            version,
            nodes[0],
            Some(CanvasSize {
                width: 0.0,
                height: 10.0,
            }),
        ),
        "tx.invalid_size",
    );
    assert_rejected_without_side_effects(
        &runtime,
        GraphCommand::set_node_size(
            version,
            nodes[0],
            Some(CanvasSize {
                width: f32::INFINITY,
                height: 10.0,
            }),
        ),
        "tx.non_finite",
    );

    let mut invalid_node = template;
    invalid_node.pos.x = f32::NEG_INFINITY;
    assert_rejected_without_side_effects(
        &runtime,
        GraphCommand::topology(
            version,
            TopologyEdit::new().add_node(NodeId::from_u128(99), invalid_node, []),
        ),
        "tx.non_finite",
    );
}

#[test]
fn failed_history_install_preserves_graph_history_stats_and_notifications() {
    let (graph, nodes) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    runtime
        .execute(GraphCommand::topology(
            runtime.snapshot().unwrap().version(),
            TopologyEdit::new().remove_node(nodes[0], IncidentEdgePolicy::Reject),
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("node removal applies");
    runtime.set_next_incarnation_for_test(u64::MAX);

    let before = runtime.snapshot().unwrap();
    let history_before = runtime.history_state().unwrap();
    let stats_before = runtime.stats().unwrap();
    let event_count = Rc::new(Cell::new(0));
    let callback_count = event_count.clone();
    runtime
        .subscribe(move |_| callback_count.set(callback_count.get() + 1))
        .unwrap();

    let error = runtime
        .execute(GraphCommand::undo(before.version()))
        .into_result()
        .expect_err("incarnation allocation fails before history installation");

    assert!(matches!(error, GraphRuntimeError::IdentityExhausted));
    assert!(before.ptr_eq(&runtime.snapshot().unwrap()));
    assert_eq!(runtime.history_state().unwrap(), history_before);
    assert_eq!(runtime.stats().unwrap(), stats_before);
    assert_eq!(event_count.get(), 0);
}
