use super::fixtures::{
    insert_data_input, insert_data_output, insert_edge, insert_node, make_data_input,
    make_data_output, make_node,
};

use crate::rules::{
    InsertNodeSpec, plan_connect, plan_connect_by_inserting_node, plan_split_edge_by_inserting_node,
};
use jellyflow_core::core::{EdgeId, GraphBuilder, NodeId, PortCapacity, PortId};
use jellyflow_core::ops::GraphTransaction;

#[test]
fn plan_connect_by_inserting_node_disconnects_single_target() {
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
    insert_data_output(&mut graph, out1, a, "out1", PortCapacity::Multi);
    insert_data_output(&mut graph, out2, c, "out2", PortCapacity::Multi);
    insert_data_input(&mut graph, inn, b, "in", PortCapacity::Single);

    let plan1 = plan_connect(&graph, out1, inn);
    let tx1 = GraphTransaction::from_ops(plan1.into_ops());
    let mut graph = graph.build_unchecked();
    tx1.apply_to(&mut graph).unwrap();
    assert_eq!(graph.edges().len(), 1);

    let inserted_node_id = NodeId::new();
    let inserted_in = PortId::new();
    let inserted_out = PortId::new();

    let spec = InsertNodeSpec {
        node_id: inserted_node_id,
        node: make_node("demo.convert"),
        ports: vec![
            (
                inserted_in,
                make_data_input(inserted_node_id, "in", PortCapacity::Single),
            ),
            (
                inserted_out,
                make_data_output(inserted_node_id, "out", PortCapacity::Multi),
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

    assert_eq!(graph.nodes().len(), 4);
    assert_eq!(graph.edges().len(), 2);
}

#[test]
fn plan_split_edge_by_inserting_node_preserves_edge_id() {
    let mut graph = GraphBuilder::default();

    let a = NodeId::new();
    let b = NodeId::new();
    insert_node(&mut graph, a, "core.a");
    insert_node(&mut graph, b, "core.b");

    let out = PortId::new();
    let inn = PortId::new();
    insert_data_output(&mut graph, out, a, "out", PortCapacity::Multi);
    insert_data_input(&mut graph, inn, b, "in", PortCapacity::Single);

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
                make_data_input(inserted_node_id, "in", PortCapacity::Single),
            ),
            (
                inserted_out,
                make_data_output(inserted_node_id, "out", PortCapacity::Multi),
            ),
        ],
        input: inserted_in,
        output: inserted_out,
    };

    let new_edge_id = EdgeId::new();
    let plan = plan_split_edge_by_inserting_node(&graph, edge_id, new_edge_id, spec);
    assert!(plan.is_accept());

    let tx = GraphTransaction::from_ops(plan.into_ops());
    let mut graph = graph.build_unchecked();
    tx.apply_to(&mut graph).unwrap();

    assert_eq!(graph.edges().len(), 2);
    assert!(graph.edges().contains_key(&edge_id));
    assert!(graph.edges().contains_key(&new_edge_id));
    assert_eq!(graph.edges().get(&edge_id).unwrap().to, inserted_in);
    assert_eq!(graph.edges().get(&new_edge_id).unwrap().from, inserted_out);
}

#[test]
fn insert_node_planners_reject_invalid_inserted_spec_consistently() {
    let mut graph = GraphBuilder::default();

    let a = NodeId::new();
    let b = NodeId::new();
    insert_node(&mut graph, a, "core.a");
    insert_node(&mut graph, b, "core.b");

    let out = PortId::new();
    let inn = PortId::new();
    insert_data_output(&mut graph, out, a, "out", PortCapacity::Multi);
    insert_data_input(&mut graph, inn, b, "in", PortCapacity::Single);

    let edge_id = EdgeId::new();
    insert_edge(&mut graph, edge_id, out, inn);

    let same_role_node_id = NodeId::new();
    let same_role_port = PortId::new();
    let missing_input_node_id = NodeId::new();
    let missing_input = PortId::new();
    let missing_input_output = PortId::new();
    let missing_output_node_id = NodeId::new();
    let missing_output_input = PortId::new();
    let missing_output = PortId::new();
    let wrong_direction_node_id = NodeId::new();
    let wrong_direction_input = PortId::new();
    let wrong_direction_output = PortId::new();

    let invalid_specs = vec![
        (
            "inserted input/output ports must be distinct",
            InsertNodeSpec {
                node_id: same_role_node_id,
                node: make_node("demo.invalid"),
                ports: vec![(
                    same_role_port,
                    make_data_input(same_role_node_id, "io", PortCapacity::Single),
                )],
                input: same_role_port,
                output: same_role_port,
            },
        ),
        (
            "inserted input port is missing from spec",
            InsertNodeSpec {
                node_id: missing_input_node_id,
                node: make_node("demo.invalid"),
                ports: vec![(
                    missing_input_output,
                    make_data_output(missing_input_node_id, "out", PortCapacity::Multi),
                )],
                input: missing_input,
                output: missing_input_output,
            },
        ),
        (
            "inserted output port is missing from spec",
            InsertNodeSpec {
                node_id: missing_output_node_id,
                node: make_node("demo.invalid"),
                ports: vec![(
                    missing_output_input,
                    make_data_input(missing_output_node_id, "in", PortCapacity::Single),
                )],
                input: missing_output_input,
                output: missing_output,
            },
        ),
        (
            "inserted ports must be in -> out",
            InsertNodeSpec {
                node_id: wrong_direction_node_id,
                node: make_node("demo.invalid"),
                ports: vec![
                    (
                        wrong_direction_input,
                        make_data_output(wrong_direction_node_id, "in", PortCapacity::Single),
                    ),
                    (
                        wrong_direction_output,
                        make_data_input(wrong_direction_node_id, "out", PortCapacity::Multi),
                    ),
                ],
                input: wrong_direction_input,
                output: wrong_direction_output,
            },
        ),
    ];

    for (expected_message, spec) in invalid_specs {
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
            assert_eq!(plan.diagnostics()[0].message, expected_message);
            assert!(plan.ops().is_empty());
        }
    }
}
