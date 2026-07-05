use jellyflow_core::core::{
    CanvasRect, CanvasSize, EdgeKind, Graph, Node, NodeId, NodeKindKey, PortId,
};
use jellyflow_core::types::{DefaultTypeCompatibility, TypeDesc};

use crate::rules::plan_connect_typed;
use crate::schema::NodeChromeKind;
use crate::schema::NodeControlKind;
use crate::schema::kit::{
    NodeKitContentDensity, NodeKitKey, builtin_node_kits, erd_table_manifest,
    mind_map_knowledge_canvas_manifest, shader_blueprint_manifest, workflow_automation_manifest,
};
use crate::schema::{
    NodeActionDescriptor, NodeRepeatableCollectionDescriptor, NodeSurfaceOverflowIndicator,
    NodeSurfaceSlotKind,
};

#[test]
fn builtin_node_kits_register_the_first_three_families() {
    let registry = builtin_node_kits();

    let manifests = registry.manifests().collect::<Vec<_>>();
    assert_eq!(manifests.len(), 4);
    assert_eq!(manifests[0].key, NodeKitKey::new("erd.table"));
    assert_eq!(
        manifests[1].key,
        NodeKitKey::new("mind-map.knowledge-canvas")
    );
    assert_eq!(manifests[2].key, NodeKitKey::new("shader.blueprint"));
    assert_eq!(manifests[3].key, NodeKitKey::new("workflow.automation"));
}

#[test]
fn builtin_repeatable_collection_action_refs_resolve_to_descriptors() {
    let registry = builtin_node_kits();

    for manifest in registry.manifests() {
        for recipe in &manifest.recipes {
            for collection in &recipe.repeatable_collections {
                assert_collection_action_refs_resolve(&recipe.actions, collection, &recipe.kind.0);
            }
            for blackboard in &recipe.blackboards {
                assert_collection_action_refs_resolve(
                    &recipe.actions,
                    &blackboard.collection,
                    &recipe.kind.0,
                );
                for action_key in &blackboard.action_keys {
                    assert_action_ref_resolves(&recipe.actions, &recipe.kind.0, action_key);
                }
            }
            for menu in &recipe.menus {
                for action_key in &menu.action_keys {
                    assert_action_ref_resolves(&recipe.actions, &recipe.kind.0, action_key);
                }
            }
            for inspector in &recipe.inspectors {
                for action_key in &inspector.action_keys {
                    assert_action_ref_resolves(&recipe.actions, &recipe.kind.0, action_key);
                }
            }
        }
    }
}

#[test]
fn workflow_automation_fixture_materializes_to_graph() {
    let manifest = workflow_automation_manifest();
    let graph = manifest
        .build_fixture_graph("workflow.review")
        .expect("workflow fixture graph");

    assert_eq!(graph.nodes().len(), 4);
    assert_eq!(graph.edges().len(), 3);
    assert!(
        graph
            .nodes()
            .values()
            .any(|node| node.kind == NodeKindKey::new("demo.decision"))
    );
    assert!(
        graph
            .edges()
            .values()
            .any(|edge| edge.kind == EdgeKind::Exec)
    );
}

#[test]
fn kit_registry_resolves_builtin_recipes() {
    let registry = builtin_node_kits();
    let recipe = registry
        .recipe_for_kind(&NodeKindKey::new("demo.table"))
        .expect("table recipe");

    assert_eq!(recipe.renderer_key.as_deref(), Some("table-card"));
    assert_eq!(recipe.surface_slots.len(), 6);
    assert!(
        recipe
            .surface_slots
            .iter()
            .any(|slot| slot.kind == NodeSurfaceSlotKind::MetricBadge)
    );
}

#[test]
fn workflow_llm_recipe_exposes_adapter_owned_chrome_semantics() {
    let registry = builtin_node_kits();
    let recipe = registry
        .recipe_for_kind(&NodeKindKey::new("demo.llm"))
        .expect("llm recipe");

    let chrome_kinds: Vec<_> = recipe.chrome.iter().map(|chrome| chrome.kind).collect();
    assert_eq!(
        chrome_kinds,
        vec![
            NodeChromeKind::Resizer,
            NodeChromeKind::Toolbar,
            NodeChromeKind::StatusStrip,
            NodeChromeKind::RunActionStrip,
        ]
    );
    assert!(
        recipe
            .chrome
            .iter()
            .any(|chrome| chrome.renderer_key.as_deref() == Some("run-actions"))
    );
    assert!(
        recipe
            .chrome
            .iter()
            .any(|chrome| chrome.key == "status.run" && !chrome.interactive)
    );
}

