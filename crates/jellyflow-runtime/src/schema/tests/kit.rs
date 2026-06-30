use jellyflow_core::core::{EdgeKind, Graph, NodeId, NodeKindKey, PortId};
use jellyflow_core::types::{DefaultTypeCompatibility, TypeDesc};

use crate::rules::plan_connect_typed;
use crate::schema::NodeChromeKind;
use crate::schema::NodeSurfaceSlotKind;
use crate::schema::kit::{
    NodeKitContentDensity, NodeKitKey, builtin_node_kits, erd_table_manifest,
    mind_map_knowledge_canvas_manifest, shader_blueprint_manifest, workflow_automation_manifest,
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
