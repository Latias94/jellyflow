use super::{default_editor_config, make_graph};

use crate::io::NodeGraphViewState;
use crate::runtime::commit::NodeGraphPatch;
use crate::runtime::store::NodeGraphStore;
use crate::runtime::xyflow::apply::{apply_edge_changes, apply_node_changes};
use crate::runtime::xyflow::callbacks::{
    ConnectionChange, NodeGraphCommitCallbacks, NodeGraphGestureCallbacks, NodeGraphViewCallbacks,
    connection_changes_from_transaction, install_callbacks,
};
use crate::runtime::xyflow::changes::{EdgeChange, NodeChange, NodeGraphChanges};
use jellyflow_core::core::{
    CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeId, EdgeKind, EdgeReconnectable, Graph, GroupId,
    Node, NodeExtent, NodeId, NodeKindKey, Port,
};
use jellyflow_core::ops::{GraphOp, GraphTransaction};

#[test]
fn changes_from_transaction_maps_ops() {
    let (_g, a, _b, _out_port, _in_port, eid) = make_graph();

    let tx = GraphTransaction {
        label: None,
        ops: vec![
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
        ],
    };

    let changes = NodeGraphChanges::from_transaction(&tx);
    assert_eq!(changes.nodes.len(), 1);
    assert_eq!(changes.edges.len(), 1);

    match &changes.nodes[0] {
        NodeChange::Position {
            id: node_id,
            position: node_position,
        } => {
            assert_eq!(*node_id, a);
            assert_eq!(*node_position, CanvasPoint { x: 10.0, y: 20.0 });
        }
        other => panic!("unexpected node change: {other:?}"),
    }

    match &changes.edges[0] {
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

    let tx = GraphTransaction {
        label: None,
        ops: vec![
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
        ],
    };

    let changes = NodeGraphChanges::from_transaction(&tx);
    assert_eq!(changes.nodes.len(), 1);
    assert_eq!(changes.edges.len(), 1);

    match &changes.nodes[0] {
        NodeChange::Hidden { id, hidden } => {
            assert_eq!(*id, a);
            assert!(*hidden);
        }
        other => panic!("unexpected node change: {other:?}"),
    }

    match &changes.edges[0] {
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

    let tx = GraphTransaction {
        label: None,
        ops: vec![
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
        ],
    };

    let changes = NodeGraphChanges::from_transaction(&tx);
    assert_eq!(changes.nodes.len(), 8);
    assert_eq!(changes.edges.len(), 2);

    assert!(
        changes
            .nodes
            .iter()
            .any(|change| matches!(change, NodeChange::Selectable { id, selectable: Some(false) } if *id == a))
    );
    assert!(changes.nodes.iter().any(
        |change| matches!(change, NodeChange::Draggable { id, draggable: Some(true) } if *id == a)
    ));
    assert!(
        changes
            .nodes
            .iter()
            .any(|change| matches!(change, NodeChange::Connectable { id, connectable: Some(false) } if *id == a))
    );
    assert!(changes.nodes.iter().any(
        |change| matches!(change, NodeChange::Deletable { id, deletable: Some(true) } if *id == a)
    ));
    assert!(
        changes
            .nodes
            .iter()
            .any(|change| matches!(change, NodeChange::Parent { id, parent: Some(found) } if *id == a && *found == group))
    );
    assert!(
        changes
            .nodes
            .iter()
            .any(|change| matches!(change, NodeChange::Extent { id, extent: Some(found) } if *id == a && *found == extent))
    );
    assert!(
        changes
            .nodes
            .iter()
            .any(|change| matches!(change, NodeChange::ExpandParent { id, expand_parent: Some(true) } if *id == a))
    );
    assert!(
        changes
            .nodes
            .iter()
            .any(|change| matches!(change, NodeChange::Ports { id, ports } if *id == a && ports == &vec![out_port, in_port]))
    );
    assert!(
        changes
            .edges
            .iter()
            .any(|change| matches!(change, EdgeChange::Selectable { id, selectable: Some(false) } if *id == eid))
    );
    assert!(changes.edges.iter().any(
        |change| matches!(change, EdgeChange::Deletable { id, deletable: Some(true) } if *id == eid)
    ));
}

#[test]
fn changes_from_transaction_reports_cascaded_edge_removals() {
    let (g, a, _b, out_port, _in_port, eid) = make_graph();
    let node = g.nodes.get(&a).expect("node").clone();
    let port = g.ports.get(&out_port).expect("port").clone();
    let edge = g.edges.get(&eid).expect("edge").clone();

    let remove_node_tx = GraphTransaction {
        label: None,
        ops: vec![GraphOp::RemoveNode {
            id: a,
            node,
            ports: vec![(out_port, port.clone())],
            edges: vec![(eid, edge.clone())],
        }],
    };
    let remove_node_changes = NodeGraphChanges::from_transaction(&remove_node_tx);
    assert!(
        remove_node_changes
            .edges
            .iter()
            .any(|change| matches!(change, EdgeChange::Remove { id } if *id == eid))
    );

    let remove_port_tx = GraphTransaction {
        label: None,
        ops: vec![GraphOp::RemovePort {
            id: out_port,
            port,
            edges: vec![(eid, edge)],
        }],
    };
    let remove_port_changes = NodeGraphChanges::from_transaction(&remove_port_tx);
    assert!(
        remove_port_changes
            .edges
            .iter()
            .any(|change| matches!(change, EdgeChange::Remove { id } if *id == eid))
    );
}

#[test]
fn changes_to_transaction_is_reversible_and_applicable() {
    let (g0, a, _b, out_port, in_port, eid) = make_graph();

    let changes = NodeGraphChanges {
        nodes: vec![NodeChange::Position {
            id: a,
            position: CanvasPoint { x: 42.0, y: 7.0 },
        }],
        edges: vec![EdgeChange::Endpoints {
            id: eid,
            from: out_port,
            to: in_port,
        }],
    };

    let tx = changes.to_transaction(&g0).expect("tx");
    let mut g1 = g0.clone();
    tx.apply_to(&mut g1).expect("apply");

    assert_eq!(
        g1.nodes.get(&a).unwrap().pos,
        CanvasPoint { x: 42.0, y: 7.0 }
    );
    assert_eq!(g1.edges.get(&eid).unwrap().from, out_port);
    assert_eq!(g1.edges.get(&eid).unwrap().to, in_port);
}

#[test]
fn changes_to_transaction_remove_node_captures_ports_and_edges() {
    let (g0, a, b, out_port, in_port, eid) = make_graph();

    let changes = NodeGraphChanges {
        nodes: vec![NodeChange::Remove { id: a }],
        edges: Vec::new(),
    };

    let tx = changes.to_transaction(&g0).expect("tx");
    assert_eq!(tx.ops.len(), 1);
    match &tx.ops[0] {
        GraphOp::RemoveNode {
            id, ports, edges, ..
        } => {
            assert_eq!(*id, a);
            assert_eq!(
                ports.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
                vec![out_port]
            );
            assert_eq!(
                edges.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
                vec![eid]
            );
        }
        other => panic!("expected remove node op, got {other:?}"),
    }

    let mut g1 = g0.clone();
    tx.apply_to(&mut g1).expect("apply");
    assert!(!g1.nodes.contains_key(&a));
    assert!(g1.nodes.contains_key(&b));
    assert!(!g1.ports.contains_key(&out_port));
    assert!(g1.ports.contains_key(&in_port));
    assert!(!g1.edges.contains_key(&eid));
}

#[test]
fn apply_node_changes_removes_ports_and_incident_edges() {
    let (mut g0, a, b, out_port, in_port, eid) = make_graph();

    let report = apply_node_changes(&mut g0, &[NodeChange::Remove { id: a }]);
    assert!(report.did_change());
    assert_eq!(report.ignored, 0);

    assert!(!g0.nodes.contains_key(&a));
    assert!(g0.nodes.contains_key(&b));

    assert!(!g0.ports.contains_key(&out_port));
    assert!(g0.ports.contains_key(&in_port));

    assert!(!g0.edges.contains_key(&eid));
}

#[test]
fn apply_edge_changes_updates_kind_and_ignores_missing() {
    let (mut g0, _a, _b, _out_port, _in_port, eid) = make_graph();
    let missing = EdgeId::new();

    let report = apply_edge_changes(
        &mut g0,
        &[
            EdgeChange::Kind {
                id: eid,
                kind: EdgeKind::Exec,
            },
            EdgeChange::Remove { id: missing },
        ],
    );
    assert!(report.did_change());
    assert_eq!(report.ignored, 1);
    assert_eq!(g0.edges.get(&eid).unwrap().kind, EdgeKind::Exec);
}

#[test]
fn connection_changes_from_transaction_maps_edge_ops() {
    let (_g0, _a, _b, out_port, in_port, eid) = make_graph();

    let tx = GraphTransaction {
        label: None,
        ops: vec![
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
        ],
    };

    let changes = connection_changes_from_transaction(&tx);
    assert_eq!(changes.len(), 3);
    assert!(matches!(changes[0], ConnectionChange::Connected(_)));
    assert!(matches!(changes[1], ConnectionChange::Reconnected { .. }));
    assert!(matches!(changes[2], ConnectionChange::Disconnected(_)));
}

#[test]
fn install_callbacks_receives_graph_and_view_events() {
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Clone)]
    struct Recorder {
        log: Rc<RefCell<Vec<&'static str>>>,
    }

    impl NodeGraphCommitCallbacks for Recorder {
        fn on_graph_commit(&mut self, _patch: &NodeGraphPatch) {
            self.log.borrow_mut().push("commit");
        }

        fn on_nodes_change(&mut self, _changes: &[NodeChange]) {
            self.log.borrow_mut().push("nodes");
        }

        fn on_edges_change(&mut self, _changes: &[EdgeChange]) {
            self.log.borrow_mut().push("edges");
        }

        fn on_connection_change(&mut self, _change: ConnectionChange) {
            self.log.borrow_mut().push("conn");
        }
    }

    impl NodeGraphViewCallbacks for Recorder {
        fn on_view_change(&mut self, _changes: &[crate::runtime::events::ViewChange]) {
            self.log.borrow_mut().push("view");
        }
    }

    impl NodeGraphGestureCallbacks for Recorder {}

    let (g0, a, _b, _out_port, _in_port, _eid) = make_graph();
    let mut store = NodeGraphStore::new(g0, NodeGraphViewState::default(), default_editor_config());

    let log: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
    let recorder = Recorder { log: log.clone() };
    let _token = install_callbacks(&mut store, recorder);

    let tx = GraphTransaction {
        label: None,
        ops: vec![GraphOp::SetNodePos {
            id: a,
            from: CanvasPoint { x: 0.0, y: 0.0 },
            to: CanvasPoint { x: 1.0, y: 2.0 },
        }],
    };
    let _ = store.dispatch_transaction(&tx).expect("dispatch");

    store.update_view_state(|s| {
        s.pan = CanvasPoint { x: 10.0, y: 20.0 };
    });

    let got = log.borrow().clone();
    assert!(got.contains(&"commit"));
    assert!(got.contains(&"nodes"));
    assert!(got.contains(&"view"));
}

