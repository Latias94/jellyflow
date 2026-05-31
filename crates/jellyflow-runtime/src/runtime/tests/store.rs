use super::*;

#[test]
fn store_lookups_update_after_dispatch_transaction() {
    let (mut g, _a, _b, out_port, in_port, eid) = make_graph();
    g.edges.clear();

    let mut store = NodeGraphStore::new(g, NodeGraphViewState::default(), default_editor_config());
    assert!(store.lookups().edge_lookup.is_empty());

    let tx = GraphTransaction {
        label: None,
        ops: vec![GraphOp::AddEdge {
            id: eid,
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

    store.dispatch_transaction(&tx).expect("dispatch");
    assert!(store.lookups().edge_lookup.contains_key(&eid));
}

#[test]
fn store_lookups_update_node_hidden_after_dispatch_transaction() {
    let (g, a, _b, _out_port, _in_port, _eid) = make_graph();
    let mut store = NodeGraphStore::new(g, NodeGraphViewState::default(), default_editor_config());
    assert!(!store.lookups().node_lookup.get(&a).unwrap().hidden);

    let tx = GraphTransaction {
        label: None,
        ops: vec![GraphOp::SetNodeHidden {
            id: a,
            from: false,
            to: true,
        }],
    };

    store.dispatch_transaction(&tx).expect("dispatch");
    assert!(store.lookups().node_lookup.get(&a).unwrap().hidden);
}

#[test]
fn store_lookups_update_edge_reconnectable_after_dispatch_transaction() {
    let (g, _a, _b, _out_port, _in_port, eid) = make_graph();
    let mut store = NodeGraphStore::new(g, NodeGraphViewState::default(), default_editor_config());
    assert_eq!(
        store.lookups().edge_lookup.get(&eid).unwrap().reconnectable,
        None
    );

    let tx = GraphTransaction {
        label: None,
        ops: vec![GraphOp::SetEdgeReconnectable {
            id: eid,
            from: None,
            to: Some(EdgeReconnectable::Bool(false)),
        }],
    };

    store.dispatch_transaction(&tx).expect("dispatch");
    assert_eq!(
        store.lookups().edge_lookup.get(&eid).unwrap().reconnectable,
        Some(EdgeReconnectable::Bool(false))
    );
}

#[test]
fn store_lookups_update_edge_kind_in_connection_lookup_after_dispatch_transaction() {
    let (mut g, a, b, out_port, in_port, eid) = make_graph();
    g.ports.get_mut(&out_port).unwrap().kind = PortKind::Exec;
    g.ports.get_mut(&in_port).unwrap().kind = PortKind::Exec;
    let mut store = NodeGraphStore::new(g, NodeGraphViewState::default(), default_editor_config());

    let tx = GraphTransaction {
        label: None,
        ops: vec![GraphOp::SetEdgeKind {
            id: eid,
            from: EdgeKind::Data,
            to: EdgeKind::Exec,
        }],
    };

    store.dispatch_transaction(&tx).expect("dispatch");

    assert_eq!(
        store.lookups().edge_lookup.get(&eid).unwrap().kind,
        EdgeKind::Exec
    );
    assert_eq!(
        store
            .lookups()
            .connections_for_port(a, ConnectionSide::Source, out_port)
            .expect("source connections")
            .get(&eid)
            .unwrap()
            .kind,
        EdgeKind::Exec
    );
    assert_eq!(
        store
            .lookups()
            .connections_for_port(b, ConnectionSide::Target, in_port)
            .expect("target connections")
            .get(&eid)
            .unwrap()
            .kind,
        EdgeKind::Exec
    );
}

#[test]
fn store_lookups_remove_port_updates_node_ports_and_incident_edges() {
    let (g, a, _b, out_port, _in_port, eid) = make_graph();
    let port = g.ports.get(&out_port).expect("port").clone();
    let edge = g.edges.get(&eid).expect("edge").clone();
    let mut store = NodeGraphStore::new(g, NodeGraphViewState::default(), default_editor_config());

    assert!(
        store
            .lookups()
            .node_lookup
            .get(&a)
            .unwrap()
            .ports
            .contains(&out_port)
    );
    assert!(store.lookups().edge_lookup.contains_key(&eid));

    let tx = GraphTransaction {
        label: None,
        ops: vec![GraphOp::RemovePort {
            id: out_port,
            port,
            edges: vec![(eid, edge)],
        }],
    };

    store.dispatch_transaction(&tx).expect("dispatch");
    assert!(
        !store
            .lookups()
            .node_lookup
            .get(&a)
            .unwrap()
            .ports
            .contains(&out_port)
    );
    assert!(!store.lookups().edge_lookup.contains_key(&eid));
}

#[test]
fn store_lookups_remove_group_clears_detached_node_parent() {
    let (mut g, a, _b, _out_port, _in_port, _eid) = make_graph();
    let group_id = GroupId::new();
    let group = Group {
        title: "Group".to_string(),
        rect: CanvasRect {
            origin: CanvasPoint { x: -10.0, y: -10.0 },
            size: CanvasSize {
                width: 200.0,
                height: 100.0,
            },
        },
        color: None,
    };
    g.groups.insert(group_id, group.clone());
    g.nodes.get_mut(&a).expect("node").parent = Some(group_id);

    let mut store = NodeGraphStore::new(g, NodeGraphViewState::default(), default_editor_config());
    assert_eq!(
        store.lookups().node_lookup.get(&a).unwrap().parent,
        Some(group_id)
    );

    let tx = GraphTransaction {
        label: None,
        ops: vec![GraphOp::RemoveGroup {
            id: group_id,
            group,
            detached: vec![(a, Some(group_id))],
        }],
    };

    store.dispatch_transaction(&tx).expect("dispatch");
    assert_eq!(store.lookups().node_lookup.get(&a).unwrap().parent, None);
}

#[test]
fn store_dispatch_changes_records_history_and_supports_undo() {
    let (g0, a, _b, _out_port, _in_port, _eid) = make_graph();
    let mut store = NodeGraphStore::new(g0, NodeGraphViewState::default(), default_editor_config());

    let changes = NodeGraphChanges {
        nodes: vec![NodeChange::Position {
            id: a,
            position: CanvasPoint { x: 12.0, y: 34.0 },
        }],
        edges: Vec::new(),
    };

    let outcome = store.dispatch_changes(&changes).expect("dispatch");
    assert!(!outcome.patch.ops().is_empty());
    assert!(store.can_undo());
    assert_eq!(
        store.graph().nodes.get(&a).unwrap().pos,
        CanvasPoint { x: 12.0, y: 34.0 }
    );

    let undo = store.undo().expect("undo").expect("did undo");
    assert!(!undo.patch.ops().is_empty());
    assert_eq!(
        store.graph().nodes.get(&a).unwrap().pos,
        CanvasPoint { x: 0.0, y: 0.0 }
    );
}

#[test]
fn store_dispatch_pipeline_publishes_coherent_commit_state() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let (g0, a, _b, _out_port, _in_port, eid) = make_graph();
    let mut store = NodeGraphStore::new(g0, NodeGraphViewState::default(), default_editor_config());

    let observed: Rc<RefCell<Option<(bool, Option<EdgeReconnectable>)>>> =
        Rc::new(RefCell::new(None));
    let observed2 = observed.clone();
    store.subscribe(move |ev| {
        if let NodeGraphStoreEvent::GraphCommitted { patch } = ev {
            let node_edge_changes = NodeGraphChanges::from_patch(patch);
            let hidden = node_edge_changes
                .nodes
                .iter()
                .any(|change| matches!(change, NodeChange::Hidden { hidden: true, .. }));
            let reconnectable = node_edge_changes
                .edges
                .iter()
                .find_map(|change| match change {
                    EdgeChange::Reconnectable { reconnectable, .. } => *reconnectable,
                    _ => None,
                });
            *observed2.borrow_mut() = Some((hidden, reconnectable));
        }
    });

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

    let outcome = store.dispatch_transaction(&tx).expect("dispatch");

    assert!(store.graph().nodes.get(&a).unwrap().hidden);
    assert!(store.lookups().node_lookup.get(&a).unwrap().hidden);
    assert_eq!(
        store.lookups().edge_lookup.get(&eid).unwrap().reconnectable,
        Some(EdgeReconnectable::Bool(false))
    );
    assert!(store.can_undo());
    let node_edge_changes = NodeGraphChanges::from_patch(&outcome.patch);
    assert!(
        node_edge_changes
            .nodes
            .iter()
            .any(|change| matches!(change, NodeChange::Hidden { id, hidden: true } if *id == a))
    );
    assert!(node_edge_changes.edges.iter().any(|change| matches!(
        change,
        EdgeChange::Reconnectable {
            id,
            reconnectable: Some(EdgeReconnectable::Bool(false))
        } if *id == eid
    )));
    assert_eq!(
        *observed.borrow(),
        Some((true, Some(EdgeReconnectable::Bool(false))))
    );
}

#[test]
fn store_dispatch_with_external_profile_uses_same_commit_pipeline() {
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::rules::{ConnectPlan, Diagnostic};
    use jellyflow_core::types::TypeDesc;

    #[derive(Default)]
    struct PassProfile;

    impl crate::profile::GraphProfile for PassProfile {
        fn type_of_port(
            &mut self,
            _graph: &Graph,
            _port: jellyflow_core::core::PortId,
        ) -> Option<TypeDesc> {
            None
        }

        fn plan_connect(
            &mut self,
            _graph: &Graph,
            _a: jellyflow_core::core::PortId,
            _b: jellyflow_core::core::PortId,
            _mode: jellyflow_core::interaction::NodeGraphConnectionMode,
        ) -> ConnectPlan {
            ConnectPlan::reject("not used in this test")
        }

        fn validate_graph(&mut self, _graph: &Graph) -> Vec<Diagnostic> {
            Vec::new()
        }
    }

    #[derive(Debug)]
    struct TraceMiddleware {
        trace: Rc<RefCell<Vec<&'static str>>>,
    }

    impl NodeGraphStoreMiddleware for TraceMiddleware {
        fn before_dispatch(
            &mut self,
            _snapshot: crate::runtime::events::NodeGraphStoreSnapshot<'_>,
            tx: &mut GraphTransaction,
        ) -> Result<(), crate::profile::ApplyPipelineError> {
            self.trace.borrow_mut().push("before");
            tx.ops.push(GraphOp::SetNodeHidden {
                id: tx
                    .ops
                    .first()
                    .and_then(node_pos_id)
                    .expect("node position op"),
                from: false,
                to: true,
            });
            Ok(())
        }

        fn after_dispatch(
            &mut self,
            snapshot: crate::runtime::events::NodeGraphStoreSnapshot<'_>,
            patch: &NodeGraphPatch,
        ) {
            self.trace.borrow_mut().push("after");
            assert!(snapshot.history.can_undo());
            assert_eq!(patch.ops().len(), 2);
            let node_edge_changes = NodeGraphChanges::from_patch(patch);
            assert_eq!(node_edge_changes.nodes.len(), 2);
        }
    }

    fn node_pos_id(op: &GraphOp) -> Option<NodeId> {
        match op {
            GraphOp::SetNodePos { id, .. } => Some(*id),
            _ => None,
        }
    }

    let (g0, a, _b, _out_port, _in_port, _eid) = make_graph();
    let trace = Rc::new(RefCell::new(Vec::new()));
    let mut store = NodeGraphStore::new(g0, NodeGraphViewState::default(), default_editor_config())
        .with_middleware(TraceMiddleware {
            trace: trace.clone(),
        });
    let mut profile = PassProfile;

    let observed: Rc<RefCell<Option<(usize, bool)>>> = Rc::new(RefCell::new(None));
    let observed2 = observed.clone();
    store.subscribe(move |ev| {
        if let NodeGraphStoreEvent::GraphCommitted { patch } = ev {
            let node_edge_changes = NodeGraphChanges::from_patch(patch);
            *observed2.borrow_mut() = Some((
                node_edge_changes.nodes.len(),
                node_edge_changes
                    .nodes
                    .iter()
                    .any(|change| matches!(change, NodeChange::Hidden { hidden: true, .. })),
            ));
        }
    });

    let tx = GraphTransaction {
        label: Some("external profile commit".to_string()),
        ops: vec![GraphOp::SetNodePos {
            id: a,
            from: CanvasPoint { x: 0.0, y: 0.0 },
            to: CanvasPoint { x: 42.0, y: 24.0 },
        }],
    };

    let outcome = store
        .dispatch_transaction_with_profile(&tx, &mut profile)
        .expect("dispatch with external profile");

    assert_eq!(
        store.graph().nodes.get(&a).unwrap().pos,
        CanvasPoint { x: 42.0, y: 24.0 }
    );
    assert!(store.graph().nodes.get(&a).unwrap().hidden);
    assert!(store.can_undo());
    assert_eq!(outcome.patch.ops().len(), 2);
    assert_eq!(&*trace.borrow(), &["before", "after"]);
    assert_eq!(*observed.borrow(), Some((2, true)));
}

#[test]
fn store_does_not_commit_rejected_profile_edits() {
    use crate::rules::{ConnectPlan, Diagnostic, DiagnosticSeverity, DiagnosticTarget};
    use jellyflow_core::types::TypeDesc;

    struct RejectProfile;

    impl crate::profile::GraphProfile for RejectProfile {
        fn type_of_port(
            &mut self,
            _graph: &Graph,
            _port: jellyflow_core::core::PortId,
        ) -> Option<TypeDesc> {
            None
        }

        fn plan_connect(
            &mut self,
            _graph: &Graph,
            _a: jellyflow_core::core::PortId,
            _b: jellyflow_core::core::PortId,
            _mode: jellyflow_core::interaction::NodeGraphConnectionMode,
        ) -> ConnectPlan {
            ConnectPlan::reject("not used in this test")
        }

        fn validate_graph(&mut self, _graph: &Graph) -> Vec<Diagnostic> {
            vec![Diagnostic {
                key: "test.reject".to_string(),
                severity: DiagnosticSeverity::Error,
                target: DiagnosticTarget::Graph,
                message: "rejected by test profile".to_string(),
                fixes: Vec::new(),
            }]
        }
    }

    let (g0, a, _b, _out_port, _in_port, _eid) = make_graph();
    let mut store = NodeGraphStore::with_profile(
        g0.clone(),
        NodeGraphViewState::default(),
        default_editor_config(),
        Box::new(RejectProfile),
    );

    let tx = GraphTransaction {
        label: None,
        ops: vec![GraphOp::SetNodePos {
            id: a,
            from: CanvasPoint { x: 0.0, y: 0.0 },
            to: CanvasPoint { x: 999.0, y: 999.0 },
        }],
    };

    let err = store.dispatch_transaction(&tx).expect_err("reject");
    let crate::runtime::store::DispatchError::Apply(crate::profile::ApplyPipelineError::Rejected {
        diagnostics,
        ..
    }) = err
    else {
        panic!("unexpected error: {err:?}");
    };
    assert!(!diagnostics.is_empty());

    assert_eq!(
        store.graph().nodes.get(&a).unwrap().pos,
        g0.nodes.get(&a).unwrap().pos
    );
    assert!(!store.can_undo());
}

#[test]
fn store_rejects_non_finite_transactions() {
    let g = Graph::new(jellyflow_core::core::GraphId::from_u128(1));
    let node_id = NodeId::new();

    let tx = GraphTransaction {
        label: None,
        ops: vec![GraphOp::AddNode {
            id: node_id,
            node: Node {
                kind: NodeKindKey::new("demo.node"),
                kind_version: 1,
                pos: CanvasPoint {
                    x: f32::NAN,
                    y: 0.0,
                },
                selectable: None,
                draggable: None,
                connectable: None,
                deletable: None,
                parent: None,
                extent: None,
                expand_parent: None,
                size: Some(jellyflow_core::core::CanvasSize {
                    width: 10.0,
                    height: 10.0,
                }),
                hidden: false,
                collapsed: false,
                ports: Vec::new(),
                data: serde_json::Value::Null,
            },
        }],
    };

    let mut store = NodeGraphStore::new(
        g.clone(),
        NodeGraphViewState::default(),
        default_editor_config(),
    );
    let err = store.dispatch_transaction(&tx).expect_err("reject");
    let crate::runtime::store::DispatchError::Apply(crate::profile::ApplyPipelineError::Rejected {
        diagnostics,
        ..
    }) = err
    else {
        panic!("unexpected error: {err:?}");
    };
    assert_eq!(diagnostics[0].key, "tx.non_finite");
    assert!(store.graph().nodes.is_empty());
    assert_eq!(store.graph().graph_id, g.graph_id);
    assert!(!store.can_undo());
}

#[test]
fn store_rejects_invalid_size_transactions() {
    let g = Graph::new(jellyflow_core::core::GraphId::from_u128(1));
    let node_id = NodeId::new();

    let tx = GraphTransaction {
        label: None,
        ops: vec![GraphOp::AddNode {
            id: node_id,
            node: Node {
                kind: NodeKindKey::new("demo.node"),
                kind_version: 1,
                pos: CanvasPoint { x: 0.0, y: 0.0 },
                selectable: None,
                draggable: None,
                connectable: None,
                deletable: None,
                parent: None,
                extent: None,
                expand_parent: None,
                size: Some(jellyflow_core::core::CanvasSize {
                    width: 0.0,
                    height: 10.0,
                }),
                hidden: false,
                collapsed: false,
                ports: Vec::new(),
                data: serde_json::Value::Null,
            },
        }],
    };

    let mut store = NodeGraphStore::new(
        g.clone(),
        NodeGraphViewState::default(),
        default_editor_config(),
    );
    let err = store.dispatch_transaction(&tx).expect_err("reject");
    let crate::runtime::store::DispatchError::Apply(crate::profile::ApplyPipelineError::Rejected {
        diagnostics,
        ..
    }) = err
    else {
        panic!("unexpected error: {err:?}");
    };
    assert_eq!(diagnostics[0].key, "tx.invalid_size");
    assert!(store.graph().nodes.is_empty());
    assert_eq!(store.graph().graph_id, g.graph_id);
    assert!(!store.can_undo());
}

#[test]
fn store_middleware_can_rewrite_transactions() {
    #[derive(Debug, Default)]
    struct DropOps;

    impl NodeGraphStoreMiddleware for DropOps {
        fn before_dispatch(
            &mut self,
            _snapshot: crate::runtime::events::NodeGraphStoreSnapshot<'_>,
            tx: &mut GraphTransaction,
        ) -> Result<(), crate::profile::ApplyPipelineError> {
            tx.ops.clear();
            Ok(())
        }
    }

    let (g0, a, _b, _out_port, _in_port, _eid) = make_graph();
    let mut store = NodeGraphStore::new(g0, NodeGraphViewState::default(), default_editor_config())
        .with_middleware(DropOps);

    let tx = GraphTransaction {
        label: None,
        ops: vec![GraphOp::SetNodePos {
            id: a,
            from: CanvasPoint { x: 0.0, y: 0.0 },
            to: CanvasPoint { x: 10.0, y: 20.0 },
        }],
    };

    let outcome = store.dispatch_transaction(&tx).expect("dispatch");
    assert!(outcome.patch.ops().is_empty());
    assert_eq!(
        store.graph().nodes.get(&a).unwrap().pos,
        CanvasPoint { x: 0.0, y: 0.0 }
    );
    assert!(!store.can_undo());
}

#[test]
fn store_middleware_can_reject_transactions() {
    use crate::rules::{Diagnostic, DiagnosticSeverity, DiagnosticTarget};

    #[derive(Debug, Default)]
    struct RejectAll;

    impl NodeGraphStoreMiddleware for RejectAll {
        fn before_dispatch(
            &mut self,
            _snapshot: crate::runtime::events::NodeGraphStoreSnapshot<'_>,
            _tx: &mut GraphTransaction,
        ) -> Result<(), crate::profile::ApplyPipelineError> {
            Err(crate::profile::ApplyPipelineError::Rejected {
                message: "rejected by middleware".to_string(),
                diagnostics: vec![Diagnostic {
                    key: "test.middleware.reject".to_string(),
                    severity: DiagnosticSeverity::Error,
                    target: DiagnosticTarget::Graph,
                    message: "rejected by middleware".to_string(),
                    fixes: Vec::new(),
                }],
            })
        }
    }

    let (g0, a, _b, _out_port, _in_port, _eid) = make_graph();
    let mut store = NodeGraphStore::new(
        g0.clone(),
        NodeGraphViewState::default(),
        default_editor_config(),
    )
    .with_middleware(RejectAll);

    let tx = GraphTransaction {
        label: None,
        ops: vec![GraphOp::SetNodePos {
            id: a,
            from: CanvasPoint { x: 0.0, y: 0.0 },
            to: CanvasPoint { x: 10.0, y: 20.0 },
        }],
    };

    let err = store.dispatch_transaction(&tx).expect_err("reject");
    let crate::runtime::store::DispatchError::Apply(crate::profile::ApplyPipelineError::Rejected {
        diagnostics,
        ..
    }) = err
    else {
        panic!("unexpected error: {err:?}");
    };
    assert_eq!(diagnostics[0].key, "test.middleware.reject");
    assert_eq!(
        store.graph().nodes.get(&a).unwrap().pos,
        g0.nodes.get(&a).unwrap().pos
    );
    assert!(!store.can_undo());
}

#[test]
fn store_subscription_receives_graph_and_view_events_and_can_unsubscribe() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let (g0, a, _b, _out_port, _in_port, _eid) = make_graph();
    let mut store = NodeGraphStore::new(g0, NodeGraphViewState::default(), default_editor_config());

    let events: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
    let events2 = events.clone();

    let token = store.subscribe(move |ev| match ev {
        NodeGraphStoreEvent::DocumentReplaced { .. } => events2.borrow_mut().push("document"),
        NodeGraphStoreEvent::GraphCommitted { .. } => events2.borrow_mut().push("graph"),
        NodeGraphStoreEvent::ViewChanged { changes, .. } => {
            assert!(!changes.is_empty());
            events2.borrow_mut().push("view");
        }
    });

    store.set_viewport(jellyflow_core::core::CanvasPoint { x: 1.0, y: 2.0 }, 1.25);
    store.set_selection(vec![a], Vec::new(), Vec::new());

    let changes = NodeGraphChanges {
        nodes: vec![NodeChange::Position {
            id: a,
            position: CanvasPoint { x: 5.0, y: 6.0 },
        }],
        edges: Vec::new(),
    };
    store.dispatch_changes(&changes).expect("dispatch");

    let got = events.borrow().clone();
    assert!(got.contains(&"view"));
    assert!(got.contains(&"graph"));

    assert!(store.unsubscribe(token));
    assert!(!store.unsubscribe(token));

    let before_len = events.borrow().len();
    store.set_viewport(jellyflow_core::core::CanvasPoint { x: 3.0, y: 4.0 }, 2.0);
    store.dispatch_changes(&changes).expect("dispatch");
    assert_eq!(events.borrow().len(), before_len);
}

#[test]
fn store_selector_subscription_dedupes_and_tracks_graph_and_view_projections() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let (g0, a, _b, _out_port, _in_port, _eid) = make_graph();
    let mut store = NodeGraphStore::new(g0, NodeGraphViewState::default(), default_editor_config());

    let node_counts: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(Vec::new()));
    let node_counts2 = node_counts.clone();
    store.subscribe_selector(
        |s| s.graph.nodes.len(),
        move |v| node_counts2.borrow_mut().push(*v),
    );

    let selection_counts: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(Vec::new()));
    let selection_counts2 = selection_counts.clone();
    store.subscribe_selector(
        |s| s.view_state.selected_nodes.len(),
        move |v| selection_counts2.borrow_mut().push(*v),
    );

    // Same selection twice should dedupe (no extra callback).
    store.set_selection(vec![a], Vec::new(), Vec::new());
    store.set_selection(vec![a], Vec::new(), Vec::new());

    assert_eq!(selection_counts.borrow().as_slice(), &[1]);
    assert!(node_counts.borrow().is_empty());

    // Adding a node should trigger only the node-count selector.
    let new_id = NodeId::new();
    let node = Node {
        kind: NodeKindKey::new("demo.c"),
        kind_version: 1,
        pos: CanvasPoint { x: 0.0, y: 0.0 },
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
        ports: Vec::new(),
        data: serde_json::Value::Null,
    };

    let tx = GraphTransaction {
        label: None,
        ops: vec![GraphOp::AddNode { id: new_id, node }],
    };
    store.dispatch_transaction(&tx).expect("dispatch");

    assert_eq!(node_counts.borrow().as_slice(), &[3]);
    assert_eq!(selection_counts.borrow().as_slice(), &[1]);
}

