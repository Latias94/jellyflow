use super::*;

fn symbol(name: &str, ty: Option<jellyflow_core::types::TypeDesc>) -> Symbol {
    Symbol {
        name: name.to_owned(),
        ty,
        default_value: None,
        meta: serde_json::Value::Null,
    }
}

fn graph_with_symbols(symbols: impl IntoIterator<Item = (SymbolId, Symbol)>) -> Graph {
    let (graph, _) = graph_with_nodes(1);
    let mut builder = GraphBuilder::new(graph.graph_id());
    for (id, node) in graph.nodes() {
        builder.insert_node(*id, node.clone());
    }
    for (id, symbol) in symbols {
        builder.insert_symbol(id, symbol);
    }
    builder.build().expect("valid symbol fixture")
}

fn assert_symbol_eq(actual: &Symbol, expected: &Symbol) {
    assert_eq!(actual.name, expected.name);
    assert_eq!(actual.ty, expected.ty);
    assert_eq!(actual.default_value, expected.default_value);
    assert_eq!(actual.meta, expected.meta);
}

#[test]
fn one_symbol_edit_adds_updates_and_removes_with_exact_undo_redo() {
    let added_id = SymbolId::from_u128(10_001);
    let updated_id = SymbolId::from_u128(10_002);
    let removed_id = SymbolId::from_u128(10_003);
    let original_updated = symbol("threshold", Some(jellyflow_core::types::TypeDesc::Int));
    let original_removed = symbol("obsolete", None);
    let added = Symbol {
        name: "prompt".to_owned(),
        ty: Some(jellyflow_core::types::TypeDesc::String),
        default_value: Some(serde_json::json!("hello")),
        meta: serde_json::json!({ "group": "inputs" }),
    };
    let updated = Symbol {
        name: "temperature".to_owned(),
        ty: Some(jellyflow_core::types::TypeDesc::Float),
        default_value: Some(serde_json::json!(0.7)),
        meta: serde_json::json!({ "group": "model", "order": 1 }),
    };
    let graph = graph_with_symbols([
        (updated_id, original_updated.clone()),
        (removed_id, original_removed.clone()),
    ]);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();

    let outcome = runtime
        .execute(
            GraphCommand::symbols(
                runtime.snapshot().unwrap().version(),
                SymbolEdit::new()
                    .add(added_id, added.clone())
                    .update(updated_id, updated.clone())
                    .remove(removed_id),
            )
            .with_label("edit blackboard"),
        )
        .into_result()
        .unwrap();

    let receipt = outcome.applied().expect("symbol edit applies");
    assert_eq!(receipt.revision(), 1);
    assert_eq!(
        receipt.footprint().symbols,
        [added_id, updated_id, removed_id].into_iter().collect()
    );
    let graph = runtime.snapshot().unwrap();
    assert_symbol_eq(graph.graph().symbol(&added_id).unwrap(), &added);
    assert_symbol_eq(graph.graph().symbol(&updated_id).unwrap(), &updated);
    assert!(graph.graph().symbol(&removed_id).is_none());
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);

    undo(&runtime);
    let graph = runtime.snapshot().unwrap();
    assert!(graph.graph().symbol(&added_id).is_none());
    assert_symbol_eq(
        graph.graph().symbol(&updated_id).unwrap(),
        &original_updated,
    );
    assert_symbol_eq(
        graph.graph().symbol(&removed_id).unwrap(),
        &original_removed,
    );

    redo(&runtime);
    let graph = runtime.snapshot().unwrap();
    assert_symbol_eq(graph.graph().symbol(&added_id).unwrap(), &added);
    assert_symbol_eq(graph.graph().symbol(&updated_id).unwrap(), &updated);
    assert!(graph.graph().symbol(&removed_id).is_none());
}

#[test]
fn identical_symbol_replacement_is_a_noop() {
    let id = SymbolId::from_u128(10_011);
    let existing = Symbol {
        name: "unchanged".to_owned(),
        ty: Some(jellyflow_core::types::TypeDesc::Bool),
        default_value: Some(serde_json::json!(true)),
        meta: serde_json::json!({ "stable": true }),
    };
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph_with_symbols([(id, existing.clone())]))
        .unwrap();
    let before = runtime.snapshot().unwrap();
    let history_before = runtime.history_state().unwrap();
    let event_count = Rc::new(Cell::new(0));
    let callback_count = event_count.clone();
    runtime
        .subscribe(move |_| callback_count.set(callback_count.get() + 1))
        .unwrap();

    let outcome = runtime
        .execute(GraphCommand::symbols(
            before.version(),
            SymbolEdit::new().update(id, existing),
        ))
        .into_result()
        .unwrap();
    let GraphCommandOutcome::Noop(noop) = outcome else {
        panic!("identical replacement must be a no-op");
    };

    assert_eq!(noop.revision(), 0);
    assert!(noop.footprint().is_empty());
    assert!(before.ptr_eq(&runtime.snapshot().unwrap()));
    assert_eq!(runtime.history_state().unwrap(), history_before);
    assert_eq!(event_count.get(), 0);
}

