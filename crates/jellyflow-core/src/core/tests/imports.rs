use super::*;

#[test]
fn import_closure_is_deterministic_and_postordered() {
    let a = GraphId::from_u128(1);
    let b = GraphId::from_u128(2);
    let c = GraphId::from_u128(3);
    let d = GraphId::from_u128(4);

    let mut g_a = Graph::new(a);
    g_a.insert_import(b, GraphImport::default());
    g_a.insert_import(c, GraphImport::default());

    let mut g_b = Graph::new(b);
    g_b.insert_import(d, GraphImport::default());

    let g_c = Graph::new(c);
    let g_d = Graph::new(d);

    let mut db = std::collections::BTreeMap::new();
    db.insert(a, g_a);
    db.insert(b, g_b);
    db.insert(c, g_c);
    db.insert(d, g_d);

    let root = db.get(&a).expect("root graph must exist");
    let closure = resolve_import_closure(root, |id| db.get(&id)).expect("resolve imports");

    assert_eq!(closure.order, vec![d, b, c]);
    assert!(closure.contains(b));
    assert!(closure.contains(c));
    assert!(closure.contains(d));
    assert!(!closure.contains(a));
}

#[test]
fn import_closure_rejects_missing_graph() {
    let a = GraphId::from_u128(1);
    let missing = GraphId::from_u128(9);

    let mut g_a = Graph::new(a);
    g_a.insert_import(missing, GraphImport::default());

    let mut db = std::collections::BTreeMap::new();
    db.insert(a, g_a);

    let root = db.get(&a).expect("root graph must exist");
    let err =
        resolve_import_closure(root, |id| db.get(&id)).expect_err("expected missing graph error");
    assert_eq!(
        err,
        GraphImportError::MissingGraph {
            from: a,
            to: missing
        }
    );
}

#[test]
fn import_closure_rejects_cycles_with_stable_path() {
    let a = GraphId::from_u128(1);
    let b = GraphId::from_u128(2);
    let c = GraphId::from_u128(3);

    let mut g_a = Graph::new(a);
    g_a.insert_import(b, GraphImport::default());
    let mut g_b = Graph::new(b);
    g_b.insert_import(c, GraphImport::default());
    let mut g_c = Graph::new(c);
    g_c.insert_import(a, GraphImport::default());

    let mut db = std::collections::BTreeMap::new();
    db.insert(a, g_a);
    db.insert(b, g_b);
    db.insert(c, g_c);

    let root = db.get(&a).expect("root graph must exist");
    let err = resolve_import_closure(root, |id| db.get(&id)).expect_err("expected cycle error");
    assert_eq!(
        err,
        GraphImportError::Cycle {
            cycle: vec![a, b, c, a]
        }
    );
}
