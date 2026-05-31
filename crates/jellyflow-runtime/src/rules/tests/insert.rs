use super::fixtures::{insert_edge, insert_port, make_node, make_port};

use crate::rules::{
    InsertNodeSpec, plan_connect, plan_connect_by_inserting_node, plan_split_edge_by_inserting_node,
};
use jellyflow_core::core::{EdgeId, Graph, NodeId, PortCapacity, PortDirection, PortId, PortKind};
use jellyflow_core::ops::GraphTransaction;

#[test]
fn plan_connect_by_inserting_node_disconnects_single_target() {
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
            PortCapacity::Single,
        ),
    );

    let plan1 = plan_connect(&graph, out1, inn);
    let tx1 = GraphTransaction::from_ops(plan1.into_ops());
    tx1.apply_to(&mut graph).unwrap();
    assert_eq!(graph.edges.len(), 1);

    let inserted_node_id = NodeId::new();
    let inserted_in = PortId::new();
    let inserted_out = PortId::new();

    let spec = InsertNodeSpec {
        node_id: inserted_node_id,
        node: make_node("demo.convert"),
        ports: vec![
            (
                inserted_in,
                make_port(
                    inserted_node_id,
                    "in",
                    PortDirection::In,
                    PortKind::Data,
                    PortCapacity::Single,
                ),
            ),
            (
                inserted_out,
                make_port(
                    inserted_node_id,
                    "out",
                    PortDirection::Out,
                    PortKind::Data,
                    PortCapacity::Multi,
                ),
            ),
        ],
        input: inserted_in,
        output: inserted_out,
    };

    let edge_a = EdgeId::new();
    let edge_b = EdgeId::new();
    let plan2 = plan_connect_by_inserting_node(&graph, out2, inn, edge_a, edge_b, spec);
    assert!(plan2.is_accept());

    let tx2 = GraphTransaction::from_ops(plan2.into_ops());
    tx2.apply_to(&mut graph).unwrap();

    assert_eq!(graph.nodes.len(), 4);
    assert_eq!(graph.edges.len(), 2);
}

#[test]
fn plan_split_edge_by_inserting_node_preserves_edge_id() {
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

    let edge_id = EdgeId::new();
    insert_edge(&mut graph, edge_id, out, inn);

    let inserted_node_id = NodeId::new();
    let inserted_in = PortId::new();
    let inserted_out = PortId::new();
    let spec = InsertNodeSpec {
        node_id: inserted_node_id,
        node: make_node("demo.reroute"),
        ports: vec![
            (
                inserted_in,
                make_port(
                    inserted_node_id,
                    "in",
                    PortDirection::In,
                    PortKind::Data,
                    PortCapacity::Single,
                ),
            ),
            (
                inserted_out,
                make_port(
                    inserted_node_id,
                    "out",
                    PortDirection::Out,
                    PortKind::Data,
                    PortCapacity::Multi,
                ),
            ),
        ],
        input: inserted_in,
        output: inserted_out,
    };

    let new_edge_id = EdgeId::new();
    let plan = plan_split_edge_by_inserting_node(&graph, edge_id, new_edge_id, spec);
    assert!(plan.is_accept());

    let tx = GraphTransaction::from_ops(plan.into_ops());
    tx.apply_to(&mut graph).unwrap();

    assert_eq!(graph.edges.len(), 2);
    assert!(graph.edges.contains_key(&edge_id));
    assert!(graph.edges.contains_key(&new_edge_id));
    assert_eq!(graph.edges.get(&edge_id).unwrap().to, inserted_in);
    assert_eq!(graph.edges.get(&new_edge_id).unwrap().from, inserted_out);
}

#[test]
fn insert_node_planners_reject_invalid_inserted_spec_consistently() {
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

    let edge_id = EdgeId::new();
    insert_edge(&mut graph, edge_id, out, inn);

    let inserted_node_id = NodeId::new();
    let inserted_port = PortId::new();
    let spec = InsertNodeSpec {
        node_id: inserted_node_id,
        node: make_node("demo.invalid"),
        ports: vec![(
            inserted_port,
            make_port(
                inserted_node_id,
                "io",
                PortDirection::In,
                PortKind::Data,
                PortCapacity::Single,
            ),
        )],
        input: inserted_port,
        output: inserted_port,
    };

    let connect_plan = plan_connect_by_inserting_node(
        &graph,
        out,
        inn,
        EdgeId::new(),
        EdgeId::new(),
        spec.clone(),
    );
    let split_plan = plan_split_edge_by_inserting_node(&graph, edge_id, EdgeId::new(), spec);

    for plan in [connect_plan, split_plan] {
        assert!(plan.is_reject());
        assert_eq!(
            plan.diagnostics()[0].message,
            "inserted input/output ports must be distinct"
        );
        assert!(plan.ops().is_empty());
    }
}
