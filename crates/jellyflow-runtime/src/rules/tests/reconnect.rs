use super::fixtures::{insert_edge, insert_port, make_node, make_port};

use crate::rules::{EdgeEndpoint, plan_reconnect_edge};
use jellyflow_core::core::{
    EdgeId, EdgeReconnectable, EdgeReconnectableEndpoint, Graph, NodeId, PortCapacity,
    PortDirection, PortId, PortKind,
};
use jellyflow_core::ops::GraphTransaction;

#[test]
fn plan_reconnect_preserves_edge_id() {
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
            "out1",
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
            "out2",
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
            PortCapacity::Multi,
        ),
    );

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
    graph.nodes.insert(a, make_node("core.a"));
    graph.nodes.insert(b, make_node("core.b"));
    graph.nodes.insert(c, make_node("core.c"));

    let out1 = PortId::new();
    let out2 = PortId::new();
    let in1 = PortId::new();
    let in2 = PortId::new();
    insert_port(
        &mut graph,
        out1,
        make_port(
            a,
            "out1",
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
            "out2",
            PortDirection::Out,
            PortKind::Data,
            PortCapacity::Multi,
        ),
    );
    insert_port(
        &mut graph,
        in1,
        make_port(
            b,
            "in1",
            PortDirection::In,
            PortKind::Data,
            PortCapacity::Multi,
        ),
    );
    insert_port(
        &mut graph,
        in2,
        make_port(
            c,
            "in2",
            PortDirection::In,
            PortKind::Data,
            PortCapacity::Multi,
        ),
    );

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
    graph.nodes.insert(a, make_node("core.a"));
    graph.nodes.insert(b, make_node("core.b"));
    graph.nodes.insert(c, make_node("core.c"));
    graph.nodes.insert(d, make_node("core.d"));

    let out1 = PortId::new();
    let out2 = PortId::new();
    let out3 = PortId::new();
    let inn = PortId::new();
    insert_port(
        &mut graph,
        out1,
        make_port(
            a,
            "out1",
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
            "out2",
            PortDirection::Out,
            PortKind::Data,
            PortCapacity::Multi,
        ),
    );
    insert_port(
        &mut graph,
        out3,
        make_port(
            d,
            "out3",
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
            "out1",
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
            "out2",
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
            PortCapacity::Multi,
        ),
    );

    let edge_a = EdgeId::new();
    let edge_b = EdgeId::new();
    insert_edge(&mut graph, edge_a, out1, inn);
    insert_edge(&mut graph, edge_b, out2, inn);

    let plan = plan_reconnect_edge(&graph, edge_a, EdgeEndpoint::From, out2);
    assert!(plan.is_reject());
}
