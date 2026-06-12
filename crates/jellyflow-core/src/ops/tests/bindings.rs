use super::*;

fn source_binding(node_id: NodeId) -> Binding {
    Binding {
        subject: BindingEndpoint::graph_local(GraphLocalBindingTarget::Node { id: node_id }),
        target: BindingEndpoint::source(SourceAnchor::new(
            "source.pdf",
            serde_json::json!({ "page": 7 }),
        )),
        kind: Some("excerpt".to_string()),
        meta: serde_json::json!({ "color": "yellow" }),
    }
}

#[test]
fn binding_ops_apply_and_inverse_roundtrip() {
    let mut graph = Graph::default();
    let node_id = NodeId::new();
    graph.nodes.insert(node_id, make_node("core.note"));
    let binding_id = BindingId::new();
    let binding = source_binding(node_id);

    let tx = GraphTransaction::from_ops([GraphOp::AddBinding {
        id: binding_id,
        binding: binding.clone(),
    }]);
    apply_transaction(&mut graph, &tx).expect("add binding");
    assert_eq!(graph.bindings.get(&binding_id), Some(&binding));

    let inverse = invert_transaction(&tx);
    apply_transaction(&mut graph, &inverse).expect("remove binding through inverse");
    assert!(!graph.bindings.contains_key(&binding_id));
}

#[test]
fn binding_setters_coalesce_and_roundtrip() {
    let mut graph = Graph::default();
    let node_id = NodeId::new();
    graph.nodes.insert(node_id, make_node("core.note"));
    let binding_id = BindingId::new();
    graph.bindings.insert(binding_id, source_binding(node_id));

    let tx = GraphTransaction::from_ops([
        GraphOp::SetBindingKind {
            id: binding_id,
            from: Some("excerpt".to_string()),
            to: Some("quote".to_string()),
        },
        GraphOp::SetBindingKind {
            id: binding_id,
            from: Some("quote".to_string()),
            to: Some("highlight".to_string()),
        },
    ]);

    let normalized = crate::ops::normalize_transaction(tx);
    assert!(matches!(
        normalized.ops(),
        [GraphOp::SetBindingKind {
            id,
            from: Some(from),
            to: Some(to),
        }] if *id == binding_id && from == "excerpt" && to == "highlight"
    ));

    apply_transaction(&mut graph, &normalized).expect("apply binding setter");
    assert_eq!(
        graph.bindings[&binding_id].kind.as_deref(),
        Some("highlight")
    );
    apply_transaction(&mut graph, &invert_transaction(&normalized)).expect("undo binding setter");
    assert_eq!(graph.bindings[&binding_id].kind.as_deref(), Some("excerpt"));
}

#[test]
fn graph_diff_roundtrips_binding_changes() {
    let mut from = Graph::default();
    let node_id = NodeId::new();
    from.nodes.insert(node_id, make_node("core.note"));
    let binding_id = BindingId::new();
    from.bindings.insert(binding_id, source_binding(node_id));

    let mut to = from.clone();
    let updated = to.bindings.get_mut(&binding_id).expect("binding");
    updated.kind = Some("quote".to_string());
    updated.meta = serde_json::json!({ "color": "blue" });
    updated.target = BindingEndpoint::source(SourceAnchor::new(
        "source.pdf",
        serde_json::json!({ "page": 8 }),
    ));

    let tx = graph_diff(&from, &to);
    assert!(
        tx.ops()
            .iter()
            .any(|op| matches!(op, GraphOp::SetBindingTarget { id, .. } if *id == binding_id))
    );
    assert!(
        tx.ops()
            .iter()
            .any(|op| matches!(op, GraphOp::SetBindingKind { id, .. } if *id == binding_id))
    );
    assert!(
        tx.ops()
            .iter()
            .any(|op| matches!(op, GraphOp::SetBindingMeta { id, .. } if *id == binding_id))
    );

    let mut patched = from.clone();
    apply_transaction(&mut patched, &tx).expect("apply binding diff");
    assert_eq!(
        serde_json::to_value(&patched).unwrap(),
        serde_json::to_value(&to).unwrap()
    );
}

#[test]
fn removing_node_cascades_attached_bindings_and_undo_restores_them() {
    let mut graph = Graph::default();
    let node_id = NodeId::new();
    graph.nodes.insert(node_id, make_node("core.note"));
    let binding_id = BindingId::new();
    graph.bindings.insert(binding_id, source_binding(node_id));

    let baseline = serde_json::to_value(&graph).unwrap();
    let tx = graph
        .build_remove_node_tx(node_id, "remove source-bound node")
        .expect("remove node tx");
    assert!(matches!(
        tx.ops(),
        [GraphOp::RemoveNode { bindings, .. }] if bindings.len() == 1 && bindings[0].0 == binding_id
    ));

    apply_transaction(&mut graph, &tx).expect("remove node and binding");
    assert!(graph.nodes.is_empty());
    assert!(graph.bindings.is_empty());

    apply_transaction(&mut graph, &invert_transaction(&tx)).expect("undo node removal");
    assert_eq!(serde_json::to_value(&graph).unwrap(), baseline);
}

#[test]
fn fragment_paste_remaps_graph_local_binding_and_preserves_source_anchor() {
    let mut graph = Graph::default();
    let node_id = NodeId::new();
    graph.nodes.insert(node_id, make_node("core.note"));
    let binding_id = BindingId::from_u128(42);
    graph.bindings.insert(binding_id, source_binding(node_id));

    let fragment = GraphFragment::from_nodes(&graph, [node_id]);
    assert!(fragment.bindings.contains_key(&binding_id));

    let remapper = IdRemapper::new(IdRemapSeed(Uuid::nil()));
    let tx = fragment.to_paste_transaction(&remapper, PasteTuning::default());
    let mut pasted = Graph::default();
    apply_transaction(&mut pasted, &tx).expect("apply binding fragment paste");

    let pasted_binding_id = remapper.remap_binding(binding_id);
    let pasted_node_id = remapper.remap_node(node_id);
    let pasted_binding = pasted.bindings.get(&pasted_binding_id).expect("binding");

    assert_eq!(
        pasted_binding.subject,
        BindingEndpoint::graph_local(GraphLocalBindingTarget::Node { id: pasted_node_id })
    );
    assert!(matches!(
        &pasted_binding.target,
        BindingEndpoint::Source { anchor }
            if anchor.source_id == "source.pdf" && anchor.payload == serde_json::json!({ "page": 7 })
    ));
}

#[test]
fn fragment_excludes_bindings_with_graph_local_targets_outside_fragment() {
    let mut graph = Graph::default();
    let included = NodeId::new();
    let omitted = NodeId::new();
    graph.nodes.insert(included, make_node("core.included"));
    graph.nodes.insert(omitted, make_node("core.omitted"));
    let binding_id = BindingId::new();
    graph.bindings.insert(
        binding_id,
        Binding {
            subject: BindingEndpoint::graph_local(GraphLocalBindingTarget::Node { id: included }),
            target: BindingEndpoint::graph_local(GraphLocalBindingTarget::Node { id: omitted }),
            kind: None,
            meta: serde_json::Value::Null,
        },
    );

    let fragment = GraphFragment::from_nodes(&graph, [included]);

    assert!(
        !fragment.bindings.contains_key(&binding_id),
        "fragment must not keep bindings with graph-local endpoints outside the copied set"
    );
}
