use super::*;

#[test]
fn apply_transaction_rejects_node_with_missing_ports_atomically() {
    let mut graph = Graph::default();
    let before = graph.clone();

    let node_id = NodeId::new();
    let missing_port = PortId::new();
    let mut node = make_node("core.a");
    node.ports.push(missing_port);

    let tx = GraphTransaction::from_ops([GraphOp::AddNode { id: node_id, node }]);

    let err = apply_transaction(&mut graph, &tx).expect_err("invalid transaction must fail");
    assert!(matches!(err, ApplyError::InvalidTransactionResult { .. }));
    assert_eq!(
        serde_json::to_value(&graph).unwrap(),
        serde_json::to_value(&before).unwrap()
    );
}

#[test]
fn apply_transaction_rejects_unordered_added_port_atomically() {
    let mut graph = Graph::default();
    let node_id = NodeId::new();
    graph.nodes.insert(node_id, make_node("core.a"));
    let before = graph.clone();

    let port_id = PortId::new();
    let tx = GraphTransaction::from_ops([GraphOp::AddPort {
        id: port_id,
        port: make_port(node_id, "out", PortDirection::Out),
    }]);

    let err = apply_transaction(&mut graph, &tx).expect_err("invalid transaction must fail");
    assert!(matches!(err, ApplyError::InvalidTransactionResult { .. }));
    assert_eq!(
        serde_json::to_value(&graph).unwrap(),
        serde_json::to_value(&before).unwrap()
    );
}

#[test]
fn graph_transaction_facade_diff_apply_and_inverse_roundtrip() {
    let from = Graph::default();

    let node_id = NodeId::from_u128(10);
    let port_id = PortId::from_u128(11);
    let mut node = make_node("core.a");
    node.ports.push(port_id);

    let mut to = from.clone();
    to.nodes.insert(node_id, node);
    to.ports
        .insert(port_id, make_port(node_id, "out", PortDirection::Out));

    let tx = GraphTransaction::diff(&from, &to);
    let mut patched = from.clone();
    tx.apply_to(&mut patched).expect("apply diff facade");
    assert_eq!(
        serde_json::to_value(&patched).unwrap(),
        serde_json::to_value(&to).unwrap()
    );

    tx.inverse()
        .apply_to(&mut patched)
        .expect("apply inverse facade");
    assert_eq!(
        serde_json::to_value(&patched).unwrap(),
        serde_json::to_value(&from).unwrap()
    );
}

#[test]
fn graph_transaction_facade_consumes_ops_and_parts() {
    let node_id = NodeId::new();
    let tx = GraphTransaction::from_ops([GraphOp::SetNodeHidden {
        id: node_id,
        from: false,
        to: true,
    }])
    .with_label("Hide node");

    let (label, ops) = tx.clone().into_parts();
    assert_eq!(label.as_deref(), Some("Hide node"));
    assert!(matches!(
        ops.as_slice(),
        [GraphOp::SetNodeHidden {
            id,
            from: false,
            to: true
        }] if *id == node_id
    ));

    let ops = tx.into_ops();
    assert_eq!(ops.len(), 1);
}