#[test]
fn store_selector_diff_provides_prev_and_next() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let (g0, a, _b, _out_port, _in_port, _eid) = make_graph();
    let mut store = NodeGraphStore::new(g0, NodeGraphViewState::default(), default_editor_config());

    let deltas: Rc<RefCell<Vec<(usize, usize)>>> = Rc::new(RefCell::new(Vec::new()));
    let deltas2 = deltas.clone();
    store.subscribe_selector_diff(
        |s| s.view_state.selected_nodes.len(),
        move |prev, next| deltas2.borrow_mut().push((*prev, *next)),
    );

    store.set_selection(vec![a], Vec::new(), Vec::new());
    store.set_selection(vec![a], Vec::new(), Vec::new());
    store.set_selection(Vec::new(), Vec::new(), Vec::new());

    assert_eq!(deltas.borrow().as_slice(), &[(0, 1), (1, 0)]);
}

#[test]
fn store_replace_view_state_emits_view_changed_event() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let (g0, _a, _b, _out_port, _in_port, _eid) = make_graph();
    let mut store = NodeGraphStore::new(g0, NodeGraphViewState::default(), default_editor_config());

    let events: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
    let events2 = events.clone();
    store.subscribe(move |ev| match ev {
        NodeGraphStoreEvent::DocumentReplaced { .. } => events2.borrow_mut().push("document"),
        NodeGraphStoreEvent::ViewChanged { .. } => events2.borrow_mut().push("view"),
        NodeGraphStoreEvent::GraphCommitted { .. } => events2.borrow_mut().push("graph"),
    });

    let mut vs = NodeGraphViewState::default();
    vs.pan = jellyflow_core::core::CanvasPoint { x: 10.0, y: 20.0 };
    vs.zoom = 1.5;
    store.replace_view_state(vs);

    assert_eq!(events.borrow().as_slice(), &["view"]);
}