#[test]
fn workflow_llm_recipe_describes_dify_style_config_status_and_metrics() {
    let registry = builtin_node_kits();
    let recipe = registry
        .recipe_for_kind(&NodeKindKey::new("demo.llm"))
        .expect("llm recipe");
    let kinds = recipe
        .surface_slots
        .iter()
        .map(|slot| slot.kind)
        .collect::<Vec<_>>();

    assert!(kinds.contains(&NodeSurfaceSlotKind::ConfigGroup));
    assert!(kinds.contains(&NodeSurfaceSlotKind::StatusBanner));
    assert!(kinds.contains(&NodeSurfaceSlotKind::MetricBadge));
    assert!(
        recipe
            .surface_slots
            .iter()
            .any(|slot| slot.key == "config.model" && slot.slot.as_deref() == Some("config.model"))
    );
    let controls = recipe
        .surface_slots
        .iter()
        .flat_map(|slot| slot.controls.iter())
        .collect::<Vec<_>>();
    assert!(
        controls
            .iter()
            .any(|control| control.kind == NodeControlKind::TextArea)
    );
    assert!(
        controls
            .iter()
            .any(|control| control.kind == NodeControlKind::Select)
    );
    assert!(
        controls
            .iter()
            .any(|control| control.kind == NodeControlKind::VariablePicker)
    );
    assert!(
        controls
            .iter()
            .any(|control| control.kind == NodeControlKind::Toggle)
    );
    assert!(
        controls
            .iter()
            .any(|control| control.key == "control.model"
                && control.data_key() == Some("meta.model"))
    );
    assert!(
        recipe
            .actions
            .iter()
            .any(|action| action.key == "action.llm.run")
    );
    assert!(recipe.menus.iter().any(|menu| {
        menu.key == "menu.dropped_wire.llm" && menu.action_keys == vec!["action.insert.llm"]
    }));
    assert!(
        recipe
            .inspectors
            .iter()
            .any(|inspector| inspector.key == "inspector.llm")
    );
    let params = recipe
        .repeatable_collections
        .iter()
        .find(|collection| collection.key == "llm.params")
        .expect("llm parameter collection")
        .item_projections(&recipe.default_data);
    assert_eq!(params[0].anchor, "param.topic");
    assert_eq!(params[1].item_id, "priority");
}

#[test]
fn builtin_kits_expose_first_authoring_control_fixtures() {
    let registry = builtin_node_kits();
    let table = registry
        .recipe_for_kind(&NodeKindKey::new("demo.table"))
        .expect("table recipe");
    let shader_mix = registry
        .recipe_for_kind(&NodeKindKey::new("demo.shader.mix"))
        .expect("shader mix recipe");
    let texture = registry
        .recipe_for_kind(&NodeKindKey::new("demo.shader.texture_sample"))
        .expect("texture sample recipe");

    assert!(
        table
            .surface_slots
            .iter()
            .flat_map(|slot| slot.controls.iter())
            .any(|control| control.kind == NodeControlKind::PortBinding)
    );
    assert!(
        table
            .surface_slots
            .iter()
            .flat_map(|slot| slot.controls.iter())
            .any(|control| control.kind == NodeControlKind::Select
                && control.data_key() == Some("schema.field.type"))
    );
    let columns = table
        .repeatable_collections
        .iter()
        .find(|collection| collection.key == "table.columns")
        .expect("table columns collection")
        .item_projections(&table.default_data);
    assert_eq!(columns[0].anchor, "field.column.id");
    assert_eq!(columns[1].port_key.as_deref(), Some("field_email"));
    assert!(
        table
            .actions
            .iter()
            .any(|action| action.key == "action.column.add")
    );
    assert!(
        table
            .inspectors
            .iter()
            .any(|inspector| inspector.key == "inspector.column.email")
    );
    assert!(
        shader_mix
            .surface_slots
            .iter()
            .flat_map(|slot| slot.controls.iter())
            .any(|control| control.kind == NodeControlKind::Slider
                && control.data_key() == Some("config.factor.default"))
    );
    let dynamic_inputs = shader_mix
        .repeatable_collections
        .iter()
        .find(|collection| collection.key == "shader.inputs")
        .expect("shader inputs collection")
        .item_projections(&shader_mix.default_data);
    assert_eq!(dynamic_inputs[0].anchor, "rail.inputs.a");
    assert_eq!(dynamic_inputs[2].port_key.as_deref(), Some("factor"));
    assert!(
        shader_mix
            .blackboards
            .iter()
            .any(|blackboard| blackboard.key == "blackboard.shader.properties")
    );
    assert!(
        texture
            .surface_slots
            .iter()
            .flat_map(|slot| slot.controls.iter())
            .any(|control| control.kind == NodeControlKind::Asset)
    );
}

