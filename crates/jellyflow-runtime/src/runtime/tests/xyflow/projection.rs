use super::super::fixtures::make_graph;

use crate::runtime::xyflow::callbacks::{
    ConnectionChange, connection_changes_from_transaction, delete_changes_from_transaction,
};
use crate::runtime::xyflow::changes::{EdgeChange, NodeChange, NodeGraphChanges};
use jellyflow_core::core::{
    CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeKind, EdgeReconnectable, GroupId, NodeExtent,
    PortId, StickyNoteId,
};
use jellyflow_core::ops::{GraphOp, GraphTransaction};

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

#[test]
fn changes_from_transaction_reports_cascaded_edge_removals() {
    let (g, a, _b, out_port, _in_port, eid) = make_graph();
    let node = g.nodes.get(&a).expect("node").clone();
    let port = g.ports.get(&out_port).expect("port").clone();
    let edge = g.edges.get(&eid).expect("edge").clone();

    let remove_node_tx = GraphTransaction::from_ops([GraphOp::RemoveNode {
        id: a,
        node,
        ports: vec![(out_port, port.clone())],
        edges: vec![(eid, edge.clone())],
    }]);
    let remove_node_changes = NodeGraphChanges::from_transaction(&remove_node_tx);
    assert!(
        remove_node_changes
            .edges()
            .iter()
            .any(|change| matches!(change, EdgeChange::Remove { id } if *id == eid))
    );

    let remove_port_tx = GraphTransaction::from_ops([GraphOp::RemovePort {
        id: out_port,
        port,
        edges: vec![(eid, edge)],
    }]);
    let remove_port_changes = NodeGraphChanges::from_transaction(&remove_port_tx);
    assert!(
        remove_port_changes
            .edges()
            .iter()
            .any(|change| matches!(change, EdgeChange::Remove { id } if *id == eid))
    );
}

#[test]
fn changes_from_transaction_deduplicates_repeated_edge_removes() {
    let (g, _a, _b, out_port, _in_port, eid) = make_graph();
    let port = g.ports.get(&out_port).expect("port").clone();
    let edge = g.edges.get(&eid).expect("edge").clone();

    let repeated_remove_tx = GraphTransaction::from_ops([
        GraphOp::RemovePort {
            id: out_port,
            port,
            edges: vec![(eid, edge.clone())],
        },
        GraphOp::RemoveEdge {
            id: eid,
            edge: edge.clone(),
        },
    ]);

    let repeated_remove_changes = NodeGraphChanges::from_transaction(&repeated_remove_tx);
    let edge_removes = repeated_remove_changes
        .edges()
        .iter()
        .filter(|change| matches!(change, EdgeChange::Remove { id } if *id == eid))
        .count();
    assert_eq!(edge_removes, 1);

    let remove_add_remove_tx = GraphTransaction::from_ops([
        GraphOp::RemoveEdge {
            id: eid,
            edge: edge.clone(),
        },
        GraphOp::AddEdge {
            id: eid,
            edge: edge.clone(),
        },
        GraphOp::RemoveEdge { id: eid, edge },
    ]);

    let remove_add_remove_changes = NodeGraphChanges::from_transaction(&remove_add_remove_tx);
    assert!(matches!(
        remove_add_remove_changes.edges(),
        [
            EdgeChange::Remove { id: first },
            EdgeChange::Add { id: added, .. },
            EdgeChange::Remove { id: second },
        ] if *first == eid && *added == eid && *second == eid
    ));
}

#[test]
fn connection_changes_from_transaction_maps_edge_ops() {
    let (_g0, _a, _b, out_port, in_port, eid) = make_graph();

    let tx = GraphTransaction::from_ops([
        GraphOp::AddEdge {
            id: eid,
            edge: Edge {
                kind: EdgeKind::Data,
                from: out_port,
                to: in_port,
                selectable: None,
                deletable: None,
                reconnectable: None,
            },
        },
        GraphOp::SetEdgeEndpoints {
            id: eid,
            from: jellyflow_core::ops::EdgeEndpoints {
                from: out_port,
                to: in_port,
            },
            to: jellyflow_core::ops::EdgeEndpoints {
                from: out_port,
                to: in_port,
            },
        },
        GraphOp::RemoveEdge {
            id: eid,
            edge: Edge {
                kind: EdgeKind::Data,
                from: out_port,
                to: in_port,
                selectable: None,
                deletable: None,
                reconnectable: None,
            },
        },
    ]);

    let changes = connection_changes_from_transaction(&tx);
    assert_eq!(changes.len(), 3);
    assert!(matches!(changes[0], ConnectionChange::Connected(_)));
    assert!(matches!(changes[1], ConnectionChange::Reconnected { .. }));
    assert!(matches!(changes[2], ConnectionChange::Disconnected(_)));
}

#[test]
fn delete_changes_from_transaction_maps_resource_removals_and_cascaded_edges() {
    let (g, a, _b, out_port, _in_port, eid) = make_graph();
    let node = g.nodes.get(&a).expect("node").clone();
    let port = g.ports.get(&out_port).expect("port").clone();
    let edge = g.edges.get(&eid).expect("edge").clone();
    let group_id = GroupId::new();
    let note_id = StickyNoteId::new();
    let rect = CanvasRect {
        origin: CanvasPoint { x: 0.0, y: 0.0 },
        size: CanvasSize {
            width: 10.0,
            height: 10.0,
        },
    };

    let tx = GraphTransaction::from_ops([
        GraphOp::RemoveNode {
            id: a,
            node,
            ports: vec![(out_port, port.clone())],
            edges: vec![(eid, edge.clone())],
        },
        GraphOp::RemovePort {
            id: out_port,
            port,
            edges: vec![(eid, edge)],
        },
        GraphOp::RemoveGroup {
            id: group_id,
            group: jellyflow_core::core::Group {
                title: "group".to_owned(),
                rect,
                color: None,
            },
            detached: vec![(a, None)],
        },
        GraphOp::RemoveStickyNote {
            id: note_id,
            note: jellyflow_core::core::StickyNote {
                text: "note".to_owned(),
                rect,
                color: None,
            },
        },
    ]);

    let deleted = delete_changes_from_transaction(&tx);

    assert_eq!(deleted.nodes(), &[a]);
    assert_eq!(deleted.edges(), &[eid]);
    assert_eq!(deleted.groups(), &[group_id]);
    assert_eq!(deleted.sticky_notes(), &[note_id]);
}
