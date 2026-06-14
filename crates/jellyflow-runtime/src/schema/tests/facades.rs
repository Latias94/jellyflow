use std::sync::Arc;

use serde_json::json;

use super::{IdentityMigrator, demo_add_node, demo_add_schema};
use crate::schema::NodeRegistry;
use jellyflow_core::core::{GraphBuilder, GraphId, NodeId, NodeKindKey};

#[test]
fn schema_plan_facades_consume_parts() {
    let mut registry = NodeRegistry::new();
    registry.register(demo_add_schema(2, vec!["demo.add.v1"]));
    registry.register_migrator(NodeKindKey::new("demo.add"), Arc::new(IdentityMigrator));

    let id = NodeId::new();
    let mut graph = GraphBuilder::new(GraphId::new());
    graph.insert_node(id, demo_add_node("demo.add.v1", 1, json!({"x": 1})));

    let (canonicalize_tx, rewrites) = registry.plan_canonicalize_kinds(&graph).into_parts();
    assert_eq!(canonicalize_tx.label(), Some("Canonicalize node kinds"));
    assert_eq!(rewrites.len(), 1);
    assert_eq!(rewrites[0].node(), id);

    let (migrate_tx, report) = registry.plan_migrate_nodes(&graph).into_parts();
    assert_eq!(migrate_tx.label(), Some("Migrate node kinds"));
    assert_eq!(report.upgraded().len(), 1);
}
