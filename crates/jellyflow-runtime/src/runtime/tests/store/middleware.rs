use super::*;

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
