use super::*;
use crate::runtime::tests::fixtures::fixture_remove_edge;

#[test]
fn lookups_apply_edge_reconnectable_recovers_missing_edge_lookup_with_connections() {
    let (mut g, a, b, out_port, in_port, eid) = make_graph();
    let reconnectable = Some(EdgeReconnectable::Bool(false));
    g.update_edge(&eid, |edge| edge.reconnectable = reconnectable)
        .expect("edge exists");
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

    g.update_edge(&eid, |edge| edge.reconnectable = reconnectable)
        .expect("edge exists");
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
    g.update_port(&out_port, |port| port.kind = PortKind::Exec)
        .expect("port exists");
    g.update_port(&in_port, |port| port.kind = PortKind::Exec)
        .expect("port exists");
    g.update_edge(&eid, |edge| edge.kind = EdgeKind::Exec)
        .expect("edge exists");
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
    g.update_port(&out_port, |port| port.kind = PortKind::Exec)
        .expect("port exists");
    g.update_port(&in_port, |port| port.kind = PortKind::Exec)
        .expect("port exists");
    let mut lookups = NodeGraphLookups::default();
    lookups.rebuild_from(&g);
    lookups.connection_lookup.clear();

    g.update_edge(&eid, |edge| edge.kind = EdgeKind::Exec)
        .expect("edge exists");
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
    fixture_remove_edge(&mut g, &eid);
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
    let edge = fixture_remove_edge(&mut g, &eid).unwrap();
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
