use super::*;

#[test]
fn invert_transaction_restores_graph_state() {
    let mut graph = Graph::default();
    let a = NodeId::new();
    let b = NodeId::new();
    graph.nodes.insert(a, make_node("core.a"));
    graph.nodes.insert(b, make_node("core.b"));

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

    let baseline = serde_json::to_value(&graph).unwrap();

    let tx = graph.build_remove_node_tx(a, "Delete Node A").expect("tx");
    apply_transaction(&mut graph, &tx).expect("apply forward");

    let inverse = invert_transaction(&tx);
    apply_transaction(&mut graph, &inverse).expect("apply inverse");

    let restored = serde_json::to_value(&graph).unwrap();
    assert_eq!(restored, baseline);
}

#[test]
fn history_undo_redo_roundtrip() {
    let mut graph = Graph::default();
    let a = NodeId::new();
    let b = NodeId::new();
    graph.nodes.insert(a, make_node("core.a"));
    graph.nodes.insert(b, make_node("core.b"));

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

    let baseline = serde_json::to_value(&graph).unwrap();

    let tx = graph.build_remove_node_tx(a, "Delete Node A").expect("tx");
    apply_transaction(&mut graph, &tx).expect("apply forward");
    let forward_state = serde_json::to_value(&graph).unwrap();

    let mut history = GraphHistory::default();
    history.record(tx.clone());

    history
        .undo(|undo_tx| {
            apply_transaction(&mut graph, undo_tx).expect("apply undo");
            Ok::<GraphTransaction, ()>(undo_tx.clone())
        })
        .unwrap();
    assert_eq!(serde_json::to_value(&graph).unwrap(), baseline);

    history
        .redo(|redo_tx| {
            apply_transaction(&mut graph, redo_tx).expect("apply redo");
            Ok::<GraphTransaction, ()>(redo_tx.clone())
        })
        .unwrap();
    assert_eq!(serde_json::to_value(&graph).unwrap(), forward_state);
}
