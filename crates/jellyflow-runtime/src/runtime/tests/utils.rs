use crate::runtime::lookups::NodeGraphLookups;
use crate::runtime::utils::{
    GetNodesBoundsOptions, GetNodesInsideOptions, NodeInclusion, get_connected_edges,
    get_connected_edges_for_nodes, get_incomers, get_node_position_with_origin, get_node_rect,
    get_nodes_bounds, get_nodes_inside, get_outgoers,
};
use jellyflow_core::core::{
    CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeId, EdgeKind, Graph, GraphId, Node, NodeId,
    NodeKindKey, Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
};

fn node_at(pos: CanvasPoint, size: Option<CanvasSize>) -> Node {
    Node {
        kind: NodeKindKey::new("test.node"),
        kind_version: 1,
        pos,
        selectable: None,
        draggable: None,
        connectable: None,
        deletable: None,
        parent: None,
        extent: None,
        expand_parent: None,
        size,
        hidden: false,
        collapsed: false,
        ports: Vec::new(),
        data: serde_json::Value::Null,
    }
}

fn out_port(node: NodeId) -> (PortId, Port) {
    let pid = PortId::new();
    (
        pid,
        Port {
            node,
            key: PortKey::new("out"),
            dir: PortDirection::Out,
            kind: PortKind::Data,
            capacity: PortCapacity::Multi,
            connectable: None,
            connectable_start: None,
            connectable_end: None,
            ty: None,
            data: serde_json::Value::Null,
        },
    )
}

fn in_port(node: NodeId, key: &str) -> (PortId, Port) {
    let pid = PortId::new();
    (
        pid,
        Port {
            node,
            key: PortKey::new(key),
            dir: PortDirection::In,
            kind: PortKind::Data,
            capacity: PortCapacity::Single,
            connectable: None,
            connectable_start: None,
            connectable_end: None,
            ty: None,
            data: serde_json::Value::Null,
        },
    )
}

#[test]
fn outgoers_incomers_connected_edges_are_derived_from_connections() {
    let mut g = Graph::new(GraphId::from_u128(1));

    let a = NodeId::new();
    let b = NodeId::new();
    let c = NodeId::new();

    g.nodes
        .insert(a, node_at(CanvasPoint { x: 0.0, y: 0.0 }, None));
    g.nodes
        .insert(b, node_at(CanvasPoint { x: 0.0, y: 0.0 }, None));
    g.nodes
        .insert(c, node_at(CanvasPoint { x: 0.0, y: 0.0 }, None));

    let (a_out_id, a_out) = out_port(a);
    let (b_in_id, b_in) = in_port(b, "in0");
    let (c_in_id, c_in) = in_port(c, "in0");
    g.ports.insert(a_out_id, a_out);
    g.ports.insert(b_in_id, b_in);
    g.ports.insert(c_in_id, c_in);

    let e1 = EdgeId::new();
    let e2 = EdgeId::new();
    g.edges.insert(
        e1,
        Edge {
            kind: EdgeKind::Data,
            from: a_out_id,
            to: b_in_id,
            selectable: None,
            deletable: None,
            reconnectable: None,
        },
    );
    g.edges.insert(
        e2,
        Edge {
            kind: EdgeKind::Data,
            from: a_out_id,
            to: c_in_id,
            selectable: None,
            deletable: None,
            reconnectable: None,
        },
    );

    let mut lookups = NodeGraphLookups::default();
    lookups.rebuild_from(&g);

    let mut expected_outgoers = vec![b, c];
    expected_outgoers.sort();
    assert_eq!(get_outgoers(&lookups, a), expected_outgoers);
    assert_eq!(get_incomers(&lookups, b), vec![a]);
    assert_eq!(get_incomers(&lookups, c), vec![a]);

    let connected = get_connected_edges(&lookups, a);
    assert_eq!(connected.len(), 2);
    assert!(connected.contains(&e1));
    assert!(connected.contains(&e2));

    let connected_for_nodes = get_connected_edges_for_nodes(&lookups, [b, c]);
    let mut expected = vec![e1, e2];
    expected.sort();
    assert_eq!(connected_for_nodes, expected);
}