#[test]
fn store_set_viewport_emits_exact_zoom_changes_below_projection_epsilon() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let (g0, _a, _b, _out_port, _in_port, _eid) = make_graph();
    let mut store = NodeGraphStore::new(g0, NodeGraphViewState::default(), default_editor_config());

    let pan = CanvasPoint { x: 10.0, y: 20.0 };
    store.set_viewport(pan, 1.0);

    let zooms: Rc<RefCell<Vec<f32>>> = Rc::new(RefCell::new(Vec::new()));
    let zooms2 = zooms.clone();
    store.subscribe(move |ev| {
        if let NodeGraphStoreEvent::ViewChanged { changes, .. } = ev {
            for change in changes {
                if let crate::runtime::events::ViewChange::Viewport { zoom, .. } = change {
                    zooms2.borrow_mut().push(*zoom);
                }
            }
        }
    });

    let zoom = 1.0 + 5.0e-7;
    store.set_viewport(pan, zoom);

    assert_eq!(zooms.borrow().as_slice(), &[zoom]);
}

#[test]
fn store_replace_document_emits_single_document_event_and_clears_history() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let (g0, a, b, _out_port, _in_port, _eid) = make_graph();
    let replacement_node = g0.nodes.get(&b).expect("replacement node").clone();
    let mut store = NodeGraphStore::new(g0, NodeGraphViewState::default(), default_editor_config());

    let from = store.graph().nodes.get(&a).expect("node a").pos;
    let tx = GraphTransaction {
        label: Some("seed history".to_string()),
        ops: vec![GraphOp::SetNodePos {
            id: a,
            from,
            to: CanvasPoint {
                x: from.x + 10.0,
                y: from.y + 5.0,
            },
        }],
    };
    store.dispatch_transaction(&tx).expect("seed history");
    assert!(store.can_undo());

    let before_revision = store.graph_revision();
    let events: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
    let events2 = events.clone();
    type DocumentEventDetail = (GraphId, GraphId, u64, u64, Vec<NodeId>, bool);
    let details: Rc<RefCell<Option<DocumentEventDetail>>> = Rc::new(RefCell::new(None));
    let details2 = details.clone();
    store.subscribe(move |ev| match ev {
        NodeGraphStoreEvent::DocumentReplaced { before, after } => {
            events2.borrow_mut().push("document");
            *details2.borrow_mut() = Some((
                before.graph.graph_id,
                after.graph.graph_id,
                before.graph_revision,
                after.graph_revision,
                after.view_state.selected_nodes.clone(),
                after
                    .editor_config
                    .runtime_tuning
                    .only_render_visible_elements,
            ));
        }
        NodeGraphStoreEvent::ViewChanged { .. } => events2.borrow_mut().push("view"),
        NodeGraphStoreEvent::GraphCommitted { .. } => events2.borrow_mut().push("graph"),
    });

    let mut next_graph = Graph::new(GraphId::from_u128(2));
    next_graph.nodes.insert(b, replacement_node);
    let mut next_view_state = NodeGraphViewState {
        selected_nodes: vec![a, b],
        ..NodeGraphViewState::default()
    };
    next_view_state.pan = CanvasPoint { x: 8.0, y: 13.0 };
    next_view_state.zoom = 1.75;
    let mut next_editor_config = default_editor_config();
    next_editor_config
        .runtime_tuning
        .only_render_visible_elements = false;

    store.replace_document(
        next_graph.clone(),
        next_view_state,
        next_editor_config.clone(),
    );

    assert_eq!(events.borrow().as_slice(), &["document"]);
    let detail = details.borrow().clone().expect("document event detail");
    assert_eq!(detail.0, GraphId::from_u128(1));
    assert_eq!(detail.1, GraphId::from_u128(2));
    assert_eq!(detail.2, before_revision);
    assert!(detail.3 > detail.2);
    assert_eq!(detail.4, vec![b]);
    assert_eq!(
        detail.5,
        next_editor_config
            .runtime_tuning
            .only_render_visible_elements
    );
    assert_eq!(store.graph().graph_id, next_graph.graph_id);
    assert_eq!(store.view_state().selected_nodes, vec![b]);
    assert_eq!(store.editor_config(), next_editor_config);
    assert!(!store.can_undo());
    assert!(!store.can_redo());
}

