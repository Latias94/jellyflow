use super::*;

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

    let tx = GraphTransaction::from_ops([GraphOp::SetNodePos {
        id: a,
        from: CanvasPoint { x: 0.0, y: 0.0 },
        to: CanvasPoint { x: 999.0, y: 999.0 },
    }]);

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
        store.graph().nodes().get(&a).unwrap().pos,
        g0.nodes().get(&a).unwrap().pos
    );
    assert!(!store.can_undo());
}

#[test]
fn store_rejects_non_finite_transactions() {
    let g = Graph::new(jellyflow_core::core::GraphId::from_u128(1));
    let node_id = NodeId::new();

    let tx = GraphTransaction::from_ops([GraphOp::AddNode {
        id: node_id,
        node: Node {
            kind: NodeKindKey::new("demo.node"),
            kind_version: 1,
            pos: CanvasPoint {
                x: f32::NAN,
                y: 0.0,
            },
            origin: None,
            selectable: None,
            focusable: None,
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
    }]);

    let mut store = make_store(g.clone());
    let err = store.dispatch_transaction(&tx).expect_err("reject");
    let crate::runtime::store::DispatchError::Apply(crate::profile::ApplyPipelineError::Rejected {
        diagnostics,
        ..
    }) = err
    else {
        panic!("unexpected error: {err:?}");
    };
    assert_eq!(diagnostics[0].key, "tx.non_finite");
    assert!(store.graph().nodes().is_empty());
    assert_eq!(store.graph().graph_id(), g.graph_id());
    assert!(!store.can_undo());
}

#[test]
fn store_rejects_invalid_size_transactions() {
    let g = Graph::new(jellyflow_core::core::GraphId::from_u128(1));
    let node_id = NodeId::new();

    let tx = GraphTransaction::from_ops([GraphOp::AddNode {
        id: node_id,
        node: Node {
            kind: NodeKindKey::new("demo.node"),
            kind_version: 1,
            pos: CanvasPoint { x: 0.0, y: 0.0 },
            origin: None,
            selectable: None,
            focusable: None,
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
    }]);

    let mut store = make_store(g.clone());
    let err = store.dispatch_transaction(&tx).expect_err("reject");
    let crate::runtime::store::DispatchError::Apply(crate::profile::ApplyPipelineError::Rejected {
        diagnostics,
        ..
    }) = err
    else {
        panic!("unexpected error: {err:?}");
    };
    assert_eq!(diagnostics[0].key, "tx.invalid_size");
    assert!(store.graph().nodes().is_empty());
    assert_eq!(store.graph().graph_id(), g.graph_id());
    assert!(!store.can_undo());
}