#[test]
fn helpers_are_deterministic_under_insertion_order_variance() {
    fn build_graph(insert_a_first: bool) -> (Graph, NodeId, NodeId, NodeId, EdgeId, EdgeId) {
        let mut g = Graph::new(GraphId::from_u128(1));

        let a = NodeId(uuid::Uuid::from_u128(1));
        let b = NodeId(uuid::Uuid::from_u128(2));
        let c = NodeId(uuid::Uuid::from_u128(3));

        let nodes = [
            (a, node_at(CanvasPoint { x: 0.0, y: 0.0 }, None)),
            (b, node_at(CanvasPoint { x: 0.0, y: 0.0 }, None)),
            (c, node_at(CanvasPoint { x: 0.0, y: 0.0 }, None)),
        ];
        if insert_a_first {
            for (id, node) in nodes {
                g.nodes.insert(id, node);
            }
        } else {
            for (id, node) in nodes.into_iter().rev() {
                g.nodes.insert(id, node);
            }
        }

        let a_out_id = PortId(uuid::Uuid::from_u128(10));
        let b_in_id = PortId(uuid::Uuid::from_u128(11));
        let c_in_id = PortId(uuid::Uuid::from_u128(12));
        let a_out = Port {
            node: a,
            key: PortKey::new("out"),
            dir: PortDirection::Out,
            kind: PortKind::Data,
            capacity: PortCapacity::Multi,
            connectable: None,
            connectable_start: None,
            connectable_end: None,
            ty: None,
            data: serde_json::Value::Null,
        };
        let b_in = Port {
            node: b,
            key: PortKey::new("in0"),
            dir: PortDirection::In,
            kind: PortKind::Data,
            capacity: PortCapacity::Single,
            connectable: None,
            connectable_start: None,
            connectable_end: None,
            ty: None,
            data: serde_json::Value::Null,
        };
        let c_in = Port {
            node: c,
            key: PortKey::new("in0"),
            dir: PortDirection::In,
            kind: PortKind::Data,
            capacity: PortCapacity::Single,
            connectable: None,
            connectable_start: None,
            connectable_end: None,
            ty: None,
            data: serde_json::Value::Null,
        };

        if insert_a_first {
            g.ports.insert(a_out_id, a_out);
            g.ports.insert(b_in_id, b_in);
            g.ports.insert(c_in_id, c_in);
        } else {
            g.ports.insert(c_in_id, c_in);
            g.ports.insert(b_in_id, b_in);
            g.ports.insert(a_out_id, a_out);
        }

        let e1 = EdgeId(uuid::Uuid::from_u128(20));
        let e2 = EdgeId(uuid::Uuid::from_u128(21));
        if insert_a_first {
            g.edges.insert(
                e1,
                Edge {
                    kind: EdgeKind::Data,
                    from: a_out_id,
                    to: b_in_id,
                    selectable: None,
                    deletable: None,
                    reconnectable: None,
                },
            );
            g.edges.insert(
                e2,
                Edge {
                    kind: EdgeKind::Data,
                    from: a_out_id,
                    to: c_in_id,
                    selectable: None,
                    deletable: None,
                    reconnectable: None,
                },
            );
        } else {
            g.edges.insert(
                e2,
                Edge {
                    kind: EdgeKind::Data,
                    from: a_out_id,
                    to: c_in_id,
                    selectable: None,
                    deletable: None,
                    reconnectable: None,
                },
            );
            g.edges.insert(
                e1,
                Edge {
                    kind: EdgeKind::Data,
                    from: a_out_id,
                    to: b_in_id,
                    selectable: None,
                    deletable: None,
                    reconnectable: None,
                },
            );
        }

        (g, a, b, c, e1, e2)
    }

    let (g1, a1, b1, c1, e11, e21) = build_graph(true);
    let (g2, a2, b2, c2, e12, e22) = build_graph(false);
    assert_eq!((a1, b1, c1, e11, e21), (a2, b2, c2, e12, e22));

    let mut l1 = NodeGraphLookups::default();
    l1.rebuild_from(&g1);
    let mut l2 = NodeGraphLookups::default();
    l2.rebuild_from(&g2);

    assert_eq!(get_outgoers(&l1, a1), get_outgoers(&l2, a2));
    assert_eq!(get_incomers(&l1, b1), get_incomers(&l2, b2));
    assert_eq!(get_incomers(&l1, c1), get_incomers(&l2, c2));

    let mut expected_edges = vec![e11, e21];
    expected_edges.sort();
    let mut actual_edges_1 = get_connected_edges(&l1, a1);
    actual_edges_1.sort();
    let mut actual_edges_2 = get_connected_edges(&l2, a2);
    actual_edges_2.sort();
    assert_eq!(actual_edges_1, expected_edges);
    assert_eq!(actual_edges_2, expected_edges);
}

