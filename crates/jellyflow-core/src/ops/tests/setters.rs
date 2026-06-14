use super::*;

#[test]
fn remove_group_detaches_child_nodes_and_inverts() {
    let mut graph = Graph::default();
    let group_id = GroupId::new();
    graph.insert_group(
        group_id,
        Group {
            title: "Group".to_string(),
            rect: crate::core::CanvasRect {
                origin: CanvasPoint { x: 0.0, y: 0.0 },
                size: crate::core::CanvasSize {
                    width: 100.0,
                    height: 100.0,
                },
            },
            color: None,
        },
    );

    let node_id = NodeId::new();
    let mut node = make_node("core.a");
    node.parent = Some(group_id);
    graph.insert_node(node_id, node);

    let tx = graph
        .build_remove_group_tx(group_id, "Delete Group")
        .expect("tx");
    apply_transaction(&mut graph, &tx).expect("apply");
    assert!(!graph.groups().contains_key(&group_id));
    assert_eq!(graph.nodes().get(&node_id).unwrap().parent, None);

    let undo = invert_transaction(&tx);
    apply_transaction(&mut graph, &undo).expect("undo apply");
    assert!(graph.groups().contains_key(&group_id));
    assert_eq!(graph.nodes().get(&node_id).unwrap().parent, Some(group_id));
}

#[test]
fn set_group_rect_and_child_positions_roundtrip_through_invert_transaction() {
    let mut graph = Graph::default();
    let group_id = GroupId::new();
    let rect0 = crate::core::CanvasRect {
        origin: CanvasPoint { x: 10.0, y: 20.0 },
        size: crate::core::CanvasSize {
            width: 100.0,
            height: 80.0,
        },
    };
    graph.insert_group(
        group_id,
        Group {
            title: "Group".to_string(),
            rect: rect0,
            color: None,
        },
    );

    let node_a = NodeId::new();
    let node_b = NodeId::new();
    let mut a = make_node("core.a");
    a.parent = Some(group_id);
    a.pos = CanvasPoint { x: 30.0, y: 40.0 };
    let mut b = make_node("core.b");
    b.parent = Some(group_id);
    b.pos = CanvasPoint { x: 50.0, y: 60.0 };
    graph.insert_node(node_a, a);
    graph.insert_node(node_b, b);

    let rect1 = crate::core::CanvasRect {
        origin: CanvasPoint { x: 110.0, y: 120.0 },
        size: rect0.size,
    };

    let tx = GraphTransaction::from_ops([
        GraphOp::SetGroupRect {
            id: group_id,
            from: rect0,
            to: rect1,
        },
        GraphOp::SetNodePos {
            id: node_a,
            from: CanvasPoint { x: 30.0, y: 40.0 },
            to: CanvasPoint { x: 130.0, y: 140.0 },
        },
        GraphOp::SetNodePos {
            id: node_b,
            from: CanvasPoint { x: 50.0, y: 60.0 },
            to: CanvasPoint { x: 150.0, y: 160.0 },
        },
    ])
    .with_label("Move Group");

    apply_transaction(&mut graph, &tx).expect("apply");
    assert_eq!(graph.groups().get(&group_id).unwrap().rect, rect1);
    assert_eq!(
        graph.nodes().get(&node_a).unwrap().pos,
        CanvasPoint { x: 130.0, y: 140.0 }
    );
    assert_eq!(
        graph.nodes().get(&node_b).unwrap().pos,
        CanvasPoint { x: 150.0, y: 160.0 }
    );

    let undo = invert_transaction(&tx);
    apply_transaction(&mut graph, &undo).expect("undo apply");
    assert_eq!(graph.groups().get(&group_id).unwrap().rect, rect0);
    assert_eq!(
        graph.nodes().get(&node_a).unwrap().pos,
        CanvasPoint { x: 30.0, y: 40.0 }
    );
    assert_eq!(
        graph.nodes().get(&node_b).unwrap().pos,
        CanvasPoint { x: 50.0, y: 60.0 }
    );
}

#[test]
fn set_node_size_roundtrips_through_invert_transaction() {
    let mut graph = Graph::default();
    let node_id = NodeId::new();
    insert_node(&mut graph, node_id, "core.a");

    let tx = GraphTransaction::from_ops([GraphOp::SetNodeSize {
        id: node_id,
        from: None,
        to: Some(crate::core::CanvasSize {
            width: 333.0,
            height: 222.0,
        }),
    }])
    .with_label("Resize");

    apply_transaction(&mut graph, &tx).expect("apply");
    assert_eq!(
        graph.nodes().get(&node_id).unwrap().size,
        Some(crate::core::CanvasSize {
            width: 333.0,
            height: 222.0,
        })
    );

    let undo = invert_transaction(&tx);
    apply_transaction(&mut graph, &undo).expect("undo apply");
    assert_eq!(graph.nodes().get(&node_id).unwrap().size, None);
}

#[test]
fn set_group_title_roundtrips_through_invert_transaction() {
    let mut graph = Graph::default();
    let group_id = GroupId::new();
    graph.insert_group(
        group_id,
        Group {
            title: "Group".to_string(),
            rect: crate::core::CanvasRect {
                origin: CanvasPoint { x: 0.0, y: 0.0 },
                size: crate::core::CanvasSize {
                    width: 100.0,
                    height: 100.0,
                },
            },
            color: None,
        },
    );

    let tx = GraphTransaction::from_ops([GraphOp::SetGroupTitle {
        id: group_id,
        from: "Group".to_string(),
        to: "My Group".to_string(),
    }])
    .with_label("Rename Group");

    apply_transaction(&mut graph, &tx).expect("apply");
    assert_eq!(graph.groups().get(&group_id).unwrap().title, "My Group");

    let undo = invert_transaction(&tx);
    apply_transaction(&mut graph, &undo).expect("undo apply");
    assert_eq!(graph.groups().get(&group_id).unwrap().title, "Group");
}

