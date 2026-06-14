use super::fixtures::{insert_node, insert_typed_data_input, insert_typed_data_output};

use crate::rules::plan_connect_typed;
use jellyflow_core::core::{GraphBuilder, NodeId, PortCapacity, PortId};
use jellyflow_core::types::{DefaultTypeCompatibility, TypeDesc};

#[test]
fn plan_connect_typed_rejects_incompatible_data_types() {
    let mut graph = GraphBuilder::default();

    let a = NodeId::new();
    let b = NodeId::new();
    insert_node(&mut graph, a, "core.a");
    insert_node(&mut graph, b, "core.b");

    let out = PortId::new();
    let inn = PortId::new();
    insert_typed_data_output(
        &mut graph,
        out,
        a,
        "out",
        PortCapacity::Multi,
        TypeDesc::Int,
    );
    insert_typed_data_input(
        &mut graph,
        inn,
        b,
        "in",
        PortCapacity::Single,
        TypeDesc::String,
    );

    let mut compat = DefaultTypeCompatibility::default();
    let plan = plan_connect_typed(
        &graph,
        out,
        inn,
        |g, p| g.ports().get(&p).and_then(|p| p.ty.clone()),
        &mut compat,
    );
    assert!(plan.is_reject());
    assert!(plan.ops().is_empty());
}

#[test]
fn plan_connect_typed_accepts_int_to_float() {
    let mut graph = GraphBuilder::default();

    let a = NodeId::new();
    let b = NodeId::new();
    insert_node(&mut graph, a, "core.a");
    insert_node(&mut graph, b, "core.b");

    let out = PortId::new();
    let inn = PortId::new();
    insert_typed_data_output(
        &mut graph,
        out,
        a,
        "out",
        PortCapacity::Multi,
        TypeDesc::Int,
    );
    insert_typed_data_input(
        &mut graph,
        inn,
        b,
        "in",
        PortCapacity::Single,
        TypeDesc::Float,
    );

    let mut compat = DefaultTypeCompatibility::default();
    let plan = plan_connect_typed(
        &graph,
        out,
        inn,
        |g, p| g.ports().get(&p).and_then(|p| p.ty.clone()),
        &mut compat,
    );
    assert!(plan.is_accept());
    assert!(!plan.ops().is_empty());
}
