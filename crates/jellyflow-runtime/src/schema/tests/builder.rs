use std::sync::Arc;

use serde_json::json;

use super::IdentityMigrator;
use crate::io::{NodeGraphEditorConfig, NodeGraphViewState};
use crate::runtime::create_node::CreateNodeRequest;
use crate::runtime::store::NodeGraphStore;
use crate::schema::{NodeRegistry, NodeSchema, NodeSchemaBuilder, PortDecl};
use jellyflow_core::core::{
    CanvasPoint, CanvasSize, Graph, GraphId, NodeKindKey, PortCapacity, PortKey,
};
use jellyflow_core::types::TypeDesc;

#[test]
fn node_schema_builder_creates_adapter_facing_schema() {
    let schema = task_card_schema();

    assert_eq!(schema.kind, NodeKindKey::new("task.card"));
    assert_eq!(schema.latest_kind_version, 3);
    assert_eq!(schema.kind_aliases, vec![NodeKindKey::new("task.note")]);
    assert_eq!(schema.title, "Task Card");
    assert_eq!(schema.category, vec!["Workflow", "Tasks"]);
    assert_eq!(schema.keywords, vec!["todo", "kanban"]);
    assert_eq!(schema.renderer_key, Some("task-card".to_owned()));
    assert_eq!(
        schema.default_size,
        Some(CanvasSize {
            width: 180.0,
            height: 104.0,
        })
    );
    assert_eq!(schema.ports.len(), 2);
    assert_eq!(schema.ports[0].key, PortKey::new("source"));
    assert_eq!(schema.ports[0].capacity, PortCapacity::Single);
    assert_eq!(schema.ports[0].label.as_deref(), Some("Source"));
    assert_eq!(schema.ports[1].key, PortKey::new("result"));
    assert_eq!(schema.ports[1].capacity, PortCapacity::Multi);
    assert_eq!(schema.default_data, json!({ "title": "", "done": false }));
}

#[test]
fn builder_created_schema_feeds_descriptors_aliases_migrators_and_dispatch() {
    let mut registry = NodeRegistry::new();
    registry.register(task_card_schema());
    registry.register_migrator(NodeKindKey::new("task.card"), Arc::new(IdentityMigrator));

    let descriptor = registry
        .view_descriptor(&NodeKindKey::new("task.note"))
        .expect("alias descriptor");
    assert_eq!(descriptor.kind, NodeKindKey::new("task.card"));
    assert_eq!(descriptor.renderer_key, "task-card");
    assert_eq!(descriptor.ports.len(), 2);

    let mut store = NodeGraphStore::new(
        Graph::new(GraphId::from_u128(1)),
        NodeGraphViewState::default(),
        NodeGraphEditorConfig::default(),
    );

    let outcome = store
        .apply_create_node_from_schema(
            &registry,
            CreateNodeRequest::new(
                NodeKindKey::new("task.note"),
                CanvasPoint { x: 24.0, y: 48.0 },
            ),
        )
        .expect("create task node");

    let node_id = outcome.node_id();
    let port_ids = outcome.port_ids().collect::<Vec<_>>();
    assert_eq!(port_ids.len(), 2);
    assert_eq!(
        store.graph().nodes()[&node_id].kind,
        NodeKindKey::new("task.card")
    );
    assert_eq!(store.graph().nodes()[&node_id].ports, port_ids);
    assert!(outcome.dispatch.footprint().nodes.contains(&node_id));
    assert!(
        port_ids
            .iter()
            .all(|port| outcome.dispatch.footprint().ports.contains(port))
    );

    let migration_plan = registry.plan_migrate_nodes(store.graph());
    assert!(migration_plan.report().is_empty());
    assert!(migration_plan.transaction().is_empty());
}

#[test]
fn schema_builder_converts_into_schema() {
    let builder: NodeSchemaBuilder = NodeSchema::builder("task.minimal", "Minimal Task");
    let schema: NodeSchema = builder.into();

    assert_eq!(schema.kind, NodeKindKey::new("task.minimal"));
    assert_eq!(schema.latest_kind_version, 1);
    assert_eq!(schema.default_data, serde_json::Value::Null);
    assert!(schema.ports.is_empty());
}

fn task_card_schema() -> NodeSchema {
    NodeSchema::builder("task.card", "Task Card")
        .latest_kind_version(3)
        .alias("task.note")
        .category(["Workflow", "Tasks"])
        .keywords(["todo", "kanban"])
        .renderer_key("task-card")
        .default_size(CanvasSize {
            width: 180.0,
            height: 104.0,
        })
        .port(
            PortDecl::data_input("source")
                .with_type(TypeDesc::Opaque {
                    key: "markdown".to_owned(),
                    params: Vec::new(),
                })
                .with_label("Source"),
        )
        .port(PortDecl::data_output("result").with_label("Result"))
        .default_data(json!({ "title": "", "done": false }))
        .build()
}
