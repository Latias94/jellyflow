use super::{demo_add_node, demo_add_schema};
use crate::schema::NodeRegistry;
use jellyflow_core::core::{Graph, GraphId, NodeId, NodeKindKey};

#[test]
fn canonicalize_kinds_rewrites_aliases_to_canonical() {
    let mut registry = NodeRegistry::new();
    registry.register(demo_add_schema(1, vec!["demo.add.v0"]));

    let id = NodeId::new();
    let mut graph = Graph::new(GraphId::new());
    graph
        .nodes
        .insert(id, demo_add_node("demo.add.v0", 0, serde_json::Value::Null));

    let plan = registry.plan_canonicalize_kinds(&graph);
    assert_eq!(plan.rewrites().len(), 1);
    assert_eq!(plan.rewrites()[0].node(), id);

    plan.transaction().apply_to(&mut graph).unwrap();
    assert_eq!(
        graph.nodes.get(&id).unwrap().kind,
        NodeKindKey::new("demo.add")
    );
}
