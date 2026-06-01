use super::fixtures::{in_port, node_at, out_port};

use crate::runtime::lookups::NodeGraphLookups;
use crate::runtime::utils::{
    get_connected_edges, get_connected_edges_for_nodes, get_incomers, get_outgoers,
};
use jellyflow_core::core::{
    CanvasPoint, Edge, EdgeId, EdgeKind, Graph, GraphId, NodeId, Port, PortCapacity, PortDirection,
    PortId, PortKey, PortKind,
};

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
            hidden: false,
            selectable: None,
            focusable: None,
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
            hidden: false,
            selectable: None,
            focusable: None,
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
                    hidden: false,
                    selectable: None,
                    focusable: None,
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
                    hidden: false,
                    selectable: None,
                    focusable: None,
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
                    hidden: false,
                    selectable: None,
                    focusable: None,
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
                    hidden: false,
                    selectable: None,
                    focusable: None,
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
            hidden: false,
            selectable: None,
            focusable: None,
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
            hidden: false,
            selectable: None,
            focusable: None,
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
