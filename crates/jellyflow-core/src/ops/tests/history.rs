use super::*;

#[test]
fn invert_transaction_restores_graph_state() {
    let mut graph = Graph::default();
    let ids = insert_connected_pair(&mut graph);

    let baseline = serde_json::to_value(&graph).unwrap();

    let tx = graph
        .build_remove_node_tx(ids.a, "Delete Node A")
        .expect("tx");
    apply_transaction(&mut graph, &tx).expect("apply forward");

    let inverse = invert_transaction(&tx);
    apply_transaction(&mut graph, &inverse).expect("apply inverse");

    let restored = serde_json::to_value(&graph).unwrap();
    assert_eq!(restored, baseline);
}

#[test]
fn history_undo_redo_roundtrip() {
    let mut graph = Graph::default();
    let ids = insert_connected_pair(&mut graph);

    let baseline = serde_json::to_value(&graph).unwrap();

    let tx = graph
        .build_remove_node_tx(ids.a, "Delete Node A")
        .expect("tx");
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

#[test]
fn history_replay_skips_empty_committed_transactions() {
    let node = NodeId::new();
    let tx = GraphTransaction::from_ops([GraphOp::SetNodeHidden {
        id: node,
        from: false,
        to: true,
    }]);

    let mut history = GraphHistory::default();
    history.record(tx.clone());

    let did_undo = history
        .undo(|_undo_tx| Ok::<GraphTransaction, ()>(GraphTransaction::new()))
        .unwrap();

    assert!(did_undo);
    assert_eq!(history.undo_len(), 0);
    assert_eq!(history.redo_len(), 0);

    history.record(tx);
    history
        .undo(|undo_tx| Ok::<GraphTransaction, ()>(undo_tx.clone()))
        .unwrap();
    assert!(history.can_redo());

    let did_redo = history
        .redo(|_redo_tx| Ok::<GraphTransaction, ()>(GraphTransaction::new()))
        .unwrap();

    assert!(did_redo);
    assert_eq!(history.undo_len(), 0);
    assert_eq!(history.redo_len(), 0);
}
