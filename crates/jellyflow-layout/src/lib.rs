//! Optional headless layout adapters for Jellyflow.
//!
//! This crate keeps automatic layout outside the core document model. Layout engines receive a
//! projection of a Jellyflow graph and return normal [`GraphTransaction`] values that hosts can
//! apply explicitly.

#![deny(unsafe_code)]

mod dugong;
mod engine;
mod family;
mod freeform;
mod mind_map;
mod preset;
mod projection;
mod tidy_tree;

pub use dugong::{
    DugongLayoutEngine, layout_graph_to_transaction_with_dugong, layout_graph_with_dugong,
};
pub use engine::{
    DUGONG_LAYOUT_ENGINE_ID, LayoutContext, LayoutDirection, LayoutEdgeRoute, LayoutEngine,
    LayoutEngineId, LayoutEngineRegistry, LayoutEngineRequest, LayoutError, LayoutNodePosition,
    LayoutOptions, LayoutRequest, LayoutResult, LayoutScope, LayoutSpacing,
    MIND_MAP_FREEFORM_LAYOUT_ENGINE_ID, MIND_MAP_RADIAL_LAYOUT_ENGINE_ID,
    TIDY_TREE_LAYOUT_ENGINE_ID, layout_graph_to_transaction_with_engine, layout_graph_with_engine,
};
pub use family::{
    LAYERED_DAG_LAYOUT_FAMILY_ID, LayoutEngineCapability, LayoutEngineMetadata, LayoutFamilyId,
    LayoutFamilyMetadata, MIND_MAP_LAYOUT_FAMILY_ID,
};
pub use freeform::{
    MindMapFreeformLayoutEngine, layout_graph_to_transaction_with_mind_map_freeform,
    layout_graph_with_mind_map_freeform,
};
pub use mind_map::{
    MindMapRadialLayoutEngine, layout_graph_to_transaction_with_mind_map_radial,
    layout_graph_with_mind_map_radial,
};
pub use preset::LayoutPresetBuilder;
pub use tidy_tree::{
    TidyTreeLayoutEngine, layout_graph_to_transaction_with_tidy_tree, layout_graph_with_tidy_tree,
};

/// Returns a registry containing Jellyflow's built-in layout engines.
pub fn builtin_layout_engine_registry() -> LayoutEngineRegistry {
    let mut registry = LayoutEngineRegistry::new();
    let inserted_layered_family = registry.insert_family(LayoutFamilyMetadata::layered_dag());
    debug_assert!(
        inserted_layered_family.is_ok(),
        "built-in layered DAG layout family should be unique"
    );
    let inserted_mind_map_family = registry.insert_family(LayoutFamilyMetadata::mind_map());
    debug_assert!(
        inserted_mind_map_family.is_ok(),
        "built-in mind-map layout family should be unique"
    );
    let inserted_dugong = registry.insert(DugongLayoutEngine);
    debug_assert!(
        inserted_dugong.is_ok(),
        "built-in dugong engine should be unique"
    );
    let inserted_dugong_metadata = registry.insert_metadata(LayoutEngineMetadata::dugong());
    debug_assert!(
        inserted_dugong_metadata.is_ok(),
        "built-in dugong metadata should be unique"
    );
    let inserted_tidy_tree = registry.insert(TidyTreeLayoutEngine);
    debug_assert!(
        inserted_tidy_tree.is_ok(),
        "built-in tidy tree engine should be unique"
    );
    let inserted_tidy_tree_metadata = registry.insert_metadata(LayoutEngineMetadata::tidy_tree());
    debug_assert!(
        inserted_tidy_tree_metadata.is_ok(),
        "built-in tidy tree metadata should be unique"
    );
    let inserted_mind_map = registry.insert(MindMapRadialLayoutEngine);
    debug_assert!(
        inserted_mind_map.is_ok(),
        "built-in mind-map engine should be unique"
    );
    let inserted_mind_map_metadata =
        registry.insert_metadata(LayoutEngineMetadata::mind_map_radial());
    debug_assert!(
        inserted_mind_map_metadata.is_ok(),
        "built-in radial mind-map metadata should be unique"
    );
    let inserted_freeform = registry.insert(MindMapFreeformLayoutEngine);
    debug_assert!(
        inserted_freeform.is_ok(),
        "built-in freeform engine should be unique"
    );
    let inserted_freeform_metadata =
        registry.insert_metadata(LayoutEngineMetadata::mind_map_freeform());
    debug_assert!(
        inserted_freeform_metadata.is_ok(),
        "built-in freeform mind-map metadata should be unique"
    );
    registry
}

#[cfg(test)]
mod tests;
