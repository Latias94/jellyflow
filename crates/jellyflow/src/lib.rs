//! User-friendly facade for the headless Jellyflow graph engine.
//!
//! This crate is intentionally thin. It re-exports the lower-level crates under stable module names
//! and provides a small prelude for the common graph-runtime setup path. Consumers that need a
//! narrower dependency boundary can depend on `jellyflow-core`, `jellyflow-layout`, or
//! `jellyflow-runtime` directly.

#![deny(unsafe_code)]

pub use jellyflow_core as core;
pub use jellyflow_layout as layout;
pub use jellyflow_runtime as runtime;

pub use core::{Graph, GraphId};
pub use runtime::io::{ViewSelectionSeed, ViewSessionSeed, ViewStackingSeed, ViewportSeed};
pub use runtime::schema::NodeCatalog;
pub use runtime::{
    AdapterEnvironment, ConnectionDecision, ConnectionEdit, ConnectionEvaluation, FeedbackReceipt,
    FeedbackSubmissionError, FrameMaterializationBudget, FrameMaterializationOverflow,
    FrameRequest, GeometrySnapshot, GraphCommand, GraphCommandConflict, GraphCommandOutcome,
    GraphCommandReceipt, GraphRuntime, GraphRuntimeBuilder, GraphRuntimeError,
    GraphRuntimeMiddleware, GraphRuntimePolicy, GraphSnapshot, HandlePosition, IncidentEdgePolicy,
    NodeGeometryEdit, NodeInstanceKey, NodeLayout, NodeLayoutTarget, PortConnectionFacts,
    PortIncidentConnectionFacts, PortLayoutTarget, RendererStamp, RuntimePatch, SymbolEdit,
    TopologyEdit, TopologyPort, ViewCloseRequest, ViewCreateFromWireQueryError, ViewSession,
    ViewSessionError, ViewSessionOptions,
};

/// Common Jellyflow entry points for applications and adapter crates.
pub mod prelude {
    pub use crate::core::{
        CanvasPoint, CanvasRect, CanvasSize, EdgeId, Graph, GraphId, GraphMutationFootprint,
        GroupId, NodeId, NodeKindKey, PortDirection, PortId,
    };
    pub use crate::layout::{
        LayoutContext, LayoutEngineId, LayoutEngineRegistry, LayoutEngineRequest, LayoutFamilyId,
        LayoutPresetBuilder, LayoutRequest, LayoutResult, LayoutScope,
        builtin_layout_engine_registry,
    };
    pub use crate::runtime::io::{
        ViewSelectionSeed, ViewSessionSeed, ViewStackingSeed, ViewportSeed,
    };
    pub use crate::runtime::schema::NodeCatalog;
    pub use crate::runtime::{
        AdapterEnvironment, ConnectionDecision, ConnectionEdit, ConnectionEvaluation,
        FeedbackReceipt, FeedbackSubmissionError, FrameMaterializationBudget,
        FrameMaterializationOverflow, FrameRequest, GeometrySnapshot, GraphCommand,
        GraphCommandConflict, GraphCommandOutcome, GraphCommandReceipt, GraphProfile, GraphRuntime,
        GraphRuntimeBuilder, GraphRuntimeError, GraphRuntimeMiddleware, GraphRuntimePolicy,
        GraphSnapshot, HandlePosition, IncidentEdgePolicy, NodeGeometryEdit, NodeInstanceKey,
        NodeLayout, NodeLayoutTarget, PortConnectionFacts, PortIncidentConnectionFacts,
        PortLayoutTarget, PortTypeImpactContext, PortTypeRevalidation, RendererStamp, RuntimePatch,
        SymbolEdit, TopologyEdit, TopologyPort, ViewCloseRequest, ViewCreateFromWireQueryError,
        ViewSession, ViewSessionError, ViewSessionOptions,
    };
}
