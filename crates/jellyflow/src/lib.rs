//! User-friendly facade for the headless Jellyflow graph engine.
//!
//! This crate is intentionally thin. It re-exports the lower-level crates under stable module names
//! and provides a small prelude for the most common graph-store setup path. Consumers that need a
//! narrower dependency boundary can depend on `jellyflow-core`, `jellyflow-layout`, or
//! `jellyflow-runtime` directly.

#![deny(unsafe_code)]

pub use jellyflow_core as core;
pub use jellyflow_layout as layout;
pub use jellyflow_runtime as runtime;

pub use core::{Graph, GraphId};
pub use runtime::io::{NodeGraphEditorConfig, NodeGraphViewState};
pub use runtime::{DispatchError, DispatchOutcome, NodeGraphPatch, NodeGraphStore};

/// Common Jellyflow entry points for applications and adapter crates.
pub mod prelude {
    pub use crate::core::{
        CanvasPoint, CanvasRect, CanvasSize, EdgeId, Graph, GraphId, GraphOp, GraphTransaction,
        GroupId, NodeId, NodeKindKey, PortDirection, PortId,
    };
    pub use crate::layout::{
        LayoutContext, LayoutEngineId, LayoutEngineRegistry, LayoutEngineRequest, LayoutFamilyId,
        LayoutRequest, LayoutResult, builtin_layout_engine_registry,
    };
    pub use crate::runtime::io::{NodeGraphEditorConfig, NodeGraphViewState};
    pub use crate::runtime::{
        DispatchError, DispatchOutcome, GraphProfile, NodeGraphPatch, NodeGraphStore,
        apply_connect_plan_with_profile, apply_transaction_with_profile,
    };
}
