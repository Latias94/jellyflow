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
    from.insert_edge(edge_id, make_edge(out1, inn));

    let mut to = from.clone();
    to.edge_mut(&edge_id).unwrap().from = out2;

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

#[test]
fn graph_diff_roundtrips_when_edge_data_and_view_change() {
    let mut from = Graph::default();
    let ids = insert_connected_pair_with_ids(
        &mut from,
        ConnectedPairIds {
            a: NodeId::from_u128(1),
            b: NodeId::from_u128(2),
            out: PortId::from_u128(3),
            inn: PortId::from_u128(4),
            edge: EdgeId::from_u128(5),
        },
    );

    let mut to = from.clone();
    let edge = to.edge_mut(&ids.edge).expect("edge");
    edge.data = serde_json::json!({ "cardinality": "1:n" });
    edge.view = EdgeViewDescriptor {
        renderer_key: Some("erd-relation".to_string()),
        label: Some("owns".to_string()),
        label_anchor: Some(EdgeLabelAnchor::Center),
        source_marker_key: Some("one".to_string()),
        target_marker_key: Some("many".to_string()),
        style_token: Some("relation".to_string()),
        hit_target_width: Some(24.0),
    };

    let tx = graph_diff(&from, &to);
    assert!(
        tx.ops()
            .iter()
            .any(|op| matches!(op, GraphOp::SetEdgeData { id, .. } if *id == ids.edge)),
        "diff must emit edge data setter"
    );
    assert!(
        tx.ops()
            .iter()
            .any(|op| matches!(op, GraphOp::SetEdgeView { id, .. } if *id == ids.edge)),
        "diff must emit edge view setter"
    );

    let mut patched = from.clone();
    apply_transaction(&mut patched, &tx).expect("apply diff");
    assert_eq!(
        serde_json::to_value(&patched).unwrap(),
        serde_json::to_value(&to).unwrap(),
        "diff must roundtrip"
    );
}
