use super::*;

#[test]
fn symbol_ref_nodes_must_reference_declared_symbols() {
    let mut graph = Graph::default();

    let symbol_id = SymbolId::from_u128(10);
    graph.symbols.insert(
        symbol_id,
        Symbol {
            name: "S".to_string(),
            ty: None,
            default_value: None,
            meta: serde_json::Value::Null,
        },
    );

    let node_id = NodeId::new();
    let mut node = make_node(SYMBOL_REF_NODE_KIND);
    node.data = serde_json::json!({ "symbol_id": symbol_id });
    graph.nodes.insert(node_id, node);

    assert!(
        validate_symbol_ref_targets_are_declared(&graph).is_ok(),
        "expected declared symbol to satisfy binding"
    );

    let targets = collect_symbol_ref_targets(&graph).expect("collect symbol ref targets");
    assert_eq!(targets.iter().copied().collect::<Vec<_>>(), vec![symbol_id]);
}

#[test]
fn validate_graph_reports_symbol_ref_binding_errors() {
    let mut graph = Graph::default();

    let node_id = NodeId::new();
    let mut node = make_node(SYMBOL_REF_NODE_KIND);
    let symbol_id = SymbolId::from_u128(99);
    node.data = serde_json::json!({ "symbol_id": symbol_id });
    graph.nodes.insert(node_id, node);

    let report = validate_graph_structural(&graph);
    assert!(report.errors.iter().any(|e| matches!(
        e,
        GraphValidationError::SymbolRefTargetNotDeclared { node, symbol_id: s }
            if *node == node_id && *s == symbol_id
    )));

    graph.symbols.insert(
        symbol_id,
        Symbol {
            name: "S".to_string(),
            ty: None,
            default_value: None,
            meta: serde_json::Value::Null,
        },
    );
    let report = validate_graph(&graph);
    assert!(report.is_ok());

    let mut bad = make_node(SYMBOL_REF_NODE_KIND);
    bad.data = serde_json::json!({});
    let bad_id = NodeId::new();
    graph.nodes.insert(bad_id, bad);
    let report = validate_graph_structural(&graph);
    assert!(report.errors.iter().any(|e| matches!(
        e,
        GraphValidationError::SymbolRefNodeMissingSymbolId { node } if *node == bad_id
    )));
}