#[test]
fn outgoers_and_incomers_include_self_for_self_loops_and_dedup() {
    let mut g = Graph::new(GraphId::from_u128(1));

    let a = NodeId::new();
    let (a_out_id, a_out) = out_port(a);
    let (a_in_id, a_in) = in_port(a, "in0");

    g.nodes
        .insert(a, node_at(CanvasPoint { x: 0.0, y: 0.0 }, None));
    g.ports.insert(a_out_id, a_out);
    g.ports.insert(a_in_id, a_in);

    // Two self-loop edges should still dedup to a single node in outgoers/incomers.
    let e1 = EdgeId::new();
    let e2 = EdgeId::new();
    g.edges.insert(
        e1,
        Edge {
            kind: EdgeKind::Data,
            from: a_out_id,
            to: a_in_id,
            selectable: None,
            deletable: None,
            reconnectable: None,
        },
    );
    g.edges.insert(
        e2,
        Edge {
            kind: EdgeKind::Data,
            from: a_out_id,
            to: a_in_id,
            selectable: None,
            deletable: None,
            reconnectable: None,
        },
    );

    let mut lookups = NodeGraphLookups::default();
    lookups.rebuild_from(&g);

    assert_eq!(get_outgoers(&lookups, a), vec![a]);
    assert_eq!(get_incomers(&lookups, a), vec![a]);

    let connected = get_connected_edges(&lookups, a);
    assert_eq!(connected.len(), 2);
    assert!(connected.contains(&e1));
    assert!(connected.contains(&e2));
}

#[test]
fn get_node_position_with_origin_matches_bounds_top_left() {
    let mut g = Graph::new(GraphId::from_u128(1));
    let a = NodeId::new();
    g.nodes.insert(
        a,
        node_at(
            CanvasPoint { x: 20.0, y: 10.0 },
            Some(CanvasSize {
                width: 10.0,
                height: 6.0,
            }),
        ),
    );

    let mut lookups = NodeGraphLookups::default();
    lookups.rebuild_from(&g);

    let p = get_node_position_with_origin(&lookups, a, (0.5, 0.5), None).expect("pos");
    assert!((p.x - 15.0).abs() <= 1.0e-6);
    assert!((p.y - 7.0).abs() <= 1.0e-6);
}

#[test]
fn get_node_rect_is_consistent_with_get_nodes_bounds_singleton() {
    let mut g = Graph::new(GraphId::from_u128(1));
    let a = NodeId::new();
    g.nodes.insert(
        a,
        node_at(
            CanvasPoint { x: 20.0, y: 10.0 },
            Some(CanvasSize {
                width: 10.0,
                height: 6.0,
            }),
        ),
    );

    let mut lookups = NodeGraphLookups::default();
    lookups.rebuild_from(&g);

    let rect = get_node_rect(&lookups, a, (0.5, 0.5), None).expect("rect");
    let bounds = get_nodes_bounds(
        &lookups,
        [a],
        GetNodesBoundsOptions {
            node_origin: (0.5, 0.5),
            include_hidden: true,
            fallback_size: None,
        },
    )
    .expect("bounds");

    assert!((rect.origin.x - bounds.origin.x).abs() <= 1.0e-6);
    assert!((rect.origin.y - bounds.origin.y).abs() <= 1.0e-6);
    assert!((rect.size.width - bounds.size.width).abs() <= 1.0e-6);
    assert!((rect.size.height - bounds.size.height).abs() <= 1.0e-6);
}

#[test]
fn get_nodes_bounds_respects_node_origin() {
    let mut g = Graph::new(GraphId::from_u128(1));
    let a = NodeId::new();
    let b = NodeId::new();

    g.nodes.insert(
        a,
        node_at(
            CanvasPoint { x: 0.0, y: 0.0 },
            Some(CanvasSize {
                width: 10.0,
                height: 10.0,
            }),
        ),
    );
    g.nodes.insert(
        b,
        node_at(
            CanvasPoint { x: 20.0, y: 5.0 },
            Some(CanvasSize {
                width: 5.0,
                height: 5.0,
            }),
        ),
    );

    let mut lookups = NodeGraphLookups::default();
    lookups.rebuild_from(&g);

    let bounds_top_left = get_nodes_bounds(
        &lookups,
        [a, b],
        GetNodesBoundsOptions {
            node_origin: (0.0, 0.0),
            include_hidden: true,
            fallback_size: None,
        },
    )
    .expect("bounds");
    assert!((bounds_top_left.origin.x - 0.0).abs() <= 1.0e-6);
    assert!((bounds_top_left.origin.y - 0.0).abs() <= 1.0e-6);
    assert!((bounds_top_left.size.width - 25.0).abs() <= 1.0e-6);
    assert!((bounds_top_left.size.height - 10.0).abs() <= 1.0e-6);

    let bounds_center = get_nodes_bounds(
        &lookups,
        [a, b],
        GetNodesBoundsOptions {
            node_origin: (0.5, 0.5),
            include_hidden: true,
            fallback_size: None,
        },
    )
    .expect("bounds");
    assert!((bounds_center.origin.x - (-5.0)).abs() <= 1.0e-6);
    assert!((bounds_center.origin.y - (-5.0)).abs() <= 1.0e-6);
    assert!((bounds_center.size.width - 27.5).abs() <= 1.0e-6);
    assert!((bounds_center.size.height - 12.5).abs() <= 1.0e-6);
}

