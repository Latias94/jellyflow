use super::super::fixtures::{make_graph, make_store};

use crate::runtime::middleware::NodeGraphStoreMiddleware;
use jellyflow_core::core::CanvasPoint;
use jellyflow_core::ops::{GraphOp, GraphTransaction};

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
            tx.clear_ops();
            Ok(())
        }
    }

    let (g0, a, _b, _out_port, _in_port, _eid) = make_graph();
    let mut store = make_store(g0).with_middleware(DropOps);

    let tx = GraphTransaction::from_ops([GraphOp::SetNodePos {
        id: a,
        from: CanvasPoint { x: 0.0, y: 0.0 },
        to: CanvasPoint { x: 10.0, y: 20.0 },
    }]);

    let outcome = store.dispatch_transaction(&tx).expect("dispatch");
    assert!(outcome.patch.ops().is_empty());
    assert_eq!(
        store.graph().nodes().get(&a).unwrap().pos,
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
    let mut store = make_store(g0.clone()).with_middleware(RejectAll);

    let tx = GraphTransaction::from_ops([GraphOp::SetNodePos {
        id: a,
        from: CanvasPoint { x: 0.0, y: 0.0 },
        to: CanvasPoint { x: 10.0, y: 20.0 },
    }]);

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
        store.graph().nodes().get(&a).unwrap().pos,
        g0.nodes().get(&a).unwrap().pos
    );
    assert!(!store.can_undo());
}

#[test]
fn store_middleware_after_dispatch_observes_undo_and_redo() {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    #[derive(Debug)]
    struct TraceAfterDispatch {
        before_calls: Rc<Cell<usize>>,
        after_calls: Rc<RefCell<Vec<(bool, bool, usize)>>>,
    }

    impl NodeGraphStoreMiddleware for TraceAfterDispatch {
        fn before_dispatch(
            &mut self,
            _snapshot: crate::runtime::events::NodeGraphStoreSnapshot<'_>,
            _tx: &mut GraphTransaction,
        ) -> Result<(), crate::profile::ApplyPipelineError> {
            self.before_calls.set(self.before_calls.get() + 1);
            Ok(())
        }

        fn after_dispatch(
            &mut self,
            snapshot: crate::runtime::events::NodeGraphStoreSnapshot<'_>,
            patch: &crate::runtime::commit::NodeGraphPatch,
        ) {
            self.after_calls.borrow_mut().push((
                snapshot.history.can_undo(),
                snapshot.history.can_redo(),
                patch.ops().len(),
            ));
        }
    }

    let (g0, a, _b, _out_port, _in_port, _eid) = make_graph();
    let before_calls = Rc::new(Cell::new(0));
    let after_calls = Rc::new(RefCell::new(Vec::new()));
    let mut store = make_store(g0).with_middleware(TraceAfterDispatch {
        before_calls: before_calls.clone(),
        after_calls: after_calls.clone(),
    });

    let tx = GraphTransaction::from_ops([GraphOp::SetNodePos {
        id: a,
        from: CanvasPoint { x: 0.0, y: 0.0 },
        to: CanvasPoint { x: 10.0, y: 20.0 },
    }]);

    store.dispatch_transaction(&tx).expect("dispatch");
    store.undo().expect("undo").expect("undo outcome");
    store.redo().expect("redo").expect("redo outcome");

    assert_eq!(before_calls.get(), 1);
    assert_eq!(
        &*after_calls.borrow(),
        &[(true, false, 1), (false, true, 1), (true, false, 1)]
    );
}
