use super::*;

#[test]
fn graph_diff_roundtrips_when_deleting_a_node_with_ports_and_edges() {
    let mut from = Graph::default();
    let ids = insert_connected_pair_with_ids(
        &mut from,
        ConnectedPairIds {
            out: PortId::from_u128(20),
            inn: PortId::from_u128(21),
            edge: EdgeId::from_u128(456),
            ..ConnectedPairIds::new()
        },
    );

    let mut to = from.clone();
    to.remove_node(&ids.b);
    to.remove_port(&ids.inn);
    to.remove_edge(&ids.edge);

    let tx = graph_diff(&from, &to);
    assert!(
        tx.ops()
            .iter()
            .any(|op| matches!(op, GraphOp::RemoveNode { id, .. } if *id == ids.b)),
        "diff must use reversible RemoveNode for node deletion"
    );
    assert!(
        !tx.ops()
            .iter()
            .any(|op| matches!(op, GraphOp::RemovePort { id, .. } if *id == ids.inn)),
        "diff must not double-remove ports that are already removed by RemoveNode"
    );
    assert!(
        !tx.ops()
            .iter()
            .any(|op| matches!(op, GraphOp::RemoveEdge { id, .. } if *id == ids.edge)),
        "diff must not double-remove edges that are already removed by RemoveNode"
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
fn graph_diff_roundtrips_when_deleting_a_group_with_child_nodes() {
    let mut from = Graph::default();
    let group_id = GroupId::from_u128(10);
    from.insert_group(
        group_id,
        Group {
            title: "Group".to_string(),
            rect: CanvasRect {
                origin: CanvasPoint { x: 0.0, y: 0.0 },
                size: CanvasSize {
                    width: 100.0,
                    height: 100.0,
                },
            },
            color: None,
        },
    );

    let child_a = NodeId::from_u128(200);
    let child_b = NodeId::from_u128(100);
    let mut node_a = make_node("core.a");
    node_a.parent = Some(group_id);
    from.insert_node(child_a, node_a);
    let mut node_b = make_node("core.b");
    node_b.parent = Some(group_id);
    from.insert_node(child_b, node_b);

    from.insert_node(NodeId::from_u128(300), make_node("core.c"));

    let mut to = from.clone();
    to.remove_group(&group_id);
    to.node_mut(&child_a).unwrap().parent = None;
    to.node_mut(&child_b).unwrap().parent = None;

    let tx = graph_diff(&from, &to);
    let expected_detached = vec![(child_b, Some(group_id)), (child_a, Some(group_id))];
    assert!(
        tx.ops().iter().any(|op| match op {
            GraphOp::RemoveGroup { id, detached, .. } if *id == group_id => {
                detached == &expected_detached
            }
            _ => false,
        }),
        "diff must use reversible RemoveGroup and detach child nodes deterministically"
    );
    assert!(
        !tx.ops().iter().any(|op| matches!(
            op,
            GraphOp::SetNodeParent { id, .. } if *id == child_a || *id == child_b
        )),
        "diff must not emit SetNodeParent when the parent group is being removed"
    );

    let mut patched = from.clone();
    apply_transaction(&mut patched, &tx).expect("apply diff");
    assert_eq!(
        serde_json::to_value(&patched).unwrap(),
        serde_json::to_value(&to).unwrap(),
        "diff must roundtrip"
    );
}
