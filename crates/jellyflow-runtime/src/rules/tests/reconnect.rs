use super::fixtures::{insert_data_input, insert_data_output, insert_edge, insert_node};

use crate::rules::{EdgeEndpoint, plan_reconnect_edge};
use jellyflow_core::core::{
    EdgeId, EdgeReconnectable, EdgeReconnectableEndpoint, Graph, NodeId, PortCapacity, PortId,
};
use jellyflow_core::ops::GraphTransaction;

#[test]
fn plan_reconnect_preserves_edge_id() {
    let mut graph = Graph::default();

    let a = NodeId::new();
    let b = NodeId::new();
    let c = NodeId::new();
    insert_node(&mut graph, a, "core.a");
    insert_node(&mut graph, b, "core.b");
    insert_node(&mut graph, c, "core.c");

    let out1 = PortId::new();
    let out2 = PortId::new();
    let inn = PortId::new();
    insert_data_output(&mut graph, out1, a, "out1", PortCapacity::Multi);
    insert_data_output(&mut graph, out2, c, "out2", PortCapacity::Multi);
    insert_data_input(&mut graph, inn, b, "in", PortCapacity::Multi);

    let edge_id = EdgeId::new();
    insert_edge(&mut graph, edge_id, out1, inn);

    let plan = plan_reconnect_edge(&graph, edge_id, EdgeEndpoint::From, out2);
    assert!(plan.is_accept());
    assert_eq!(plan.ops().len(), 1);

    let tx = GraphTransaction::from_ops(plan.into_ops());
    tx.apply_to(&mut graph).unwrap();

    let edge = graph.edges.get(&edge_id).unwrap();
    assert_eq!(edge.from, out2);
    assert_eq!(edge.to, inn);
}

#[test]
fn plan_reconnect_respects_edge_and_port_policy() {
    let mut graph = Graph::default();

    let a = NodeId::new();
    let b = NodeId::new();
    let c = NodeId::new();
    insert_node(&mut graph, a, "core.a");
    insert_node(&mut graph, b, "core.b");
    insert_node(&mut graph, c, "core.c");

    let out1 = PortId::new();
    let out2 = PortId::new();
    let in1 = PortId::new();
    let in2 = PortId::new();
    insert_data_output(&mut graph, out1, a, "out1", PortCapacity::Multi);
    insert_data_output(&mut graph, out2, c, "out2", PortCapacity::Multi);
    insert_data_input(&mut graph, in1, b, "in1", PortCapacity::Multi);
    insert_data_input(&mut graph, in2, c, "in2", PortCapacity::Multi);

    let edge_id = EdgeId::new();
    insert_edge(&mut graph, edge_id, out1, in1);
    graph.edges.get_mut(&edge_id).unwrap().reconnectable = Some(EdgeReconnectable::Endpoint(
        EdgeReconnectableEndpoint::Target,
    ));

    let source_plan = plan_reconnect_edge(&graph, edge_id, EdgeEndpoint::From, out2);
    assert!(source_plan.is_reject());

    graph.ports.get_mut(&in2).unwrap().connectable_end = Some(false);
    let target_plan = plan_reconnect_edge(&graph, edge_id, EdgeEndpoint::To, in2);
    assert!(target_plan.is_reject());

    graph.ports.get_mut(&in2).unwrap().connectable_end = Some(true);
    let target_plan = plan_reconnect_edge(&graph, edge_id, EdgeEndpoint::To, in2);
    assert!(target_plan.is_accept());

    graph.edges.get_mut(&edge_id).unwrap().reconnectable = Some(EdgeReconnectable::Bool(false));
    let disabled_plan = plan_reconnect_edge(&graph, edge_id, EdgeEndpoint::To, in2);
    assert!(disabled_plan.is_reject());
}

#[test]
fn plan_reconnect_single_target_disconnects_other_edges() {
    let mut graph = Graph::default();

    let a = NodeId::new();
    let b = NodeId::new();
    let c = NodeId::new();
    let d = NodeId::new();
    insert_node(&mut graph, a, "core.a");
    insert_node(&mut graph, b, "core.b");
    insert_node(&mut graph, c, "core.c");
    insert_node(&mut graph, d, "core.d");

    let out1 = PortId::new();
    let out2 = PortId::new();
    let out3 = PortId::new();
    let inn = PortId::new();
    insert_data_output(&mut graph, out1, a, "out1", PortCapacity::Multi);
    insert_data_output(&mut graph, out2, c, "out2", PortCapacity::Multi);
    insert_data_output(&mut graph, out3, d, "out3", PortCapacity::Multi);
    insert_data_input(&mut graph, inn, b, "in", PortCapacity::Single);

    let edge_keep = EdgeId::new();
    let edge_drop = EdgeId::new();
    insert_edge(&mut graph, edge_keep, out1, inn);
    insert_edge(&mut graph, edge_drop, out2, inn);

    let plan = plan_reconnect_edge(&graph, edge_keep, EdgeEndpoint::From, out3);
    assert!(plan.is_accept());
    assert_eq!(plan.ops().len(), 2, "expected remove + set_endpoints");

    let tx = GraphTransaction::from_ops(plan.into_ops());
    tx.apply_to(&mut graph).unwrap();

    assert!(
        !graph.edges.contains_key(&edge_drop),
        "expected other edge removed"
    );
    let edge = graph.edges.get(&edge_keep).unwrap();
    assert_eq!(edge.from, out3);
    assert_eq!(edge.to, inn);
}

#[test]
fn plan_reconnect_rejects_duplicate_connection() {
    let mut graph = Graph::default();

    let a = NodeId::new();
    let b = NodeId::new();
    let c = NodeId::new();
    insert_node(&mut graph, a, "core.a");
    insert_node(&mut graph, b, "core.b");
    insert_node(&mut graph, c, "core.c");

    let out1 = PortId::new();
    let out2 = PortId::new();
    let inn = PortId::new();
    insert_data_output(&mut graph, out1, a, "out1", PortCapacity::Multi);
    insert_data_output(&mut graph, out2, c, "out2", PortCapacity::Multi);
    insert_data_input(&mut graph, inn, b, "in", PortCapacity::Multi);

    let edge_a = EdgeId::new();
    let edge_b = EdgeId::new();
    insert_edge(&mut graph, edge_a, out1, inn);
    insert_edge(&mut graph, edge_b, out2, inn);

    let plan = plan_reconnect_edge(&graph, edge_a, EdgeEndpoint::From, out2);
    assert!(plan.is_reject());
}
