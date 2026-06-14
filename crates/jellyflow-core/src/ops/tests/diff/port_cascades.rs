use super::*;

#[test]
fn graph_diff_roundtrips_when_deleting_a_port_with_incident_edges() {
    let mut from = Graph::default();
    let ids = insert_connected_pair_with_ids(
        &mut from,
        ConnectedPairIds {
            out: PortId::from_u128(30),
            inn: PortId::from_u128(31),
            edge: EdgeId::from_u128(789),
            ..ConnectedPairIds::new()
        },
    );

    let mut to = from.clone();
    to.node_mut(&ids.a).unwrap().ports.retain(|p| *p != ids.out);
    to.remove_port(&ids.out);
    to.remove_edge(&ids.edge);

    let tx = graph_diff(&from, &to);
    assert!(
        tx.ops()
            .iter()
            .any(|op| matches!(op, GraphOp::RemovePort { id, .. } if *id == ids.out)),
        "diff must use reversible RemovePort for port deletion"
    );
    assert!(
        !tx.ops()
            .iter()
            .any(|op| matches!(op, GraphOp::RemoveEdge { id, .. } if *id == ids.edge)),
        "diff must not double-remove edges that are already removed by RemovePort"
    );

    let mut patched = from.clone();
    apply_transaction(&mut patched, &tx).expect("apply diff");
    assert_eq!(
        serde_json::to_value(&patched).unwrap(),
        serde_json::to_value(&to).unwrap(),
        "diff must roundtrip"
    );
}

#[test]
fn graph_diff_roundtrips_when_port_deletion_moves_incident_edge() {
    let mut from = Graph::default();
    let a = NodeId::new();
    let b = NodeId::new();
    insert_node(&mut from, a, "core.a");
    insert_node(&mut from, b, "core.b");

    let out1 = PortId::from_u128(32);
    let out2 = PortId::from_u128(33);
    let inn = PortId::from_u128(34);
    insert_port(&mut from, out1, a, "out1", PortDirection::Out);
    insert_port(&mut from, out2, a, "out2", PortDirection::Out);
    insert_port(&mut from, inn, b, "in", PortDirection::In);

    let edge_id = EdgeId::from_u128(790);
    from.insert_edge(edge_id, make_edge(out1, inn));

    let mut to = from.clone();
    to.node_mut(&a).unwrap().retain_ports(|p| *p != out1);
    to.remove_port(&out1);
    to.edge_mut(&edge_id).unwrap().from = out2;

    let tx = graph_diff(&from, &to);
    assert!(
        tx.ops()
            .iter()
            .any(|op| matches!(op, GraphOp::AddEdge { id, .. } if *id == edge_id)),
        "diff must re-add a moved edge after its old endpoint is deleted"
    );
    assert!(
        !tx.ops()
            .iter()
            .any(|op| matches!(op, GraphOp::SetEdgeEndpoints { id, .. } if *id == edge_id)),
        "diff must not patch endpoints on an edge removed by a prior cascade"
    );

    let mut patched = from.clone();
    apply_transaction(&mut patched, &tx).expect("apply diff");
    assert_eq!(
        serde_json::to_value(&patched).unwrap(),
        serde_json::to_value(&to).unwrap(),
        "diff must roundtrip"
    );

    let inverse = invert_transaction(&tx);
    apply_transaction(&mut patched, &inverse).expect("apply inverse");
    assert_eq!(
        serde_json::to_value(&patched).unwrap(),
        serde_json::to_value(&from).unwrap(),
        "diff inverse must restore the source graph"
    );
}

