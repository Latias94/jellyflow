use super::*;

#[test]
fn full_history_replay_prepares_only_the_top_owned_transaction() {
    const HISTORY_DEPTH: usize = 256;
    const PAYLOAD_BYTES: usize = 16 * 1024;

    let (graph, nodes) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    for revision in 0..HISTORY_DEPTH {
        runtime
            .execute(GraphCommand::set_node_data(
                runtime.snapshot().unwrap().version(),
                nodes[0],
                serde_json::json!({
                    "revision": revision,
                    "payload": "x".repeat(PAYLOAD_BYTES),
                }),
            ))
            .into_result()
            .unwrap()
            .applied()
            .expect("history seed command applies");
    }
    assert_eq!(runtime.history_state().unwrap().undo_len(), HISTORY_DEPTH);

    crate::graph_kernel::GraphHistory::reset_prepared_transaction_count_for_test();
    undo(&runtime);
    assert_eq!(
        crate::graph_kernel::GraphHistory::prepared_transaction_count_for_test(),
        1,
    );
    assert_eq!(
        runtime.history_state().unwrap().undo_len(),
        HISTORY_DEPTH - 1
    );
    assert_eq!(runtime.history_state().unwrap().redo_len(), 1);
    let undone = runtime.snapshot().unwrap();
    let undone_data = &undone.graph().node(&nodes[0]).expect("history node").data;
    assert_eq!(
        undone_data["revision"],
        serde_json::json!(HISTORY_DEPTH - 2)
    );
    assert_eq!(
        undone_data["payload"]
            .as_str()
            .expect("owned history payload")
            .len(),
        PAYLOAD_BYTES,
    );

    crate::graph_kernel::GraphHistory::reset_prepared_transaction_count_for_test();
    redo(&runtime);
    assert_eq!(
        crate::graph_kernel::GraphHistory::prepared_transaction_count_for_test(),
        1,
    );
    assert_eq!(runtime.history_state().unwrap().undo_len(), HISTORY_DEPTH);
    assert_eq!(runtime.history_state().unwrap().redo_len(), 0);
    let redone = runtime.snapshot().unwrap();
    let redone_data = &redone.graph().node(&nodes[0]).expect("history node").data;
    assert_eq!(
        redone_data["revision"],
        serde_json::json!(HISTORY_DEPTH - 1)
    );
    assert_eq!(
        redone_data["payload"]
            .as_str()
            .expect("owned history payload")
            .len(),
        PAYLOAD_BYTES,
    );

    crate::graph_kernel::GraphHistory::reset_prepared_transaction_count_for_test();
    for expected_undo_len in (0..HISTORY_DEPTH).rev() {
        undo(&runtime);
        let history = runtime.history_state().unwrap();
        assert_eq!(history.undo_len(), expected_undo_len);
        assert_eq!(history.redo_len(), HISTORY_DEPTH - expected_undo_len);

        let snapshot = runtime.snapshot().unwrap();
        let data = &snapshot.graph().node(&nodes[0]).expect("history node").data;
        if expected_undo_len == 0 {
            assert!(data.is_null());
        } else {
            assert_eq!(data["revision"], serde_json::json!(expected_undo_len - 1));
            assert_eq!(
                data["payload"]
                    .as_str()
                    .expect("owned history payload")
                    .len(),
                PAYLOAD_BYTES,
            );
        }
    }
    assert_eq!(
        crate::graph_kernel::GraphHistory::prepared_transaction_count_for_test(),
        HISTORY_DEPTH as u64,
    );

    crate::graph_kernel::GraphHistory::reset_prepared_transaction_count_for_test();
    for expected_undo_len in 1..=HISTORY_DEPTH {
        redo(&runtime);
        let history = runtime.history_state().unwrap();
        assert_eq!(history.undo_len(), expected_undo_len);
        assert_eq!(history.redo_len(), HISTORY_DEPTH - expected_undo_len);

        let snapshot = runtime.snapshot().unwrap();
        let data = &snapshot.graph().node(&nodes[0]).expect("history node").data;
        assert_eq!(data["revision"], serde_json::json!(expected_undo_len - 1));
        assert_eq!(
            data["payload"]
                .as_str()
                .expect("owned history payload")
                .len(),
            PAYLOAD_BYTES,
        );
    }
    assert_eq!(
        crate::graph_kernel::GraphHistory::prepared_transaction_count_for_test(),
        HISTORY_DEPTH as u64,
    );
}

