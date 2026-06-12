use serde_json::json;

use crate::schema::{NodeRegistry, NodeSchema, PortDecl};
use jellyflow_core::core::{
    CanvasSize, NodeKindKey, PortCapacity, PortDirection, PortKey, PortKind,
};
use jellyflow_core::types::TypeDesc;

#[test]
fn node_registry_view_descriptors_are_adapter_facing_and_deterministic() {
    let mut registry = NodeRegistry::new();
    registry.register(NodeSchema {
        kind: NodeKindKey::new("demo.note"),
        latest_kind_version: 1,
        kind_aliases: vec![NodeKindKey::new("demo.sticky")],
        title: "Note".into(),
        category: vec!["Knowledge".into()],
        keywords: vec!["memo".into(), "markdown".into()],
        renderer_key: Some("note-card".into()),
        default_size: Some(CanvasSize {
            width: 180.0,
            height: 120.0,
        }),
        ports: vec![PortDecl {
            key: PortKey::new("source"),
            dir: PortDirection::In,
            kind: PortKind::Data,
            capacity: PortCapacity::Single,
            ty: Some(TypeDesc::Opaque {
                key: "markdown".into(),
                params: Vec::new(),
            }),
            label: Some("Source".into()),
        }],
        default_data: json!({ "body": "" }),
    });
    registry.register(NodeSchema {
        kind: NodeKindKey::new("demo.add"),
        latest_kind_version: 1,
        kind_aliases: Vec::new(),
        title: "Add".into(),
        category: Vec::new(),
        keywords: Vec::new(),
        renderer_key: None,
        default_size: None,
        ports: Vec::new(),
        default_data: serde_json::Value::Null,
    });

    let descriptors = registry.view_descriptors();

    assert_eq!(descriptors.len(), 2);
    assert_eq!(descriptors[0].kind, NodeKindKey::new("demo.add"));
    assert_eq!(
        descriptors[0].renderer_key, "demo.add",
        "schemas without an explicit renderer key fall back to the canonical kind"
    );
    assert_eq!(descriptors[1].kind, NodeKindKey::new("demo.note"));
    assert_eq!(descriptors[1].renderer_key, "note-card");
    assert_eq!(descriptors[1].title, "Note");
    assert_eq!(descriptors[1].category, vec!["Knowledge"]);
    assert_eq!(descriptors[1].keywords, vec!["memo", "markdown"]);
    assert_eq!(
        descriptors[1].default_size,
        Some(CanvasSize {
            width: 180.0,
            height: 120.0,
        })
    );
    assert_eq!(descriptors[1].ports.len(), 1);
    assert_eq!(descriptors[1].default_data, json!({ "body": "" }));

    let alias_descriptor = registry
        .view_descriptor(&NodeKindKey::new("demo.sticky"))
        .expect("descriptor by alias");
    assert_eq!(alias_descriptor.kind, NodeKindKey::new("demo.note"));
    assert_eq!(alias_descriptor.renderer_key, "note-card");
}

#[test]
fn node_schema_deserializes_without_adapter_view_fields() {
    let schema: NodeSchema = serde_json::from_value(json!({
        "kind": "demo.legacy",
        "latest_kind_version": 1,
        "title": "Legacy",
        "default_data": { "value": 1 }
    }))
    .expect("legacy schema");

    let mut registry = NodeRegistry::new();
    registry.register(schema);
    let descriptors = registry.view_descriptors();

    assert_eq!(descriptors[0].renderer_key, "demo.legacy");
    assert_eq!(descriptors[0].default_size, None);
}
