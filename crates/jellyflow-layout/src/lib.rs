//! Optional headless layout adapters for Jellyflow.
//!
//! This crate keeps automatic layout outside the core document model. Layout engines receive a
//! projection of a Jellyflow graph and return [`LayoutResult`] values. A runtime or host owns the
//! revision-checked command that applies those results.

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

pub use dugong::DugongLayoutEngine;
#[cfg(feature = "benchmark-internals")]
pub use dugong::benchmark_internals;
pub use engine::{
    DUGONG_LAYOUT_ENGINE_ID, LayoutContext, LayoutCoordinateFrame, LayoutDirection,
    LayoutEdgeRoute, LayoutEngine, LayoutEngineDescriptor, LayoutEngineId, LayoutEngineRegistry,
    LayoutEngineRequest, LayoutError, LayoutNodePlacement, LayoutNodePosition, LayoutOptions,
    LayoutPlacements, LayoutRequest, LayoutResult, LayoutScope, LayoutSpacing,
    MIND_MAP_FREEFORM_LAYOUT_ENGINE_ID, MIND_MAP_RADIAL_LAYOUT_ENGINE_ID, PreparedLayoutEdge,
    PreparedLayoutInput, PreparedLayoutNode, TIDY_TREE_LAYOUT_ENGINE_ID,
};
pub use family::{
    LAYERED_DAG_LAYOUT_FAMILY_ID, LayoutEngineCapability, LayoutEngineMetadata, LayoutFamilyId,
    LayoutFamilyMetadata, MIND_MAP_LAYOUT_FAMILY_ID,
};
pub use freeform::MindMapFreeformLayoutEngine;
pub use mind_map::MindMapRadialLayoutEngine;
pub use preset::LayoutPresetBuilder;
pub use tidy_tree::TidyTreeLayoutEngine;

/// Returns a registry containing Jellyflow's built-in layout engines.
pub fn builtin_layout_engine_registry() -> &'static LayoutEngineRegistry {
    builtin::builtin_layout_registry()
}

#[cfg(test)]
mod tests;