#[test]
fn store_replace_graph_emits_document_event_and_preserves_history_policy() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let (g0, a, b, _out_port, _in_port, _eid) = make_graph();
    let replacement_node = g0.nodes.get(&a).expect("replacement node").clone();
    let mut store = NodeGraphStore::new(g0, NodeGraphViewState::default(), default_editor_config());
    store.set_selection(vec![b], Vec::new(), Vec::new());

    let from = store.graph().nodes.get(&a).expect("node a").pos;
    let tx = GraphTransaction {
        label: Some("seed history".to_string()),
        ops: vec![GraphOp::SetNodePos {
            id: a,
            from,
            to: CanvasPoint {
                x: from.x + 10.0,
                y: from.y + 5.0,
            },
        }],
    };
    store.dispatch_transaction(&tx).expect("seed history");
    assert!(store.can_undo());

    let before_revision = store.graph_revision();
    let events: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
    let events2 = events.clone();
    let selected_after: Rc<RefCell<Option<Vec<NodeId>>>> = Rc::new(RefCell::new(None));
    let selected_after2 = selected_after.clone();
    store.subscribe(move |ev| match ev {
        NodeGraphStoreEvent::DocumentReplaced { before, after } => {
            events2.borrow_mut().push("document");
            assert_eq!(before.graph_revision, before_revision);
            assert!(after.graph_revision > before.graph_revision);
            *selected_after2.borrow_mut() = Some(after.view_state.selected_nodes.clone());
        }
        NodeGraphStoreEvent::ViewChanged { .. } => events2.borrow_mut().push("view"),
        NodeGraphStoreEvent::GraphCommitted { .. } => events2.borrow_mut().push("graph"),
    });

    let mut next_graph = Graph::new(GraphId::from_u128(3));
    next_graph.nodes.insert(a, replacement_node);
    store.replace_graph(next_graph);

    assert_eq!(events.borrow().as_slice(), &["document"]);
    assert_eq!(selected_after.borrow().clone(), Some(Vec::new()));
    assert!(store.can_undo());
}