#[test]
fn graph_diff_roundtrips_when_a_port_changes_structurally() {
    let mut from = Graph::default();
    let ids = insert_connected_pair_with_ids(
        &mut from,
        ConnectedPairIds {
            out: PortId::from_u128(40),
            inn: PortId::from_u128(41),
            edge: EdgeId::from_u128(1010),
            ..ConnectedPairIds::new()
        },
    );

    let mut to = from.clone();
    to.port_mut(&ids.out).unwrap().key = PortKey::new("out2");

    let tx = graph_diff(&from, &to);
    assert!(
        tx.ops()
            .iter()
            .any(|op| matches!(op, GraphOp::RemovePort { id, .. } if *id == ids.out)),
        "diff must represent structural port changes as remove+add"
    );
    assert!(
        tx.ops()
            .iter()
            .any(|op| matches!(op, GraphOp::AddPort { id, .. } if *id == ids.out)),
        "diff must represent structural port changes as remove+add"
    );
    assert!(
        tx.ops()
            .iter()
            .any(|op| matches!(op, GraphOp::SetNodePorts { id, .. } if *id == ids.a)),
        "diff must restore node port ordering after remove+add"
    );
    assert!(
        tx.ops()
            .iter()
            .any(|op| matches!(op, GraphOp::AddEdge { id, .. } if *id == ids.edge)),
        "diff must re-add incident edges removed by RemovePort when they still exist in 'to'"
    );
    assert!(
        !tx.ops()
            .iter()
            .any(|op| matches!(op, GraphOp::RemoveEdge { id, .. } if *id == ids.edge)),
        "diff must not double-remove edges that are removed by RemovePort"
    );

    let mut patched = from.clone();
    apply_transaction(&mut patched, &tx).expect("apply diff");
    assert_eq!(
        serde_json::to_value(&patched).unwrap(),
        serde_json::to_value(&to).unwrap(),
        "diff must roundtrip"
    );

    let inverse = invert_transaction(&tx);
    apply_transaction(&mut patched, &inverse).expect("apply inverse");
    assert_eq!(
        serde_json::to_value(&patched).unwrap(),
        serde_json::to_value(&from).unwrap(),
        "diff inverse must restore the source graph"
    );
}

#[test]
fn graph_diff_inverse_roundtrips_when_structural_port_replacement_moves_edge() {
    let mut from = Graph::default();
    let a = NodeId::new();
    let b = NodeId::new();
    insert_node(&mut from, a, "core.a");
    insert_node(&mut from, b, "core.b");

    let out1 = PortId::from_u128(60);
    let out2 = PortId::from_u128(61);
    let inn = PortId::from_u128(62);
    insert_port(&mut from, out1, a, "out1", PortDirection::Out);
    insert_port(&mut from, out2, a, "out2", PortDirection::Out);
    insert_port(&mut from, inn, b, "in", PortDirection::In);

    let edge_id = EdgeId::from_u128(3030);
    from.insert_edge(edge_id, make_edge(out1, inn));

    let mut to = from.clone();
    to.port_mut(&out1).unwrap().key = PortKey::new("out1-renamed");
    to.edge_mut(&edge_id).unwrap().from = out2;

    let tx = graph_diff(&from, &to);
    assert!(
        tx.ops()
            .iter()
            .any(|op| matches!(op, GraphOp::AddEdge { id, .. } if *id == edge_id)),
        "diff must re-add the cascaded edge from the target graph"
    );
    assert!(
        !tx.ops()
            .iter()
            .any(|op| matches!(op, GraphOp::SetEdgeEndpoints { id, .. } if *id == edge_id)),
        "diff must not emit endpoint setters for an edge already restored from the target graph"
    );

    let mut patched = from.clone();
    apply_transaction(&mut patched, &tx).expect("apply diff");
    assert_eq!(
        serde_json::to_value(&patched).unwrap(),
        serde_json::to_value(&to).unwrap(),
        "diff must roundtrip"
    );

    let inverse = invert_transaction(&tx);
    apply_transaction(&mut patched, &inverse).expect("apply inverse");
    assert_eq!(
        serde_json::to_value(&patched).unwrap(),
        serde_json::to_value(&from).unwrap(),
        "diff inverse must restore the source graph"
    );
}