#[test]
fn dynamic_topology_on_large_graph_never_uses_full_graph_diff() {
    let (graph, nodes) = graph_with_nodes(10_000);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let dynamic = PortId::from_u128(10_001);

    let outcome = runtime
        .execute(GraphCommand::topology(
            runtime.snapshot().unwrap().version(),
            TopologyEdit::new().replace_node_ports(
                nodes[0],
                [TopologyPort::new(
                    dynamic,
                    port(nodes[0], "dynamic", PortDirection::Out),
                )],
                IncidentEdgePolicy::Reject,
            ),
        ))
        .into_result()
        .unwrap();
    let receipt = outcome.applied().expect("topology applies");

    let planning_work = receipt.planning_work();
    assert_eq!(planning_work.fact_reads(), 3);
    assert_eq!(planning_work.index_bucket_reads(), 0);
    assert_eq!(planning_work.index_entries_read(), 0);
    assert_eq!(planning_work.operations_emitted(), 2);
    assert_eq!(receipt.patch().facts().len(), 2);
    assert_eq!(receipt.footprint().nodes.len(), 1);
    assert_eq!(receipt.footprint().ports.len(), 1);
}

#[test]
fn single_input_replacement_reads_only_incident_edges_and_bindings_at_scale() {
    const UNRELATED_EDGES: usize = 10_000;

    let (template_graph, template_nodes) = graph_with_nodes(1);
    let template = template_graph
        .node(&template_nodes[0])
        .expect("template node")
        .clone();
    let source = NodeId::from_u128(10);
    let target = NodeId::from_u128(11);
    let unrelated_source = NodeId::from_u128(12);
    let unrelated_target = NodeId::from_u128(13);
    let old_source = PortId::from_u128(20);
    let new_source = PortId::from_u128(21);
    let single_target = PortId::from_u128(22);
    let existing_edge = EdgeId::from_u128(30);
    let replacement_edge = EdgeId::from_u128(31);
    let existing_binding = BindingId::from_u128(40);
    let mut builder = GraphBuilder::new(GraphId::from_u128(0x203));

    let mut source_node = template.clone();
    source_node.ports = vec![old_source, new_source];
    builder.insert_node(source, source_node);
    builder.insert_port(old_source, port(source, "old", PortDirection::Out));
    builder.insert_port(new_source, port(source, "new", PortDirection::Out));
    let mut target_node = template.clone();
    target_node.ports = vec![single_target];
    builder.insert_node(target, target_node);
    let mut target_port = port(target, "target", PortDirection::In);
    target_port.capacity = PortCapacity::Single;
    builder.insert_port(single_target, target_port);
    builder.insert_edge(
        existing_edge,
        Edge::new(EdgeKind::Data, old_source, single_target),
    );
    builder.insert_binding(
        existing_binding,
        Binding::graph_local_to_source(
            GraphLocalBindingTarget::edge(existing_edge),
            "fixture://existing-edge",
            serde_json::Value::Null,
        ),
    );

    let mut unrelated_source_node = template.clone();
    let mut unrelated_target_node = template;
    unrelated_source_node.ports.reserve(UNRELATED_EDGES);
    unrelated_target_node.ports.reserve(UNRELATED_EDGES);
    for offset in 0..UNRELATED_EDGES {
        let offset = offset as u128;
        let from = PortId::from_u128(100_000 + offset);
        let to = PortId::from_u128(200_000 + offset);
        let edge = EdgeId::from_u128(300_000 + offset);
        unrelated_source_node.ports.push(from);
        unrelated_target_node.ports.push(to);
        builder.insert_port(from, port(unrelated_source, "from", PortDirection::Out));
        builder.insert_port(to, port(unrelated_target, "to", PortDirection::In));
        builder.insert_edge(edge, Edge::new(EdgeKind::Data, from, to));
        builder.insert_binding(
            BindingId::from_u128(400_000 + offset),
            Binding::graph_local_to_source(
                GraphLocalBindingTarget::Graph,
                "fixture://unrelated",
                serde_json::Value::Null,
            ),
        );
    }
    builder.insert_node(unrelated_source, unrelated_source_node);
    builder.insert_node(unrelated_target, unrelated_target_node);
    let runtime = GraphRuntime::builder(Default::default())
        .build(builder.build().expect("valid scale graph"))
        .unwrap();

    let outcome = runtime
        .execute(GraphCommand::connect(
            runtime.snapshot().unwrap().version(),
            replacement_edge,
            new_source,
            single_target,
            NodeGraphConnectionMode::Strict,
        ))
        .into_result()
        .unwrap();
    let planning_work = outcome
        .applied()
        .expect("single-input replacement applies")
        .planning_work();

    assert_eq!(planning_work.index_bucket_reads(), 3);
    assert_eq!(planning_work.index_entries_read(), 2);
    assert!(planning_work.fact_reads() < 32);
    let snapshot = runtime.snapshot().unwrap();
    assert!(snapshot.graph().edge(&existing_edge).is_none());
    assert!(snapshot.graph().binding(&existing_binding).is_none());
    assert_eq!(
        snapshot
            .graph()
            .edge(&replacement_edge)
            .expect("replacement edge")
            .to,
        single_target
    );
}

