use super::*;

#[test]
fn validate_allows_edges_regardless_of_port_direction() {
    let mut graph = Graph::default();
    let a = NodeId::new();
    let b = NodeId::new();
    graph.nodes.insert(a, make_node("core.a"));
    graph.nodes.insert(b, make_node("core.b"));

    let out_a = PortId::new();
    let out_b = PortId::new();
    attach_port(
        &mut graph,
        out_a,
        make_port(
            a,
            "out",
            PortDirection::Out,
            PortKind::Data,
            PortCapacity::Multi,
        ),
    );
    attach_port(
        &mut graph,
        out_b,
        make_port(
            b,
            "out",
            PortDirection::Out,
            PortKind::Data,
            PortCapacity::Multi,
        ),
    );

    let edge_id = EdgeId::new();
    graph.edges.insert(
        edge_id,
        Edge {
            kind: EdgeKind::Data,
            from: out_a,
            to: out_b,
            selectable: None,
            focusable: None,
            deletable: None,
            reconnectable: None,
        },
    );

    let report = validate_graph(&graph);
    assert!(report.is_ok());
}

#[test]
fn validate_rejects_port_missing_from_owner_ports() {
    let mut graph = Graph::default();
    let node_id = NodeId::new();
    graph.nodes.insert(node_id, make_node("core.a"));

    let port_id = PortId::new();
    graph.ports.insert(
        port_id,
        make_port(
            node_id,
            "out",
            PortDirection::Out,
            PortKind::Data,
            PortCapacity::Multi,
        ),
    );

    let report = validate_graph(&graph);
    assert!(report.errors.iter().any(|e| matches!(
        e,
        GraphValidationError::PortMissingFromOwner { port, node }
            if *port == port_id && *node == node_id
    )));
}

#[test]
fn validate_rejects_node_with_missing_parent_group() {
    let mut graph = Graph::default();
    let n = NodeId::new();
    let mut node = make_node("core.a");
    node.parent = Some(GroupId::new());
    graph.nodes.insert(n, node);

    let report = validate_graph(&graph);
    assert!(report.errors.iter().any(|e| matches!(
        e,
        GraphValidationError::NodeParentMissingGroup { node, .. } if *node == n
    )));
}

#[test]
fn validate_rejects_node_with_invalid_size() {
    let mut graph = Graph::default();
    let n = NodeId::new();
    let mut node = make_node("core.a");
    node.size = Some(CanvasSize {
        width: -1.0,
        height: 10.0,
    });
    graph.nodes.insert(n, node);

    let report = validate_graph(&graph);
    assert!(report.errors.iter().any(|e| matches!(
        e,
        GraphValidationError::NodeInvalidSize { node, .. } if *node == n
    )));
}

#[test]
fn validate_rejects_edge_kind_that_does_not_match_port_kind() {
    let mut graph = Graph::default();
    let a = NodeId::new();
    let b = NodeId::new();
    graph.nodes.insert(a, make_node("core.a"));
    graph.nodes.insert(b, make_node("core.b"));

    let out_a = PortId::new();
    let in_b = PortId::new();
    attach_port(
        &mut graph,
        out_a,
        make_port(
            a,
            "out",
            PortDirection::Out,
            PortKind::Data,
            PortCapacity::Multi,
        ),
    );
    attach_port(
        &mut graph,
        in_b,
        make_port(
            b,
            "in",
            PortDirection::In,
            PortKind::Data,
            PortCapacity::Single,
        ),
    );

    let edge_id = EdgeId::new();
    graph.edges.insert(
        edge_id,
        Edge {
            kind: EdgeKind::Exec,
            from: out_a,
            to: in_b,
            selectable: None,
            focusable: None,
            deletable: None,
            reconnectable: None,
        },
    );

    let report = validate_graph(&graph);
    assert!(!report.is_ok());
}

#[test]
fn validate_reports_both_missing_edge_endpoints() {
    let mut graph = Graph::default();
    let edge_id = EdgeId::new();
    let from = PortId::new();
    let to = PortId::new();
    graph.edges.insert(
        edge_id,
        Edge {
            kind: EdgeKind::Data,
            from,
            to,
            selectable: None,
            focusable: None,
            deletable: None,
            reconnectable: None,
        },
    );

    let report = validate_graph(&graph);
    let missing_ports = report
        .errors()
        .iter()
        .filter_map(|err| match err {
            GraphValidationError::EdgeMissingPort { edge, port } if *edge == edge_id => Some(*port),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(missing_ports, vec![from, to]);
}

#[test]
fn validate_reports_duplicate_edges() {
    let mut graph = Graph::default();
    let a = NodeId::new();
    let b = NodeId::new();
    graph.nodes.insert(a, make_node("core.a"));
    graph.nodes.insert(b, make_node("core.b"));

    let out_a = PortId::new();
    let in_b = PortId::new();
    attach_port(
        &mut graph,
        out_a,
        make_port(
            a,
            "out",
            PortDirection::Out,
            PortKind::Data,
            PortCapacity::Multi,
        ),
    );
    attach_port(
        &mut graph,
        in_b,
        make_port(
            b,
            "in",
            PortDirection::In,
            PortKind::Data,
            PortCapacity::Multi,
        ),
    );

    let first = EdgeId::new();
    let duplicate = EdgeId::new();
    for edge_id in [first, duplicate] {
        graph.edges.insert(
            edge_id,
            Edge {
                kind: EdgeKind::Data,
                from: out_a,
                to: in_b,
                selectable: None,
                focusable: None,
                deletable: None,
                reconnectable: None,
            },
        );
    }

    let report = validate_graph(&graph);
    let duplicate_edges = report
        .errors()
        .iter()
        .filter_map(|err| match err {
            GraphValidationError::DuplicateEdge { edge } => Some(*edge),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(duplicate_edges.len(), 1);
    assert!([first, duplicate].contains(&duplicate_edges[0]));
}

#[test]
fn validate_reports_single_port_capacity_exceeded() {
    let mut graph = Graph::default();
    let a = NodeId::new();
    let b = NodeId::new();
    let c = NodeId::new();
    graph.nodes.insert(a, make_node("core.a"));
    graph.nodes.insert(b, make_node("core.b"));
    graph.nodes.insert(c, make_node("core.c"));

    let out_a = PortId::new();
    let out_c = PortId::new();
    let in_b = PortId::new();
    attach_port(
        &mut graph,
        out_a,
        make_port(
            a,
            "out",
            PortDirection::Out,
            PortKind::Data,
            PortCapacity::Multi,
        ),
    );
    attach_port(
        &mut graph,
        out_c,
        make_port(
            c,
            "out",
            PortDirection::Out,
            PortKind::Data,
            PortCapacity::Multi,
        ),
    );
    attach_port(
        &mut graph,
        in_b,
        make_port(
            b,
            "in",
            PortDirection::In,
            PortKind::Data,
            PortCapacity::Single,
        ),
    );

    for from in [out_a, out_c] {
        graph.edges.insert(
            EdgeId::new(),
            Edge {
                kind: EdgeKind::Data,
                from,
                to: in_b,
                selectable: None,
                focusable: None,
                deletable: None,
                reconnectable: None,
            },
        );
    }

    let report = validate_graph(&graph);

    assert!(report.errors().iter().any(|err| matches!(
        err,
        GraphValidationError::PortCapacityExceeded { port, count: 2, .. } if *port == in_b
    )));
}
