use super::fixtures::make_graph;

use crate::runtime::lookups::{ConnectionSide, NodeGraphLookups};
use jellyflow_core::core::{
    CanvasPoint, CanvasRect, CanvasSize, EdgeKind, EdgeReconnectable, Graph, GraphId, Group,
    GroupId, Node, NodeId, NodeKindKey, PortKind,
};
use jellyflow_core::ops::{EdgeEndpoints, GraphOp, GraphTransaction};

fn node_with_parent(pos: CanvasPoint, parent: Option<GroupId>) -> Node {
    Node {
        kind: NodeKindKey::new("test.node"),
        kind_version: 1,
        pos,
        selectable: None,
        draggable: None,
        connectable: None,
        deletable: None,
        parent,
        extent: None,
        expand_parent: None,
        size: Some(CanvasSize {
            width: 10.0,
            height: 10.0,
        }),
        hidden: false,
        collapsed: false,
        ports: Vec::new(),
        data: serde_json::Value::Null,
    }
}

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

#[test]
fn lookups_rebuild_populates_connection_lookup() {
    let (g, a, b, out_port, in_port, eid) = make_graph();

    let mut lookups = NodeGraphLookups::default();
    lookups.rebuild_from(&g);

    assert!(lookups.node_lookup.contains_key(&a));
    assert!(lookups.node_lookup.contains_key(&b));
    assert_eq!(lookups.node_lookup.get(&a).unwrap().ports, vec![out_port]);
    assert_eq!(lookups.node_lookup.get(&b).unwrap().ports, vec![in_port]);

    assert_eq!(lookups.edge_lookup.get(&eid).unwrap().from, out_port);
    assert_eq!(lookups.edge_lookup.get(&eid).unwrap().to, in_port);

    let a_out = lookups
        .connections_for_port(a, ConnectionSide::Source, out_port)
        .expect("connections");
    assert_eq!(a_out.get(&eid).unwrap().target_node, b);

    let b_all = lookups.connections_for_node(b).expect("connections");
    assert!(b_all.contains_key(&eid));
}

#[test]
fn edge_position_lookup_exposes_connection_for_edge() {
    let (g, a, b, out_port, in_port, eid) = make_graph();

    let mut lookups = NodeGraphLookups::default();
    lookups.rebuild_from(&g);

    let conn = lookups.connection_for_edge(eid).expect("connection");
    assert_eq!(conn.source_node, a);
    assert_eq!(conn.source_port, out_port);
    assert_eq!(conn.target_node, b);
    assert_eq!(conn.target_port, in_port);
}

#[test]
fn lookups_connections_for_node_side_filters_by_direction() {
    let (g, a, b, out_port, in_port, eid) = make_graph();

    let mut lookups = NodeGraphLookups::default();
    lookups.rebuild_from(&g);

    let a_source = lookups
        .connections_for_node_side(a, ConnectionSide::Source)
        .expect("connections");
    assert!(a_source.contains_key(&eid));

    let a_target = lookups.connections_for_node_side(a, ConnectionSide::Target);
    assert!(a_target.is_none() || !a_target.unwrap().contains_key(&eid));

    let b_target = lookups
        .connections_for_node_side(b, ConnectionSide::Target)
        .expect("connections");
    assert!(b_target.contains_key(&eid));

    let b_source = lookups.connections_for_node_side(b, ConnectionSide::Source);
    assert!(b_source.is_none() || !b_source.unwrap().contains_key(&eid));

    let _ = (out_port, in_port);
}

#[test]
fn lookups_apply_edge_reconnectable_recovers_missing_edge_lookup_with_connections() {
    let (mut g, a, b, out_port, in_port, eid) = make_graph();
    let reconnectable = Some(EdgeReconnectable::Bool(false));
    g.edges.get_mut(&eid).unwrap().reconnectable = reconnectable;
    let tx = GraphTransaction::from_ops([GraphOp::SetEdgeReconnectable {
        id: eid,
        from: None,
        to: reconnectable,
    }]);

    let mut lookups = NodeGraphLookups::default();
    lookups.apply_transaction(&g, &tx);

    assert_eq!(
        lookups.edge_lookup.get(&eid).unwrap().reconnectable,
        reconnectable
    );
    assert!(
        lookups
            .connections_for_port(a, ConnectionSide::Source, out_port)
            .expect("source connections")
            .contains_key(&eid)
    );
    assert!(
        lookups
            .connections_for_port(b, ConnectionSide::Target, in_port)
            .expect("target connections")
            .contains_key(&eid)
    );
}

