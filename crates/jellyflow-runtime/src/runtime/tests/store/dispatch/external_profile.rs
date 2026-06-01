use super::*;

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
            let id = tx
                .ops()
                .first()
                .and_then(node_pos_id)
                .expect("node position op");
            tx.push(GraphOp::SetNodeHidden {
                id,
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
            assert_eq!(node_edge_changes.nodes().len(), 2);
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
    let mut store = make_store(g0).with_middleware(TraceMiddleware {
        trace: trace.clone(),
    });
    let mut profile = PassProfile;

    let observed: Rc<RefCell<Option<(usize, bool)>>> = Rc::new(RefCell::new(None));
    let observed2 = observed.clone();
    store.subscribe(move |ev| {
        if let NodeGraphStoreEvent::GraphCommitted { patch } = ev {
            let node_edge_changes = NodeGraphChanges::from_patch(patch);
            *observed2.borrow_mut() = Some((
                node_edge_changes.nodes().len(),
                node_edge_changes
                    .nodes()
                    .iter()
                    .any(|change| matches!(change, NodeChange::Hidden { hidden: true, .. })),
            ));
        }
    });

    let tx = GraphTransaction::from_ops([GraphOp::SetNodePos {
        id: a,
        from: CanvasPoint { x: 0.0, y: 0.0 },
        to: CanvasPoint { x: 42.0, y: 24.0 },
    }])
    .with_label("external profile commit");

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