#[test]
fn get_nodes_bounds_uses_fallback_size_for_unsized_nodes() {
    let mut g = Graph::new(GraphId::from_u128(1));
    let a = NodeId::new();

    g.nodes
        .insert(a, node_at(CanvasPoint { x: 5.0, y: 7.0 }, None));

    let mut lookups = NodeGraphLookups::default();
    lookups.rebuild_from(&g);

    assert!(
        get_nodes_bounds(&lookups, [a], GetNodesBoundsOptions::default()).is_none(),
        "unsized nodes should not contribute without fallback dimensions"
    );

    let bounds = get_nodes_bounds(
        &lookups,
        [a],
        GetNodesBoundsOptions {
            fallback_size: Some(CanvasSize {
                width: 10.0,
                height: 20.0,
            }),
            ..GetNodesBoundsOptions::default()
        },
    )
    .expect("fallback bounds");

    assert!((bounds.origin.x - 5.0).abs() <= 1.0e-6);
    assert!((bounds.origin.y - 7.0).abs() <= 1.0e-6);
    assert!((bounds.size.width - 10.0).abs() <= 1.0e-6);
    assert!((bounds.size.height - 20.0).abs() <= 1.0e-6);
}

#[test]
fn get_nodes_bounds_skips_hidden_nodes_unless_included() {
    let mut g = Graph::new(GraphId::from_u128(1));
    let a = NodeId::new();
    let b = NodeId::new();

    g.nodes.insert(
        a,
        node_at(
            CanvasPoint { x: 0.0, y: 0.0 },
            Some(CanvasSize {
                width: 10.0,
                height: 10.0,
            }),
        ),
    );

    let mut hidden = node_at(
        CanvasPoint { x: 100.0, y: 0.0 },
        Some(CanvasSize {
            width: 10.0,
            height: 10.0,
        }),
    );
    hidden.hidden = true;
    g.nodes.insert(b, hidden);

    let mut lookups = NodeGraphLookups::default();
    lookups.rebuild_from(&g);

    let visible_bounds =
        get_nodes_bounds(&lookups, [a, b], GetNodesBoundsOptions::default()).expect("bounds");
    assert!((visible_bounds.origin.x - 0.0).abs() <= 1.0e-6);
    assert!((visible_bounds.size.width - 10.0).abs() <= 1.0e-6);

    let all_bounds = get_nodes_bounds(
        &lookups,
        [a, b],
        GetNodesBoundsOptions {
            include_hidden: true,
            ..GetNodesBoundsOptions::default()
        },
    )
    .expect("bounds");
    assert!((all_bounds.origin.x - 0.0).abs() <= 1.0e-6);
    assert!((all_bounds.size.width - 110.0).abs() <= 1.0e-6);
}

#[test]
fn get_nodes_inside_supports_partial_vs_full_inclusion() {
    let mut g = Graph::new(GraphId::from_u128(1));
    let a = NodeId::new();
    let b = NodeId::new();

    g.nodes.insert(
        a,
        node_at(
            CanvasPoint { x: 0.0, y: 0.0 },
            Some(CanvasSize {
                width: 10.0,
                height: 10.0,
            }),
        ),
    );
    g.nodes.insert(
        b,
        node_at(
            CanvasPoint { x: 9.0, y: 9.0 },
            Some(CanvasSize {
                width: 5.0,
                height: 5.0,
            }),
        ),
    );

    let mut lookups = NodeGraphLookups::default();
    lookups.rebuild_from(&g);

    let rect = CanvasRect {
        origin: CanvasPoint { x: 0.0, y: 0.0 },
        size: CanvasSize {
            width: 10.0,
            height: 10.0,
        },
    };

    let partial = get_nodes_inside(
        &lookups,
        rect,
        GetNodesInsideOptions {
            inclusion: NodeInclusion::Partial,
            node_origin: (0.0, 0.0),
            include_hidden: true,
            fallback_size: None,
        },
    );
    let mut expected = vec![a, b];
    expected.sort();
    assert_eq!(partial, expected);

    let full = get_nodes_inside(
        &lookups,
        rect,
        GetNodesInsideOptions {
            inclusion: NodeInclusion::Full,
            node_origin: (0.0, 0.0),
            include_hidden: true,
            fallback_size: None,
        },
    );
    assert_eq!(full, vec![a]);
}