#[test]
fn install_callbacks_receives_full_patch_for_port_only_commits() {
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Clone)]
    struct Recorder {
        saw_port_patch: Rc<RefCell<bool>>,
        node_edge_counts: Rc<RefCell<Vec<(usize, usize)>>>,
    }

    impl NodeGraphCommitCallbacks for Recorder {
        fn on_graph_commit(&mut self, patch: &NodeGraphPatch) {
            *self.saw_port_patch.borrow_mut() = patch
                .ops()
                .iter()
                .any(|op| matches!(op, GraphOp::SetPortData { .. }));
        }

        fn on_node_edge_changes(&mut self, changes: &NodeGraphChanges) {
            self.node_edge_counts
                .borrow_mut()
                .push((changes.nodes.len(), changes.edges.len()));
        }
    }

    impl NodeGraphViewCallbacks for Recorder {}
    impl NodeGraphGestureCallbacks for Recorder {}

    let (g0, _a, _b, out_port, _in_port, _eid) = make_graph();
    let mut store = NodeGraphStore::new(g0, NodeGraphViewState::default(), default_editor_config());

    let saw_port_patch = Rc::new(RefCell::new(false));
    let node_edge_counts = Rc::new(RefCell::new(Vec::new()));
    let recorder = Recorder {
        saw_port_patch: saw_port_patch.clone(),
        node_edge_counts: node_edge_counts.clone(),
    };
    let _token = install_callbacks(&mut store, recorder);

    let tx = GraphTransaction {
        label: Some("Port Data".into()),
        ops: vec![GraphOp::SetPortData {
            id: out_port,
            from: serde_json::Value::Null,
            to: serde_json::json!({ "unit": "kg" }),
        }],
    };
    let outcome = store.dispatch_transaction(&tx).expect("dispatch");

    assert!(matches!(
        outcome.patch.ops().first(),
        Some(GraphOp::SetPortData { id, .. }) if *id == out_port
    ));
    let node_edge_changes = NodeGraphChanges::from_patch(&outcome.patch);
    assert!(node_edge_changes.nodes.is_empty());
    assert!(node_edge_changes.edges.is_empty());
    assert!(*saw_port_patch.borrow());
    assert_eq!(&*node_edge_counts.borrow(), &[(0, 0)]);
}

