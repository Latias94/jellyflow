use super::fixtures::{insert_data_input, insert_data_output, insert_edge, insert_node};

use crate::io::NodeGraphInteractionState;
use crate::rules::{
    plan_delete_edge, plan_delete_elements_with_policy, plan_delete_node_with_policy,
};
use jellyflow_core::core::{EdgeId, GraphBuilder, NodeId, PortCapacity, PortId};
use jellyflow_core::ops::{GraphOp, GraphTransaction};

#[test]
fn plan_delete_node_respects_policy_and_cascades_incident_edges() {
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

    let disabled = NodeGraphInteractionState {
        nodes_deletable: false,
        edges_deletable: false,
        ..NodeGraphInteractionState::default()
    };
    let rejected = plan_delete_node_with_policy(&graph, a, &disabled);
    assert!(rejected.is_reject());
    assert!(rejected.ops().is_empty());

    graph
        .update_node(&a, |node| node.deletable = Some(true))
        .expect("node exists");
    let accepted = plan_delete_node_with_policy(&graph, a, &disabled);
    assert!(accepted.is_accept());
    assert_eq!(accepted.ops().len(), 1);
    assert!(matches!(
        &accepted.ops()[0],
        GraphOp::RemoveNode { id, edges, .. }
            if *id == a && edges.iter().any(|(id, _)| *id == edge_id)
    ));

    let mut graph = graph.build_unchecked();
    GraphTransaction::from_ops(accepted.into_ops())
        .apply_to(&mut graph)
        .unwrap();

    assert!(!graph.nodes().contains_key(&a));
    assert!(graph.nodes().contains_key(&b));
    assert!(!graph.ports().contains_key(&out));
    assert!(graph.ports().contains_key(&inn));
    assert!(!graph.edges().contains_key(&edge_id));
}

#[test]
fn plan_delete_edge_respects_policy_overrides() {
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

    let disabled = NodeGraphInteractionState {
        edges_deletable: false,
        ..NodeGraphInteractionState::default()
    };
    let default_plan = plan_delete_edge(&graph, edge_id);
    assert!(default_plan.is_accept());

    let rejected = plan_delete_elements_with_policy(&graph, [], [edge_id], &disabled);
    assert!(rejected.is_reject());
    assert!(rejected.ops().is_empty());

    graph
        .update_edge(&edge_id, |edge| edge.deletable = Some(true))
        .expect("edge exists");
    let accepted = plan_delete_elements_with_policy(&graph, [], [edge_id], &disabled);
    assert!(accepted.is_accept());
    assert_eq!(accepted.ops().len(), 1);

    let mut graph = graph.build_unchecked();
    GraphTransaction::from_ops(accepted.into_ops())
        .apply_to(&mut graph)
        .unwrap();

    assert!(!graph.edges().contains_key(&edge_id));
}

#[test]
fn plan_delete_elements_dedupes_node_cascaded_edges() {
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
    graph
        .update_node(&a, |node| node.deletable = Some(true))
        .expect("node exists");
    graph
        .update_node(&b, |node| node.deletable = Some(true))
        .expect("node exists");
    graph
        .update_edge(&edge_id, |edge| edge.deletable = Some(false))
        .expect("edge exists");

    let disabled = NodeGraphInteractionState {
        nodes_deletable: false,
        edges_deletable: false,
        ..NodeGraphInteractionState::default()
    };
    let plan = plan_delete_elements_with_policy(&graph, [a, b], [edge_id], &disabled);
    assert!(plan.is_accept());
    assert_eq!(plan.ops().len(), 2);
    assert!(
        plan.ops()
            .iter()
            .any(|op| matches!(op, GraphOp::RemoveNode { id, .. } if *id == a))
    );
    assert!(
        plan.ops()
            .iter()
            .any(|op| matches!(op, GraphOp::RemoveNode { id, .. } if *id == b))
    );

    let mut graph = graph.build_unchecked();
    GraphTransaction::from_ops(plan.into_ops())
        .apply_to(&mut graph)
        .unwrap();

    assert!(!graph.nodes().contains_key(&a));
    assert!(!graph.nodes().contains_key(&b));
    assert!(!graph.edges().contains_key(&edge_id));
}
