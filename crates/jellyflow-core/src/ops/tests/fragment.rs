use super::*;

#[test]
fn fragment_paste_transaction_is_deterministic_for_seed() {
    let mut graph = Graph::default();
    let group_id = GroupId::new();
    graph.groups.insert(
        group_id,
        Group {
            title: "G".to_string(),
            rect: CanvasRect {
                origin: CanvasPoint { x: 0.0, y: 0.0 },
                size: CanvasSize {
                    width: 100.0,
                    height: 100.0,
                },
            },
            color: None,
        },
    );
    let a = NodeId::new();
    let b = NodeId::new();
    let mut na = make_node("core.a");
    na.parent = Some(group_id);
    let mut nb = make_node("core.b");
    nb.parent = Some(group_id);
    graph.nodes.insert(a, na);
    graph.nodes.insert(b, nb);

    let out = PortId::new();
    let inn = PortId::new();
    graph
        .ports
        .insert(out, make_port(a, "out", PortDirection::Out));
    graph
        .ports
        .insert(inn, make_port(b, "in", PortDirection::In));
    graph.nodes.get_mut(&a).unwrap().ports.push(out);
    graph.nodes.get_mut(&b).unwrap().ports.push(inn);

    let edge_id = EdgeId::new();
    graph.edges.insert(
        edge_id,
        Edge {
            kind: EdgeKind::Data,
            from: out,
            to: inn,
            selectable: None,
            deletable: None,
            reconnectable: None,
        },
    );

    let fragment = GraphFragment::from_selection(&graph, [a, b], [group_id]);
    let remapper = IdRemapper::new(IdRemapSeed(Uuid::nil()));
    let tuning = PasteTuning {
        offset: CanvasPoint { x: 10.0, y: 20.0 },
    };

    let tx1 = fragment.to_paste_transaction(&remapper, tuning);
    let tx2 = fragment.to_paste_transaction(&remapper, tuning);

    // Deterministic for a given seed and input.
    assert_eq!(
        serde_json::to_string(&tx1.ops).unwrap(),
        serde_json::to_string(&tx2.ops).unwrap()
    );

    // Apply into a new graph succeeds and preserves counts.
    let mut dst = Graph::default();
    apply_transaction(&mut dst, &tx1).unwrap();
    assert_eq!(dst.nodes.len(), 2);
    assert_eq!(dst.ports.len(), 2);
    assert_eq!(dst.edges.len(), 1);
    assert_eq!(dst.groups.len(), 1);

    let pasted_group = remapper.remap_group(group_id);
    let pasted_a = remapper.remap_node(a);
    let pasted_b = remapper.remap_node(b);
    let pasted_out = remapper.remap_port(out);
    let pasted_in = remapper.remap_port(inn);
    let pasted_edge = remapper.remap_edge(edge_id);

    assert_eq!(dst.nodes[&pasted_a].parent, Some(pasted_group));
    assert_eq!(dst.nodes[&pasted_b].parent, Some(pasted_group));
    assert_eq!(dst.nodes[&pasted_a].ports, vec![pasted_out]);
    assert_eq!(dst.nodes[&pasted_b].ports, vec![pasted_in]);
    assert_eq!(dst.edges[&pasted_edge].from, pasted_out);
    assert_eq!(dst.edges[&pasted_edge].to, pasted_in);
}

#[test]
fn fragment_from_nodes_includes_referenced_symbols() {
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

    let fragment = GraphFragment::from_nodes(&graph, [node_id]);
    assert!(
        fragment.symbols.contains_key(&symbol_id),
        "fragment must include referenced symbols for symbol-ref nodes"
    );
}

#[test]
fn fragment_paste_transaction_remaps_symbol_ref_targets_to_pasted_symbols() {
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

    let fragment = GraphFragment::from_nodes(&graph, [node_id]);
    let remapper = IdRemapper::new(IdRemapSeed(Uuid::nil()));
    let tx = fragment.to_paste_transaction(&remapper, PasteTuning::default());

    let mut dst = Graph::default();
    apply_transaction(&mut dst, &tx).expect("apply paste tx");

    assert_eq!(dst.nodes.len(), 1);
    assert_eq!(dst.symbols.len(), 1);

    let pasted_symbol_id = *dst.symbols.keys().next().expect("pasted symbol");
    let (pasted_node_id, pasted_node) = dst.nodes.iter().next().expect("pasted node");
    let target = symbol_ref_target_symbol_id(*pasted_node_id, pasted_node)
        .expect("parse symbol target")
        .expect("symbol target exists");

    assert_eq!(
        target, pasted_symbol_id,
        "symbol-ref node must point to the remapped pasted symbol"
    );
    assert_ne!(
        target, symbol_id,
        "symbol-ref target should not keep original source graph symbol id"
    );
}

#[test]
fn fragment_from_nodes_includes_referenced_subgraph_imports() {
    let mut graph = Graph::default();

    let imported_graph = GraphId::from_u128(42);
    graph.imports.insert(
        imported_graph,
        GraphImport {
            alias: Some("stdlib".to_string()),
        },
    );

    let node_id = NodeId::new();
    let mut node = make_node(SUBGRAPH_NODE_KIND);
    node.data = serde_json::json!({ "graph_id": imported_graph });
    graph.nodes.insert(node_id, node);

    let fragment = GraphFragment::from_nodes(&graph, [node_id]);
    assert!(
        fragment.imports.contains_key(&imported_graph),
        "fragment must include referenced imports for subgraph nodes"
    );
}

#[test]
fn fragment_paste_transaction_keeps_subgraph_target_graph_id_and_adds_import() {
    let mut graph = Graph::default();

    let imported_graph = GraphId::from_u128(43);
    graph.imports.insert(
        imported_graph,
        GraphImport {
            alias: Some("core".to_string()),
        },
    );

    let node_id = NodeId::new();
    let mut node = make_node(SUBGRAPH_NODE_KIND);
    node.data = serde_json::json!({ "graph_id": imported_graph });
    graph.nodes.insert(node_id, node);

    let fragment = GraphFragment::from_nodes(&graph, [node_id]);
    let remapper = IdRemapper::new(IdRemapSeed(Uuid::nil()));
    let tx = fragment.to_paste_transaction(&remapper, PasteTuning::default());

    let mut dst = Graph::default();
    apply_transaction(&mut dst, &tx).expect("apply paste tx");

    assert!(
        dst.imports.contains_key(&imported_graph),
        "paste must add referenced import before/with pasted subgraph nodes"
    );

    let (pasted_node_id, pasted_node) = dst.nodes.iter().next().expect("pasted node");
    let target = subgraph_target_graph_id(*pasted_node_id, pasted_node)
        .expect("parse subgraph target")
        .expect("subgraph target exists");
    assert_eq!(
        target, imported_graph,
        "subgraph node should keep graph_id target stable across paste"
    );
}