#[test]
fn set_node_data_roundtrips_through_invert_transaction() {
    let mut graph = Graph::default();
    let node = NodeId::new();
    insert_node(&mut graph, node, "demo.const");

    let tx = GraphTransaction::from_ops([GraphOp::SetNodeData {
        id: node,
        from: serde_json::Value::Null,
        to: serde_json::json!({ "value": 1.25 }),
    }])
    .with_label("Set value");

    apply_transaction(&mut graph, &tx).expect("apply");
    assert_eq!(
        graph.nodes().get(&node).unwrap().data,
        serde_json::json!({ "value": 1.25 })
    );

    let inverse = invert_transaction(&tx);
    apply_transaction(&mut graph, &inverse).expect("apply inverse");
    assert_eq!(
        graph.nodes().get(&node).unwrap().data,
        serde_json::Value::Null
    );
}

#[test]
fn set_edge_endpoints_updates_edge_in_place() {
    let mut graph = Graph::default();
    let a = NodeId::new();
    let b = NodeId::new();
    let c = NodeId::new();
    insert_node(&mut graph, a, "core.a");
    insert_node(&mut graph, b, "core.b");
    insert_node(&mut graph, c, "core.c");

    let out1 = PortId::new();
    let out2 = PortId::new();
    let inn = PortId::new();
    insert_port(&mut graph, out1, a, "out1", PortDirection::Out);
    insert_port(&mut graph, out2, c, "out2", PortDirection::Out);
    insert_port(&mut graph, inn, b, "in", PortDirection::In);

    let edge_id = EdgeId::new();
    graph.insert_edge(edge_id, make_edge(out1, inn));

    let tx = GraphTransaction::from_ops([GraphOp::SetEdgeEndpoints {
        id: edge_id,
        from: EdgeEndpoints {
            from: out1,
            to: inn,
        },
        to: EdgeEndpoints {
            from: out2,
            to: inn,
        },
    }]);
    apply_transaction(&mut graph, &tx).expect("apply");

    let edge = graph.edges().get(&edge_id).expect("edge");
    assert_eq!(edge.from, out2);
    assert_eq!(edge.to, inn);
}

#[test]
fn symbol_setters_roundtrip_through_normalize_and_invert() {
    let mut graph = Graph::default();
    let symbol_id = SymbolId::new();
    graph.insert_symbol(
        symbol_id,
        Symbol {
            name: "A".to_string(),
            ty: None,
            default_value: None,
            meta: serde_json::Value::Null,
        },
    );

    let baseline = serde_json::to_value(&graph).unwrap();

    let mut tx = GraphTransaction::new();
    tx.push(GraphOp::SetSymbolName {
        id: symbol_id,
        from: "A".to_string(),
        to: "B".to_string(),
    });
    tx.push(GraphOp::SetSymbolName {
        id: symbol_id,
        from: "B".to_string(),
        to: "C".to_string(),
    });
    tx.push(GraphOp::SetSymbolType {
        id: symbol_id,
        from: None,
        to: Some(TypeDesc::Int),
    });
    tx.push(GraphOp::SetSymbolDefaultValue {
        id: symbol_id,
        from: None,
        to: Some(serde_json::json!(123)),
    });

    let tx = crate::ops::normalize_transaction(tx);
    assert!(tx.len() < 4, "expected normalize to coalesce setter chain");

    apply_transaction(&mut graph, &tx).expect("apply forward");
    assert_eq!(graph.symbols().get(&symbol_id).unwrap().name, "C");
    assert_eq!(
        graph.symbols().get(&symbol_id).unwrap().ty,
        Some(TypeDesc::Int)
    );
    assert_eq!(
        graph.symbols().get(&symbol_id).unwrap().default_value,
        Some(serde_json::json!(123))
    );

    let inverse = invert_transaction(&tx);
    apply_transaction(&mut graph, &inverse).expect("apply inverse");
    assert_eq!(serde_json::to_value(&graph).unwrap(), baseline);
}

#[test]
fn graph_import_ops_roundtrip_through_normalize_and_invert() {
    let mut graph = Graph::default();
    let baseline = serde_json::to_value(&graph).unwrap();

    let imported = GraphId::from_u128(2);
    let import = GraphImport {
        alias: Some("math".to_string()),
    };

    let mut tx = GraphTransaction::new();
    tx.push(GraphOp::AddImport {
        id: imported,
        import: import.clone(),
    });
    tx.push(GraphOp::SetImportAlias {
        id: imported,
        from: Some("math".to_string()),
        to: Some("stdlib".to_string()),
    });
    tx.push(GraphOp::SetImportAlias {
        id: imported,
        from: Some("stdlib".to_string()),
        to: None,
    });

    let tx = crate::ops::normalize_transaction(tx);
    apply_transaction(&mut graph, &tx).expect("apply");
    assert!(graph.imports().contains_key(&imported));
    assert_eq!(graph.imports().get(&imported).unwrap().alias, None);

    let inverse = invert_transaction(&tx);
    apply_transaction(&mut graph, &inverse).expect("apply inverse");
    assert_eq!(serde_json::to_value(&graph).unwrap(), baseline);
}