#[test]
fn symbol_replacement_emits_only_fields_that_changed() {
    let id = SymbolId::from_u128(10_012);
    let existing = Symbol {
        name: "before".to_owned(),
        ty: Some(jellyflow_core::types::TypeDesc::Bool),
        default_value: Some(serde_json::json!(true)),
        meta: serde_json::json!({ "stable": true }),
    };
    let graph = graph_with_symbols([(id, existing.clone())]);
    let mut desired = existing;
    desired.name = "after".to_owned();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph.clone())
        .unwrap();
    let command = GraphCommand::symbols(
        runtime.snapshot().unwrap().version(),
        SymbolEdit::new().update(id, desired),
    );

    let planned = plan_graph_command(
        &graph,
        &GraphIndex::from_graph(&graph),
        &NodeCatalog::new(),
        &command,
        None,
        &crate::runtime::policy::GraphRuntimePolicy::default(),
    )
    .expect("symbol replacement plans");

    assert!(matches!(
        planned.transaction.ops(),
        [GraphOp::SetSymbolName { id: actual, from, to }]
            if *actual == id && from == "before" && to == "after"
    ));
}

#[test]
fn invalid_symbol_operations_reject_atomically_without_side_effects() {
    let existing_id = SymbolId::from_u128(10_021);
    let missing_id = SymbolId::from_u128(10_022);
    let existing = symbol("existing", None);
    let changed = symbol("changed", Some(jellyflow_core::types::TypeDesc::String));
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph_with_symbols([(existing_id, existing)]))
        .unwrap();

    for (edit, expected_key) in [
        (
            SymbolEdit::new().add(existing_id, changed.clone()),
            "symbol.already_exists",
        ),
        (
            SymbolEdit::new().update(missing_id, changed.clone()),
            "symbol.missing",
        ),
        (SymbolEdit::new().remove(missing_id), "symbol.missing"),
        (
            SymbolEdit::new()
                .update(existing_id, changed.clone())
                .remove(missing_id),
            "symbol.missing",
        ),
    ] {
        assert_rejected_without_side_effects(
            &runtime,
            GraphCommand::symbols(runtime.snapshot().unwrap().version(), edit),
            expected_key,
        );
    }
}

#[test]
fn atomic_symbol_edit_rejects_add_remove_identity_reuse_in_both_orders() {
    let new_id = SymbolId::from_u128(10_031);
    let existing_id = SymbolId::from_u128(10_032);
    let value = symbol("value", None);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph_with_symbols([(existing_id, value.clone())]))
        .unwrap();

    for edit in [
        SymbolEdit::new().add(new_id, value.clone()).remove(new_id),
        SymbolEdit::new()
            .remove(existing_id)
            .add(existing_id, value.clone()),
    ] {
        let outcome = assert_rejected_without_side_effects(
            &runtime,
            GraphCommand::symbols(runtime.snapshot().unwrap().version(), edit),
            "symbol.lifecycle_conflict",
        );
        assert!(matches!(
            outcome.rejection().unwrap().diagnostics()[0].target,
            DiagnosticTarget::Symbol { .. }
        ));
    }
}

