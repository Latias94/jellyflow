use super::*;

#[test]
fn mutation_planner_add_node_with_ports_preserves_port_order_and_undo() {
    let mut graph = Graph::default();
    let before = graph.clone();
    let node_id = NodeId::new();
    let out = PortId::new();
    let inn = PortId::new();

    let tx = GraphMutationPlanner::new(&graph)
        .add_node_with_ports_tx(
            node_id,
            make_node("core.a"),
            vec![
                (out, make_port(node_id, "out", PortDirection::Out)),
                (inn, make_port(node_id, "in", PortDirection::In)),
            ],
            "Add Node",
        )
        .expect("tx");

    assert_eq!(tx.ops().len(), 4);
    assert!(matches!(tx.ops()[0], GraphOp::AddNode { .. }));
    assert!(matches!(tx.ops()[1], GraphOp::AddPort { id, .. } if id == out));
    assert!(matches!(tx.ops()[2], GraphOp::AddPort { id, .. } if id == inn));
    assert!(matches!(
        &tx.ops()[3],
        GraphOp::SetNodePorts { id, from, to } if *id == node_id && from.is_empty() && to == &vec![out, inn]
    ));

    apply_transaction(&mut graph, &tx).expect("apply");
    assert_eq!(graph.nodes().get(&node_id).unwrap().ports, vec![out, inn]);
    assert!(graph.ports().contains_key(&out));
    assert!(graph.ports().contains_key(&inn));

    let undo = invert_transaction(&tx);
    apply_transaction(&mut graph, &undo).expect("undo");
    assert_eq!(
        serde_json::to_value(&graph).unwrap(),
        serde_json::to_value(&before).unwrap()
    );
}

#[test]
fn mutation_planner_add_port_updates_node_ports_at_requested_index() {
    let mut graph = Graph::default();
    let node_id = NodeId::new();
    let existing = PortId::new();
    let inserted = PortId::new();

    insert_node(&mut graph, node_id, "core.a");
    insert_port(&mut graph, existing, node_id, "out", PortDirection::Out);

    let before = graph.clone();
    let tx = GraphMutationPlanner::new(&graph)
        .add_port_tx(
            inserted,
            make_port(node_id, "in", PortDirection::In),
            PortInsert::At(0),
            "Add Port",
        )
        .expect("tx");

    apply_transaction(&mut graph, &tx).expect("apply");
    assert_eq!(
        graph.nodes().get(&node_id).unwrap().ports,
        vec![inserted, existing]
    );
    assert!(graph.ports().contains_key(&inserted));

    let undo = invert_transaction(&tx);
    apply_transaction(&mut graph, &undo).expect("undo");
    assert_eq!(
        serde_json::to_value(&graph).unwrap(),
        serde_json::to_value(&before).unwrap()
    );
}

#[test]
fn mutation_planner_remove_port_tx_roundtrips_through_inverse() {
    let mut graph = Graph::default();
    let ids = insert_connected_pair(&mut graph);

    let before = graph.clone();
    let tx = GraphMutationPlanner::new(&graph)
        .remove_port_tx(ids.out, "Remove Port")
        .expect("tx");

    apply_transaction(&mut graph, &tx).expect("apply");
    assert!(!graph.ports().contains_key(&ids.out));
    assert!(!graph.edges().contains_key(&ids.edge));
    assert!(!graph.nodes().get(&ids.a).unwrap().ports.contains(&ids.out));

    let undo = invert_transaction(&tx);
    apply_transaction(&mut graph, &undo).expect("undo");
    assert_eq!(
        serde_json::to_value(&graph).unwrap(),
        serde_json::to_value(&before).unwrap()
    );
}

#[test]
fn mutation_planner_rejects_port_owner_mismatch_before_emitting_ops() {
    let graph = Graph::default();
    let node_id = NodeId::new();
    let other_node = NodeId::new();
    let port_id = PortId::new();

    let err = GraphMutationPlanner::new(&graph)
        .add_node_with_ports_ops(
            node_id,
            make_node("core.a"),
            vec![(port_id, make_port(other_node, "out", PortDirection::Out))],
        )
        .expect_err("owner mismatch");

    assert!(matches!(
        err,
        GraphMutationError::PortOwnerMismatch { port, expected, got }
            if port == port_id && expected == node_id && got == other_node
    ));
}

#[test]
fn mutation_planner_connect_and_disconnect_edges() {
    let mut graph = Graph::default();
    let a = NodeId::new();
    let b = NodeId::new();
    insert_node(&mut graph, a, "core.a");
    insert_node(&mut graph, b, "core.b");

    let out = PortId::new();
    let inn = PortId::new();
    insert_port(&mut graph, out, a, "out", PortDirection::Out);
    insert_port(&mut graph, inn, b, "in", PortDirection::In);

    let edge_id = EdgeId::new();
    let connect = GraphMutationPlanner::new(&graph)
        .add_edge_tx(edge_id, make_edge(out, inn), "Connect")
        .expect("connect tx");

    apply_transaction(&mut graph, &connect).expect("connect apply");
    assert!(graph.edges().contains_key(&edge_id));

    let disconnect_ops = GraphMutationPlanner::new(&graph)
        .disconnect_port_ops(inn)
        .expect("disconnect ops");
    assert_eq!(disconnect_ops.len(), 1);

    let disconnect = GraphTransaction::from_ops(disconnect_ops).with_label("Disconnect");
    apply_transaction(&mut graph, &disconnect).expect("disconnect apply");
    assert!(graph.edges().is_empty());
}

