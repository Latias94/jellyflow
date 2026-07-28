use super::*;

#[test]
fn batch_geometry_is_one_atomic_history_boundary() {
    let (graph, nodes) = graph_with_nodes(3);
    let before = graph.clone();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let first_position = CanvasPoint { x: 120.0, y: 80.0 };
    let second_position = CanvasPoint { x: 360.0, y: 220.0 };
    let second_size = CanvasSize {
        width: 280.0,
        height: 190.0,
    };

    let outcome = runtime
        .execute(GraphCommand::set_node_geometry(
            runtime.snapshot().unwrap().version(),
            [
                NodeGeometryEdit::new(nodes[0]).with_position(first_position),
                NodeGeometryEdit::new(nodes[1])
                    .with_position(second_position)
                    .with_size(Some(second_size)),
            ],
        ))
        .into_result()
        .unwrap();
    let receipt = outcome.applied().expect("batch geometry applies");

    let snapshot = runtime.snapshot().unwrap();
    assert_eq!(
        snapshot.graph().node(&nodes[0]).unwrap().pos,
        first_position
    );
    assert_eq!(
        snapshot.graph().node(&nodes[1]).unwrap().pos,
        second_position
    );
    assert_eq!(
        snapshot.graph().node(&nodes[1]).unwrap().size,
        Some(second_size)
    );
    assert_eq!(receipt.footprint().nodes.len(), 2);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);

    undo(&runtime);
    for node in &nodes {
        let snapshot = runtime.snapshot().unwrap();
        let restored = snapshot.graph().node(node).unwrap();
        let original = before.node(node).unwrap();
        assert_eq!(restored.pos, original.pos);
        assert_eq!(restored.size, original.size);
    }
    redo(&runtime);
    let snapshot = runtime.snapshot().unwrap();
    assert_eq!(
        snapshot.graph().node(&nodes[0]).unwrap().pos,
        first_position
    );
    assert_eq!(
        snapshot.graph().node(&nodes[1]).unwrap().pos,
        second_position
    );
}

#[test]
fn batch_geometry_conflict_names_every_affected_node_without_mutation() {
    let (graph, nodes) = graph_with_nodes(3);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let stale_version = runtime.snapshot().unwrap().version();
    runtime
        .execute(GraphCommand::set_node_data(
            stale_version,
            nodes[2],
            serde_json::json!({ "winner": true }),
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("winning command applies");
    let before = runtime.snapshot().unwrap();
    let history_before = runtime.history_state().unwrap();

    let outcome = runtime
        .execute(GraphCommand::set_node_geometry(
            stale_version,
            [
                NodeGeometryEdit::new(nodes[0]).with_position(CanvasPoint { x: 20.0, y: 30.0 }),
                NodeGeometryEdit::new(nodes[1]).with_position(CanvasPoint { x: 40.0, y: 50.0 }),
            ],
        ))
        .into_result()
        .unwrap();
    let conflict = outcome.conflict().expect("stale batch conflicts");

    assert_eq!(conflict.expected_revision(), 0);
    assert_eq!(conflict.current().revision(), 1);
    assert_eq!(
        conflict.footprint().nodes,
        nodes[..2].iter().copied().collect()
    );
    assert!(before.ptr_eq(&runtime.snapshot().unwrap()));
    assert_eq!(runtime.history_state().unwrap(), history_before);
}

#[test]
fn duplicate_batch_geometry_target_is_structurally_rejected() {
    let (graph, nodes) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();

    assert_rejected_without_side_effects(
        &runtime,
        GraphCommand::set_node_geometry(
            runtime.snapshot().unwrap().version(),
            [
                NodeGeometryEdit::new(nodes[0]).with_position(CanvasPoint { x: 10.0, y: 20.0 }),
                NodeGeometryEdit::new(nodes[0]).with_size(Some(CanvasSize {
                    width: 200.0,
                    height: 120.0,
                })),
            ],
        ),
        "geometry.node_duplicate",
    );
}

#[test]
fn empty_or_unchanged_batch_geometry_is_a_zero_side_effect_noop() {
    let (graph, nodes) = graph_with_nodes(1);
    let original = graph.node(&nodes[0]).unwrap().clone();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let version = runtime.snapshot().unwrap().version();

    for (expected_fact_reads, command) in [
        (0, GraphCommand::set_node_geometry(version, [])),
        (
            1,
            GraphCommand::set_node_geometry(
                version,
                [NodeGeometryEdit::new(nodes[0])
                    .with_position(original.pos)
                    .with_size(original.size)],
            ),
        ),
    ] {
        let outcome = runtime.execute(command).into_result().unwrap();
        let GraphCommandOutcome::Noop(noop) = outcome else {
            panic!("unchanged geometry must be a no-op");
        };
        assert_eq!(noop.planning_work().fact_reads(), expected_fact_reads);
        assert_eq!(noop.planning_work().index_bucket_reads(), 0);
        assert_eq!(noop.planning_work().index_entries_read(), 0);
        assert_eq!(noop.planning_work().operations_emitted(), 0);
        assert_eq!(noop.planning_work().full_type_revalidations(), 0);
        assert_eq!(runtime.snapshot().unwrap().revision(), 0);
        assert_eq!(runtime.history_state().unwrap().undo_len(), 0);
    }
}

#[test]
fn derived_geometry_advances_revision_without_polluting_user_history() {
    let (graph, nodes) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let derived_size = CanvasSize {
        width: 240.0,
        height: 176.0,
    };

    runtime
        .execute(
            GraphCommand::set_node_size(
                runtime.snapshot().unwrap().version(),
                nodes[0],
                Some(derived_size),
            )
            .without_history(),
        )
        .into_result()
        .unwrap()
        .applied()
        .expect("derived geometry applies");

    let snapshot = runtime.snapshot().unwrap();
    assert_eq!(snapshot.revision(), 1);
    assert_eq!(
        snapshot.graph().node(&nodes[0]).unwrap().size,
        Some(derived_size)
    );
    assert_eq!(runtime.history_state().unwrap().undo_len(), 0);
    assert_eq!(runtime.history_state().unwrap().redo_len(), 0);
}

#[test]
fn batch_geometry_planning_reads_only_declared_nodes_at_scale() {
    let (graph, nodes) = graph_with_nodes(10_000);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();

    let outcome = runtime
        .execute(GraphCommand::set_node_geometry(
            runtime.snapshot().unwrap().version(),
            [
                NodeGeometryEdit::new(nodes[0]).with_position(CanvasPoint { x: 1.0, y: 2.0 }),
                NodeGeometryEdit::new(nodes[5_000]).with_position(CanvasPoint { x: 3.0, y: 4.0 }),
                NodeGeometryEdit::new(nodes[9_999]).with_position(CanvasPoint { x: 5.0, y: 6.0 }),
            ],
        ))
        .into_result()
        .unwrap();
    let planning_work = outcome
        .applied()
        .expect("geometry batch applies")
        .planning_work();

    assert_eq!(planning_work.fact_reads(), 3);
    assert_eq!(planning_work.index_bucket_reads(), 0);
    assert_eq!(planning_work.index_entries_read(), 0);
    assert_eq!(planning_work.operations_emitted(), 3);
}