#[test]
fn knowledge_kits_expose_title_source_and_preview_controls() {
    let registry = builtin_node_kits();
    let topic = registry
        .recipe_for_kind(&NodeKindKey::new("demo.topic"))
        .expect("topic recipe");
    let idea = registry
        .recipe_for_kind(&NodeKindKey::new("demo.idea"))
        .expect("idea recipe");
    let source = registry
        .recipe_for_kind(&NodeKindKey::new("demo.source"))
        .expect("source recipe");

    assert!(
        topic
            .surface_slots
            .iter()
            .flat_map(|slot| slot.controls.iter())
            .any(|control| control.key == "control.topic.title"
                && control.data_key() == Some("title"))
    );
    assert!(
        idea.surface_slots
            .iter()
            .flat_map(|slot| slot.controls.iter())
            .any(|control| control.key == "control.idea.title")
    );
    assert!(
        source
            .surface_slots
            .iter()
            .flat_map(|slot| slot.controls.iter())
            .any(|control| control.key == "control.source.asset"
                && control.data_key() == Some("preview"))
    );
}

#[test]
fn shader_blueprint_fixture_materializes_typed_port_rail_nodes() {
    let manifest = shader_blueprint_manifest();
    let graph = manifest
        .build_fixture_graph("shader.material_mix")
        .expect("shader fixture graph");
    let mix = manifest
        .recipe_for_kind(&NodeKindKey::new("demo.shader.mix"))
        .expect("mix recipe");

    assert_eq!(graph.nodes().len(), 2);
    assert_eq!(graph.edges().len(), 1);
    assert!(mix.surface_slots.iter().any(|slot| {
        slot.kind == NodeSurfaceSlotKind::PortRail && slot.anchor.as_deref() == Some("rail.inputs")
    }));
    assert!(mix.surface_slots.iter().any(|slot| {
        slot.kind == NodeSurfaceSlotKind::ConfigGroup
            && slot.anchor.as_deref() == Some("config.factor")
    }));
    assert!(mix.surface_slots.iter().any(|slot| {
        slot.kind == NodeSurfaceSlotKind::Preview
            && slot.anchor.as_deref() == Some("preview.result")
    }));
}

#[test]
fn shader_blueprint_fixture_types_reject_invalid_targets() {
    let graph = shader_blueprint_manifest()
        .build_fixture_graph("shader.material_mix")
        .expect("shader fixture graph");
    let texture = find_node(&graph, "demo.shader.texture_sample");
    let mix = find_node(&graph, "demo.shader.mix");
    let color = find_port(&graph, texture, "color");
    let mix_b = find_port(&graph, mix, "b");
    let factor = find_port(&graph, mix, "factor");

    assert_eq!(port_type(&graph, color), Some(shader_vec(4)));
    assert_eq!(port_type(&graph, mix_b), Some(shader_vec(4)));
    assert_eq!(port_type(&graph, factor), Some(TypeDesc::Float));

    let mut compatible = DefaultTypeCompatibility;
    let valid_plan = plan_connect_typed(
        &graph,
        color,
        mix_b,
        |graph, port| graph.ports().get(&port).and_then(|port| port.ty.clone()),
        &mut compatible,
    );
    assert!(valid_plan.is_accept());

    let mut incompatible = DefaultTypeCompatibility;
    let invalid_plan = plan_connect_typed(
        &graph,
        color,
        factor,
        |graph, port| graph.ports().get(&port).and_then(|port| port.ty.clone()),
        &mut incompatible,
    );
    assert!(invalid_plan.is_reject());
    assert!(invalid_plan.ops().is_empty());
}

