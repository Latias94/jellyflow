use super::*;

#[test]
fn lookups_parent_queries_are_sorted_and_update_from_transactions() {
    let mut g = Graph::new(GraphId::from_u128(1));
    let group = GroupId::from_u128(100);
    g.groups.insert(
        group,
        Group {
            title: "group".to_string(),
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

    let a = NodeId::from_u128(3);
    let b = NodeId::from_u128(1);
    let root = NodeId::from_u128(2);
    g.nodes.insert(
        a,
        node_with_parent(CanvasPoint { x: 30.0, y: 0.0 }, Some(group)),
    );
    g.nodes.insert(
        root,
        node_with_parent(CanvasPoint { x: 20.0, y: 0.0 }, None),
    );
    g.nodes.insert(
        b,
        node_with_parent(CanvasPoint { x: 10.0, y: 0.0 }, Some(group)),
    );

    let mut lookups = NodeGraphLookups::default();
    lookups.rebuild_from(&g);

    assert_eq!(lookups.parent_for_node(a), Some(group));
    assert_eq!(lookups.parent_for_node(root), None);
    assert_eq!(lookups.child_nodes_for_parent(group), vec![b, a]);
    assert_eq!(
        lookups.child_nodes_by_parent().get(&group).cloned(),
        Some(vec![b, a])
    );
    assert_eq!(lookups.root_nodes(), vec![root]);
    assert!(
        lookups
            .child_nodes_for_parent(GroupId::from_u128(999))
            .is_empty()
    );

    g.nodes.get_mut(&root).expect("root").parent = Some(group);
    let tx = GraphTransaction::from_ops([GraphOp::SetNodeParent {
        id: root,
        from: None,
        to: Some(group),
    }]);
    lookups.apply_transaction(&g, &tx);

    assert_eq!(lookups.parent_for_node(root), Some(group));
    assert_eq!(lookups.child_nodes_for_parent(group), vec![b, root, a]);
    assert!(lookups.root_nodes().is_empty());
}