#[test]
fn store_replace_editor_config_notifies_selectors_for_runtime_tuning_only_changes() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let (g0, _a, _b, _out_port, _in_port, _eid) = make_graph();
    let mut store = NodeGraphStore::new(g0, NodeGraphViewState::default(), default_editor_config());

    let events: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
    let events2 = events.clone();
    store.subscribe(move |ev| match ev {
        NodeGraphStoreEvent::DocumentReplaced { .. } => events2.borrow_mut().push("document"),
        NodeGraphStoreEvent::ViewChanged { .. } => events2.borrow_mut().push("view"),
        NodeGraphStoreEvent::GraphCommitted { .. } => events2.borrow_mut().push("graph"),
    });

    let runtime_flags: Rc<RefCell<Vec<bool>>> = Rc::new(RefCell::new(Vec::new()));
    let runtime_flags2 = runtime_flags.clone();
    store.subscribe_selector(
        |s| s.runtime_tuning.only_render_visible_elements,
        move |value| runtime_flags2.borrow_mut().push(*value),
    );

    let mut editor_config = store.editor_config();
    editor_config.runtime_tuning.only_render_visible_elements = false;
    store.replace_editor_config(editor_config);

    assert!(events.borrow().is_empty());
    assert_eq!(runtime_flags.borrow().as_slice(), &[false]);
    assert!(!store.runtime_tuning().only_render_visible_elements);
}

