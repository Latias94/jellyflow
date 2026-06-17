use serde_json::json;

use crate::schema::{
    NodeRegistry, NodeSchema, PortDecl, PortHandleVisibility, PortViewDescriptor, PortViewSide,
};
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
            view: PortViewDescriptor::left()
                .with_order(10)
                .with_group("input")
                .with_anchor("field.source")
                .with_icon_key("file-text"),
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
    assert_eq!(descriptors[1].ports[0].view.side, Some(PortViewSide::Left));
    assert_eq!(descriptors[1].ports[0].view.order, Some(10));
    assert_eq!(descriptors[1].ports[0].view.group.as_deref(), Some("input"));
    assert_eq!(
        descriptors[1].ports[0].view.anchor.as_deref(),
        Some("field.source")
    );
    assert_eq!(
        descriptors[1].ports[0].view.icon_key.as_deref(),
        Some("file-text")
    );
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

#[test]
fn port_view_descriptors_cover_sides_anchors_and_visibility() {
    let schema = NodeSchema::builder("demo.table", "Table")
        .port(
            PortDecl::data_input("filter")
                .on_top()
                .with_view_order(0)
                .with_label("Filter"),
        )
        .port(
            PortDecl::data_output("rows")
                .on_right()
                .with_view_group("result")
                .with_view_order(1)
                .with_label("Rows"),
        )
        .port(
            PortDecl::data_input("field.id")
                .with_view(
                    PortViewDescriptor::left()
                        .with_anchor("field.id")
                        .with_lane("fields")
                        .with_slot("id")
                        .with_label("id")
                        .with_icon_key("key"),
                )
                .hidden_handle(),
        )
        .port(
            PortDecl::data_output("summary")
                .on_bottom()
                .with_view_anchor("footer.summary")
                .with_view_order(2),
        )
        .build();

    let mut registry = NodeRegistry::new();
    registry.register(schema);
    let descriptor = registry
        .view_descriptor(&NodeKindKey::new("demo.table"))
        .expect("descriptor");

    assert_eq!(descriptor.ports.len(), 4);
    assert_eq!(descriptor.ports[0].view.side, Some(PortViewSide::Top));
    assert_eq!(descriptor.ports[1].view.side, Some(PortViewSide::Right));
    assert_eq!(descriptor.ports[1].view.group.as_deref(), Some("result"));
    assert_eq!(descriptor.ports[2].view.side, Some(PortViewSide::Left));
    assert_eq!(descriptor.ports[2].view.anchor.as_deref(), Some("field.id"));
    assert_eq!(descriptor.ports[2].view.lane.as_deref(), Some("fields"));
    assert_eq!(descriptor.ports[2].view.slot.as_deref(), Some("id"));
    assert_eq!(
        descriptor.ports[2].view.visibility,
        Some(PortHandleVisibility::Hidden)
    );
    assert_eq!(descriptor.ports[3].view.side, Some(PortViewSide::Bottom));
    assert_eq!(
        descriptor.ports[3].view.anchor.as_deref(),
        Some("footer.summary")
    );
}

#[test]
fn builder_helpers_match_explicit_port_view_descriptor_construction() {
    let explicit = PortDecl::data_output("result").with_view(PortViewDescriptor {
        side: Some(PortViewSide::Right),
        order: Some(3),
        group: Some("output".to_owned()),
        anchor: Some("row.result".to_owned()),
        lane: None,
        slot: None,
        label: None,
        icon_key: None,
        visibility: None,
    });
    let helper = PortDecl::data_output("result")
        .on_right()
        .with_view_order(3)
        .with_view_group("output")
        .with_view_anchor("row.result");

    assert_eq!(helper, explicit);
}
