use super::*;

#[test]
fn graph_diff_roundtrips_when_edge_endpoints_change() {
    let mut from = Graph::default();
    let a = NodeId::new();
    let b = NodeId::new();
    insert_node(&mut from, a, "core.a");
    insert_node(&mut from, b, "core.b");

    let out1 = PortId::from_u128(50);
    let out2 = PortId::from_u128(51);
    let inn = PortId::from_u128(52);
    insert_port(&mut from, out1, a, "out1", PortDirection::Out);
    insert_port(&mut from, out2, a, "out2", PortDirection::Out);
    insert_port(&mut from, inn, b, "in", PortDirection::In);

    let edge_id = EdgeId::from_u128(2020);
    from.edges.insert(edge_id, make_edge(out1, inn));

    let mut to = from.clone();
    to.edges.get_mut(&edge_id).unwrap().from = out2;

    let tx = graph_diff(&from, &to);
    assert!(
        tx.ops()
            .iter()
            .any(|op| matches!(op, GraphOp::SetEdgeEndpoints { id, .. } if *id == edge_id)),
        "diff must preserve edge identity when endpoints change"
    );
    assert!(
        !tx.ops().iter().any(|op| {
            matches!(op, GraphOp::RemoveEdge { id, .. } if *id == edge_id)
                || matches!(op, GraphOp::AddEdge { id, .. } if *id == edge_id)
        }),
        "diff must not fall back to remove+add for edge endpoint changes"
    );

    let mut patched = from.clone();
    apply_transaction(&mut patched, &tx).expect("apply diff");
    assert_eq!(
        serde_json::to_value(&patched).unwrap(),
        serde_json::to_value(&to).unwrap(),
        "diff must roundtrip"
    );
}
