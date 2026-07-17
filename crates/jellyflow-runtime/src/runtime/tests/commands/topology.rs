use super::*;

#[test]
fn runtime_add_node_requires_exact_canonical_registered_kind() {
    let (graph, nodes) = graph_with_nodes(1);
    let template = graph.node(&nodes[0]).unwrap().clone();
    let runtime = GraphRuntime::builder(canonical_add_catalog())
        .build(graph)
        .unwrap();

    for (offset, kind, version) in [(1, "legacy.add", 2), (2, "math.add", 1), (3, "math.add", 3)] {
        let node = NodeId::from_u128(0xCA00 + offset);
        let rejected = assert_rejected_without_side_effects(
            &runtime,
            GraphCommand::topology(
                0,
                TopologyEdit::new().add_node(
                    node,
                    node_with_kind(template.clone(), kind, version),
                    [],
                ),
            ),
            "topology.node_kind_not_canonical",
        );
        assert!(matches!(
            rejected.rejection().unwrap().diagnostics()[0].target,
            DiagnosticTarget::Node { id } if id == node
        ));
        assert!(runtime.node_incarnation(node).unwrap().is_none());
    }

    let canonical = NodeId::from_u128(0xCA10);
    runtime
        .execute(GraphCommand::topology(
            0,
            TopologyEdit::new().add_node(
                canonical,
                node_with_kind(template.clone(), "math.add", 2),
                [],
            ),
        ))
        .unwrap()
        .applied()
        .expect("latest canonical registered kind applies");
    let published = runtime.snapshot().unwrap();
    let node = published.graph().node(&canonical).unwrap();
    assert_eq!(node.kind, NodeKindKey::new("math.add"));
    assert_eq!(node.kind_version, 2);

    let opaque = NodeId::from_u128(0xCA11);
    let mut opaque_node = node_with_kind(template, "future.opaque", 77);
    opaque_node.data = serde_json::json!({ "sentinel": [1, 2, 3] });
    runtime
        .execute(GraphCommand::topology(
            1,
            TopologyEdit::new().add_node(opaque, opaque_node.clone(), []),
        ))
        .unwrap()
        .applied()
        .expect("unregistered kinds remain opaque");
    let published = runtime.snapshot().unwrap();
    let node = published.graph().node(&opaque).unwrap();
    assert_eq!(node.kind, opaque_node.kind);
    assert_eq!(node.kind_version, 77);
    assert_eq!(node.data, opaque_node.data);
}

#[test]
fn noncanonical_add_rejects_the_entire_atomic_topology_edit() {
    let (graph, nodes) = graph_with_nodes(1);
    let template = graph.node(&nodes[0]).unwrap().clone();
    let runtime = GraphRuntime::builder(canonical_add_catalog())
        .build(graph)
        .unwrap();
    let valid = NodeId::from_u128(0xCA20);
    let invalid = NodeId::from_u128(0xCA21);

    assert_rejected_without_side_effects(
        &runtime,
        GraphCommand::topology(
            0,
            TopologyEdit::new()
                .add_node(valid, node_with_kind(template.clone(), "math.add", 2), [])
                .add_node(invalid, node_with_kind(template, "legacy.add", 2), []),
        ),
        "topology.node_kind_not_canonical",
    );

    let snapshot = runtime.snapshot().unwrap();
    assert!(snapshot.graph().node(&valid).is_none());
    assert!(snapshot.graph().node(&invalid).is_none());
    assert!(runtime.node_incarnation(valid).unwrap().is_none());
    assert!(runtime.node_incarnation(invalid).unwrap().is_none());
}

#[test]
fn dynamic_port_replacement_is_one_undoable_command() {
    let (graph, nodes) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let first = PortId::from_u128(101);
    let second = PortId::from_u128(102);

    let outcome = runtime
        .execute(
            GraphCommand::topology(
                0,
                TopologyEdit::new().replace_node_ports(
                    nodes[0],
                    [
                        TopologyPort::new(first, port(nodes[0], "first", PortDirection::Out)),
                        TopologyPort::new(second, port(nodes[0], "second", PortDirection::Out)),
                    ],
                    IncidentEdgePolicy::Reject,
                ),
            )
            .with_label("add dynamic branches"),
        )
        .unwrap();

    let receipt = outcome.applied().expect("topology applies");
    assert_eq!(receipt.revision(), 1);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);
    let snapshot = runtime.snapshot().unwrap();
    assert_eq!(
        snapshot.graph().node(&nodes[0]).unwrap().ports,
        [first, second]
    );
    assert!(snapshot.graph().port(&first).is_some());
    assert!(snapshot.graph().port(&second).is_some());

    undo(&runtime);
    let snapshot = runtime.snapshot().unwrap();
    assert!(snapshot.graph().node(&nodes[0]).unwrap().ports.is_empty());
    assert!(snapshot.graph().port(&first).is_none());
    assert!(snapshot.graph().port(&second).is_none());

    redo(&runtime);
    let snapshot = runtime.snapshot().unwrap();
    assert_eq!(
        snapshot.graph().node(&nodes[0]).unwrap().ports,
        [first, second]
    );
}