#[test]
fn controlled_graph_can_apply_store_changes_via_callbacks() {
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Clone)]
    struct ControlledApply {
        graph: Rc<RefCell<Graph>>,
    }

    impl NodeGraphCommitCallbacks for ControlledApply {
        fn on_nodes_change(&mut self, changes: &[NodeChange]) {
            apply_node_changes(&mut self.graph.borrow_mut(), changes);
        }

        fn on_edges_change(&mut self, changes: &[EdgeChange]) {
            apply_edge_changes(&mut self.graph.borrow_mut(), changes);
        }
    }

    impl NodeGraphViewCallbacks for ControlledApply {}

    impl NodeGraphGestureCallbacks for ControlledApply {}

    let (g0, a, _b, _out_port, _in_port, eid) = make_graph();
    let mut store = NodeGraphStore::new(
        g0.clone(),
        NodeGraphViewState::default(),
        default_editor_config(),
    );

    let controlled = Rc::new(RefCell::new(g0));
    let _token = install_callbacks(
        &mut store,
        ControlledApply {
            graph: controlled.clone(),
        },
    );

    let tx = GraphTransaction {
        label: None,
        ops: vec![
            GraphOp::SetNodePos {
                id: a,
                from: CanvasPoint { x: 0.0, y: 0.0 },
                to: CanvasPoint { x: 123.0, y: 456.0 },
            },
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
        ],
    };
    let _ = store.dispatch_transaction(&tx).expect("dispatch");

    let store_json = serde_json::to_value(store.graph()).expect("store json");
    let controlled_json = serde_json::to_value(&*controlled.borrow()).expect("controlled json");
    assert_eq!(store_json, controlled_json);
}

