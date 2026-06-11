//! Optional headless layout adapters for Jellyflow.
//!
//! This crate keeps automatic layout outside the core document model. Layout engines receive a
//! projection of a Jellyflow graph and return normal [`GraphTransaction`] values that hosts can
//! apply explicitly.

#![deny(unsafe_code)]

mod dugong;
mod engine;
mod mind_map;

pub use dugong::{
    DugongLayoutEngine, layout_graph_to_transaction_with_dugong, layout_graph_with_dugong,
};
pub use engine::{
    DUGONG_LAYOUT_ENGINE_ID, LayoutContext, LayoutDirection, LayoutEdgeRoute, LayoutEngine,
    LayoutEngineId, LayoutEngineRegistry, LayoutEngineRequest, LayoutError, LayoutNodePosition,
    LayoutOptions, LayoutRequest, LayoutResult, LayoutScope, LayoutSpacing,
    MIND_MAP_RADIAL_LAYOUT_ENGINE_ID, layout_graph_to_transaction_with_engine,
    layout_graph_with_engine,
};
pub use mind_map::{
    MindMapRadialLayoutEngine, layout_graph_to_transaction_with_mind_map_radial,
    layout_graph_with_mind_map_radial,
};

/// Returns a registry containing Jellyflow's built-in layout engines.
pub fn builtin_layout_engine_registry() -> LayoutEngineRegistry {
    let mut registry = LayoutEngineRegistry::new();
    let inserted_dugong = registry.insert(DugongLayoutEngine);
    debug_assert!(
        inserted_dugong.is_ok(),
        "built-in dugong engine should be unique"
    );
    let inserted_mind_map = registry.insert(MindMapRadialLayoutEngine);
    debug_assert!(
        inserted_mind_map.is_ok(),
        "built-in mind-map engine should be unique"
    );
    registry
}

#[cfg(test)]
mod tests;