#[test]
fn referenced_symbol_removal_is_rejected_and_node_data_updates_refresh_the_index() {
    let first = SymbolId::from_u128(10_041);
    let second = SymbolId::from_u128(10_042);
    let node_id = NodeId::from_u128(10_043);
    let (_, nodes) = graph_with_nodes(1);
    let template_graph = graph_with_symbols([]);
    let mut node = template_graph
        .node(&nodes[0])
        .expect("template node")
        .clone();
    node.kind = jellyflow_core::core::NodeKindKey::new(SYMBOL_REF_NODE_KIND);
    node.data = symbol_ref_node_data(first);
    let graph = GraphBuilder::new(GraphId::from_u128(10_044))
        .with_symbol(first, symbol("first", None))
        .with_symbol(second, symbol("second", None))
        .with_node(node_id, node)
        .build()
        .expect("valid referenced-symbol graph");
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();

    assert_rejected_without_side_effects(
        &runtime,
        GraphCommand::symbols(
            runtime.snapshot().unwrap().version(),
            SymbolEdit::new().remove(first),
        ),
        "symbol.referenced",
    );

    runtime
        .execute(GraphCommand::set_node_data(
            runtime.snapshot().unwrap().version(),
            node_id,
            symbol_ref_node_data(second),
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("reference retarget applies");
    runtime
        .execute(GraphCommand::symbols(
            runtime.snapshot().unwrap().version(),
            SymbolEdit::new().remove(first),
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("unreferenced symbol can be removed");
    assert_rejected_without_side_effects(
        &runtime,
        GraphCommand::symbols(
            runtime.snapshot().unwrap().version(),
            SymbolEdit::new().remove(second),
        ),
        "symbol.referenced",
    );
}

#[test]
fn symbol_ref_index_tracks_topology_adds_and_removals() {
    let symbol_id = SymbolId::from_u128(10_045);
    let node_id = NodeId::from_u128(10_046);
    let (_, nodes) = graph_with_nodes(1);
    let template_graph = graph_with_symbols([]);
    let mut node = template_graph
        .node(&nodes[0])
        .expect("template node")
        .clone();
    node.kind = jellyflow_core::core::NodeKindKey::new(SYMBOL_REF_NODE_KIND);
    node.data = symbol_ref_node_data(symbol_id);
    let graph = GraphBuilder::new(GraphId::from_u128(10_047))
        .with_symbol(symbol_id, symbol("tracked", None))
        .build()
        .expect("valid symbol graph");
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();

    runtime
        .execute(GraphCommand::topology(
            runtime.snapshot().unwrap().version(),
            TopologyEdit::new().add_node(node_id, node, []),
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("symbol ref node addition applies");
    assert_rejected_without_side_effects(
        &runtime,
        GraphCommand::symbols(
            runtime.snapshot().unwrap().version(),
            SymbolEdit::new().remove(symbol_id),
        ),
        "symbol.referenced",
    );
    runtime
        .execute(GraphCommand::topology(
            runtime.snapshot().unwrap().version(),
            TopologyEdit::new().remove_node(node_id, IncidentEdgePolicy::Reject),
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("symbol ref node removal applies");
    runtime
        .execute(GraphCommand::symbols(
            runtime.snapshot().unwrap().version(),
            SymbolEdit::new().remove(symbol_id),
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("symbol can be removed after its referrer");
}

#[test]
fn stale_symbol_command_conflict_declares_symbols_and_current_referrers() {
    let symbol_id = SymbolId::from_u128(10_051);
    let node_id = NodeId::from_u128(10_052);
    let (_, nodes) = graph_with_nodes(1);
    let template_graph = graph_with_symbols([]);
    let mut node = template_graph
        .node(&nodes[0])
        .expect("template node")
        .clone();
    node.kind = jellyflow_core::core::NodeKindKey::new(SYMBOL_REF_NODE_KIND);
    node.data = symbol_ref_node_data(symbol_id);
    let graph = GraphBuilder::new(GraphId::from_u128(10_053))
        .with_symbol(symbol_id, symbol("referenced", None))
        .with_node(node_id, node)
        .build()
        .expect("valid referenced-symbol graph");
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let stale_version = runtime.snapshot().unwrap().version();
    runtime
        .execute(GraphCommand::set_node_position(
            stale_version,
            node_id,
            CanvasPoint { x: 42.0, y: 8.0 },
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("winning command applies");
    let before = runtime.snapshot().unwrap();
    let stats_before = runtime.stats().unwrap();

    let outcome = runtime
        .execute(GraphCommand::symbols(
            stale_version,
            SymbolEdit::new().remove(symbol_id),
        ))
        .into_result()
        .unwrap();
    let conflict = outcome.conflict().expect("symbol command is stale");

    assert_eq!(conflict.expected_revision(), 0);
    assert_eq!(conflict.current().revision(), 1);
    assert!(conflict.footprint().symbols.contains(&symbol_id));
    assert!(conflict.footprint().nodes.contains(&node_id));
    assert!(before.ptr_eq(&runtime.snapshot().unwrap()));
    assert_eq!(runtime.stats().unwrap(), stats_before);
}
