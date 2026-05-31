use super::fixtures::{insert_port, make_node, make_port};

use crate::rules::{ConnectDecision, plan_connect, plan_connect_with_mode};
use jellyflow_core::core::{Graph, NodeId, PortCapacity, PortDirection, PortId, PortKind};
use jellyflow_core::interaction::NodeGraphConnectionMode;
use jellyflow_core::ops::{GraphOp, GraphTransaction};

#[test]
fn plan_connect_swaps_in_out() {
    let mut graph = Graph::default();

    let a = NodeId::new();
    let b = NodeId::new();
    graph.nodes.insert(a, make_node("core.a"));
    graph.nodes.insert(b, make_node("core.b"));

    let out = PortId::new();
    let inn = PortId::new();
    insert_port(
        &mut graph,
        out,
        make_port(
            a,
            "out",
            PortDirection::Out,
            PortKind::Data,
            PortCapacity::Multi,
        ),
    );
    insert_port(
        &mut graph,
        inn,
        make_port(
            b,
            "in",
            PortDirection::In,
            PortKind::Data,
            PortCapacity::Single,
        ),
    );

    let plan = plan_connect(&graph, inn, out);
    assert_eq!(plan.ops.len(), 1);
}

#[test]
fn plan_connect_strict_allows_same_node_out_to_in() {
    let mut graph = Graph::default();

    let a = NodeId::new();
    graph.nodes.insert(a, make_node("core.a"));

    let out = PortId::new();
    let inn = PortId::new();
    insert_port(
        &mut graph,
        out,
        make_port(
            a,
            "out",
            PortDirection::Out,
            PortKind::Data,
            PortCapacity::Multi,
        ),
    );
    insert_port(
        &mut graph,
        inn,
        make_port(
            a,
            "in",
            PortDirection::In,
            PortKind::Data,
            PortCapacity::Single,
        ),
    );

    let plan = plan_connect(&graph, out, inn);
    assert_eq!(plan.decision, ConnectDecision::Accept);
    assert!(!plan.ops.is_empty());
}

#[test]
fn plan_connect_loose_allows_out_to_out_and_preserves_order() {
    let mut graph = Graph::default();

    let a = NodeId::new();
    let b = NodeId::new();
    graph.nodes.insert(a, make_node("core.a"));
    graph.nodes.insert(b, make_node("core.b"));

    let out_a = PortId::new();
    let out_b = PortId::new();
    insert_port(
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
    insert_port(
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

    let plan = plan_connect_with_mode(&graph, out_a, out_b, NodeGraphConnectionMode::Loose);
    assert_eq!(plan.decision, ConnectDecision::Accept);
    assert!(plan.ops.iter().any(
        |op| matches!(op, GraphOp::AddEdge { edge, .. } if edge.from == out_a && edge.to == out_b)
    ));
}

#[test]
fn plan_connect_single_input_disconnects_existing() {
    let mut graph = Graph::default();

    let a = NodeId::new();
    let b = NodeId::new();
    let c = NodeId::new();
    graph.nodes.insert(a, make_node("core.a"));
    graph.nodes.insert(b, make_node("core.b"));
    graph.nodes.insert(c, make_node("core.c"));

    let out1 = PortId::new();
    let out2 = PortId::new();
    let inn = PortId::new();
    insert_port(
        &mut graph,
        out1,
        make_port(
            a,
            "out",
            PortDirection::Out,
            PortKind::Data,
            PortCapacity::Multi,
        ),
    );
    insert_port(
        &mut graph,
        out2,
        make_port(
            c,
            "out",
            PortDirection::Out,
            PortKind::Data,
            PortCapacity::Multi,
        ),
    );
    insert_port(
        &mut graph,
        inn,
        make_port(
            b,
            "in",
            PortDirection::In,
            PortKind::Data,
            PortCapacity::Single,
        ),
    );

    let plan1 = plan_connect(&graph, out1, inn);
    let tx1 = GraphTransaction {
        label: None,
        ops: plan1.ops,
    };
    tx1.apply_to(&mut graph).unwrap();
    assert_eq!(graph.edges.len(), 1);

    let plan2 = plan_connect(&graph, out2, inn);
    assert_eq!(plan2.ops.len(), 2);
}

#[test]
fn plan_connect_respects_node_and_port_connectability() {
    let mut graph = Graph::default();

    let a = NodeId::new();
    let b = NodeId::new();
    graph.nodes.insert(a, make_node("core.a"));
    graph.nodes.insert(b, make_node("core.b"));

    let out = PortId::new();
    let inn = PortId::new();
    insert_port(
        &mut graph,
        out,
        make_port(
            a,
            "out",
            PortDirection::Out,
            PortKind::Data,
            PortCapacity::Multi,
        ),
    );
    insert_port(
        &mut graph,
        inn,
        make_port(
            b,
            "in",
            PortDirection::In,
            PortKind::Data,
            PortCapacity::Single,
        ),
    );

    graph.nodes.get_mut(&a).unwrap().connectable = Some(false);
    let plan = plan_connect(&graph, out, inn);
    assert_eq!(plan.decision, ConnectDecision::Reject);
    assert!(plan.ops.is_empty());

    graph.nodes.get_mut(&a).unwrap().connectable = Some(true);
    graph.ports.get_mut(&out).unwrap().connectable_start = Some(false);
    let plan = plan_connect(&graph, out, inn);
    assert_eq!(plan.decision, ConnectDecision::Reject);

    graph.ports.get_mut(&out).unwrap().connectable_start = Some(true);
    graph.ports.get_mut(&inn).unwrap().connectable_end = Some(false);
    let plan = plan_connect(&graph, out, inn);
    assert_eq!(plan.decision, ConnectDecision::Reject);

    graph.ports.get_mut(&inn).unwrap().connectable_end = Some(true);
    let plan = plan_connect(&graph, out, inn);
    assert_eq!(plan.decision, ConnectDecision::Accept);
}
