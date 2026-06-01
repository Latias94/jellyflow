use super::super::super::fixtures::{make_graph, make_store};

use crate::runtime::commit::NodeGraphPatch;
use crate::runtime::xyflow::apply::{apply_edge_changes, apply_node_changes};
use crate::runtime::xyflow::callbacks::{
    EdgeConnection, NodeGraphCommitCallbacks, NodeGraphGestureCallbacks, NodeGraphViewCallbacks,
    install_callbacks,
};
use crate::runtime::xyflow::changes::{EdgeChange, NodeChange, NodeGraphChanges};
use jellyflow_core::core::{CanvasPoint, EdgeId, EdgeReconnectable, Graph, NodeId};
use jellyflow_core::ops::{GraphOp, GraphOpBuilderExt, GraphTransaction};

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
    let mut store = make_store(g0);

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
    let mut store = make_store(g0.clone());

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
    let mut store = make_store(g0);

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