#[test]
fn batched_connections_query_only_port_local_staged_edges() {
    const CONNECTIONS: usize = 256;

    let (template_graph, template_nodes) = graph_with_nodes(1);
    let template = template_graph
        .node(&template_nodes[0])
        .expect("template node")
        .clone();
    let mut builder = GraphBuilder::new(GraphId::from_u128(0x204));
    let mut edit = TopologyEdit::new();

    for index in 0..CONNECTIONS {
        let offset = index as u128;
        let source = NodeId::from_u128(10_000 + offset);
        let target = NodeId::from_u128(20_000 + offset);
        let source_port = PortId::from_u128(30_000 + offset);
        let target_port = PortId::from_u128(40_000 + offset);
        let edge = EdgeId::from_u128(50_000 + offset);

        let mut source_node = template.clone();
        source_node.ports = vec![source_port];
        builder.insert_node(source, source_node);
        builder.insert_port(source_port, port(source, "out", PortDirection::Out));

        let mut target_node = template.clone();
        target_node.ports = vec![target_port];
        builder.insert_node(target, target_node);
        builder.insert_port(target_port, port(target, "in", PortDirection::In));

        edit = edit.connection(ConnectionEdit::connect(
            edge,
            source_port,
            target_port,
            NodeGraphConnectionMode::Strict,
        ));
    }

    let runtime = GraphRuntime::builder(Default::default())
        .build(builder.build().expect("valid batched graph"))
        .unwrap();
    let outcome = runtime
        .execute(GraphCommand::topology(
            runtime.snapshot().unwrap().version(),
            edit,
        ))
        .into_result()
        .unwrap();
    let planning_work = outcome
        .applied()
        .expect("batched connections apply")
        .planning_work();

    assert_eq!(planning_work.index_bucket_reads(), CONNECTIONS as u64);
    assert_eq!(planning_work.index_entries_read(), 0);
    assert_eq!(planning_work.operations_emitted(), CONNECTIONS as u64);
    assert_eq!(
        runtime.snapshot().unwrap().graph().edges().len(),
        CONNECTIONS
    );
}

#[test]
fn add_node_preserves_declared_port_order_and_plans_from_affected_facts() {
    let (graph, _) = graph_with_nodes(0);
    let (template_graph, template_nodes) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let node_id = jellyflow_core::core::NodeId::from_u128(10_000);
    let first = PortId::from_u128(900);
    let second = PortId::from_u128(100);
    let third = PortId::from_u128(500);
    let mut node = template_graph
        .node(&template_nodes[0])
        .expect("template node")
        .clone();
    node.ports = vec![second];

    let outcome = runtime
        .execute(GraphCommand::topology(
            runtime.snapshot().unwrap().version(),
            TopologyEdit::new().add_node(
                node_id,
                node,
                [
                    TopologyPort::new(first, port(node_id, "first", PortDirection::Out)),
                    TopologyPort::new(second, port(node_id, "second", PortDirection::Out)),
                    TopologyPort::new(third, port(node_id, "third", PortDirection::Out)),
                ],
            ),
        ))
        .into_result()
        .unwrap();
    let planning_work = outcome
        .applied()
        .expect("node topology applies")
        .planning_work();

    let snapshot = runtime.snapshot().unwrap();
    assert_eq!(
        snapshot.graph().node(&node_id).expect("added node").ports,
        [first, second, third]
    );
    assert_eq!(planning_work.fact_reads(), 4);
    assert_eq!(planning_work.index_bucket_reads(), 0);
    assert_eq!(planning_work.index_entries_read(), 0);
    assert_eq!(planning_work.operations_emitted(), 5);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);

    undo(&runtime);
    assert!(runtime.snapshot().unwrap().graph().node(&node_id).is_none());
    redo(&runtime);
    assert_eq!(
        runtime
            .snapshot()
            .unwrap()
            .graph()
            .node(&node_id)
            .expect("restored node")
            .ports,
        [first, second, third]
    );
}

