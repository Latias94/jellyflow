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
                runtime.snapshot().unwrap().version(),
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
        assert!(runtime.node_lifetime_key(node).unwrap().is_none());
    }

    let canonical = NodeId::from_u128(0xCA10);
    runtime
        .execute(GraphCommand::topology(
            runtime.snapshot().unwrap().version(),
            TopologyEdit::new().add_node(
                canonical,
                node_with_kind(template.clone(), "math.add", 2),
                [],
            ),
        ))
        .into_result()
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
            runtime.snapshot().unwrap().version(),
            TopologyEdit::new().add_node(opaque, opaque_node.clone(), []),
        ))
        .into_result()
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
            runtime.snapshot().unwrap().version(),
            TopologyEdit::new()
                .add_node(valid, node_with_kind(template.clone(), "math.add", 2), [])
                .add_node(invalid, node_with_kind(template, "legacy.add", 2), []),
        ),
        "topology.node_kind_not_canonical",
    );

    let snapshot = runtime.snapshot().unwrap();
    assert!(snapshot.graph().node(&valid).is_none());
    assert!(snapshot.graph().node(&invalid).is_none());
    assert!(runtime.node_lifetime_key(valid).unwrap().is_none());
    assert!(runtime.node_lifetime_key(invalid).unwrap().is_none());
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
                runtime.snapshot().unwrap().version(),
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
        .into_result()
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
fn late_failure_rolls_back_data_ports_and_incident_edges_atomically() {
    let (mut graph, ids) = graph_with_routed_edge();
    graph
        .with_detached_builder(|builder| {
            builder.node_mut(&ids.source).map(|node| {
                node.data = serde_json::json!({ "cases": [{ "id": "case-a" }] });
            })
        })
        .expect("source node");
    let initial_graph = frozen_graph_value(&graph);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let replacement = PortId::from_u128(103);
    let invalid_owner = PortId::from_u128(104);

    assert_rejected_without_side_effects(
        &runtime,
        GraphCommand::topology(
            runtime.snapshot().unwrap().version(),
            TopologyEdit::new()
                .set_node_data(
                    ids.source,
                    serde_json::json!({ "cases": [{ "id": "case-b" }] }),
                )
                .replace_node_ports(
                    ids.source,
                    [TopologyPort::new(
                        replacement,
                        port(ids.source, "case-b", PortDirection::Out),
                    )],
                    IncidentEdgePolicy::Cascade,
                )
                .replace_node_ports(
                    ids.target,
                    [TopologyPort::new(
                        invalid_owner,
                        port(ids.source, "wrong-owner", PortDirection::In),
                    )],
                    IncidentEdgePolicy::Reject,
                ),
        ),
        "topology.port_owner_mismatch",
    );

    let snapshot = runtime.snapshot().unwrap();
    assert_eq!(frozen_graph_value(snapshot.graph()), initial_graph);
    assert!(snapshot.graph().port(&replacement).is_none());
    assert!(snapshot.graph().edge(&ids.edge).is_some());
}

