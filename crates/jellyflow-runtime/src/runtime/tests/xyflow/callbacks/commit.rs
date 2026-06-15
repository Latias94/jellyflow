use super::super::super::fixtures::{
    fixture_insert_group, fixture_insert_sticky_note, make_graph, make_store,
};

use crate::runtime::commit::NodeGraphPatch;
use crate::runtime::xyflow::ControlledGraph;
use crate::runtime::xyflow::callbacks::{
    DeleteChange, EdgeConnection, NodeGraphCommitCallbacks, NodeGraphGestureCallbacks,
    NodeGraphViewCallbacks, install_callbacks,
};
use crate::runtime::xyflow::changes::{EdgeChange, NodeChange, NodeGraphChanges};
use jellyflow_core::core::{
    CanvasPoint, CanvasRect, CanvasSize, EdgeId, EdgeReconnectable, Group, GroupId, NodeId,
    StickyNote, StickyNoteId,
};
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
        outcome.patch().ops().first(),
        Some(GraphOp::SetPortData { id, .. }) if *id == out_port
    ));
    let node_edge_changes = NodeGraphChanges::from_patch(outcome.patch());
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
        graph: Rc<RefCell<ControlledGraph>>,
    }

    impl NodeGraphCommitCallbacks for ControlledApply {
        fn on_nodes_change(&mut self, changes: &[NodeChange]) {
            self.graph.borrow_mut().apply_node_changes(changes);
        }

        fn on_edges_change(&mut self, changes: &[EdgeChange]) {
            self.graph.borrow_mut().apply_edge_changes(changes);
        }
    }

    impl NodeGraphViewCallbacks for ControlledApply {}

    impl NodeGraphGestureCallbacks for ControlledApply {}

    let (g0, a, _b, _out_port, _in_port, eid) = make_graph();
    let mut store = make_store(g0.clone());

    let controlled = Rc::new(RefCell::new(ControlledGraph::new(g0)));
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
    let controlled_json =
        serde_json::to_value(controlled.borrow().graph()).expect("controlled json");
    assert_eq!(store_json, controlled_json);
}

#[test]
fn install_callbacks_preserves_commit_callback_order_for_controlled_updates() {
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Clone)]
    struct Recorder {
        order: Rc<RefCell<Vec<&'static str>>>,
    }

    impl NodeGraphCommitCallbacks for Recorder {
        fn on_graph_commit(&mut self, _patch: &NodeGraphPatch) {
            self.order.borrow_mut().push("graph_commit");
        }

        fn on_node_edge_changes(&mut self, _changes: &NodeGraphChanges) {
            self.order.borrow_mut().push("node_edge_changes");
        }

        fn on_nodes_change(&mut self, _changes: &[NodeChange]) {
            self.order.borrow_mut().push("nodes_change");
        }

        fn on_edges_change(&mut self, _changes: &[EdgeChange]) {
            self.order.borrow_mut().push("edges_change");
        }
    }

    impl NodeGraphViewCallbacks for Recorder {}
    impl NodeGraphGestureCallbacks for Recorder {}

    let (g0, a, _b, _out_port, _in_port, eid) = make_graph();
    let mut store = make_store(g0);
    let order = Rc::new(RefCell::new(Vec::new()));
    let _token = install_callbacks(
        &mut store,
        Recorder {
            order: order.clone(),
        },
    );

    let tx = GraphTransaction::from_ops([
        GraphOp::SetNodePos {
            id: a,
            from: CanvasPoint { x: 0.0, y: 0.0 },
            to: CanvasPoint { x: 123.0, y: 456.0 },
        },
        GraphOp::SetEdgeReconnectable {
            id: eid,
            from: None,
            to: Some(EdgeReconnectable::Bool(false)),
        },
    ]);
    let _ = store.dispatch_transaction(&tx).expect("dispatch");

    assert_eq!(
        order.borrow().as_slice(),
        &[
            "graph_commit",
            "node_edge_changes",
            "nodes_change",
            "edges_change",
        ],
    );
}