#[test]
fn store_update_editor_config_notifies_selectors_only_when_changed() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let (g0, _a, _b, _out_port, _in_port, _eid) = make_graph();
    let mut store = NodeGraphStore::new(g0, NodeGraphViewState::default(), default_editor_config());

    let runtime_flags: Rc<RefCell<Vec<bool>>> = Rc::new(RefCell::new(Vec::new()));
    let runtime_flags2 = runtime_flags.clone();
    store.subscribe_selector(
        |s| s.runtime_tuning.only_render_visible_elements,
        move |value| runtime_flags2.borrow_mut().push(*value),
    );

    store.update_editor_config(|_| {});
    assert!(runtime_flags.borrow().is_empty());

    store.update_editor_config(|config| {
        config.runtime_tuning.only_render_visible_elements = false;
    });
    assert_eq!(runtime_flags.borrow().as_slice(), &[false]);

    store.update_editor_config(|config| {
        config.runtime_tuning.only_render_visible_elements = false;
    });
    assert_eq!(runtime_flags.borrow().as_slice(), &[false]);
}

#[test]
fn store_update_view_state_notifies_selectors_for_draw_order_only_changes() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let (g0, a, b, _out_port, _in_port, _eid) = make_graph();
    let mut store = NodeGraphStore::new(g0, NodeGraphViewState::default(), default_editor_config());

    let events: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
    let events2 = events.clone();
    store.subscribe(move |ev| match ev {
        NodeGraphStoreEvent::DocumentReplaced { .. } => events2.borrow_mut().push("document"),
        NodeGraphStoreEvent::ViewChanged { .. } => events2.borrow_mut().push("view"),
        NodeGraphStoreEvent::GraphCommitted { .. } => events2.borrow_mut().push("graph"),
    });

    let draw_order_snapshots: Rc<RefCell<Vec<Vec<NodeId>>>> = Rc::new(RefCell::new(Vec::new()));
    let draw_order_snapshots2 = draw_order_snapshots.clone();
    store.subscribe_selector(
        |s| s.view_state.draw_order.clone(),
        move |value| draw_order_snapshots2.borrow_mut().push(value.clone()),
    );

    store.update_view_state(|s| {
        s.draw_order = vec![b, a];
    });

    assert!(events.borrow().is_empty());
    assert_eq!(draw_order_snapshots.borrow().as_slice(), &[vec![b, a]]);
    assert_eq!(store.view_state().draw_order.as_slice(), &[b, a]);
}

