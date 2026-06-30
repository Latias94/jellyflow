use super::*;

#[test]
fn normalize_transaction_drops_noop_set_ops() {
    let node_id = NodeId::new();
    let edge_id = EdgeId::new();
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
        GraphOp::SetEdgeData {
            id: edge_id,
            from: serde_json::Value::Null,
            to: serde_json::Value::Null,
        },
        GraphOp::SetEdgeView {
            id: edge_id,
            from: EdgeViewDescriptor::default(),
            to: EdgeViewDescriptor::default(),
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
fn normalize_transaction_coalesces_node_port_order_chains() {
    let node_id = NodeId::new();
    let a = PortId::from_u128(1);
    let b = PortId::from_u128(2);
    let c = PortId::from_u128(3);

    let tx = GraphTransaction::from_ops([
        GraphOp::SetNodePorts {
            id: node_id,
            from: vec![a],
            to: vec![a, b],
        },
        GraphOp::SetNodePorts {
            id: node_id,
            from: vec![a, b],
            to: vec![b, a],
        },
        GraphOp::SetNodePorts {
            id: node_id,
            from: vec![b, a],
            to: vec![b, a, c],
        },
    ]);

    let normalized = crate::ops::normalize_transaction(tx);
    assert!(matches!(
        normalized.ops(),
        [GraphOp::SetNodePorts {
            id,
            from,
            to
        }] if *id == node_id && from == &vec![a] && to == &vec![b, a, c]
    ));
}

#[test]
fn normalize_transaction_coalesces_edge_data_and_view_chains() {
    let edge_id = EdgeId::new();
    let view_a = EdgeViewDescriptor {
        renderer_key: Some("branch-edge".to_string()),
        label: Some("A".to_string()),
        label_anchor: Some(EdgeLabelAnchor::Center),
        source_marker_key: None,
        target_marker_key: None,
        style_token: None,
        route_kind: None,
        hit_target_width: None,
    };
    let view_b = EdgeViewDescriptor {
        label: Some("B".to_string()),
        ..view_a.clone()
    };

    let tx = GraphTransaction::from_ops([
        GraphOp::SetEdgeData {
            id: edge_id,
            from: serde_json::Value::Null,
            to: serde_json::json!({ "branch": "A" }),
        },
        GraphOp::SetEdgeData {
            id: edge_id,
            from: serde_json::json!({ "branch": "A" }),
            to: serde_json::json!({ "branch": "B" }),
        },
        GraphOp::SetEdgeView {
            id: edge_id,
            from: EdgeViewDescriptor::default(),
            to: view_a,
        },
        GraphOp::SetEdgeView {
            id: edge_id,
            from: EdgeViewDescriptor {
                renderer_key: Some("branch-edge".to_string()),
                label: Some("A".to_string()),
                label_anchor: Some(EdgeLabelAnchor::Center),
                source_marker_key: None,
                target_marker_key: None,
                style_token: None,
                route_kind: None,
                hit_target_width: None,
            },
            to: view_b.clone(),
        },
    ]);

    let normalized = crate::ops::normalize_transaction(tx);
    assert!(matches!(
        &normalized.ops()[0],
        GraphOp::SetEdgeData { id, from, to }
            if *id == edge_id && from == &serde_json::Value::Null && to == &serde_json::json!({ "branch": "B" })
    ));
    assert!(matches!(
        &normalized.ops()[1],
        GraphOp::SetEdgeView { id, from, to }
            if *id == edge_id && from == &EdgeViewDescriptor::default() && to == &view_b
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