#[test]
fn install_callbacks_calls_delete_hooks_for_removed_resources() {
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Clone)]
    struct Recorder {
        nodes_deleted: Rc<RefCell<Vec<NodeId>>>,
        edges_deleted: Rc<RefCell<Vec<EdgeId>>>,
        groups_deleted: Rc<RefCell<Vec<GroupId>>>,
        sticky_notes_deleted: Rc<RefCell<Vec<StickyNoteId>>>,
        deleted: Rc<RefCell<Vec<DeleteChange>>>,
        disconnected: Rc<RefCell<Vec<EdgeId>>>,
    }

    impl NodeGraphCommitCallbacks for Recorder {
        fn on_nodes_delete(&mut self, nodes: &[NodeId]) {
            self.nodes_deleted.borrow_mut().extend_from_slice(nodes);
        }

        fn on_edges_delete(&mut self, edges: &[EdgeId]) {
            self.edges_deleted.borrow_mut().extend_from_slice(edges);
        }

        fn on_groups_delete(&mut self, groups: &[GroupId]) {
            self.groups_deleted.borrow_mut().extend_from_slice(groups);
        }

        fn on_sticky_notes_delete(&mut self, notes: &[StickyNoteId]) {
            self.sticky_notes_deleted
                .borrow_mut()
                .extend_from_slice(notes);
        }

        fn on_delete(&mut self, change: DeleteChange) {
            self.deleted.borrow_mut().push(change);
        }

        fn on_disconnect(&mut self, conn: EdgeConnection) {
            self.disconnected.borrow_mut().push(conn.edge);
        }
    }

    impl NodeGraphViewCallbacks for Recorder {}

    impl NodeGraphGestureCallbacks for Recorder {}

    let (mut g0, a, _b, _out_port, _in_port, eid) = make_graph();
    let group_id = GroupId::new();
    let sticky_note_id = StickyNoteId::new();
    let rect = CanvasRect {
        origin: CanvasPoint { x: 0.0, y: 0.0 },
        size: CanvasSize {
            width: 10.0,
            height: 10.0,
        },
    };
    let group = Group {
        title: "delete group".to_owned(),
        rect,
        color: None,
    };
    let sticky_note = StickyNote {
        text: "delete note".to_owned(),
        rect,
        color: None,
    };
    fixture_insert_group(&mut g0, group_id, group.clone());
    fixture_insert_sticky_note(&mut g0, sticky_note_id, sticky_note.clone());
    let mut store = make_store(g0);

    let nodes_deleted: Rc<RefCell<Vec<NodeId>>> = Rc::new(RefCell::new(Vec::new()));
    let edges_deleted: Rc<RefCell<Vec<EdgeId>>> = Rc::new(RefCell::new(Vec::new()));
    let groups_deleted: Rc<RefCell<Vec<GroupId>>> = Rc::new(RefCell::new(Vec::new()));
    let sticky_notes_deleted: Rc<RefCell<Vec<StickyNoteId>>> = Rc::new(RefCell::new(Vec::new()));
    let deleted: Rc<RefCell<Vec<DeleteChange>>> = Rc::new(RefCell::new(Vec::new()));
    let disconnected: Rc<RefCell<Vec<EdgeId>>> = Rc::new(RefCell::new(Vec::new()));

    let _token = install_callbacks(
        &mut store,
        Recorder {
            nodes_deleted: nodes_deleted.clone(),
            edges_deleted: edges_deleted.clone(),
            groups_deleted: groups_deleted.clone(),
            sticky_notes_deleted: sticky_notes_deleted.clone(),
            deleted: deleted.clone(),
            disconnected: disconnected.clone(),
        },
    );

    let op = store
        .graph()
        .build_remove_node_op(a)
        .expect("remove node op");
    let tx = GraphTransaction::from_ops([
        op,
        GraphOp::RemoveGroup {
            id: group_id,
            group,
            detached: Vec::new(),
            bindings: Vec::new(),
        },
        GraphOp::RemoveStickyNote {
            id: sticky_note_id,
            note: sticky_note,
            bindings: Vec::new(),
        },
    ]);
    let _ = store.dispatch_transaction(&tx).expect("dispatch remove");

    assert_eq!(nodes_deleted.borrow().as_slice(), &[a]);
    assert_eq!(edges_deleted.borrow().as_slice(), &[eid]);
    assert_eq!(groups_deleted.borrow().as_slice(), &[group_id]);
    assert_eq!(sticky_notes_deleted.borrow().as_slice(), &[sticky_note_id]);
    assert_eq!(
        deleted.borrow().as_slice(),
        &[DeleteChange::from_parts(
            vec![a],
            vec![eid],
            vec![group_id],
            vec![sticky_note_id],
        )],
    );
    assert_eq!(disconnected.borrow().as_slice(), &[eid]);
}