#[test]
fn install_callbacks_calls_viewport_selection_and_connection_hooks() {
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::runtime::xyflow::callbacks::SelectionChange;
    use jellyflow_core::core::{GroupId, PortCapacity, PortDirection, PortKind};
    use jellyflow_core::ops::EdgeEndpoints;

    #[derive(Clone)]
    struct Recorder {
        log: Rc<RefCell<Vec<&'static str>>>,
    }

    impl NodeGraphCommitCallbacks for Recorder {
        fn on_connect(&mut self, _conn: crate::runtime::xyflow::callbacks::EdgeConnection) {
            self.log.borrow_mut().push("connect");
        }

        fn on_disconnect(&mut self, _conn: crate::runtime::xyflow::callbacks::EdgeConnection) {
            self.log.borrow_mut().push("disconnect");
        }

        fn on_reconnect(&mut self, _edge: EdgeId, _from: EdgeEndpoints, _to: EdgeEndpoints) {
            self.log.borrow_mut().push("reconnect");
        }

        fn on_edge_update(&mut self, _edge: EdgeId, _from: EdgeEndpoints, _to: EdgeEndpoints) {
            self.log.borrow_mut().push("edge_update");
        }
    }

    impl NodeGraphViewCallbacks for Recorder {
        fn on_viewport_change(&mut self, _pan: CanvasPoint, _zoom: f32) {
            self.log.borrow_mut().push("viewport");
        }

        fn on_move(&mut self, _pan: CanvasPoint, _zoom: f32) {
            self.log.borrow_mut().push("move");
        }

        fn on_selection_change(&mut self, _sel: SelectionChange) {
            self.log.borrow_mut().push("selection");
        }
    }

    impl NodeGraphGestureCallbacks for Recorder {}

    let (mut g0, a, _b, out_port, in_port, eid) = make_graph();

    let in2 = jellyflow_core::core::PortId::new();
    let c = NodeId::new();
    g0.nodes.insert(
        c,
        Node {
            kind: NodeKindKey::new("demo.c"),
            kind_version: 1,
            pos: CanvasPoint { x: 200.0, y: 0.0 },
            selectable: None,
            draggable: None,
            connectable: None,
            deletable: None,
            parent: None,
            extent: None,
            expand_parent: None,
            size: None,
            hidden: false,
            collapsed: false,
            ports: vec![in2],
            data: serde_json::Value::Null,
        },
    );
    g0.ports.insert(
        in2,
        Port {
            node: c,
            key: jellyflow_core::core::PortKey::new("in2"),
            dir: PortDirection::In,
            kind: PortKind::Data,
            capacity: PortCapacity::Single,
            connectable: None,
            connectable_start: None,
            connectable_end: None,
            ty: None,
            data: serde_json::Value::Null,
        },
    );

    let mut store = NodeGraphStore::new(g0, NodeGraphViewState::default(), default_editor_config());

    let log: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
    let recorder = Recorder { log: log.clone() };
    let _token = install_callbacks(&mut store, recorder);

    store.set_viewport(CanvasPoint { x: 10.0, y: 20.0 }, 1.25);
    store.set_selection(vec![a], vec![eid], vec![GroupId::new()]);

    let e2 = EdgeId::new();
    let tx_add = GraphTransaction {
        label: None,
        ops: vec![GraphOp::AddEdge {
            id: e2,
            edge: Edge {
                kind: EdgeKind::Data,
                from: out_port,
                to: in_port,
                selectable: None,
                deletable: None,
                reconnectable: None,
            },
        }],
    };
    let _ = store.dispatch_transaction(&tx_add).expect("dispatch add");

    let tx_reconnect = GraphTransaction {
        label: None,
        ops: vec![GraphOp::SetEdgeEndpoints {
            id: e2,
            from: EdgeEndpoints {
                from: out_port,
                to: in_port,
            },
            to: EdgeEndpoints {
                from: out_port,
                to: in2,
            },
        }],
    };
    let _ = store
        .dispatch_transaction(&tx_reconnect)
        .expect("dispatch reconnect");

    let tx_remove = GraphTransaction {
        label: None,
        ops: vec![GraphOp::RemoveEdge {
            id: e2,
            edge: Edge {
                kind: EdgeKind::Data,
                from: out_port,
                to: in2,
                selectable: None,
                deletable: None,
                reconnectable: None,
            },
        }],
    };
    let _ = store
        .dispatch_transaction(&tx_remove)
        .expect("dispatch remove");

    let got = log.borrow().clone();
    assert!(got.contains(&"viewport"));
    assert!(got.contains(&"move"));
    assert!(got.contains(&"selection"));
    assert!(got.contains(&"connect"));
    assert!(got.contains(&"reconnect"));
    assert!(got.contains(&"edge_update"));
    assert!(got.contains(&"disconnect"));
}

