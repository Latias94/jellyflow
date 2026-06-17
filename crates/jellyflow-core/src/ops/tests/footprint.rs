use super::*;

#[test]
fn node_position_footprint_records_touched_node() {
    let node = NodeId::from_u128(1);
    let tx = GraphTransaction::from_ops([GraphOp::SetNodePos {
        id: node,
        from: CanvasPoint { x: 0.0, y: 0.0 },
        to: CanvasPoint { x: 10.0, y: 20.0 },
    }]);

    let footprint = tx.footprint();

    assert_eq!(footprint.nodes, [node].into_iter().collect());
    assert!(footprint.ports.is_empty());
    assert!(footprint.edges.is_empty());
}

#[test]
fn remove_node_footprint_records_cascaded_ports_edges_and_bindings() {
    let mut graph = Graph::default();
    let ids = insert_connected_pair(&mut graph);
    let binding = Binding::node_to_source(
        ids.a,
        "source.pdf",
        serde_json::json!({ "page": 1, "range": [2, 5] }),
    );
    let binding_id = BindingId::from_u128(99);
    graph.insert_binding(binding_id, binding);

    let tx = graph
        .build_remove_node_tx(ids.a, "remove node")
        .expect("remove node tx");
    let footprint = tx.footprint();

    assert!(footprint.nodes.contains(&ids.a));
    assert!(footprint.ports.contains(&ids.out));
    assert!(footprint.ports.contains(&ids.inn));
    assert!(footprint.edges.contains(&ids.edge));
    assert!(footprint.bindings.contains(&binding_id));
}

#[test]
fn set_edge_endpoints_footprint_records_old_and_new_ports() {
    let edge = EdgeId::from_u128(1);
    let old_from = PortId::from_u128(2);
    let old_to = PortId::from_u128(3);
    let new_from = PortId::from_u128(4);
    let new_to = PortId::from_u128(5);
    let op = GraphOp::SetEdgeEndpoints {
        id: edge,
        from: EdgeEndpoints::new(old_from, old_to),
        to: EdgeEndpoints::new(new_from, new_to),
    };

    let footprint = op.footprint();

    assert_eq!(footprint.edges, [edge].into_iter().collect());
    assert_eq!(
        footprint.ports,
        [old_from, old_to, new_from, new_to].into_iter().collect()
    );
}

#[test]
fn edge_data_and_view_footprint_records_touched_edge_only() {
    let edge = EdgeId::from_u128(1);
    let tx = GraphTransaction::from_ops([
        GraphOp::SetEdgeData {
            id: edge,
            from: serde_json::Value::Null,
            to: serde_json::json!({ "branch": "yes" }),
        },
        GraphOp::SetEdgeView {
            id: edge,
            from: EdgeViewDescriptor::default(),
            to: EdgeViewDescriptor {
                renderer_key: Some("branch-edge".to_string()),
                label: Some("Yes".to_string()),
                label_anchor: Some(EdgeLabelAnchor::Center),
                source_marker_key: None,
                target_marker_key: None,
                style_token: None,
                hit_target_width: None,
            },
        },
    ]);

    let footprint = tx.footprint();

    assert_eq!(footprint.edges, [edge].into_iter().collect());
    assert!(footprint.nodes.is_empty());
    assert!(footprint.ports.is_empty());
}

#[test]
fn set_node_parent_footprint_records_old_and_new_groups() {
    let node = NodeId::from_u128(1);
    let old_group = GroupId::from_u128(2);
    let new_group = GroupId::from_u128(3);
    let op = GraphOp::SetNodeParent {
        id: node,
        from: Some(old_group),
        to: Some(new_group),
    };

    let footprint = op.footprint();

    assert_eq!(footprint.nodes, [node].into_iter().collect());
    assert_eq!(
        footprint.groups,
        [old_group, new_group].into_iter().collect()
    );
}

#[test]
fn binding_endpoint_footprint_records_referenced_graph_local_targets() {
    let binding = BindingId::from_u128(1);
    let node = NodeId::from_u128(2);
    let edge = EdgeId::from_u128(3);
    let op = GraphOp::SetBindingSubject {
        id: binding,
        from: BindingEndpoint::node(node),
        to: BindingEndpoint::edge(edge),
    };

    let footprint = op.footprint();

    assert!(footprint.bindings.contains(&binding));
    assert!(footprint.nodes.contains(&node));
    assert!(footprint.edges.contains(&edge));
}
