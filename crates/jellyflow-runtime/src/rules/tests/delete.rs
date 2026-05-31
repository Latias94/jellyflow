use super::fixtures::{insert_edge, insert_port, make_node, make_port};

use crate::io::NodeGraphInteractionState;
use crate::rules::{
    DeleteDecision, plan_delete_edge, plan_delete_elements_with_policy,
    plan_delete_node_with_policy,
};
use jellyflow_core::core::{EdgeId, Graph, NodeId, PortCapacity, PortDirection, PortId, PortKind};
use jellyflow_core::ops::{GraphOp, GraphTransaction};

#[test]
fn plan_delete_node_respects_policy_and_cascades_incident_edges() {
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

    let disabled = NodeGraphInteractionState {
        nodes_deletable: false,
        edges_deletable: false,
        ..NodeGraphInteractionState::default()
    };
    let rejected = plan_delete_node_with_policy(&graph, a, &disabled);
    assert_eq!(rejected.decision, DeleteDecision::Reject);
    assert!(rejected.ops.is_empty());

    graph.nodes.get_mut(&a).unwrap().deletable = Some(true);
    let accepted = plan_delete_node_with_policy(&graph, a, &disabled);
    assert_eq!(accepted.decision, DeleteDecision::Accept);
    assert_eq!(accepted.ops.len(), 1);
    assert!(matches!(
        &accepted.ops[0],
        GraphOp::RemoveNode { id, edges, .. }
            if *id == a && edges.iter().any(|(id, _)| *id == edge_id)
    ));

    GraphTransaction {
        label: None,
        ops: accepted.ops,
    }
    .apply_to(&mut graph)
    .unwrap();

    assert!(!graph.nodes.contains_key(&a));
    assert!(graph.nodes.contains_key(&b));
    assert!(!graph.ports.contains_key(&out));
    assert!(graph.ports.contains_key(&inn));
    assert!(!graph.edges.contains_key(&edge_id));
}

#[test]
fn plan_delete_edge_respects_policy_overrides() {
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

    let disabled = NodeGraphInteractionState {
        edges_deletable: false,
        ..NodeGraphInteractionState::default()
    };
    let default_plan = plan_delete_edge(&graph, edge_id);
    assert_eq!(default_plan.decision, DeleteDecision::Accept);

    let rejected = plan_delete_elements_with_policy(&graph, [], [edge_id], &disabled);
    assert_eq!(rejected.decision, DeleteDecision::Reject);
    assert!(rejected.ops.is_empty());

    graph.edges.get_mut(&edge_id).unwrap().deletable = Some(true);
    let accepted = plan_delete_elements_with_policy(&graph, [], [edge_id], &disabled);
    assert_eq!(accepted.decision, DeleteDecision::Accept);
    assert_eq!(accepted.ops.len(), 1);

    GraphTransaction {
        label: None,
        ops: accepted.ops,
    }
    .apply_to(&mut graph)
    .unwrap();

    assert!(!graph.edges.contains_key(&edge_id));
}

#[test]
fn plan_delete_elements_dedupes_node_cascaded_edges() {
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
    graph.nodes.get_mut(&a).unwrap().deletable = Some(true);
    graph.nodes.get_mut(&b).unwrap().deletable = Some(true);
    graph.edges.get_mut(&edge_id).unwrap().deletable = Some(false);

    let disabled = NodeGraphInteractionState {
        nodes_deletable: false,
        edges_deletable: false,
        ..NodeGraphInteractionState::default()
    };
    let plan = plan_delete_elements_with_policy(&graph, [a, b], [edge_id], &disabled);
    assert_eq!(plan.decision, DeleteDecision::Accept);
    assert_eq!(plan.ops.len(), 2);
    assert!(
        plan.ops
            .iter()
            .any(|op| matches!(op, GraphOp::RemoveNode { id, .. } if *id == a))
    );
    assert!(
        plan.ops
            .iter()
            .any(|op| matches!(op, GraphOp::RemoveNode { id, .. } if *id == b))
    );

    GraphTransaction {
        label: None,
        ops: plan.ops,
    }
    .apply_to(&mut graph)
    .unwrap();

    assert!(!graph.nodes.contains_key(&a));
    assert!(!graph.nodes.contains_key(&b));
    assert!(!graph.edges.contains_key(&edge_id));
}
