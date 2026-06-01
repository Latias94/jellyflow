use super::*;

#[test]
fn graph_diff_roundtrips_when_adding_a_port_to_existing_node() {
    let mut from = Graph::default();
    let a = NodeId::new();
    insert_node(&mut from, a, "core.a");

    let out = PortId::from_u128(35);
    let mut to = from.clone();
    insert_port(&mut to, out, a, "out", PortDirection::Out);

    let tx = graph_diff(&from, &to);
    let add_index = tx
        .ops()
        .iter()
        .position(|op| matches!(op, GraphOp::AddPort { id, .. } if *id == out))
        .expect("diff must add the new port");
    let order_index = tx
        .ops()
        .iter()
        .position(|op| matches!(op, GraphOp::SetNodePorts { id, .. } if *id == a))
        .expect("diff must attach the new port to its owner order");
    assert!(
        add_index < order_index,
        "diff must add the port before attaching it to node.ports"
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
fn graph_diff_inverse_roundtrips_when_adding_ports_and_edge() {
    let mut from = Graph::default();
    let a = NodeId::new();
    let b = NodeId::new();
    insert_node(&mut from, a, "core.a");
    insert_node(&mut from, b, "core.b");

    let out = PortId::from_u128(41);
    let inn = PortId::from_u128(42);
    let edge_id = EdgeId::from_u128(43);
    let mut to = from.clone();
    insert_port(&mut to, out, a, "out", PortDirection::Out);
    insert_port(&mut to, inn, b, "in", PortDirection::In);
    to.edges.insert(edge_id, make_edge(out, inn));

    let tx = graph_diff(&from, &to);
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
fn graph_diff_inverse_roundtrips_when_deleted_node_port_moves_to_existing_node() {
    let mut from = Graph::default();
    let a = NodeId::new();
    let b = NodeId::new();
    insert_node(&mut from, a, "core.a");
    insert_node(&mut from, b, "core.b");

    let moved = PortId::from_u128(44);
    insert_port(&mut from, moved, a, "moved", PortDirection::Out);

    let mut to = from.clone();
    to.nodes.remove(&a);
    insert_port(&mut to, moved, b, "moved", PortDirection::Out);

    let tx = graph_diff(&from, &to);
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
fn graph_diff_inverse_roundtrips_when_port_moves_between_existing_nodes() {
    let mut from = Graph::default();
    let a = NodeId::new();
    let b = NodeId::new();
    insert_node(&mut from, a, "core.a");
    insert_node(&mut from, b, "core.b");

    let moved = PortId::from_u128(45);
    insert_port(&mut from, moved, a, "moved", PortDirection::Out);

    let mut to = from.clone();
    to.ports
        .insert(moved, make_port(b, "moved", PortDirection::Out));
    to.nodes.get_mut(&a).unwrap().ports.clear();
    to.nodes.get_mut(&b).unwrap().ports.push(moved);

    let tx = graph_diff(&from, &to);
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
fn graph_diff_inverse_roundtrips_when_node_port_membership_is_replaced() {
    let mut from = Graph::default();
    let a = NodeId::new();
    insert_node(&mut from, a, "core.a");

    let old = PortId::from_u128(36);
    let kept = PortId::from_u128(37);
    let new = PortId::from_u128(38);
    insert_port(&mut from, old, a, "old", PortDirection::Out);
    insert_port(&mut from, kept, a, "kept", PortDirection::Out);

    let mut to = from.clone();
    to.ports.remove(&old);
    to.ports
        .insert(new, make_port(a, "new", PortDirection::Out));
    to.nodes.get_mut(&a).unwrap().ports = vec![kept, new];

    let tx = graph_diff(&from, &to);
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
fn graph_diff_inverse_roundtrips_when_structural_port_replacement_adds_sibling_port() {
    let mut from = Graph::default();
    let a = NodeId::new();
    insert_node(&mut from, a, "core.a");

    let replaced = PortId::from_u128(39);
    let added = PortId::from_u128(40);
    insert_port(&mut from, replaced, a, "old", PortDirection::Out);

    let mut to = from.clone();
    to.ports.get_mut(&replaced).unwrap().key = PortKey::new("renamed");
    to.ports
        .insert(added, make_port(a, "added", PortDirection::Out));
    to.nodes.get_mut(&a).unwrap().ports = vec![replaced, added];

    let tx = graph_diff(&from, &to);
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
