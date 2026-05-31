use super::*;

#[test]
fn store_lookups_update_after_dispatch_transaction() {
    let (mut g, _a, _b, out_port, in_port, eid) = make_graph();
    g.edges.clear();

    let mut store = NodeGraphStore::new(g, NodeGraphViewState::default(), default_editor_config());
    assert!(store.lookups().edge_lookup.is_empty());

    let tx = GraphTransaction {
        label: None,
        ops: vec![GraphOp::AddEdge {
            id: eid,
            edge: Edge {
                kind: EdgeKind::Data,
                from: out_port,
                to: in_port,
                selectable: None,
                deletable: None,
                reconnectable: None,
            },
        }],
    };

    store.dispatch_transaction(&tx).expect("dispatch");
    assert!(store.lookups().edge_lookup.contains_key(&eid));
}

#[test]
fn store_lookups_update_node_hidden_after_dispatch_transaction() {
    let (g, a, _b, _out_port, _in_port, _eid) = make_graph();
    let mut store = NodeGraphStore::new(g, NodeGraphViewState::default(), default_editor_config());
    assert!(!store.lookups().node_lookup.get(&a).unwrap().hidden);

    let tx = GraphTransaction {
        label: None,
        ops: vec![GraphOp::SetNodeHidden {
            id: a,
            from: false,
            to: true,
        }],
    };

    store.dispatch_transaction(&tx).expect("dispatch");
    assert!(store.lookups().node_lookup.get(&a).unwrap().hidden);
}

#[test]
fn store_lookups_update_edge_reconnectable_after_dispatch_transaction() {
    let (g, _a, _b, _out_port, _in_port, eid) = make_graph();
    let mut store = NodeGraphStore::new(g, NodeGraphViewState::default(), default_editor_config());
    assert_eq!(
        store.lookups().edge_lookup.get(&eid).unwrap().reconnectable,
        None
    );

    let tx = GraphTransaction {
        label: None,
        ops: vec![GraphOp::SetEdgeReconnectable {
            id: eid,
            from: None,
            to: Some(EdgeReconnectable::Bool(false)),
        }],
    };

    store.dispatch_transaction(&tx).expect("dispatch");
    assert_eq!(
        store.lookups().edge_lookup.get(&eid).unwrap().reconnectable,
        Some(EdgeReconnectable::Bool(false))
    );
}

#[test]
fn store_lookups_update_edge_kind_in_connection_lookup_after_dispatch_transaction() {
    let (mut g, a, b, out_port, in_port, eid) = make_graph();
    g.ports.get_mut(&out_port).unwrap().kind = PortKind::Exec;
    g.ports.get_mut(&in_port).unwrap().kind = PortKind::Exec;
    let mut store = NodeGraphStore::new(g, NodeGraphViewState::default(), default_editor_config());

    let tx = GraphTransaction {
        label: None,
        ops: vec![GraphOp::SetEdgeKind {
            id: eid,
            from: EdgeKind::Data,
            to: EdgeKind::Exec,
        }],
    };

    store.dispatch_transaction(&tx).expect("dispatch");

    assert_eq!(
        store.lookups().edge_lookup.get(&eid).unwrap().kind,
        EdgeKind::Exec
    );
    assert_eq!(
        store
            .lookups()
            .connections_for_port(a, ConnectionSide::Source, out_port)
            .expect("source connections")
            .get(&eid)
            .unwrap()
            .kind,
        EdgeKind::Exec
    );
    assert_eq!(
        store
            .lookups()
            .connections_for_port(b, ConnectionSide::Target, in_port)
            .expect("target connections")
            .get(&eid)
            .unwrap()
            .kind,
        EdgeKind::Exec
    );
}

#[test]
fn store_lookups_remove_port_updates_node_ports_and_incident_edges() {
    let (g, a, _b, out_port, _in_port, eid) = make_graph();
    let port = g.ports.get(&out_port).expect("port").clone();
    let edge = g.edges.get(&eid).expect("edge").clone();
    let mut store = NodeGraphStore::new(g, NodeGraphViewState::default(), default_editor_config());

    assert!(
        store
            .lookups()
            .node_lookup
            .get(&a)
            .unwrap()
            .ports
            .contains(&out_port)
    );
    assert!(store.lookups().edge_lookup.contains_key(&eid));

    let tx = GraphTransaction {
        label: None,
        ops: vec![GraphOp::RemovePort {
            id: out_port,
            port,
            edges: vec![(eid, edge)],
        }],
    };

    store.dispatch_transaction(&tx).expect("dispatch");
    assert!(
        !store
            .lookups()
            .node_lookup
            .get(&a)
            .unwrap()
            .ports
            .contains(&out_port)
    );
    assert!(!store.lookups().edge_lookup.contains_key(&eid));
}

#[test]
fn store_lookups_remove_group_clears_detached_node_parent() {
    let (mut g, a, _b, _out_port, _in_port, _eid) = make_graph();
    let group_id = GroupId::new();
    let group = Group {
        title: "Group".to_string(),
        rect: CanvasRect {
            origin: CanvasPoint { x: -10.0, y: -10.0 },
            size: CanvasSize {
                width: 200.0,
                height: 100.0,
            },
        },
        color: None,
    };
    g.groups.insert(group_id, group.clone());
    g.nodes.get_mut(&a).expect("node").parent = Some(group_id);

    let mut store = NodeGraphStore::new(g, NodeGraphViewState::default(), default_editor_config());
    assert_eq!(
        store.lookups().node_lookup.get(&a).unwrap().parent,
        Some(group_id)
    );

    let tx = GraphTransaction {
        label: None,
        ops: vec![GraphOp::RemoveGroup {
            id: group_id,
            group,
            detached: vec![(a, Some(group_id))],
        }],
    };

    store.dispatch_transaction(&tx).expect("dispatch");
    assert_eq!(store.lookups().node_lookup.get(&a).unwrap().parent, None);
}
