use super::*;

#[test]
fn store_dispatch_pipeline_publishes_coherent_commit_state() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let (g0, a, _b, _out_port, _in_port, eid) = make_graph();
    let mut store = make_store(g0);

    let observed: Rc<RefCell<Option<(bool, Option<EdgeReconnectable>, bool, bool)>>> =
        Rc::new(RefCell::new(None));
    let observed2 = observed.clone();
    store.subscribe(move |ev| {
        if let NodeGraphStoreEvent::GraphCommitted { patch } = ev {
            let node_edge_changes = NodeGraphChanges::from_patch(patch);
            let hidden = node_edge_changes
                .nodes()
                .iter()
                .any(|change| matches!(change, NodeChange::Hidden { hidden: true, .. }));
            let reconnectable = node_edge_changes
                .edges()
                .iter()
                .find_map(|change| match change {
                    EdgeChange::Reconnectable { reconnectable, .. } => *reconnectable,
                    _ => None,
                });
            *observed2.borrow_mut() = Some((
                hidden,
                reconnectable,
                patch.footprint().nodes.contains(&a),
                patch.footprint().edges.contains(&eid),
            ));
        }
    });

    let tx = GraphTransaction::from_ops([
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

    let outcome = store.dispatch_transaction(&tx).expect("dispatch");

    assert!(store.graph().nodes().get(&a).unwrap().hidden);
    assert!(store.lookups().node_lookup.get(&a).unwrap().hidden);
    assert_eq!(
        store.lookups().edge_lookup.get(&eid).unwrap().reconnectable,
        Some(EdgeReconnectable::Bool(false))
    );
    assert!(store.can_undo());
    let node_edge_changes = NodeGraphChanges::from_patch(outcome.patch());
    assert!(
        node_edge_changes
            .nodes()
            .iter()
            .any(|change| matches!(change, NodeChange::Hidden { id, hidden: true } if *id == a))
    );
    assert!(node_edge_changes.edges().iter().any(|change| matches!(
        change,
        EdgeChange::Reconnectable {
            id,
            reconnectable: Some(EdgeReconnectable::Bool(false))
        } if *id == eid
    )));
    assert!(outcome.footprint().nodes.contains(&a));
    assert!(outcome.footprint().edges.contains(&eid));
    assert_eq!(outcome.footprint(), outcome.patch().footprint());
    assert_eq!(
        *observed.borrow(),
        Some((true, Some(EdgeReconnectable::Bool(false)), true, true))
    );
}
