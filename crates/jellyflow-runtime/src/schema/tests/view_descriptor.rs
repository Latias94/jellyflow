use serde_json::json;

use crate::schema::{
    ActionAvailability, ActionIntent, ActionShortcut, ActionTarget, BlackboardDescriptor,
    InspectorDescriptor, InspectorTarget, MenuDescriptor, MenuSurface, NodeActionDescriptor,
    NodeChromeDescriptor, NodeChromeKind, NodeChromePlacement, NodeChromeVisibility,
    NodeControlBinding, NodeControlDescriptor, NodeControlEditability, NodeControlKind,
    NodeControlOption, NodeControlOptionSource, NodeControlPresentation, NodeControlValidation,
    NodeControlValidationRule, NodeKitContentDensity, NodeRegistry, NodeRepeatableAnchorRule,
    NodeRepeatableCollectionDescriptor, NodeSchema, NodeSurfaceProjection,
    NodeSurfaceSlotDescriptor, NodeSurfaceSlotKind, NodeSurfaceSlotVisibility, PortDecl,
    PortHandleVisibility, PortViewDescriptor, PortViewSide,
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
        surface_slots: vec![
            NodeSurfaceSlotDescriptor::header("header.main")
                .with_label("Title")
                .with_order(0),
            NodeSurfaceSlotDescriptor::field_row("field.source")
                .with_label("Source")
                .with_anchor("field.source")
                .with_lane("fields")
                .with_slot("source")
                .with_icon_key("file-text"),
        ],
        repeatable_collections: Vec::new(),
        actions: Vec::new(),
        menus: Vec::new(),
        inspectors: Vec::new(),
        blackboards: Vec::new(),
        chrome: Vec::new(),
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
        surface_slots: Vec::new(),
        repeatable_collections: Vec::new(),
        actions: Vec::new(),
        menus: Vec::new(),
        inspectors: Vec::new(),
        blackboards: Vec::new(),
        chrome: Vec::new(),
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
    assert_eq!(descriptors[1].surface_slots.len(), 2);
    assert_eq!(
        descriptors[1].surface_slots[0].kind,
        NodeSurfaceSlotKind::Header
    );
    assert_eq!(descriptors[1].surface_slots[1].key.as_str(), "field.source");
    assert_eq!(
        descriptors[1].surface_slots[1].lane.as_deref(),
        Some("fields")
    );
    assert_eq!(descriptors[1].default_data, json!({ "body": "" }));

    let alias_descriptor = registry
        .view_descriptor(&NodeKindKey::new("demo.sticky"))
        .expect("descriptor by alias");
    assert_eq!(alias_descriptor.kind, NodeKindKey::new("demo.note"));
    assert_eq!(alias_descriptor.renderer_key, "note-card");
    assert_eq!(alias_descriptor.surface_slots.len(), 2);
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

#[test]
fn node_surface_slot_descriptors_cover_semantic_slots_without_framework_widgets() {
    let schema = NodeSchema::builder("demo.workflow_card", "Workflow Card")
        .surface_slot(NodeSurfaceSlotDescriptor::header("header.main").with_order(0))
        .surface_slot(
            NodeSurfaceSlotDescriptor::badge("badge.status")
                .with_label("Status")
                .with_renderer_key("status-badge")
                .with_visibility(NodeSurfaceSlotVisibility::Visible),
        )
        .surface_slot(
            NodeSurfaceSlotDescriptor::action_row("actions.primary")
                .with_label("Actions")
                .collapsed(),
        )
        .surface_slot(
            NodeSurfaceSlotDescriptor::status_banner("status.validation")
                .with_label("Validation")
                .with_slot("status.validation")
                .with_renderer_key("status-banner"),
        )
        .surface_slot(
            NodeSurfaceSlotDescriptor::config_group("config.model")
                .with_label("Model config")
                .with_slot("config.model"),
        )
        .surface_slot(
            NodeSurfaceSlotDescriptor::port_rail("rail.typed_inputs")
                .with_label("Inputs")
                .with_anchor("rail.typed_inputs"),
        )
        .surface_slot(
            NodeSurfaceSlotDescriptor::metric_badge("metric.cost")
                .with_label("Cost")
                .with_slot("metrics.cost"),
        )
        .build();

    let mut registry = NodeRegistry::new();
    registry.register(schema);
    let descriptor = registry
        .view_descriptor(&NodeKindKey::new("demo.workflow_card"))
        .expect("descriptor");

    assert_eq!(descriptor.surface_slots.len(), 7);
    assert_eq!(
        descriptor.surface_slots[0],
        NodeSurfaceSlotDescriptor::header("header.main").with_order(0)
    );
    assert_eq!(descriptor.surface_slots[1].kind, NodeSurfaceSlotKind::Badge);
    assert_eq!(
        descriptor.surface_slots[1].renderer_key.as_deref(),
        Some("status-badge")
    );
    assert_eq!(
        descriptor.surface_slots[2].visibility,
        Some(NodeSurfaceSlotVisibility::Collapsed)
    );
    assert_eq!(
        descriptor.surface_slots[3].kind,
        NodeSurfaceSlotKind::StatusBanner
    );
    assert_eq!(
        descriptor.surface_slots[4].kind,
        NodeSurfaceSlotKind::ConfigGroup
    );
    assert_eq!(
        descriptor.surface_slots[5].kind,
        NodeSurfaceSlotKind::PortRail
    );
    assert_eq!(
        descriptor.surface_slots[6].kind,
        NodeSurfaceSlotKind::MetricBadge
    );
}

#[test]
fn node_control_descriptors_cover_authoring_controls_without_framework_widgets() {
    let controls = vec![
        NodeControlDescriptor::text_input("control.title")
            .with_label("Title")
            .with_binding(NodeControlBinding::data_path("title"))
            .required(),
        NodeControlDescriptor::select("control.model")
            .with_label("Model")
            .with_binding(NodeControlBinding::slot("config.model"))
            .with_options([
                NodeControlOption::new("gpt-4.1", "GPT 4.1"),
                NodeControlOption::new("local", "Local").disabled(),
            ]),
        NodeControlDescriptor::toggle("control.stream")
            .with_label("Stream")
            .with_binding(NodeControlBinding::data_path("config.stream")),
        NodeControlDescriptor::code("control.system_prompt")
            .with_label("System")
            .with_binding(NodeControlBinding::json_pointer("/prompts/system"))
            .with_language("jinja"),
        NodeControlDescriptor::color("control.tint")
            .with_label("Tint")
            .with_binding(NodeControlBinding::data_path("style.tint")),
        NodeControlDescriptor::asset("control.texture")
            .with_label("Texture")
            .with_binding(NodeControlBinding::data_path("asset.texture"))
            .with_option_source(NodeControlOptionSource::Assets),
        NodeControlDescriptor::variable_picker("control.variable")
            .with_label("Variable")
            .with_binding(NodeControlBinding::graph_symbol("inputs.topic"))
            .with_option_source(NodeControlOptionSource::Variables),
        NodeControlDescriptor::text_area("control.prompt")
            .with_label("Prompt")
            .with_binding(NodeControlBinding::slot("prompt"))
            .with_placeholder("Write the prompt"),
    ];

    let encoded = serde_json::to_value(&controls).expect("serialize controls");
    let decoded: Vec<NodeControlDescriptor> =
        serde_json::from_value(encoded).expect("deserialize controls");

    assert_eq!(decoded, controls);
    assert_eq!(decoded[0].kind, NodeControlKind::TextInput);
    assert_eq!(decoded[1].options[1].label, "Local");
    assert!(decoded[1].options[1].disabled);
    assert_eq!(decoded[3].presentation.language.as_deref(), Some("jinja"));
    assert!(decoded[7].presentation.multiline);
}

#[test]
fn node_control_projection_preserves_binding_validation_and_editability() {
    let schema = NodeSchema::builder("demo.authoring", "Authoring")
        .surface_slot(
            NodeSurfaceSlotDescriptor::field_row("field.prompt")
                .with_label("Prompt")
                .with_slot("prompt")
                .with_order(0)
                .with_control(
                    NodeControlDescriptor::text_area("control.prompt")
                        .with_label("Prompt")
                        .with_binding(NodeControlBinding::slot("prompt"))
                        .with_validation(
                            NodeControlValidation::default()
                                .required()
                                .with_rule(NodeControlValidationRule::Regex {
                                    pattern: ".+".to_owned(),
                                })
                                .with_rule(NodeControlValidationRule::EnumValues {
                                    values: vec![json!("short"), json!("long")],
                                })
                                .with_rule(NodeControlValidationRule::Range {
                                    min: Some(1.0),
                                    max: Some(4096.0),
                                })
                                .with_rule(NodeControlValidationRule::ExpressionShape {
                                    language: Some("jinja".to_owned()),
                                }),
                        )
                        .with_editability(
                            NodeControlEditability::default()
                                .read_only()
                                .disabled("Prompt is inherited"),
                        ),
                ),
        )
        .surface_slot(
            NodeSurfaceSlotDescriptor::config_group("config.model")
                .with_label("Model")
                .with_slot("config.model")
                .with_order(1)
                .with_controls([
                    NodeControlDescriptor::select("control.model")
                        .with_label("Model")
                        .with_binding(NodeControlBinding::data_path("config.model"))
                        .with_options([
                            NodeControlOption::new("small", "Small"),
                            NodeControlOption::new("large", "Large"),
                        ]),
                    NodeControlDescriptor::slider("control.temperature")
                        .with_label("Temperature")
                        .with_binding(NodeControlBinding::data_path("config.temperature"))
                        .with_validation_rule(NodeControlValidationRule::Range {
                            min: Some(0.0),
                            max: Some(2.0),
                        })
                        .with_presentation(
                            NodeControlPresentation::default()
                                .with_unit("temp")
                                .compact_label(),
                        ),
                    NodeControlDescriptor::port_binding("control.prompt_port")
                        .with_label("Prompt port")
                        .with_binding(NodeControlBinding::port_anchor("field.prompt")),
                ]),
        )
        .build();
    let mut registry = NodeRegistry::new();
    registry.register(schema);
    let descriptor = registry
        .view_descriptor(&NodeKindKey::new("demo.authoring"))
        .expect("descriptor");
    let node_data = json!({
        "prompt": "Summarize {{topic}}",
        "config": {
            "model": "large",
            "temperature": 0.7
        }
    });

    let compact = descriptor.surface_slots_projection(
        &node_data,
        Some(&crate::schema::kit::NodeKitLayoutHints::default().with_zoom_range(0.3, 0.9)),
        0.1,
    );

    assert_eq!(compact.len(), 2);
    assert_eq!(compact[0].value, "Summarize {{topic}}");
    assert_eq!(compact[0].controls[0].key, "control.prompt");
    assert_eq!(compact[0].controls[0].data_key(), Some("prompt"));
    assert_eq!(compact[0].controls[0].validation.rules.len(), 5);
    assert!(compact[0].controls[0].editability.read_only);
    assert_eq!(
        compact[0].controls[0]
            .editability
            .disabled_reason
            .as_deref(),
        Some("Prompt is inherited")
    );
    assert_eq!(compact[1].controls.len(), 3);
    assert_eq!(compact[1].controls[0].data_key(), Some("config.model"));
    assert_eq!(
        compact[1].controls[1].presentation.unit.as_deref(),
        Some("temp")
    );
    assert_eq!(compact[1].controls[2].data_key(), None);
}

#[test]
fn repeatable_collections_project_stable_item_slots_and_anchors() {
    let schema = NodeSchema::builder("demo.shader.dynamic", "Dynamic shader")
        .repeatable_collection(
            NodeRepeatableCollectionDescriptor::new("shader.inputs", "inputs", "id")
                .with_label("Inputs")
                .with_empty_label("No inputs")
                .with_item_template_slot(
                    NodeSurfaceSlotDescriptor::field_row("input")
                        .with_label("Input")
                        .with_slot("inputs")
                        .with_control(
                            NodeControlDescriptor::select("control.input.type")
                                .with_label("Type")
                                .with_binding(NodeControlBinding::data_path("ty"))
                                .with_options([
                                    NodeControlOption::new("float", "Float"),
                                    NodeControlOption::new("vec4", "Vec4"),
                                ]),
                        ),
                )
                .with_anchor_rule(
                    NodeRepeatableAnchorRule::new("rail.input", "rail.input")
                        .with_port_key_path("port"),
                )
                .with_min_items(1)
                .with_max_items(3)
                .reorderable()
                .with_add_action("action.input.add")
                .with_remove_action("action.input.remove")
                .with_reorder_action("action.input.reorder"),
        )
        .build();
    let mut registry = NodeRegistry::new();
    registry.register(schema);
    let descriptor = registry
        .view_descriptor(&NodeKindKey::new("demo.shader.dynamic"))
        .expect("descriptor");
    let before = json!({
        "inputs": [
            { "id": "albedo", "name": "Albedo", "ty": "vec4", "port": "in_albedo" },
            { "id": "roughness", "name": "Roughness", "ty": "float", "port": "in_roughness" }
        ]
    });
    let after_add = json!({
        "inputs": [
            { "id": "albedo", "name": "Albedo", "ty": "vec4", "port": "in_albedo" },
            { "id": "roughness", "name": "Roughness", "ty": "float", "port": "in_roughness" },
            { "id": "normal", "name": "Normal", "ty": "vec4", "port": "in_normal" }
        ]
    });
    let after_reorder = json!({
        "inputs": [
            { "id": "roughness", "name": "Roughness", "ty": "float", "port": "in_roughness" },
            { "id": "albedo", "name": "Albedo", "ty": "vec4", "port": "in_albedo" },
            { "id": "normal", "name": "Normal", "ty": "vec4", "port": "in_normal" }
        ]
    });

    let before_items = descriptor.repeatable_items_projection(&before, "shader.inputs");
    let added_items = descriptor.repeatable_items_projection(&after_add, "shader.inputs");
    let reordered_items = descriptor.repeatable_items_projection(&after_reorder, "shader.inputs");

    assert_eq!(before_items.len(), 2);
    assert_eq!(before_items[0].slot_key, "rail.input.albedo");
    assert_eq!(before_items[0].anchor, "rail.input.albedo");
    assert_eq!(before_items[0].port_key.as_deref(), Some("in_albedo"));
    assert_eq!(before_items[0].slots[0].key, "rail.input.albedo.input");
    assert_eq!(
        before_items[0].slots[0].anchor.as_deref(),
        Some("rail.input.albedo.input")
    );
    assert_eq!(before_items[0].slots[0].controls[0].data_key(), Some("ty"));
    assert_eq!(added_items[0].anchor, "rail.input.albedo");
    assert_eq!(added_items[1].anchor, "rail.input.roughness");
    assert_eq!(added_items[2].anchor, "rail.input.normal");
    assert_eq!(reordered_items[0].anchor, "rail.input.roughness");
    assert_eq!(reordered_items[1].anchor, "rail.input.albedo");

    let numeric_id = json!({
        "inputs": [
            { "id": 42, "name": "Numeric", "ty": "float", "port": 99 }
        ]
    });
    let numeric_items = descriptor.repeatable_items_projection(&numeric_id, "shader.inputs");
    assert_eq!(numeric_items.len(), 1);
    assert_eq!(numeric_items[0].item_id, "42");
    assert_eq!(numeric_items[0].anchor, "rail.input.42");
    assert_eq!(numeric_items[0].port_key.as_deref(), Some("99"));

    let collection = descriptor
        .repeatable_collection("shader.inputs")
        .expect("collection");
    assert_eq!(
        collection.add_disabled_reason(&after_add).as_deref(),
        Some("Maximum of 3 items reached")
    );
    assert_eq!(
        collection
            .remove_disabled_reason(&json!({ "inputs": [{ "id": "albedo" }] }))
            .as_deref(),
        Some("Minimum of 1 items required")
    );
    assert!(collection.is_empty_for(&json!({ "inputs": [] })));
    assert!(
        descriptor
            .repeatable_items_projection(&json!({ "inputs": [] }), "shader.inputs")
            .is_empty(),
        "empty collections must not create fake item slots or handles"
    );
}

#[test]
fn action_menu_inspector_and_blackboard_descriptors_are_headless() {
    let insert_action = NodeActionDescriptor::new(
        "action.insert.llm",
        "Insert LLM",
        ActionTarget::DroppedWire {
            source_port_key: Some("completion".to_owned()),
        },
        ActionIntent::InsertNode {
            node_kind: "demo.llm".to_owned(),
        },
    )
    .with_group("create")
    .with_order(0)
    .with_icon_key("sparkles");
    let disabled_remove = NodeActionDescriptor::new(
        "action.param.remove",
        "Remove parameter",
        ActionTarget::RepeatableItem {
            collection_key: "params".to_owned(),
            item_id: "temperature".to_owned(),
        },
        ActionIntent::RemoveRepeatableItem {
            collection_key: "params".to_owned(),
            item_id: "temperature".to_owned(),
        },
    )
    .disabled("Minimum of 1 items required")
    .danger()
    .with_shortcut(ActionShortcut::new("Backspace").shift());
    let schema = NodeSchema::builder("demo.authoring_actions", "Actions")
        .action(insert_action.clone())
        .action(disabled_remove.clone())
        .menu(
            MenuDescriptor::new("menu.dropped_wire", MenuSurface::DroppedWire)
                .with_label("Insert compatible node")
                .with_action_key("action.insert.llm"),
        )
        .menu(
            MenuDescriptor::new("menu.node", MenuSurface::Node)
                .with_action_keys(["action.param.remove"]),
        )
        .inspector(
            InspectorDescriptor::new(
                "inspector.param.temperature",
                InspectorTarget::RepeatableItem {
                    collection_key: "params".to_owned(),
                    item_id: "temperature".to_owned(),
                },
            )
            .with_label("Temperature")
            .with_control(
                NodeControlDescriptor::slider("control.temperature")
                    .with_label("Temperature")
                    .with_binding(NodeControlBinding::data_path("value")),
            )
            .with_action_key("action.param.remove"),
        )
        .blackboard(BlackboardDescriptor::new(
            "blackboard.shader_properties",
            "Shader properties",
            NodeRepeatableCollectionDescriptor::new("shader.properties", "properties", "id")
                .with_item_template_slot(NodeSurfaceSlotDescriptor::field_row("property")),
        ))
        .build();
    let mut registry = NodeRegistry::new();
    registry.register(schema);
    let descriptor = registry
        .view_descriptor(&NodeKindKey::new("demo.authoring_actions"))
        .expect("descriptor");

    assert_eq!(descriptor.actions.len(), 2);
    assert!(
        descriptor
            .action("action.insert.llm")
            .expect("insert")
            .is_enabled()
    );
    assert_eq!(
        descriptor
            .action("action.param.remove")
            .expect("remove")
            .availability,
        ActionAvailability::disabled("Minimum of 1 items required")
    );
    assert_eq!(
        descriptor
            .menu("menu.dropped_wire")
            .expect("dropped menu")
            .action_keys,
        vec!["action.insert.llm"]
    );
    assert_eq!(
        descriptor
            .inspector("inspector.param.temperature")
            .expect("inspector")
            .controls[0]
            .data_key(),
        Some("value")
    );
    assert_eq!(
        descriptor
            .blackboard("blackboard.shader_properties")
            .expect("blackboard")
            .collection
            .key,
        "shader.properties"
    );

    let encoded = serde_json::to_value(&descriptor).expect("serialize descriptor");
    let decoded: crate::schema::NodeKindViewDescriptor =
        serde_json::from_value(encoded).expect("deserialize descriptor");
    assert_eq!(decoded.actions, descriptor.actions);
    assert_eq!(decoded.menus, descriptor.menus);
    assert_eq!(decoded.inspectors, descriptor.inspectors);
    assert_eq!(decoded.blackboards, descriptor.blackboards);
}

#[test]
fn node_chrome_descriptors_cover_adapter_owned_chrome_without_framework_widgets() {
    let schema = NodeSchema::builder("demo.llm", "LLM")
        .renderer_key("decision-card")
        .default_size(CanvasSize {
            width: 228.0,
            height: 196.0,
        })
        .chrome(NodeChromeDescriptor::resizer("resize.corner"))
        .chrome(
            NodeChromeDescriptor::toolbar("toolbar.primary", NodeChromePlacement::TopRight)
                .with_label("Tools")
                .with_renderer_key("node-toolbar")
                .with_icon_key("wrench")
                .with_order(10),
        )
        .chrome(
            NodeChromeDescriptor::status_strip("status.run", NodeChromePlacement::InsideFooter)
                .with_label("Run status")
                .with_renderer_key("run-status")
                .with_order(20),
        )
        .chrome(
            NodeChromeDescriptor::run_action_strip("actions.run", NodeChromePlacement::Bottom)
                .with_label("Run")
                .with_renderer_key("run-actions")
                .with_icon_key("play")
                .with_order(30),
        )
        .chrome(
            NodeChromeDescriptor::validation_banner(
                "validation.warning",
                NodeChromePlacement::InsideHeader,
            )
            .with_label("Warning")
            .hidden(),
        )
        .build();

    let mut registry = NodeRegistry::new();
    registry.register(schema);
    let descriptor = registry
        .view_descriptor(&NodeKindKey::new("demo.llm"))
        .expect("descriptor");

    assert_eq!(descriptor.chrome.len(), 5);
    assert_eq!(descriptor.chrome[0].kind, NodeChromeKind::Resizer);
    assert_eq!(
        descriptor.chrome[0].effective_visibility(),
        NodeChromeVisibility::Selected
    );
    assert!(descriptor.chrome[0].interactive);
    assert!(!descriptor.chrome[0].is_visible_for_state(false, false, false));
    assert!(descriptor.chrome[0].is_visible_for_state(true, false, false));

    let toolbar = &descriptor.chrome[1];
    assert_eq!(toolbar.kind, NodeChromeKind::Toolbar);
    assert_eq!(toolbar.placement, NodeChromePlacement::TopRight);
    assert_eq!(toolbar.label.as_deref(), Some("Tools"));
    assert_eq!(toolbar.renderer_key.as_deref(), Some("node-toolbar"));
    assert_eq!(toolbar.icon_key.as_deref(), Some("wrench"));
    assert!(toolbar.interactive);

    let status = &descriptor.chrome[2];
    assert_eq!(status.kind, NodeChromeKind::StatusStrip);
    assert_eq!(status.placement, NodeChromePlacement::InsideFooter);
    assert_eq!(status.effective_visibility(), NodeChromeVisibility::Always);
    assert!(status.is_visible_for_state(false, false, false));
    assert!(!status.interactive);

    let run = &descriptor.chrome[3];
    assert_eq!(run.kind, NodeChromeKind::RunActionStrip);
    assert_eq!(run.effective_visibility(), NodeChromeVisibility::Selected);
    assert!(run.interactive);

    let validation = &descriptor.chrome[4];
    assert_eq!(validation.kind, NodeChromeKind::ValidationBanner);
    assert_eq!(
        validation.effective_visibility(),
        NodeChromeVisibility::Hidden
    );
    assert!(!validation.is_visible_for_state(true, true, true));
}

#[test]
fn node_surface_projection_uses_layout_hints_for_density_and_slot_limits() {
    let schema = NodeSchema::builder("demo.surface", "Surface")
        .surface_slot(NodeSurfaceSlotDescriptor::header("header.main").with_order(0))
        .surface_slot(
            NodeSurfaceSlotDescriptor::field_row("field.tags")
                .with_label("Tags")
                .with_order(1),
        )
        .surface_slot(NodeSurfaceSlotDescriptor::action_row("actions.primary").with_order(2))
        .build();
    let mut registry = NodeRegistry::new();
    registry.register(schema);
    let descriptor = registry
        .view_descriptor(&NodeKindKey::new("demo.surface"))
        .expect("descriptor");
    let node_data = json!({
        "tags": ["root", "fallback"],
        "fields": {
            "tags": ["alpha", "beta", "gamma"]
        }
    });

    let compact = descriptor.surface_slots_projection(
        &node_data,
        Some(&crate::schema::kit::NodeKitLayoutHints::default().with_zoom_range(0.3, 0.9)),
        0.1,
    );
    let regular = descriptor.surface_slots_projection(
        &node_data,
        Some(&crate::schema::kit::NodeKitLayoutHints::default().with_zoom_range(0.3, 0.9)),
        0.5,
    );
    let full = descriptor.surface_slots_projection(
        &node_data,
        Some(&crate::schema::kit::NodeKitLayoutHints::default().with_zoom_range(0.3, 0.9)),
        1.0,
    );

    assert_eq!(compact.len(), 2);
    assert_eq!(regular.len(), 3);
    assert_eq!(full.len(), 3);
    assert_eq!(full[0].kind, NodeSurfaceSlotKind::Header);
    assert_eq!(compact[0].label, "main");
    assert_eq!(compact[1].label, "Tags");
    assert_eq!(compact[1].value, "alpha");
    assert_eq!(regular[1].value, "alpha");
    assert_eq!(full[1].value, "alpha · beta …");

    let projection = NodeSurfaceProjection::from_layout_hints(
        &crate::schema::kit::NodeKitLayoutHints::default().with_zoom_range(0.3, 0.9),
        1.0,
    );
    assert_eq!(projection.density, NodeKitContentDensity::Full);
}

#[test]
fn node_kind_view_descriptor_resolves_ports_and_slots_by_anchor() {
    let schema = NodeSchema::builder("demo.review_card", "Review Card")
        .port(
            PortDecl::data_input("assignee")
                .with_label("Assignee")
                .on_left()
                .with_view_anchor("field.assignee")
                .with_view_order(1),
        )
        .port(
            PortDecl::data_output("result")
                .with_label("Result")
                .on_right()
                .with_view_anchor("actions.primary")
                .with_view_order(0),
        )
        .surface_slot(
            NodeSurfaceSlotDescriptor::field_row("field.status")
                .with_label("Status")
                .with_anchor("field.status")
                .with_order(1),
        )
        .surface_slot(
            NodeSurfaceSlotDescriptor::field_row("field.assignee")
                .with_label("Assignee")
                .with_anchor("field.assignee")
                .with_order(0),
        )
        .surface_slot(
            NodeSurfaceSlotDescriptor::action_row("actions.primary")
                .with_label("Actions")
                .with_anchor("actions.primary")
                .with_order(2),
        )
        .build();

    let mut registry = NodeRegistry::new();
    registry.register(schema);
    let descriptor = registry
        .view_descriptor(&NodeKindKey::new("demo.review_card"))
        .expect("descriptor");

    assert_eq!(
        descriptor
            .port_decl("assignee")
            .map(|decl| decl.key.0.as_str()),
        Some("assignee")
    );
    assert_eq!(
        descriptor
            .port_decl_by_anchor("field.assignee")
            .map(|decl| decl.key.0.as_str()),
        Some("assignee")
    );
    assert_eq!(
        descriptor
            .surface_slot("field.assignee")
            .map(|slot| slot.display_label()),
        Some(Some("Assignee"))
    );

    let slots = descriptor.surface_slots_of_kind(NodeSurfaceSlotKind::FieldRow);
    assert_eq!(slots[0].key, "field.assignee");
    assert_eq!(slots[1].key, "field.status");
    assert_eq!(
        descriptor
            .surface_slot_by_anchor("actions.primary")
            .map(|slot| slot.key.as_str()),
        Some("actions.primary")
    );
}

#[test]
fn slot_and_port_visibility_helpers_follow_adapter_contract() {
    let hidden_port = PortViewDescriptor::left().hidden();
    let collapsed_port = PortViewDescriptor::right().collapsed();
    let hidden_slot = NodeSurfaceSlotDescriptor::badge("badge.priority").hidden();
    let collapsed_slot = NodeSurfaceSlotDescriptor::action_row("actions.primary").collapsed();

    assert!(hidden_port.is_hidden());
    assert!(collapsed_port.is_collapsed());
    assert!(hidden_port.is_hidden_or_collapsed());
    assert_eq!(
        hidden_port.resolved_side(PortDirection::Out),
        PortViewSide::Left
    );
    assert!(hidden_slot.is_hidden());
    assert!(collapsed_slot.is_collapsed());
    assert!(hidden_slot.is_hidden_or_collapsed());
    assert_eq!(hidden_slot.order_key(), i32::MAX);
    assert_eq!(hidden_slot.key_tail(), Some("priority"));
    assert_eq!(collapsed_slot.display_label(), Some("primary"));
    assert!(hidden_slot.is_hidden());
    assert!(collapsed_slot.is_collapsed());
}
