use super::*;

#[test]
fn built_in_node_kind_constants_use_jellyflow_namespace() {
    assert_eq!(SUBGRAPH_NODE_KIND, "jellyflow.subgraph");
    assert_eq!(SYMBOL_REF_NODE_KIND, "jellyflow.symbol_ref");
}

#[test]
fn subgraph_nodes_must_reference_declared_imports() {
    let mut graph = Graph::default();

    let node_id = NodeId::new();
    let mut node = make_node(SUBGRAPH_NODE_KIND);
    let imported = GraphId::from_u128(2);
    node.data = serde_json::json!({ "graph_id": imported });
    graph.insert_node(node_id, node);

    assert!(
        validate_subgraph_targets_are_imported(&graph).is_err(),
        "expected missing import to be rejected"
    );

    graph.insert_import(imported, GraphImport::default());
    assert!(
        validate_subgraph_targets_are_imported(&graph).is_ok(),
        "expected declared import to satisfy binding"
    );
}

#[test]
fn subgraph_targets_must_resolve_through_import_closure() {
    let a = GraphId::from_u128(1);
    let b = GraphId::from_u128(2);
    let c = GraphId::from_u128(3);

    let mut g_a = Graph::new(a);
    g_a.insert_import(b, GraphImport::default());

    let node_id = NodeId::new();
    let mut node = make_node(SUBGRAPH_NODE_KIND);
    node.data = serde_json::json!({ "graph_id": b });
    g_a.insert_node(node_id, node);

    let mut g_b = Graph::new(b);
    g_b.insert_import(c, GraphImport::default());

    let g_c = Graph::new(c);

    let mut db = std::collections::BTreeMap::new();
    db.insert(a, g_a);
    db.insert(b, g_b);
    db.insert(c, g_c);

    let root = db.get(&a).expect("root graph must exist");

    validate_subgraph_targets_are_imported(root).expect("targets must be declared imports");
    let targets = collect_subgraph_targets(root).expect("collect targets");
    assert_eq!(targets.iter().copied().collect::<Vec<_>>(), vec![b]);

    let closure = resolve_import_closure(root, |id| db.get(&id)).expect("resolve closure");
    assert_eq!(closure.order, vec![c, b]);
    assert!(closure.contains(b));
    assert!(closure.contains(c));

    for target in targets {
        assert!(closure.contains(target));
    }
}

#[test]
fn validate_graph_reports_subgraph_import_binding_errors() {
    let mut graph = Graph::default();

    let node_id = NodeId::new();
    let mut node = make_node(SUBGRAPH_NODE_KIND);
    let imported = GraphId::from_u128(2);
    node.data = serde_json::json!({ "graph_id": imported });
    graph.insert_node(node_id, node);

    let report = validate_graph_structural(&graph);
    assert!(report.errors.iter().any(|e| matches!(
        e,
        GraphValidationError::SubgraphTargetNotImported { node, graph_id }
            if *node == node_id && *graph_id == imported
    )));

    graph.insert_import(imported, GraphImport::default());
    let report = validate_graph(&graph);
    assert!(report.is_ok());

    let mut bad = make_node(SUBGRAPH_NODE_KIND);
    bad.data = serde_json::json!({});
    let bad_id = NodeId::new();
    graph.insert_node(bad_id, bad);
    let report = validate_graph_structural(&graph);
    assert!(report.errors.iter().any(|e| matches!(
        e,
        GraphValidationError::SubgraphNodeMissingGraphId { node } if *node == bad_id
    )));
}
