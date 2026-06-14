//! Headless graph primitives for Jellyflow.
//!
//! This crate owns the portable graph document model, stable IDs, type descriptors, and interaction
//! policy value types shared by higher-level runtimes and UI adapters. It must stay free of Fret UI,
//! renderer, platform, and windowing dependencies.

#![deny(unsafe_code)]

pub mod core;
pub mod interaction;
pub mod ops;
pub mod types;

pub use core::{
    Binding, BindingEndpoint, BindingId, CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeId,
    EdgeKind, EdgeReconnectable, EdgeReconnectableEndpoint, Graph, GraphBuilder, GraphElementIter,
    GraphElementKeys, GraphElementValues, GraphElements, GraphId, GraphImport, GraphImportClosure,
    GraphImportError, GraphLocalBindingTarget, GraphValidationError, GraphValidationReport, Group,
    GroupId, Node, NodeExtent, NodeId, NodeKindKey, NodeOrigin, Port, PortCapacity, PortDirection,
    PortId, PortKey, PortKind, SourceAnchor, StickyNote, StickyNoteId, Symbol, SymbolId,
};
pub use interaction::{
    NodeGraphConnectionMode, NodeGraphDragHandleMode, NodeGraphModifierKey, NodeGraphModifiers,
};
pub use ops::{
    ApplyError, DEFAULT_HISTORY_LIMIT, EdgeEndpoints, GraphFragment, GraphHistory, GraphOp,
    GraphOpBuilderExt, GraphTransaction, IdRemapSeed, IdRemapper, PasteTuning,
    find_invalid_size_in_tx, find_non_finite_in_tx, normalize_transaction,
};
pub use types::{
    DefaultTypeCompatibility, TypeCompatibility, TypeCompatibilityResult, TypeDesc, TypeVarId,
};

#[cfg(test)]
mod tests {
    #[test]
    fn manifest_stays_free_of_fret_ui_renderer_and_platform_dependencies() {
        let manifest = include_str!("../Cargo.toml");
        for forbidden in [
            "fret-core",
            "fret-ui",
            "fret-runtime",
            "fret-canvas",
            "wgpu",
            "winit",
        ] {
            assert!(
                !manifest.contains(forbidden),
                "jellyflow-core must stay headless; forbidden dependency `{forbidden}` found",
            );
        }
    }
}