#[test]
fn data_ports_and_incident_edges_commit_undo_and_redo_as_one_topology_edit() {
    let (mut graph, ids) = graph_with_routed_edge();
    let initial_data = serde_json::json!({ "cases": [{ "id": "case-a" }] });
    graph
        .with_detached_builder(|builder| {
            builder.node_mut(&ids.source).map(|node| {
                node.data = initial_data.clone();
            })
        })
        .expect("source node");
    let initial_graph = frozen_graph_value(&graph);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let replacement = PortId::from_u128(105);
    let replacement_port = port(ids.source, "case-b", PortDirection::Out);
    let updated_data = serde_json::json!({ "cases": [{ "id": "case-b" }] });

    runtime
        .execute(GraphCommand::topology(
            runtime.snapshot().unwrap().version(),
            TopologyEdit::new()
                .set_node_data(ids.source, updated_data.clone())
                .replace_node_ports(
                    ids.source,
                    [TopologyPort::new(replacement, replacement_port.clone())],
                    IncidentEdgePolicy::Cascade,
                ),
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("atomic topology edit applies");

    let applied = runtime.snapshot().unwrap();
    assert_eq!(applied.revision(), 1);
    assert_eq!(
        applied.graph().node(&ids.source).unwrap().data,
        updated_data
    );
    assert_eq!(
        applied.graph().node(&ids.source).unwrap().ports,
        [replacement]
    );
    let installed_port = applied
        .graph()
        .port(&replacement)
        .expect("replacement port is installed");
    assert_eq!(installed_port.node, replacement_port.node);
    assert_eq!(installed_port.key, replacement_port.key);
    assert_eq!(installed_port.dir, replacement_port.dir);
    assert!(applied.graph().port(&ids.source_port).is_none());
    assert!(applied.graph().edge(&ids.edge).is_none());
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);
    assert_eq!(runtime.history_state().unwrap().redo_len(), 0);
    let applied_graph = frozen_graph_value(applied.graph());

    undo(&runtime);
    let undone = runtime.snapshot().unwrap();
    assert_eq!(undone.revision(), 2);
    assert_eq!(frozen_graph_value(undone.graph()), initial_graph);
    assert_eq!(undone.graph().node(&ids.source).unwrap().data, initial_data);
    assert!(undone.graph().port(&ids.source_port).is_some());
    assert!(undone.graph().port(&replacement).is_none());
    assert!(undone.graph().edge(&ids.edge).is_some());
    assert_eq!(runtime.history_state().unwrap().undo_len(), 0);
    assert_eq!(runtime.history_state().unwrap().redo_len(), 1);

    redo(&runtime);
    let redone = runtime.snapshot().unwrap();
    assert_eq!(redone.revision(), 3);
    assert_eq!(frozen_graph_value(redone.graph()), applied_graph);
    assert!(redone.graph().port(&ids.source_port).is_none());
    assert!(redone.graph().port(&replacement).is_some());
    assert!(redone.graph().edge(&ids.edge).is_none());
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);
    assert_eq!(runtime.history_state().unwrap().redo_len(), 0);
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
            runtime.snapshot().unwrap().version(),
            TopologyEdit::new().replace_node_ports(
                nodes[0],
                [
                    TopologyPort::new(first, first_port.clone()),
                    TopologyPort::new(second, second_port.clone()),
                ],
                IncidentEdgePolicy::Reject,
            ),
        ))
        .into_result()
        .unwrap();
    let before = runtime.snapshot().unwrap();
    let node_lifetime = runtime.node_lifetime_key(nodes[0]).unwrap().unwrap();
    let second_instance = runtime.port_instance_key(second).unwrap().unwrap();
    let mut updated_second = second_port;
    updated_second.data = serde_json::json!({ "branch": "else" });

    runtime
        .execute(GraphCommand::topology(
            before.version(),
            TopologyEdit::new().replace_node_ports(
                nodes[0],
                [
                    TopologyPort::new(second, updated_second),
                    TopologyPort::new(first, first_port),
                ],
                IncidentEdgePolicy::Reject,
            ),
        ))
        .into_result()
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
    assert_eq!(
        runtime.port_instance_key(second).unwrap(),
        Some(second_instance),
        "payload and ordering edits preserve a continuous port lifetime",
    );
    assert_eq!(
        runtime.node_lifetime_key(nodes[0]).unwrap(),
        Some(node_lifetime),
    );
    assert_eq!(
        before.port_instance_key(second),
        snapshot.port_instance_key(second),
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
    assert_eq!(
        runtime.port_instance_key(second).unwrap(),
        Some(second_instance),
        "undoing mutable port facts must not replace semantic identity",
    );
}