#[test]
fn store_graph_revision_stays_stable_for_view_only_updates() {
    let (g0, a, _b, _out_port, _in_port, _eid) = make_graph();
    let mut store = NodeGraphStore::new(g0, NodeGraphViewState::default(), default_editor_config());
    let before = store.graph_revision();

    store.set_viewport(CanvasPoint { x: 3.0, y: 4.0 }, 1.5);
    assert_eq!(store.graph_revision(), before);

    store.set_selection(vec![a], Vec::new(), Vec::new());
    assert_eq!(store.graph_revision(), before);

    store.update_view_state(|state| {
        state.draw_order = vec![a];
    });
    assert_eq!(store.graph_revision(), before);
}

#[test]
fn store_graph_revision_advances_for_graph_mutations() {
    let (g0, _a, _b, _out_port, _in_port, _eid) = make_graph();
    let mut store = NodeGraphStore::new(g0, NodeGraphViewState::default(), default_editor_config());
    let node_id = NodeId::new();
    let before = store.graph_revision();

    let tx = GraphTransaction {
        label: None,
        ops: vec![GraphOp::AddNode {
            id: node_id,
            node: Node {
                kind: NodeKindKey::new("demo.c"),
                kind_version: 1,
                pos: CanvasPoint { x: 0.0, y: 0.0 },
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
                ports: Vec::new(),
                data: serde_json::Value::Null,
            },
        }],
    };

    store.dispatch_transaction(&tx).expect("dispatch");
    assert!(store.graph_revision() > before);

    let after_dispatch = store.graph_revision();
    store.undo().expect("undo").expect("undo outcome");
    assert!(store.graph_revision() > after_dispatch);

    let after_undo = store.graph_revision();
    store.redo().expect("redo").expect("redo outcome");
    assert!(store.graph_revision() > after_undo);
}
