//! Headless graph primitives for Jellyflow.
//!
//! This crate owns the portable graph document model, stable IDs, type descriptors, and interaction
//! policy value types shared by higher-level runtimes and UI adapters. It must stay free of Fret UI,
//! renderer, platform, and windowing dependencies.
//!
//! Reversible operations are private to the authoritative runtime and are not a core API:
//!
//! ```compile_fail
//! use jellyflow_core::ops::GraphTransaction;
//! ```

#![deny(unsafe_code)]

pub mod core;
pub mod interaction;
pub mod types;

pub use core::{
    Binding, BindingEndpoint, BindingId, CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeId,
    EdgeKind, EdgeLabelAnchor, EdgeReconnectable, EdgeReconnectableEndpoint, EdgeRouteSpec,
    EdgeRouteSpecValidationError, EdgeViewDescriptor, GRAPH_VERSION, Graph, GraphBuilder,
    GraphElementIter, GraphElementKeys, GraphElementValues, GraphElements, GraphId, GraphImport,
    GraphImportClosure, GraphImportError, GraphLocalBindingTarget, GraphMutationFootprint,
    GraphValidationError, GraphValidationReport, Group, GroupId, Node, NodeExtent, NodeId,
    NodeKindKey, NodeOrigin, Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
    SourceAnchor, StickyNote, StickyNoteId, Symbol, SymbolId,
};
pub use interaction::NodeGraphConnectionMode;
pub use types::{
    DefaultTypeCompatibility, TypeCompatibility, TypeCompatibilityResult, TypeDesc, TypeVarId,
};
