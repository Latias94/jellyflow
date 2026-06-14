use std::sync::Arc;

use serde_json::json;

use super::{DummyMigrator, IdentityMigrator, demo_add_node, demo_add_schema};
use crate::schema::NodeRegistry;
use jellyflow_core::core::{GraphBuilder, GraphId, NodeId, NodeKindKey};
use jellyflow_core::ops::GraphOp;

#[test]
fn migrate_nodes_emits_set_node_data_and_version_and_reports_upgraded() {
    let mut registry = NodeRegistry::new();
    registry.register(demo_add_schema(2, Vec::new()));
    registry.register_migrator(NodeKindKey::new("demo.add"), Arc::new(DummyMigrator));

    let id = NodeId::new();
    let mut graph = GraphBuilder::new(GraphId::new());
    graph.insert_node(id, demo_add_node("demo.add", 0, json!({"x": 1})));

    let plan = registry.plan_migrate_nodes(&graph);
    assert_eq!(plan.report().upgraded().len(), 1);
    assert!(plan.report().missing_schema().is_empty());
    assert!(plan.report().missing_migrator().is_empty());
    assert!(plan.report().errors().is_empty());

    plan.transaction().apply_to(&mut graph).unwrap();
    let node = graph.nodes().get(&id).unwrap();
    assert_eq!(node.kind_version, 2);
    assert_eq!(node.data["migrated"], json!(true));
}

#[test]
fn migrate_nodes_reports_missing_migrator_and_emits_no_tx() {
    let mut registry = NodeRegistry::new();
    registry.register(demo_add_schema(2, Vec::new()));

    let id = NodeId::new();
    let mut graph = GraphBuilder::new(GraphId::new());
    graph.insert_node(id, demo_add_node("demo.add", 0, serde_json::Value::Null));

    let plan = registry.plan_migrate_nodes(&graph);
    assert_eq!(plan.report().missing_migrator().len(), 1);
    assert!(plan.transaction().is_empty());
}

#[test]
fn migrate_nodes_skips_noop_data_updates() {
    let mut registry = NodeRegistry::new();
    registry.register(demo_add_schema(2, Vec::new()));
    registry.register_migrator(NodeKindKey::new("demo.add"), Arc::new(IdentityMigrator));

    let id = NodeId::new();
    let mut graph = GraphBuilder::new(GraphId::new());
    graph.insert_node(id, demo_add_node("demo.add", 1, json!({"x": 1})));

    let plan = registry.plan_migrate_nodes(&graph);
    assert_eq!(plan.report().upgraded().len(), 1);
    assert!(
        plan.transaction()
            .ops()
            .iter()
            .any(|op| matches!(op, GraphOp::SetNodeKindVersion { id: op_id, .. } if *op_id == id))
    );
    assert!(
        !plan
            .transaction()
            .ops()
            .iter()
            .any(|op| matches!(op, GraphOp::SetNodeData { id: op_id, .. } if *op_id == id))
    );
}