#[test]
fn stable_ports_can_reorder_and_update_payload_without_reallocation() {
    let (graph, nodes) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let first = PortId::from_u128(111);
    let second = PortId::from_u128(112);
    let first_port = port(nodes[0], "first", PortDirection::Out);
    let second_port = port(nodes[0], "second", PortDirection::Out);

    runtime
        .execute(GraphCommand::topology(
            0,
            TopologyEdit::new().replace_node_ports(
                nodes[0],
                [
                    TopologyPort::new(first, first_port.clone()),
                    TopologyPort::new(second, second_port.clone()),
                ],
                IncidentEdgePolicy::Reject,
            ),
        ))
        .unwrap();
    let mut updated_second = second_port;
    updated_second.data = serde_json::json!({ "branch": "else" });

    runtime
        .execute(GraphCommand::topology(
            1,
            TopologyEdit::new().replace_node_ports(
                nodes[0],
                [
                    TopologyPort::new(second, updated_second),
                    TopologyPort::new(first, first_port),
                ],
                IncidentEdgePolicy::Reject,
            ),
        ))
        .unwrap()
        .applied()
        .expect("reorder applies");

    let snapshot = runtime.snapshot().unwrap();
    assert_eq!(
        snapshot.graph().node(&nodes[0]).unwrap().ports,
        [second, first]
    );
    assert_eq!(
        snapshot.graph().port(&second).unwrap().data,
        serde_json::json!({ "branch": "else" })
    );

    undo(&runtime);
    let snapshot = runtime.snapshot().unwrap();
    assert_eq!(
        snapshot.graph().node(&nodes[0]).unwrap().ports,
        [first, second]
    );
    assert_eq!(
        snapshot.graph().port(&second).unwrap().data,
        serde_json::Value::Null
    );
}

#[test]
fn connected_port_removal_obeys_reject_and_cascade_policies() {
    let (graph, ids) = graph_with_routed_edge();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();

    let rejected = runtime
        .execute(GraphCommand::topology(
            0,
            TopologyEdit::new().replace_node_ports(ids.source, [], IncidentEdgePolicy::Reject),
        ))
        .unwrap();
    assert!(rejected.rejection().is_some());
    assert_eq!(runtime.snapshot().unwrap().revision(), 0);
    assert!(
        runtime
            .snapshot()
            .unwrap()
            .graph()
            .edge(&ids.edge)
            .is_some()
    );

    runtime
        .execute(GraphCommand::topology(
            0,
            TopologyEdit::new().replace_node_ports(ids.source, [], IncidentEdgePolicy::Cascade),
        ))
        .unwrap()
        .applied()
        .expect("cascade applies");
    let snapshot = runtime.snapshot().unwrap();
    assert!(snapshot.graph().port(&ids.source_port).is_none());
    assert!(snapshot.graph().edge(&ids.edge).is_none());

    undo(&runtime);
    let snapshot = runtime.snapshot().unwrap();
    assert!(snapshot.graph().port(&ids.source_port).is_some());
    assert!(snapshot.graph().edge(&ids.edge).is_some());
}

#[test]
fn node_removal_cascades_ports_and_edges_and_undo_restores_them() {
    let (graph, ids) = graph_with_routed_edge();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();

    runtime
        .execute(GraphCommand::topology(
            0,
            TopologyEdit::new().remove_node(ids.source, IncidentEdgePolicy::Cascade),
        ))
        .unwrap()
        .applied()
        .expect("cascade node removal applies");

    let snapshot = runtime.snapshot().unwrap();
    assert!(snapshot.graph().node(&ids.source).is_none());
    assert!(snapshot.graph().port(&ids.source_port).is_none());
    assert!(snapshot.graph().edge(&ids.edge).is_none());

    undo(&runtime);
    let snapshot = runtime.snapshot().unwrap();
    assert!(snapshot.graph().node(&ids.source).is_some());
    assert!(snapshot.graph().port(&ids.source_port).is_some());
    assert!(snapshot.graph().edge(&ids.edge).is_some());
}

