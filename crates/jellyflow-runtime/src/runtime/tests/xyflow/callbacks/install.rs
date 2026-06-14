use super::super::super::fixtures::{
    fixture_insert_node, fixture_insert_port, make_graph, make_store,
};

use crate::runtime::commit::NodeGraphPatch;
use crate::runtime::xyflow::callbacks::{
    ConnectionChange, EdgeConnection, NodeGraphCommitCallbacks, NodeGraphGestureCallbacks,
    NodeGraphViewCallbacks, SelectionChange, install_callbacks,
};
use crate::runtime::xyflow::changes::{EdgeChange, NodeChange};
use jellyflow_core::core::{
    CanvasPoint, Edge, EdgeId, EdgeKind, GroupId, Node, NodeId, NodeKindKey, Port, PortCapacity,
    PortDirection, PortId, PortKind,
};
use jellyflow_core::ops::{EdgeEndpoints, GraphOp, GraphTransaction};

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
    let mut store = make_store(g0);

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
    }

    impl NodeGraphViewCallbacks for Recorder {
        fn on_viewport_change(&mut self, _pan: CanvasPoint, _zoom: f32) {
            self.log.borrow_mut().push("viewport");
        }

        fn on_selection_change(&mut self, _sel: SelectionChange) {
            self.log.borrow_mut().push("selection");
        }
    }

    impl NodeGraphGestureCallbacks for Recorder {}

    let (mut g0, a, _b, out_port, in_port, eid) = make_graph();

    let in2 = PortId::new();
    let c = NodeId::new();
    fixture_insert_node(
        &mut g0,
        c,
        Node {
            kind: NodeKindKey::new("demo.c"),
            kind_version: 1,
            pos: CanvasPoint { x: 200.0, y: 0.0 },
            origin: None,
            selectable: None,
            focusable: None,
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
    fixture_insert_port(
        &mut g0,
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

    let mut store = make_store(g0);

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
            hidden: false,
            selectable: None,
            focusable: None,
            interaction_width: None,
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
            hidden: false,
            selectable: None,
            focusable: None,
            interaction_width: None,
            deletable: None,
            reconnectable: None,
        },
        bindings: Vec::new(),
    }]);
    let _ = store
        .dispatch_transaction(&tx_remove)
        .expect("dispatch remove");

    let got = log.borrow().clone();
    assert!(got.contains(&"viewport"));
    assert!(got.contains(&"selection"));
    assert!(got.contains(&"connect"));
    assert!(got.contains(&"reconnect"));
    assert!(got.contains(&"disconnect"));
}
