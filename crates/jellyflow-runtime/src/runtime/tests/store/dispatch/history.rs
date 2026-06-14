use super::*;

#[test]
fn store_dispatch_changes_records_history_and_supports_undo() {
    let (g0, a, _b, _out_port, _in_port, _eid) = make_graph();
    let mut store = make_store(g0);

    let changes = NodeGraphChanges::from_parts(
        vec![NodeChange::Position {
            id: a,
            position: CanvasPoint { x: 12.0, y: 34.0 },
        }],
        Vec::new(),
    );

    let outcome = store.dispatch_changes(&changes).expect("dispatch");
    assert!(!outcome.patch.ops().is_empty());
    assert!(store.can_undo());
    assert_eq!(
        store.graph().nodes().get(&a).unwrap().pos,
        CanvasPoint { x: 12.0, y: 34.0 }
    );

    let undo = store.undo().expect("undo").expect("did undo");
    assert!(!undo.patch.ops().is_empty());
    assert_eq!(
        store.graph().nodes().get(&a).unwrap().pos,
        CanvasPoint { x: 0.0, y: 0.0 }
    );
}