#[test]
fn get_nodes_inside_linear_fallback_sorts_results() {
    let mut g = Graph::new(GraphId::from_u128(1));
    let high = NodeId::from_u128(30);
    let low = NodeId::from_u128(10);

    g.nodes.insert(
        high,
        node_at(
            CanvasPoint { x: 5.0, y: 5.0 },
            Some(CanvasSize {
                width: 10.0,
                height: 10.0,
            }),
        ),
    );
    g.nodes.insert(
        low,
        node_at(
            CanvasPoint { x: 15.0, y: 5.0 },
            Some(CanvasSize {
                width: 10.0,
                height: 10.0,
            }),
        ),
    );

    let mut lookups = NodeGraphLookups::default();
    lookups.rebuild_from(&g);

    let found = get_nodes_inside(
        &lookups,
        CanvasRect {
            origin: CanvasPoint { x: 0.0, y: 0.0 },
            size: CanvasSize {
                width: 40.0,
                height: 40.0,
            },
        },
        GetNodesInsideOptions {
            inclusion: NodeInclusion::Partial,
            node_origin: (0.0, 0.0),
            include_hidden: true,
            fallback_size: None,
        },
    );

    assert_eq!(found, vec![low, high]);
}

#[test]
fn get_nodes_inside_applies_hidden_and_fallback_size_policies() {
    let mut g = Graph::new(GraphId::from_u128(1));
    let visible = NodeId::from_u128(1);
    let hidden_id = NodeId::from_u128(2);
    let unsized_node = NodeId::from_u128(3);

    g.nodes.insert(
        visible,
        node_at(
            CanvasPoint { x: 0.0, y: 0.0 },
            Some(CanvasSize {
                width: 10.0,
                height: 10.0,
            }),
        ),
    );

    let mut hidden = node_at(
        CanvasPoint { x: 20.0, y: 0.0 },
        Some(CanvasSize {
            width: 10.0,
            height: 10.0,
        }),
    );
    hidden.hidden = true;
    g.nodes.insert(hidden_id, hidden);
    g.nodes
        .insert(unsized_node, node_at(CanvasPoint { x: 40.0, y: 0.0 }, None));

    let mut lookups = NodeGraphLookups::default();
    lookups.rebuild_from(&g);

    let rect = CanvasRect {
        origin: CanvasPoint { x: 0.0, y: 0.0 },
        size: CanvasSize {
            width: 80.0,
            height: 40.0,
        },
    };

    assert_eq!(
        get_nodes_inside(&lookups, rect, GetNodesInsideOptions::default()),
        vec![visible]
    );

    assert_eq!(
        get_nodes_inside(
            &lookups,
            rect,
            GetNodesInsideOptions {
                include_hidden: true,
                ..GetNodesInsideOptions::default()
            },
        ),
        vec![visible, hidden_id]
    );

    assert_eq!(
        get_nodes_inside(
            &lookups,
            rect,
            GetNodesInsideOptions {
                include_hidden: true,
                fallback_size: Some(CanvasSize {
                    width: 10.0,
                    height: 10.0,
                }),
                ..GetNodesInsideOptions::default()
            },
        ),
        vec![visible, hidden_id, unsized_node]
    );
}

#[test]
fn get_nodes_inside_rejects_non_finite_query_rect() {
    let mut g = Graph::new(GraphId::from_u128(1));
    let a = NodeId::new();
    g.nodes.insert(
        a,
        node_at(
            CanvasPoint { x: 0.0, y: 0.0 },
            Some(CanvasSize {
                width: 10.0,
                height: 10.0,
            }),
        ),
    );

    let mut lookups = NodeGraphLookups::default();
    lookups.rebuild_from(&g);

    let found = get_nodes_inside(
        &lookups,
        CanvasRect {
            origin: CanvasPoint {
                x: f32::INFINITY,
                y: 0.0,
            },
            size: CanvasSize {
                width: 10.0,
                height: 10.0,
            },
        },
        GetNodesInsideOptions {
            inclusion: NodeInclusion::Partial,
            node_origin: (0.0, 0.0),
            include_hidden: true,
            fallback_size: None,
        },
    );

    assert!(found.is_empty());
}