#[test]
fn kit_layout_hints_drive_density_thresholds() {
    let registry = builtin_node_kits();
    let hints = registry
        .layout_hints_for_kind(&NodeKindKey::new("demo.llm"))
        .expect("llm layout hints");

    assert_eq!(
        hints.content_density_for_zoom(0.10),
        NodeKitContentDensity::Compact
    );
    assert_eq!(
        hints.content_density_for_zoom(0.75),
        NodeKitContentDensity::Regular
    );
    assert_eq!(
        hints.content_density_for_zoom(0.95),
        NodeKitContentDensity::Full
    );
}

#[test]
fn builtin_product_kits_expose_readable_layout_budgets() {
    let registry = builtin_node_kits();
    let cases = [
        (
            "demo.llm",
            CanvasSize {
                width: 340.0,
                height: 288.0,
            },
            CanvasSize {
                width: 364.0,
                height: 312.0,
            },
            Some(3),
            Some(4),
            Some(3),
        ),
        (
            "demo.table",
            CanvasSize {
                width: 420.0,
                height: 330.0,
            },
            CanvasSize {
                width: 448.0,
                height: 356.0,
            },
            Some(4),
            Some(4),
            Some(2),
        ),
        (
            "demo.shader.texture_sample",
            CanvasSize {
                width: 360.0,
                height: 272.0,
            },
            CanvasSize {
                width: 384.0,
                height: 296.0,
            },
            Some(3),
            Some(3),
            Some(2),
        ),
        (
            "demo.shader.mix",
            CanvasSize {
                width: 360.0,
                height: 272.0,
            },
            CanvasSize {
                width: 392.0,
                height: 340.0,
            },
            Some(3),
            Some(3),
            Some(2),
        ),
        (
            "demo.topic",
            CanvasSize {
                width: 320.0,
                height: 220.0,
            },
            CanvasSize {
                width: 344.0,
                height: 240.0,
            },
            None,
            Some(3),
            Some(2),
        ),
        (
            "demo.source",
            CanvasSize {
                width: 328.0,
                height: 220.0,
            },
            CanvasSize {
                width: 352.0,
                height: 240.0,
            },
            None,
            Some(3),
            Some(2),
        ),
    ];

    for (
        kind,
        min_readable_size,
        preferred_size,
        repeatable_visible_items,
        slot_line_budget,
        control_line_budget,
    ) in cases
    {
        let recipe = registry
            .recipe_for_kind(&NodeKindKey::new(kind))
            .expect("builtin product recipe");
        let budget = &recipe.layout_budget;

        assert_eq!(budget.min_readable_size, Some(min_readable_size), "{kind}");
        assert_eq!(budget.preferred_size, Some(preferred_size), "{kind}");
        assert_eq!(
            budget.repeatable_visible_items, repeatable_visible_items,
            "{kind}"
        );
        assert_eq!(budget.slot_line_budget, slot_line_budget, "{kind}");
        assert_eq!(budget.control_line_budget, control_line_budget, "{kind}");
        assert_eq!(
            budget.overflow_indicator,
            Some(NodeSurfaceOverflowIndicator::Count),
            "{kind}"
        );
        assert_eq!(
            budget.density_priority.as_slice(),
            &[
                NodeKitContentDensity::Full,
                NodeKitContentDensity::Regular,
                NodeKitContentDensity::Compact,
            ],
            "{kind}"
        );
    }
}

fn find_node(graph: &Graph, kind: &str) -> NodeId {
    graph
        .nodes()
        .iter()
        .find_map(|(node_id, node)| (node.kind == NodeKindKey::new(kind)).then_some(*node_id))
        .expect("node kind exists in fixture")
}

