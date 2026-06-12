use super::*;

#[test]
fn changes_from_transaction_reports_cascaded_edge_removals() {
    let (g, a, _b, out_port, _in_port, eid) = make_graph();
    let node = g.nodes.get(&a).expect("node").clone();
    let port = g.ports.get(&out_port).expect("port").clone();
    let edge = g.edges.get(&eid).expect("edge").clone();

    let remove_node_tx = GraphTransaction::from_ops([GraphOp::RemoveNode {
        id: a,
        node,
        ports: vec![(out_port, port.clone())],
        edges: vec![(eid, edge.clone())],
        bindings: Vec::new(),
    }]);
    let remove_node_changes = NodeGraphChanges::from_transaction(&remove_node_tx);
    assert!(
        remove_node_changes
            .edges()
            .iter()
            .any(|change| matches!(change, EdgeChange::Remove { id } if *id == eid))
    );

    let remove_port_tx = GraphTransaction::from_ops([GraphOp::RemovePort {
        id: out_port,
        port,
        edges: vec![(eid, edge)],
        bindings: Vec::new(),
    }]);
    let remove_port_changes = NodeGraphChanges::from_transaction(&remove_port_tx);
    assert!(
        remove_port_changes
            .edges()
            .iter()
            .any(|change| matches!(change, EdgeChange::Remove { id } if *id == eid))
    );
}

#[test]
fn changes_from_transaction_deduplicates_repeated_edge_removes() {
    let (g, _a, _b, out_port, _in_port, eid) = make_graph();
    let port = g.ports.get(&out_port).expect("port").clone();
    let edge = g.edges.get(&eid).expect("edge").clone();

    let repeated_remove_tx = GraphTransaction::from_ops([
        GraphOp::RemovePort {
            id: out_port,
            port,
            edges: vec![(eid, edge.clone())],
            bindings: Vec::new(),
        },
        GraphOp::RemoveEdge {
            id: eid,
            edge: edge.clone(),
            bindings: Vec::new(),
        },
    ]);

    let repeated_remove_changes = NodeGraphChanges::from_transaction(&repeated_remove_tx);
    let edge_removes = repeated_remove_changes
        .edges()
        .iter()
        .filter(|change| matches!(change, EdgeChange::Remove { id } if *id == eid))
        .count();
    assert_eq!(edge_removes, 1);

    let remove_add_remove_tx = GraphTransaction::from_ops([
        GraphOp::RemoveEdge {
            id: eid,
            edge: edge.clone(),
            bindings: Vec::new(),
        },
        GraphOp::AddEdge {
            id: eid,
            edge: edge.clone(),
        },
        GraphOp::RemoveEdge {
            id: eid,
            edge,
            bindings: Vec::new(),
        },
    ]);

    let remove_add_remove_changes = NodeGraphChanges::from_transaction(&remove_add_remove_tx);
    assert!(matches!(
        remove_add_remove_changes.edges(),
        [
            EdgeChange::Remove { id: first },
            EdgeChange::Add { id: added, .. },
            EdgeChange::Remove { id: second },
        ] if *first == eid && *added == eid && *second == eid
    ));
}
