//! Optional headless layout adapters for Jellyflow.
//!
//! This crate keeps automatic layout outside the core document model. Layout engines receive a
//! projection of a Jellyflow graph and return normal [`GraphTransaction`] values that hosts can
//! apply explicitly.

#![deny(unsafe_code)]

mod builtin;
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
    builtin::builtin_layout_registry()
}

#[cfg(test)]
mod tests;