#[test]
fn mutation_planner_remove_node_tx_captures_ports_and_edges() {
    let mut graph = Graph::default();
    let ids = insert_connected_pair(&mut graph);

    let tx = GraphMutationPlanner::new(&graph)
        .remove_node_tx(ids.a, "Delete Node A")
        .expect("tx");

    assert!(matches!(
        &tx.ops()[0],
        GraphOp::RemoveNode { id, ports, edges, .. }
            if *id == ids.a
                && ports.iter().any(|(id, _)| *id == ids.out)
                && edges.iter().any(|(id, _)| *id == ids.edge)
    ));

    apply_transaction(&mut graph, &tx).expect("apply");
    assert!(!graph.nodes().contains_key(&ids.a));
    assert!(!graph.ports().contains_key(&ids.out));
    assert!(!graph.edges().contains_key(&ids.edge));
}

#[test]
fn mutation_batch_planner_allows_edges_to_staged_ports() {
    let mut graph = Graph::default();
    let a = NodeId::new();
    let b = NodeId::new();
    insert_node(&mut graph, a, "core.a");
    insert_node(&mut graph, b, "core.b");

    let out = PortId::new();
    let inn = PortId::new();
    insert_port(&mut graph, out, a, "out", PortDirection::Out);
    insert_port(&mut graph, inn, b, "in", PortDirection::In);

    let inserted = NodeId::new();
    let inserted_in = PortId::new();
    let inserted_out = PortId::new();
    let edge_a = EdgeId::new();
    let edge_b = EdgeId::new();

    let mut batch = GraphMutationBatchPlanner::new(&graph);
    batch
        .add_node_with_ports(
            inserted,
            make_node("core.convert"),
            vec![
                (inserted_in, make_port(inserted, "in", PortDirection::In)),
                (inserted_out, make_port(inserted, "out", PortDirection::Out)),
            ],
        )
        .expect("add staged node");
    batch
        .add_edge(edge_a, make_edge(out, inserted_in))
        .expect("add first edge");
    batch
        .add_edge(edge_b, make_edge(inserted_out, inn))
        .expect("add second edge");

    let tx = GraphTransaction::from_ops(batch.into_ops());
    apply_transaction(&mut graph, &tx).expect("apply");

    assert_eq!(
        graph.nodes().get(&inserted).unwrap().ports,
        vec![inserted_in, inserted_out]
    );
    assert_eq!(graph.edges().get(&edge_a).unwrap().to, inserted_in);
    assert_eq!(graph.edges().get(&edge_b).unwrap().from, inserted_out);
}

#[test]
fn mutation_batch_planner_rejects_edge_to_unknown_port() {
    let mut graph = Graph::default();
    let node = NodeId::new();
    insert_node(&mut graph, node, "core.a");

    let out = PortId::new();
    insert_port(&mut graph, out, node, "out", PortDirection::Out);

    let missing = PortId::new();
    let err = GraphMutationBatchPlanner::new(&graph)
        .add_edge(EdgeId::new(), make_edge(out, missing))
        .expect_err("missing port");

    assert!(matches!(err, GraphMutationError::MissingPort(id) if id == missing));
}

#[test]
fn mutation_batch_planner_set_edge_endpoints_can_target_staged_port() {
    let mut graph = Graph::default();
    let ids = insert_connected_pair(&mut graph);

    let inserted = NodeId::new();
    let inserted_in = PortId::new();
    let inserted_out = PortId::new();

    let mut batch = GraphMutationBatchPlanner::new(&graph);
    batch
        .add_node_with_ports(
            inserted,
            make_node("core.reroute"),
            vec![
                (inserted_in, make_port(inserted, "in", PortDirection::In)),
                (inserted_out, make_port(inserted, "out", PortDirection::Out)),
            ],
        )
        .expect("add staged node");
    batch
        .set_edge_endpoints(
            ids.edge,
            EdgeEndpoints {
                from: ids.out,
                to: inserted_in,
            },
        )
        .expect("set endpoint");

    let tx = GraphTransaction::from_ops(batch.into_ops());
    apply_transaction(&mut graph, &tx).expect("apply");

    assert_eq!(graph.edges().get(&ids.edge).unwrap().to, inserted_in);
}

#[test]
fn build_remove_node_tx_captures_ports_and_edges() {
    let mut graph = Graph::default();
    let ids = insert_connected_pair(&mut graph);

    let tx = graph
        .build_remove_node_tx(ids.a, "Delete Node A")
        .expect("tx");
    assert_eq!(tx.ops().len(), 1);

    apply_transaction(&mut graph, &tx).expect("apply");

    assert!(!graph.nodes().contains_key(&ids.a));
    assert!(!graph.ports().contains_key(&ids.out));
    assert!(!graph.edges().contains_key(&ids.edge));
}

#[test]
fn build_disconnect_port_ops_removes_incident_edges() {
    let mut graph = Graph::default();
    let ids = insert_connected_pair(&mut graph);

    let ops = graph
        .build_disconnect_port_ops(ids.inn)
        .expect("disconnect ops");
    assert_eq!(ops.len(), 1);

    let tx = GraphTransaction::from_ops(ops);
    apply_transaction(&mut graph, &tx).expect("apply");
    assert!(graph.edges().is_empty());
}
