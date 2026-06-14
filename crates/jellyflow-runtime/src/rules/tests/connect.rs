use super::fixtures::{insert_data_input, insert_data_output, insert_node};

use crate::rules::{plan_connect, plan_connect_with_mode};
use jellyflow_core::core::{GraphBuilder, NodeId, PortCapacity, PortId};
use jellyflow_core::interaction::NodeGraphConnectionMode;
use jellyflow_core::ops::{GraphOp, GraphTransaction};

#[test]
fn plan_connect_swaps_in_out() {
    let mut graph = GraphBuilder::default();

    let a = NodeId::new();
    let b = NodeId::new();
    insert_node(&mut graph, a, "core.a");
    insert_node(&mut graph, b, "core.b");

    let out = PortId::new();
    let inn = PortId::new();
    insert_data_output(&mut graph, out, a, "out", PortCapacity::Multi);
    insert_data_input(&mut graph, inn, b, "in", PortCapacity::Single);

    let plan = plan_connect(&graph, inn, out);
    assert_eq!(plan.ops().len(), 1);
}

#[test]
fn plan_connect_strict_allows_same_node_out_to_in() {
    let mut graph = GraphBuilder::default();

    let a = NodeId::new();
    insert_node(&mut graph, a, "core.a");

    let out = PortId::new();
    let inn = PortId::new();
    insert_data_output(&mut graph, out, a, "out", PortCapacity::Multi);
    insert_data_input(&mut graph, inn, a, "in", PortCapacity::Single);

    let plan = plan_connect(&graph, out, inn);
    assert!(plan.is_accept());
    assert!(!plan.ops().is_empty());
}

#[test]
fn plan_connect_loose_allows_out_to_out_and_preserves_order() {
    let mut graph = GraphBuilder::default();

    let a = NodeId::new();
    let b = NodeId::new();
    insert_node(&mut graph, a, "core.a");
    insert_node(&mut graph, b, "core.b");

    let out_a = PortId::new();
    let out_b = PortId::new();
    insert_data_output(&mut graph, out_a, a, "out", PortCapacity::Multi);
    insert_data_output(&mut graph, out_b, b, "out", PortCapacity::Multi);

    let plan = plan_connect_with_mode(&graph, out_a, out_b, NodeGraphConnectionMode::Loose);
    assert!(plan.is_accept());
    assert!(plan.ops().iter().any(
        |op| matches!(op, GraphOp::AddEdge { edge, .. } if edge.from == out_a && edge.to == out_b)
    ));
}

#[test]
fn plan_connect_single_input_disconnects_existing() {
    let mut graph = GraphBuilder::default();

    let a = NodeId::new();
    let b = NodeId::new();
    let c = NodeId::new();
    insert_node(&mut graph, a, "core.a");
    insert_node(&mut graph, b, "core.b");
    insert_node(&mut graph, c, "core.c");

    let out1 = PortId::new();
    let out2 = PortId::new();
    let inn = PortId::new();
    insert_data_output(&mut graph, out1, a, "out", PortCapacity::Multi);
    insert_data_output(&mut graph, out2, c, "out", PortCapacity::Multi);
    insert_data_input(&mut graph, inn, b, "in", PortCapacity::Single);

    let plan1 = plan_connect(&graph, out1, inn);
    let tx1 = GraphTransaction::from_ops(plan1.into_ops());
    let mut graph = graph.build_unchecked();
    tx1.apply_to(&mut graph).unwrap();
    assert_eq!(graph.edges().len(), 1);

    let plan2 = plan_connect(&graph, out2, inn);
    assert_eq!(plan2.ops().len(), 2);
}

#[test]
fn plan_connect_respects_node_and_port_connectability() {
    let mut graph = GraphBuilder::default();

    let a = NodeId::new();
    let b = NodeId::new();
    insert_node(&mut graph, a, "core.a");
    insert_node(&mut graph, b, "core.b");

    let out = PortId::new();
    let inn = PortId::new();
    insert_data_output(&mut graph, out, a, "out", PortCapacity::Multi);
    insert_data_input(&mut graph, inn, b, "in", PortCapacity::Single);

    graph
        .update_node(&a, |node| node.connectable = Some(false))
        .expect("node exists");
    let plan = plan_connect(&graph, out, inn);
    assert!(plan.is_reject());
    assert!(plan.ops().is_empty());

    graph
        .update_node(&a, |node| node.connectable = Some(true))
        .expect("node exists");
    graph
        .update_port(&out, |port| port.connectable_start = Some(false))
        .expect("port exists");
    let plan = plan_connect(&graph, out, inn);
    assert!(plan.is_reject());

    graph
        .update_port(&out, |port| port.connectable_start = Some(true))
        .expect("port exists");
    graph
        .update_port(&inn, |port| port.connectable_end = Some(false))
        .expect("port exists");
    let plan = plan_connect(&graph, out, inn);
    assert!(plan.is_reject());

    graph
        .update_port(&inn, |port| port.connectable_end = Some(true))
        .expect("port exists");
    let plan = plan_connect(&graph, out, inn);
    assert!(plan.is_accept());
}
