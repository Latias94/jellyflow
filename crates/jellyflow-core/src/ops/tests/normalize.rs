use super::*;

#[test]
fn normalize_transaction_drops_noop_set_ops() {
    let node_id = NodeId::new();
    let p0 = CanvasPoint { x: 10.0, y: 20.0 };

    let tx = GraphTransaction::from_ops([
        GraphOp::SetNodePos {
            id: node_id,
            from: p0,
            to: p0,
        },
        GraphOp::SetNodeCollapsed {
            id: node_id,
            from: false,
            to: false,
        },
        GraphOp::SetNodeData {
            id: node_id,
            from: serde_json::Value::Null,
            to: serde_json::Value::Null,
        },
        GraphOp::AddNode {
            id: node_id,
            node: make_node("core.a"),
        },
    ])
    .with_label("Normalize");

    let normalized = crate::ops::normalize_transaction(tx);
    assert_eq!(normalized.label(), Some("Normalize"));
    assert_eq!(normalized.ops().len(), 1);
    assert!(matches!(normalized.ops()[0], GraphOp::AddNode { .. }));
}

#[test]
fn normalize_transaction_keeps_non_noop_set_ops() {
    let node_id = NodeId::new();

    let tx = GraphTransaction::from_ops([GraphOp::SetNodePos {
        id: node_id,
        from: CanvasPoint { x: 10.0, y: 20.0 },
        to: CanvasPoint { x: 11.0, y: 21.0 },
    }]);

    let normalized = crate::ops::normalize_transaction(tx);
    assert_eq!(normalized.ops().len(), 1);
    assert!(matches!(normalized.ops()[0], GraphOp::SetNodePos { .. }));
}

#[test]
fn normalize_transaction_coalesces_setter_chains_and_drops_resulting_noops() {
    let node_id = NodeId::new();

    let tx = GraphTransaction::from_ops([
        GraphOp::SetNodePos {
            id: node_id,
            from: CanvasPoint { x: 0.0, y: 0.0 },
            to: CanvasPoint { x: 10.0, y: 20.0 },
        },
        GraphOp::SetNodePos {
            id: node_id,
            from: CanvasPoint { x: 10.0, y: 20.0 },
            to: CanvasPoint { x: 0.0, y: 0.0 },
        },
    ]);

    let normalized = crate::ops::normalize_transaction(tx);
    assert!(normalized.is_empty());
}

#[test]
fn normalize_transaction_coalesces_setter_chains_when_chained() {
    let node_id = NodeId::new();

    let tx = GraphTransaction::from_ops([
        GraphOp::SetNodeCollapsed {
            id: node_id,
            from: false,
            to: true,
        },
        GraphOp::SetNodeCollapsed {
            id: node_id,
            from: true,
            to: false,
        },
        GraphOp::SetNodeCollapsed {
            id: node_id,
            from: false,
            to: true,
        },
    ]);

    let normalized = crate::ops::normalize_transaction(tx);
    assert_eq!(normalized.ops().len(), 1);
    assert!(matches!(
        &normalized.ops()[0],
        GraphOp::SetNodeCollapsed {
            id,
            from: false,
            to: true
        } if *id == node_id
    ));
}

#[test]
fn normalize_transaction_does_not_coalesce_non_contiguous_setter_chains() {
    let node_id = NodeId::new();

    let tx = GraphTransaction::from_ops([
        GraphOp::SetNodePos {
            id: node_id,
            from: CanvasPoint { x: 0.0, y: 0.0 },
            to: CanvasPoint { x: 10.0, y: 10.0 },
        },
        GraphOp::SetNodePos {
            id: node_id,
            from: CanvasPoint { x: 20.0, y: 20.0 },
            to: CanvasPoint { x: 30.0, y: 30.0 },
        },
    ]);

    let normalized = crate::ops::normalize_transaction(tx);

    assert_eq!(normalized.ops().len(), 2);
}