#[test]
fn install_callbacks_calls_delete_hooks_for_remove_node() {
    use std::cell::RefCell;
    use std::rc::Rc;

    use jellyflow_core::ops::GraphOpBuilderExt;

    #[derive(Clone)]
    struct Recorder {
        nodes_deleted: Rc<RefCell<Vec<NodeId>>>,
        edges_deleted: Rc<RefCell<Vec<EdgeId>>>,
        disconnected: Rc<RefCell<Vec<EdgeId>>>,
    }

    impl NodeGraphCommitCallbacks for Recorder {
        fn on_nodes_delete(&mut self, nodes: &[NodeId]) {
            self.nodes_deleted.borrow_mut().extend_from_slice(nodes);
        }

        fn on_edges_delete(&mut self, edges: &[EdgeId]) {
            self.edges_deleted.borrow_mut().extend_from_slice(edges);
        }

        fn on_disconnect(&mut self, conn: crate::runtime::xyflow::callbacks::EdgeConnection) {
            self.disconnected.borrow_mut().push(conn.edge);
        }
    }

    impl NodeGraphViewCallbacks for Recorder {}

    impl NodeGraphGestureCallbacks for Recorder {}

    let (g0, a, _b, _out_port, _in_port, eid) = make_graph();
    let mut store = NodeGraphStore::new(g0, NodeGraphViewState::default(), default_editor_config());

    let nodes_deleted: Rc<RefCell<Vec<NodeId>>> = Rc::new(RefCell::new(Vec::new()));
    let edges_deleted: Rc<RefCell<Vec<EdgeId>>> = Rc::new(RefCell::new(Vec::new()));
    let disconnected: Rc<RefCell<Vec<EdgeId>>> = Rc::new(RefCell::new(Vec::new()));

    let _token = install_callbacks(
        &mut store,
        Recorder {
            nodes_deleted: nodes_deleted.clone(),
            edges_deleted: edges_deleted.clone(),
            disconnected: disconnected.clone(),
        },
    );

    let op = store
        .graph()
        .build_remove_node_op(a)
        .expect("remove node op");
    let tx = GraphTransaction {
        label: None,
        ops: vec![op],
    };
    let _ = store.dispatch_transaction(&tx).expect("dispatch remove");

    assert!(nodes_deleted.borrow().contains(&a));
    assert!(edges_deleted.borrow().contains(&eid));
    assert!(disconnected.borrow().contains(&eid));
}
