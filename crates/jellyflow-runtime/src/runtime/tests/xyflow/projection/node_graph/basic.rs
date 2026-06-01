use super::*;

#[test]
fn changes_from_transaction_maps_ops() {
    let (_g, a, _b, _out_port, _in_port, eid) = make_graph();

    let tx = GraphTransaction::from_ops([
        GraphOp::SetNodePos {
            id: a,
            from: CanvasPoint { x: 0.0, y: 0.0 },
            to: CanvasPoint { x: 10.0, y: 20.0 },
        },
        GraphOp::SetEdgeKind {
            id: eid,
            from: EdgeKind::Data,
            to: EdgeKind::Exec,
        },
    ]);

    let changes = NodeGraphChanges::from_transaction(&tx);
    assert_eq!(changes.nodes().len(), 1);
    assert_eq!(changes.edges().len(), 1);

    match &changes.nodes()[0] {
        NodeChange::Position {
            id: node_id,
            position: node_position,
        } => {
            assert_eq!(*node_id, a);
            assert_eq!(*node_position, CanvasPoint { x: 10.0, y: 20.0 });
        }
        other => panic!("unexpected node change: {other:?}"),
    }

    match &changes.edges()[0] {
        EdgeChange::Kind {
            id: edge_id,
            kind: edge_kind,
        } => {
            assert_eq!(*edge_id, eid);
            assert_eq!(*edge_kind, EdgeKind::Exec);
        }
        other => panic!("unexpected edge change: {other:?}"),
    }
}

#[test]
fn changes_from_transaction_ignores_non_node_edge_resource_ops() {
    let tx = GraphTransaction::from_ops([
        GraphOp::SetPortData {
            id: PortId::new(),
            from: serde_json::Value::Null,
            to: serde_json::json!({ "port": true }),
        },
        GraphOp::SetGroupTitle {
            id: GroupId::new(),
            from: "old".to_owned(),
            to: "new".to_owned(),
        },
    ]);

    let changes = NodeGraphChanges::from_transaction(&tx);

    assert!(changes.is_empty());
}
