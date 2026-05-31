use super::super::fixtures::{default_editor_config, make_graph};

use crate::io::NodeGraphViewState;
use crate::runtime::commit::NodeGraphPatch;
use crate::runtime::store::NodeGraphStore;
use crate::runtime::xyflow::apply::{apply_edge_changes, apply_node_changes};
use crate::runtime::xyflow::callbacks::{
    ConnectionChange, DeleteChange, EdgeConnection, NodeGraphCommitCallbacks,
    NodeGraphGestureCallbacks, NodeGraphViewCallbacks, SelectionChange, install_callbacks,
};
use crate::runtime::xyflow::changes::{EdgeChange, NodeChange, NodeGraphChanges};
use jellyflow_core::core::{
    CanvasPoint, Edge, EdgeId, EdgeKind, EdgeReconnectable, Graph, GroupId, Node, NodeId,
    NodeKindKey, Port, PortCapacity, PortDirection, PortId, PortKind, StickyNoteId,
};
use jellyflow_core::ops::{EdgeEndpoints, GraphOp, GraphOpBuilderExt, GraphTransaction};

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

    let tx = GraphTransaction::from_ops([GraphOp::SetNodePos {
        id: a,
        from: CanvasPoint { x: 0.0, y: 0.0 },
        to: CanvasPoint { x: 1.0, y: 2.0 },
    }]);
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
                .push((changes.nodes().len(), changes.edges().len()));
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

    let tx = GraphTransaction::from_ops([GraphOp::SetPortData {
        id: out_port,
        from: serde_json::Value::Null,
        to: serde_json::json!({ "unit": "kg" }),
    }])
    .with_label("Port Data");
    let outcome = store.dispatch_transaction(&tx).expect("dispatch");

    assert!(matches!(
        outcome.patch.ops().first(),
        Some(GraphOp::SetPortData { id, .. }) if *id == out_port
    ));
    let node_edge_changes = NodeGraphChanges::from_patch(&outcome.patch);
    assert!(node_edge_changes.nodes().is_empty());
    assert!(node_edge_changes.edges().is_empty());
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

    let tx = GraphTransaction::from_ops([
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
    ]);
    let _ = store.dispatch_transaction(&tx).expect("dispatch");

    let store_json = serde_json::to_value(store.graph()).expect("store json");
    let controlled_json = serde_json::to_value(&*controlled.borrow()).expect("controlled json");
    assert_eq!(store_json, controlled_json);
}

#[test]
fn delete_change_facade_consumes_parts() {
    let node = NodeId::new();
    let edge = EdgeId::new();
    let group = GroupId::new();
    let sticky_note = StickyNoteId::new();
    let change = DeleteChange::from_parts(vec![node], vec![edge], vec![group], vec![sticky_note]);

    let (nodes, edges, groups, sticky_notes) = change.into_parts();

    assert_eq!(nodes, vec![node]);
    assert_eq!(edges, vec![edge]);
    assert_eq!(groups, vec![group]);
    assert_eq!(sticky_notes, vec![sticky_note]);
}

#[test]
fn selection_change_facade_exposes_and_consumes_parts() {
    let node = NodeId::new();
    let edge = EdgeId::new();
    let group = GroupId::new();
    let change = SelectionChange::new(vec![node], vec![edge], vec![group]);

    assert!(!change.is_empty());
    assert_eq!(change.nodes(), &[node]);
    assert_eq!(change.edges(), &[edge]);
    assert_eq!(change.groups(), &[group]);

    let (nodes, edges, groups) = change.into_parts();

    assert_eq!(nodes, vec![node]);
    assert_eq!(edges, vec![edge]);
    assert_eq!(groups, vec![group]);
}

#[test]
fn install_callbacks_calls_viewport_selection_and_connection_hooks() {
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Clone)]
    struct Recorder {
        log: Rc<RefCell<Vec<&'static str>>>,
    }

    impl NodeGraphCommitCallbacks for Recorder {
        fn on_connect(&mut self, _conn: EdgeConnection) {
            self.log.borrow_mut().push("connect");
        }

        fn on_disconnect(&mut self, _conn: EdgeConnection) {
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

    let in2 = PortId::new();
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
    let tx_add = GraphTransaction::from_ops([GraphOp::AddEdge {
        id: e2,
        edge: Edge {
            kind: EdgeKind::Data,
            from: out_port,
            to: in_port,
            selectable: None,
            deletable: None,
            reconnectable: None,
        },
    }]);
    let _ = store.dispatch_transaction(&tx_add).expect("dispatch add");

    let tx_reconnect = GraphTransaction::from_ops([GraphOp::SetEdgeEndpoints {
        id: e2,
        from: EdgeEndpoints {
            from: out_port,
            to: in_port,
        },
        to: EdgeEndpoints {
            from: out_port,
            to: in2,
        },
    }]);
    let _ = store
        .dispatch_transaction(&tx_reconnect)
        .expect("dispatch reconnect");

    let tx_remove = GraphTransaction::from_ops([GraphOp::RemoveEdge {
        id: e2,
        edge: Edge {
            kind: EdgeKind::Data,
            from: out_port,
            to: in2,
            selectable: None,
            deletable: None,
            reconnectable: None,
        },
    }]);
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

        fn on_disconnect(&mut self, conn: EdgeConnection) {
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
    let tx = GraphTransaction::from_ops([op]);
    let _ = store.dispatch_transaction(&tx).expect("dispatch remove");

    assert!(nodes_deleted.borrow().contains(&a));
    assert!(edges_deleted.borrow().contains(&eid));
    assert!(disconnected.borrow().contains(&eid));
}