#[test]
fn lookups_apply_edge_reconnectable_repairs_missing_connection_lookup() {
    let (mut g, a, b, out_port, in_port, eid) = make_graph();
    let reconnectable = Some(EdgeReconnectable::Bool(false));
    let mut lookups = NodeGraphLookups::default();
    lookups.rebuild_from(&g);
    lookups.connection_lookup.clear();

    g.edges.get_mut(&eid).unwrap().reconnectable = reconnectable;
    let tx = GraphTransaction::from_ops([GraphOp::SetEdgeReconnectable {
        id: eid,
        from: None,
        to: reconnectable,
    }]);
    lookups.apply_transaction(&g, &tx);

    assert_eq!(
        lookups.edge_lookup.get(&eid).unwrap().reconnectable,
        reconnectable
    );
    assert!(
        lookups
            .connections_for_port(a, ConnectionSide::Source, out_port)
            .expect("source connections")
            .contains_key(&eid)
    );
    assert!(
        lookups
            .connections_for_port(b, ConnectionSide::Target, in_port)
            .expect("target connections")
            .contains_key(&eid)
    );
}

#[test]
fn lookups_apply_edge_kind_recovers_missing_edge_lookup_with_connections() {
    let (mut g, a, b, out_port, in_port, eid) = make_graph();
    g.ports.get_mut(&out_port).unwrap().kind = PortKind::Exec;
    g.ports.get_mut(&in_port).unwrap().kind = PortKind::Exec;
    g.edges.get_mut(&eid).unwrap().kind = EdgeKind::Exec;
    let tx = GraphTransaction::from_ops([GraphOp::SetEdgeKind {
        id: eid,
        from: EdgeKind::Data,
        to: EdgeKind::Exec,
    }]);

    let mut lookups = NodeGraphLookups::default();
    lookups.apply_transaction(&g, &tx);

    assert_eq!(lookups.edge_lookup.get(&eid).unwrap().kind, EdgeKind::Exec);
    assert_eq!(
        lookups
            .connections_for_port(a, ConnectionSide::Source, out_port)
            .expect("source connections")
            .get(&eid)
            .unwrap()
            .kind,
        EdgeKind::Exec
    );
    assert_eq!(
        lookups
            .connections_for_port(b, ConnectionSide::Target, in_port)
            .expect("target connections")
            .get(&eid)
            .unwrap()
            .kind,
        EdgeKind::Exec
    );
}

#[test]
fn lookups_apply_edge_kind_repairs_missing_connection_lookup() {
    let (mut g, a, b, out_port, in_port, eid) = make_graph();
    g.ports.get_mut(&out_port).unwrap().kind = PortKind::Exec;
    g.ports.get_mut(&in_port).unwrap().kind = PortKind::Exec;
    let mut lookups = NodeGraphLookups::default();
    lookups.rebuild_from(&g);
    lookups.connection_lookup.clear();

    g.edges.get_mut(&eid).unwrap().kind = EdgeKind::Exec;
    let tx = GraphTransaction::from_ops([GraphOp::SetEdgeKind {
        id: eid,
        from: EdgeKind::Data,
        to: EdgeKind::Exec,
    }]);
    lookups.apply_transaction(&g, &tx);

    assert_eq!(lookups.edge_lookup.get(&eid).unwrap().kind, EdgeKind::Exec);
    assert!(
        lookups
            .connections_for_port(a, ConnectionSide::Source, out_port)
            .expect("source connections")
            .contains_key(&eid)
    );
    assert!(
        lookups
            .connections_for_port(b, ConnectionSide::Target, in_port)
            .expect("target connections")
            .contains_key(&eid)
    );
}

#[test]
fn lookups_apply_edge_endpoints_rebuilds_when_edge_is_missing() {
    let (mut g, _a, _b, out_port, in_port, eid) = make_graph();
    g.edges.remove(&eid);
    let endpoints = EdgeEndpoints::new(out_port, in_port);
    let tx = GraphTransaction::from_ops([GraphOp::SetEdgeEndpoints {
        id: eid,
        from: endpoints,
        to: endpoints,
    }]);

    let mut lookups = NodeGraphLookups::default();
    lookups.apply_transaction(&g, &tx);

    assert!(!lookups.edge_lookup.contains_key(&eid));
    assert!(
        lookups
            .connection_lookup
            .values()
            .all(|connections| !connections.contains_key(&eid))
    );
}

#[test]
fn lookups_apply_add_edge_rebuilds_when_edge_is_missing() {
    let (mut g, _a, _b, _out_port, _in_port, eid) = make_graph();
    let edge = g.edges.remove(&eid).unwrap();
    let tx = GraphTransaction::from_ops([GraphOp::AddEdge { id: eid, edge }]);

    let mut lookups = NodeGraphLookups::default();
    lookups.apply_transaction(&g, &tx);

    assert!(!lookups.edge_lookup.contains_key(&eid));
    assert!(
        lookups
            .connection_lookup
            .values()
            .all(|connections| !connections.contains_key(&eid))
    );
}
