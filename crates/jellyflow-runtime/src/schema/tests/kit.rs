use jellyflow_core::core::EdgeKind;
use jellyflow_core::core::NodeKindKey;

use crate::schema::kit::{
    NodeKitContentDensity, NodeKitKey, builtin_node_kits, erd_table_manifest,
    mind_map_knowledge_canvas_manifest, workflow_automation_manifest,
};

#[test]
fn builtin_node_kits_register_the_first_three_families() {
    let registry = builtin_node_kits();

    let manifests = registry.manifests().collect::<Vec<_>>();
    assert_eq!(manifests.len(), 3);
    assert_eq!(manifests[0].key, NodeKitKey::new("erd.table"));
    assert_eq!(
        manifests[1].key,
        NodeKitKey::new("mind-map.knowledge-canvas")
    );
    assert_eq!(manifests[2].key, NodeKitKey::new("workflow.automation"));
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
    assert_eq!(recipe.surface_slots.len(), 5);
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