fn find_port(graph: &Graph, node_id: NodeId, key: &str) -> PortId {
    graph
        .ports()
        .iter()
        .find_map(|(port_id, port)| (port.node == node_id && port.key.0 == key).then_some(*port_id))
        .expect("port exists on fixture node")
}

fn port_type(graph: &Graph, port_id: PortId) -> Option<TypeDesc> {
    graph.ports().get(&port_id).and_then(|port| port.ty.clone())
}

fn shader_vec(width: u8) -> TypeDesc {
    TypeDesc::Opaque {
        key: format!("shader.vec{width}"),
        params: Vec::new(),
    }
}

fn assert_collection_action_refs_resolve(
    actions: &[NodeActionDescriptor],
    collection: &NodeRepeatableCollectionDescriptor,
    recipe_kind: &str,
) {
    for action_key in [
        collection.add_action.as_deref(),
        collection.remove_action.as_deref(),
        collection.reorder_action.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        assert_action_ref_resolves(actions, recipe_kind, action_key);
    }
}

fn assert_action_ref_resolves(
    actions: &[NodeActionDescriptor],
    recipe_kind: &str,
    action_key: &str,
) {
    assert!(
        actions.iter().any(|action| action.key == action_key),
        "{recipe_kind} references missing action `{action_key}`"
    );
}

#[test]
fn builtin_erd_fixture_materializes_to_graph() {
    let graph = erd_table_manifest()
        .build_fixture_graph("erd.customer_orders")
        .expect("erd fixture graph");

    assert_eq!(graph.nodes().len(), 3);
    assert_eq!(graph.edges().len(), 2);
    assert!(
        graph
            .nodes()
            .values()
            .any(|node| node.kind == NodeKindKey::new("demo.table"))
    );
    assert!(
        graph
            .edges()
            .values()
            .all(|edge| edge.kind == EdgeKind::Data)
    );
}

#[test]
fn builtin_mind_map_fixture_materializes_to_graph() {
    let graph = mind_map_knowledge_canvas_manifest()
        .build_fixture_graph("mind-map.strategy")
        .expect("mind map fixture graph");

    assert_eq!(graph.nodes().len(), 4);
    assert_eq!(graph.edges().len(), 3);
    assert!(
        graph
            .nodes()
            .values()
            .any(|node| node.kind == NodeKindKey::new("demo.topic"))
    );
    assert!(
        graph
            .nodes()
            .values()
            .any(|node| node.kind == NodeKindKey::new("demo.source"))
    );
}

#[test]
fn builtin_mind_map_fixture_materializes_non_overlapping_node_bounds() {
    let graph = mind_map_knowledge_canvas_manifest()
        .build_fixture_graph("mind-map.strategy")
        .expect("mind map fixture graph");

    let rects = graph
        .nodes()
        .iter()
        .map(|(id, node)| (*id, node_default_rect(node)))
        .collect::<Vec<_>>();

    for (index, (left_id, left_rect)) in rects.iter().enumerate() {
        for (right_id, right_rect) in rects.iter().skip(index + 1) {
            assert_ne!(
                left_rect.origin, right_rect.origin,
                "mind map fixture nodes {left_id:?} and {right_id:?} share an origin"
            );
            assert!(
                !rects_overlap(*left_rect, *right_rect),
                "mind map fixture nodes {left_id:?} and {right_id:?} overlap: {left_rect:?} vs {right_rect:?}"
            );
        }
    }
}

fn node_default_rect(node: &Node) -> CanvasRect {
    CanvasRect {
        origin: node.pos,
        size: node.size.unwrap_or(CanvasSize {
            width: 228.0,
            height: 168.0,
        }),
    }
}

fn rects_overlap(left: CanvasRect, right: CanvasRect) -> bool {
    let left_max_x = left.origin.x + left.size.width;
    let left_max_y = left.origin.y + left.size.height;
    let right_max_x = right.origin.x + right.size.width;
    let right_max_y = right.origin.y + right.size.height;

    left.origin.x < right_max_x
        && left_max_x > right.origin.x
        && left.origin.y < right_max_y
        && left_max_y > right.origin.y
}
