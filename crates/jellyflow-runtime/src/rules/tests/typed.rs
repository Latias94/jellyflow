use super::fixtures::{insert_port, make_node, make_port};

use crate::rules::{ConnectDecision, plan_connect_typed};
use jellyflow_core::core::{Graph, NodeId, PortCapacity, PortDirection, PortId, PortKind};
use jellyflow_core::types::{DefaultTypeCompatibility, TypeDesc};

#[test]
fn plan_connect_typed_rejects_incompatible_data_types() {
    let mut graph = Graph::default();

    let a = NodeId::new();
    let b = NodeId::new();
    graph.nodes.insert(a, make_node("core.a"));
    graph.nodes.insert(b, make_node("core.b"));

    let out = PortId::new();
    let inn = PortId::new();
    let mut out_port = make_port(
        a,
        "out",
        PortDirection::Out,
        PortKind::Data,
        PortCapacity::Multi,
    );
    out_port.ty = Some(TypeDesc::Int);
    insert_port(&mut graph, out, out_port);

    let mut in_port = make_port(
        b,
        "in",
        PortDirection::In,
        PortKind::Data,
        PortCapacity::Single,
    );
    in_port.ty = Some(TypeDesc::String);
    insert_port(&mut graph, inn, in_port);

    let mut compat = DefaultTypeCompatibility::default();
    let plan = plan_connect_typed(
        &graph,
        out,
        inn,
        |g, p| g.ports.get(&p).and_then(|p| p.ty.clone()),
        &mut compat,
    );
    assert_eq!(plan.decision, ConnectDecision::Reject);
    assert!(plan.ops.is_empty());
}

#[test]
fn plan_connect_typed_accepts_int_to_float() {
    let mut graph = Graph::default();

    let a = NodeId::new();
    let b = NodeId::new();
    graph.nodes.insert(a, make_node("core.a"));
    graph.nodes.insert(b, make_node("core.b"));

    let out = PortId::new();
    let inn = PortId::new();
    let mut out_port = make_port(
        a,
        "out",
        PortDirection::Out,
        PortKind::Data,
        PortCapacity::Multi,
    );
    out_port.ty = Some(TypeDesc::Int);
    insert_port(&mut graph, out, out_port);

    let mut in_port = make_port(
        b,
        "in",
        PortDirection::In,
        PortKind::Data,
        PortCapacity::Single,
    );
    in_port.ty = Some(TypeDesc::Float);
    insert_port(&mut graph, inn, in_port);

    let mut compat = DefaultTypeCompatibility::default();
    let plan = plan_connect_typed(
        &graph,
        out,
        inn,
        |g, p| g.ports.get(&p).and_then(|p| p.ty.clone()),
        &mut compat,
    );
    assert_eq!(plan.decision, ConnectDecision::Accept);
    assert!(!plan.ops.is_empty());
}
