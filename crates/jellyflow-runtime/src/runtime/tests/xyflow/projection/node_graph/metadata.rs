use super::*;

#[test]
fn changes_from_transaction_maps_node_edge_policy_ops() {
    let (_g, a, _b, _out_port, _in_port, eid) = make_graph();

    let tx = GraphTransaction::from_ops([
        GraphOp::SetNodeHidden {
            id: a,
            from: false,
            to: true,
        },
        GraphOp::SetEdgeReconnectable {
            id: eid,
            from: None,
            to: Some(EdgeReconnectable::Bool(false)),
        },
    ]);

    let changes = NodeGraphChanges::from_transaction(&tx);
    assert_eq!(changes.nodes().len(), 1);
    assert_eq!(changes.edges().len(), 1);

    match &changes.nodes()[0] {
        NodeChange::Hidden { id, hidden } => {
            assert_eq!(*id, a);
            assert!(*hidden);
        }
        other => panic!("unexpected node change: {other:?}"),
    }

    match &changes.edges()[0] {
        EdgeChange::Reconnectable { id, reconnectable } => {
            assert_eq!(*id, eid);
            assert_eq!(*reconnectable, Some(EdgeReconnectable::Bool(false)));
        }
        other => panic!("unexpected edge change: {other:?}"),
    }
}

#[test]
fn changes_from_transaction_maps_all_node_edge_metadata_ops() {
    let (_g, a, _b, out_port, in_port, eid) = make_graph();
    let group = GroupId::new();
    let extent = NodeExtent::Rect {
        rect: CanvasRect {
            origin: CanvasPoint { x: 1.0, y: 2.0 },
            size: CanvasSize {
                width: 30.0,
                height: 40.0,
            },
        },
    };

    let tx = GraphTransaction::from_ops([
        GraphOp::SetNodeSelectable {
            id: a,
            from: None,
            to: Some(false),
        },
        GraphOp::SetNodeDraggable {
            id: a,
            from: None,
            to: Some(true),
        },
        GraphOp::SetNodeConnectable {
            id: a,
            from: None,
            to: Some(false),
        },
        GraphOp::SetNodeDeletable {
            id: a,
            from: None,
            to: Some(true),
        },
        GraphOp::SetNodeParent {
            id: a,
            from: None,
            to: Some(group),
        },
        GraphOp::SetNodeExtent {
            id: a,
            from: None,
            to: Some(extent),
        },
        GraphOp::SetNodeExpandParent {
            id: a,
            from: None,
            to: Some(true),
        },
        GraphOp::SetNodePorts {
            id: a,
            from: vec![out_port],
            to: vec![out_port, in_port],
        },
        GraphOp::SetEdgeSelectable {
            id: eid,
            from: None,
            to: Some(false),
        },
        GraphOp::SetEdgeDeletable {
            id: eid,
            from: None,
            to: Some(true),
        },
    ]);

    let changes = NodeGraphChanges::from_transaction(&tx);
    assert_eq!(changes.nodes().len(), 8);
    assert_eq!(changes.edges().len(), 2);

    assert!(
        changes
            .nodes()
            .iter()
            .any(|change| matches!(change, NodeChange::Selectable { id, selectable: Some(false) } if *id == a))
    );
    assert!(changes.nodes().iter().any(
        |change| matches!(change, NodeChange::Draggable { id, draggable: Some(true) } if *id == a)
    ));
    assert!(
        changes
            .nodes()
            .iter()
            .any(|change| matches!(change, NodeChange::Connectable { id, connectable: Some(false) } if *id == a))
    );
    assert!(changes.nodes().iter().any(
        |change| matches!(change, NodeChange::Deletable { id, deletable: Some(true) } if *id == a)
    ));
    assert!(
        changes
            .nodes()
            .iter()
            .any(|change| matches!(change, NodeChange::Parent { id, parent: Some(found) } if *id == a && *found == group))
    );
    assert!(
        changes
            .nodes()
            .iter()
            .any(|change| matches!(change, NodeChange::Extent { id, extent: Some(found) } if *id == a && *found == extent))
    );
    assert!(
        changes
            .nodes()
            .iter()
            .any(|change| matches!(change, NodeChange::ExpandParent { id, expand_parent: Some(true) } if *id == a))
    );
    assert!(
        changes
            .nodes()
            .iter()
            .any(|change| matches!(change, NodeChange::Ports { id, ports } if *id == a && ports == &vec![out_port, in_port]))
    );
    assert!(
        changes
            .edges()
            .iter()
            .any(|change| matches!(change, EdgeChange::Selectable { id, selectable: Some(false) } if *id == eid))
    );
    assert!(changes.edges().iter().any(
        |change| matches!(change, EdgeChange::Deletable { id, deletable: Some(true) } if *id == eid)
    ));
}