#[test]
fn semantic_port_replacement_retires_identity_across_execute_undo_and_redo() {
    let (graph, nodes) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let port_id = PortId::from_u128(113);
    let initial_port = port(nodes[0], "value", PortDirection::Out);

    runtime
        .execute(GraphCommand::topology(
            runtime.snapshot().unwrap().version(),
            TopologyEdit::new().replace_node_ports(
                nodes[0],
                [TopologyPort::new(port_id, initial_port.clone())],
                IncidentEdgePolicy::Reject,
            ),
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("initial port applies");
    let initial_snapshot = runtime.snapshot().unwrap();
    let node_lifetime = runtime.node_lifetime_key(nodes[0]).unwrap().unwrap();
    let initial_instance = runtime.port_instance_key(port_id).unwrap().unwrap();

    let mut replacement = initial_port.clone();
    replacement.key = PortKey::new("flow");
    replacement.dir = PortDirection::In;
    replacement.kind = PortKind::Exec;
    replacement.capacity = PortCapacity::Single;
    runtime
        .execute(GraphCommand::topology(
            initial_snapshot.version(),
            TopologyEdit::new().replace_node_ports(
                nodes[0],
                [TopologyPort::new(port_id, replacement.clone())],
                IncidentEdgePolicy::Reject,
            ),
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("same-ID semantic replacement applies atomically");

    let replaced_snapshot = runtime.snapshot().unwrap();
    let replaced_instance = runtime.port_instance_key(port_id).unwrap().unwrap();
    assert_ne!(replaced_instance, initial_instance);
    assert_eq!(replaced_instance.owner(), node_lifetime);
    assert_eq!(
        runtime.node_lifetime_key(nodes[0]).unwrap(),
        Some(node_lifetime),
        "replacing one port must not replace its owning node",
    );
    assert_eq!(
        initial_snapshot.port_instance_key(port_id),
        Some(initial_instance),
        "retained snapshots preserve the retired lifetime",
    );
    let installed = replaced_snapshot.graph().port(&port_id).unwrap();
    assert_eq!(installed.key, replacement.key);
    assert_eq!(installed.dir, replacement.dir);
    assert_eq!(installed.kind, replacement.kind);
    assert_eq!(installed.capacity, replacement.capacity);

    undo(&runtime);
    let undone_instance = runtime.port_instance_key(port_id).unwrap().unwrap();
    assert_ne!(undone_instance, initial_instance);
    assert_ne!(undone_instance, replaced_instance);
    assert_eq!(
        runtime
            .snapshot()
            .unwrap()
            .graph()
            .port(&port_id)
            .unwrap()
            .key,
        initial_port.key,
    );

    redo(&runtime);
    let redone_instance = runtime.port_instance_key(port_id).unwrap().unwrap();
    assert_ne!(redone_instance, initial_instance);
    assert_ne!(redone_instance, replaced_instance);
    assert_ne!(redone_instance, undone_instance);
    assert_eq!(
        runtime
            .snapshot()
            .unwrap()
            .graph()
            .port(&port_id)
            .unwrap()
            .key,
        replacement.key,
    );
}

#[test]
fn node_snapshot_identity_never_revives_after_same_id_replacement_or_history_replay() {
    let (graph, nodes) = graph_with_nodes(1);
    let node_id = nodes[0];
    let node_record = graph.node(&node_id).unwrap().clone();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let initial = runtime.snapshot().unwrap();
    let initial_lifetime = initial
        .node_lifetime_key(node_id)
        .expect("initial snapshot owns node identity");
    assert_eq!(
        runtime.node_lifetime_key(node_id).unwrap(),
        Some(initial_lifetime),
    );

    runtime
        .execute(GraphCommand::topology(
            initial.version(),
            TopologyEdit::new().remove_node(node_id, IncidentEdgePolicy::Reject),
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("node removal applies");
    let removed = runtime.snapshot().unwrap();
    assert_eq!(removed.node_lifetime_key(node_id), None);
    assert_eq!(
        initial.node_lifetime_key(node_id),
        Some(initial_lifetime),
        "retained snapshots keep their original node lifetime",
    );

    runtime
        .execute(GraphCommand::topology(
            removed.version(),
            TopologyEdit::new().add_node(node_id, node_record, []),
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("same node ID can be created in a later command");
    let replaced = runtime.snapshot().unwrap();
    let replaced_lifetime = replaced
        .node_lifetime_key(node_id)
        .expect("replacement snapshot owns a new node identity");
    assert_ne!(replaced_lifetime, initial_lifetime);

    undo(&runtime);
    assert_eq!(runtime.snapshot().unwrap().node_lifetime_key(node_id), None);
    undo(&runtime);
    let restored = runtime.snapshot().unwrap();
    let restored_lifetime = restored
        .node_lifetime_key(node_id)
        .expect("undo restores the node under a new lifetime");
    assert_ne!(restored_lifetime, initial_lifetime);
    assert_ne!(restored_lifetime, replaced_lifetime);

    redo(&runtime);
    assert_eq!(runtime.snapshot().unwrap().node_lifetime_key(node_id), None);
    redo(&runtime);
    let redone_lifetime = runtime
        .snapshot()
        .unwrap()
        .node_lifetime_key(node_id)
        .expect("redo recreates the node under another lifetime");
    assert_ne!(redone_lifetime, initial_lifetime);
    assert_ne!(redone_lifetime, replaced_lifetime);
    assert_ne!(redone_lifetime, restored_lifetime);
}

#[test]
fn connected_port_removal_obeys_reject_and_cascade_policies() {
    let (graph, ids) = graph_with_routed_edge();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();

    let rejected = runtime
        .execute(GraphCommand::topology(
            runtime.snapshot().unwrap().version(),
            TopologyEdit::new().replace_node_ports(ids.source, [], IncidentEdgePolicy::Reject),
        ))
        .into_result()
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
            runtime.snapshot().unwrap().version(),
            TopologyEdit::new().replace_node_ports(ids.source, [], IncidentEdgePolicy::Cascade),
        ))
        .into_result()
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
            runtime.snapshot().unwrap().version(),
            TopologyEdit::new().remove_node(ids.source, IncidentEdgePolicy::Cascade),
        ))
        .into_result()
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
            runtime.snapshot().unwrap().version(),
            TopologyEdit::new().replace_node_ports(
                nodes[0],
                [
                    TopologyPort::new(duplicate, port.clone()),
                    TopologyPort::new(duplicate, port),
                ],
                IncidentEdgePolicy::Reject,
            ),
        ))
        .into_result()
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
            runtime.snapshot().unwrap().version(),
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
            runtime.snapshot().unwrap().version(),
            TopologyEdit::new().remove_node(nodes[0], IncidentEdgePolicy::Reject),
        ))
        .into_result()
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
            runtime.snapshot().unwrap().version(),
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
    assert!(runtime.node_lifetime_key(new_node).unwrap().is_none());

    assert_rejected_without_side_effects(
        &runtime,
        GraphCommand::topology(
            runtime.snapshot().unwrap().version(),
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
            runtime.snapshot().unwrap().version(),
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
            runtime.snapshot().unwrap().version(),
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
            runtime.snapshot().unwrap().version(),
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
