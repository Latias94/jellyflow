use jellyflow::prelude::*;
use jellyflow::{core, layout, runtime};

#[test]
fn facade_exposes_common_graph_store_entrypoints() {
    let graph = Graph::new(GraphId::new());
    let store = NodeGraphStore::new(
        graph.clone(),
        NodeGraphViewState::default(),
        NodeGraphEditorConfig::default(),
    );

    assert_eq!(store.graph().graph_id, graph.graph_id);
    assert!(NodeGraphPatch::default().is_empty());
    let _ = std::mem::size_of::<DispatchOutcome>();
    let _ = std::mem::size_of::<DispatchError>();
}

#[test]
fn facade_modules_expose_underlying_crates() {
    let _: core::Graph = Graph::new(GraphId::new());

    let registry = builtin_layout_engine_registry();
    assert!(registry.get(&LayoutEngineId::dugong()).is_some());
    assert!(registry.get(&LayoutEngineId::tidy_tree()).is_some());
    assert!(
        registry
            .engines_for_family(&LayoutFamilyId::mind_map())
            .count()
            >= 2
    );
    let _ = std::mem::size_of::<LayoutEngineRegistry>();
    let _ = std::mem::size_of::<LayoutContext>();
    let _ = std::mem::size_of::<LayoutEngineRequest>();
    let _ = std::mem::size_of::<LayoutPresetBuilder>();
    let _ = std::mem::size_of::<LayoutRequest>();
    let _ = std::mem::size_of::<LayoutResult>();
    let _preset_request = LayoutPresetBuilder::workflow().build();
    let _: fn() -> layout::LayoutEngineRegistry = layout::builtin_layout_engine_registry;
    let _ = layout::TIDY_TREE_LAYOUT_ENGINE_ID;
    let _ = std::mem::size_of::<layout::TidyTreeLayoutEngine>();

    let _patch = runtime::NodeGraphPatch::default();
}