#[test]
fn duplicate_ports_reject_without_revision_or_history_changes() {
    let (graph, nodes) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let duplicate = PortId::from_u128(121);
    let port = port(nodes[0], "duplicate", PortDirection::Out);

    let outcome = runtime
        .execute(GraphCommand::topology(
            0,
            TopologyEdit::new().replace_node_ports(
                nodes[0],
                [
                    TopologyPort::new(duplicate, port.clone()),
                    TopologyPort::new(duplicate, port),
                ],
                IncidentEdgePolicy::Reject,
            ),
        ))
        .unwrap();

    assert!(matches!(outcome, GraphCommandOutcome::Rejected(_)));
    assert_eq!(runtime.snapshot().unwrap().revision(), 0);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 0);
    assert_eq!(runtime.stats().unwrap().patch_builds, 0);
}

#[test]
fn runtime_policy_governs_topology_node_deletion_with_element_override_precedence() {
    let policy = GraphRuntimePolicy::default().with_node_deletion(false);
    let (graph, nodes) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .with_policy(policy)
        .build(graph)
        .unwrap();
    assert_rejected_without_side_effects(
        &runtime,
        GraphCommand::topology(
            0,
            TopologyEdit::new().remove_node(nodes[0], IncidentEdgePolicy::Reject),
        ),
        "delete.node_not_deletable",
    );

    let (mut graph, nodes) = graph_with_nodes(1);
    graph
        .with_detached_builder(|builder| {
            builder
                .node_mut(&nodes[0])
                .map(|node| node.deletable = Some(true))
        })
        .expect("node");
    let runtime = GraphRuntime::builder(Default::default())
        .with_policy(policy)
        .build(graph)
        .unwrap();
    runtime
        .execute(GraphCommand::topology(
            0,
            TopologyEdit::new().remove_node(nodes[0], IncidentEdgePolicy::Reject),
        ))
        .unwrap()
        .applied()
        .expect("per-node override permits deletion");
    assert!(
        runtime
            .snapshot()
            .unwrap()
            .graph()
            .node(&nodes[0])
            .is_none()
    );
}

#[test]
fn atomic_topology_rejects_node_port_and_edge_identity_reuse() {
    let (graph, nodes) = graph_with_nodes(1);
    let existing = graph.node(&nodes[0]).expect("existing node").clone();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let new_node = NodeId::from_u128(91_001);

    let add_then_remove = assert_rejected_without_side_effects(
        &runtime,
        GraphCommand::topology(
            0,
            TopologyEdit::new()
                .add_node(new_node, existing.clone(), [])
                .remove_node(new_node, IncidentEdgePolicy::Reject),
        ),
        "topology.node_lifecycle_conflict",
    );
    assert!(matches!(
        add_then_remove.rejection().unwrap().diagnostics()[0].target,
        DiagnosticTarget::Node { id } if id == new_node
    ));
    assert!(runtime.node_incarnation(new_node).unwrap().is_none());

    assert_rejected_without_side_effects(
        &runtime,
        GraphCommand::topology(
            0,
            TopologyEdit::new()
                .remove_node(nodes[0], IncidentEdgePolicy::Reject)
                .add_node(nodes[0], existing, []),
        ),
        "topology.node_lifecycle_conflict",
    );

    let (graph, ids) = graph_with_routed_edge();
    let source_port = graph.port(&ids.source_port).expect("source port").clone();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let port_reuse = assert_rejected_without_side_effects(
        &runtime,
        GraphCommand::topology(
            0,
            TopologyEdit::new()
                .replace_node_ports(ids.source, [], IncidentEdgePolicy::Cascade)
                .replace_node_ports(
                    ids.source,
                    [TopologyPort::new(ids.source_port, source_port)],
                    IncidentEdgePolicy::Reject,
                ),
        ),
        "topology.port_lifecycle_conflict",
    );
    assert!(matches!(
        port_reuse.rejection().unwrap().diagnostics()[0].target,
        DiagnosticTarget::Port { id } if id == ids.source_port
    ));

    let edge_reuse = assert_rejected_without_side_effects(
        &runtime,
        GraphCommand::topology(
            0,
            TopologyEdit::new()
                .connection(ConnectionEdit::disconnect(ids.edge))
                .connection(ConnectionEdit::connect(
                    ids.edge,
                    ids.source_port,
                    ids.target_port,
                    NodeGraphConnectionMode::Strict,
                )),
        ),
        "topology.edge_lifecycle_conflict",
    );
    assert!(matches!(
        edge_reuse.rejection().unwrap().diagnostics()[0].target,
        DiagnosticTarget::Edge { id } if id == ids.edge
    ));

    let new_edge = EdgeId::from_u128(91_002);
    assert_rejected_without_side_effects(
        &runtime,
        GraphCommand::topology(
            0,
            TopologyEdit::new()
                .connection(ConnectionEdit::connect(
                    new_edge,
                    ids.alternate_source_port,
                    ids.target_port,
                    NodeGraphConnectionMode::Strict,
                ))
                .connection(ConnectionEdit::disconnect(new_edge)),
        ),
        "topology.edge_lifecycle_conflict",
    );
}
