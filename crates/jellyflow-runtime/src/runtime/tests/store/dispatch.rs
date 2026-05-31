use super::*;

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