#[test]
fn profiled_commands_keep_type_propagation_local_on_a_ten_thousand_edge_graph() {
    const UNRELATED_EDGES: usize = 10_000;

    let (template_graph, template_nodes) = graph_with_nodes(1);
    let template = template_graph
        .node(&template_nodes[0])
        .expect("template node")
        .clone();
    let affected_source = NodeId::from_u128(1);
    let affected_target = NodeId::from_u128(2);
    let unrelated_source = NodeId::from_u128(3);
    let unrelated_target = NodeId::from_u128(4);
    let affected_output = PortId::from_u128(10);
    let affected_input = PortId::from_u128(11);
    let mut builder = GraphBuilder::new(GraphId::from_u128(0x205));

    let mut source_node = template.clone();
    source_node.ports = vec![affected_output];
    builder.insert_node(affected_source, source_node);
    builder.insert_port(
        affected_output,
        port(affected_source, "affected-out", PortDirection::Out),
    );
    let mut target_node = template.clone();
    target_node.ports = vec![affected_input];
    builder.insert_node(affected_target, target_node);
    builder.insert_port(
        affected_input,
        port(affected_target, "affected-in", PortDirection::In),
    );
    builder.insert_edge(
        EdgeId::from_u128(20),
        Edge::new(EdgeKind::Data, affected_output, affected_input),
    );

    let mut unrelated_source_node = template.clone();
    let mut unrelated_target_node = template;
    unrelated_source_node.ports.reserve(UNRELATED_EDGES);
    unrelated_target_node.ports.reserve(UNRELATED_EDGES);
    for offset in 0..UNRELATED_EDGES {
        let offset = offset as u128;
        let from = PortId::from_u128(100_000 + offset);
        let to = PortId::from_u128(200_000 + offset);
        unrelated_source_node.ports.push(from);
        unrelated_target_node.ports.push(to);
        builder.insert_port(
            from,
            port(unrelated_source, "unrelated-out", PortDirection::Out),
        );
        builder.insert_port(
            to,
            port(unrelated_target, "unrelated-in", PortDirection::In),
        );
        builder.insert_edge(
            EdgeId::from_u128(300_000 + offset),
            Edge::new(EdgeKind::Data, from, to),
        );
    }
    builder.insert_node(unrelated_source, unrelated_source_node);
    builder.insert_node(unrelated_target, unrelated_target_node);
    let graph = builder.build().expect("valid profiled scale graph");
    let runtime = profiled_runtime(graph, Box::new(PassThroughProfile)).unwrap();

    let position_outcome = runtime
        .execute(GraphCommand::set_node_position(
            runtime.snapshot().unwrap().version(),
            affected_source,
            CanvasPoint { x: 50.0, y: 25.0 },
        ))
        .into_result()
        .unwrap();
    let position_work = position_outcome
        .applied()
        .expect("profiled position command applies")
        .planning_work();

    assert_eq!(position_work.fact_reads(), 1);
    assert_eq!(position_work.index_bucket_reads(), 0);
    assert_eq!(position_work.index_entries_read(), 0);
    assert_eq!(position_work.full_type_revalidations(), 0);

    let data_outcome = runtime
        .execute(GraphCommand::set_node_data(
            runtime.snapshot().unwrap().version(),
            affected_source,
            serde_json::json!({ "value": 1 }),
        ))
        .into_result()
        .unwrap();
    let data_work = data_outcome
        .applied()
        .expect("profiled payload command applies")
        .planning_work();

    assert_eq!(data_work.fact_reads(), 6);
    assert_eq!(data_work.index_bucket_reads(), 1);
    assert_eq!(data_work.index_entries_read(), 1);
    assert_eq!(data_work.operations_emitted(), 1);
    assert_eq!(data_work.full_type_revalidations(), 0);
}
