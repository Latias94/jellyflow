//! Headless runtime, rules, schema, and profile pipeline for Jellyflow.
//!
//! This crate owns the portable graph runtime and view-session protocol built on top of
//! `jellyflow-core`. It must stay free of UI, renderer, platform, and windowing dependencies.
//!
//! Public API shape:
//! - crate-root re-exports are the canonical runtime entry points,
//! - [`io`] owns persisted graph files and versioned view-session seeds,
//! - [`runtime::graph_runtime`] owns graph revisions and commands,
//! - [`runtime::view_session`] owns view-local geometry and frame feedback.
//!
//! Applications mutate an authoritative runtime through revisioned commands. Raw rule plans and
//! their reversible operation journals are not public mutation surfaces:
//!
//! ```compile_fail
//! use jellyflow_runtime::rules::ConnectPlan;
//! ```

#![deny(unsafe_code)]

mod graph_kernel;
pub mod io;
pub mod profile;
mod rules;
pub mod runtime;
pub mod schema;

pub use profile::{ApplyPipelineError, GraphProfile, PortTypeImpactContext, PortTypeRevalidation};
pub use rules::{Diagnostic, DiagnosticSeverity, DiagnosticTarget, EdgeEndpoint};
pub use runtime::adapter_protocol::{
    AdapterEnvironment, AdapterFrame, DEFAULT_FRAME_REQUIRED_NODE_BUDGET,
    DEFAULT_FRAME_ROUTING_DEPENDENCY_BUDGET, DEFAULT_FRAME_VISIBLE_NODE_BUDGET,
    DEFAULT_FRAME_VISIBLE_NODE_SCAN_LIMIT, FeedbackCollector, FeedbackCollectorError,
    FeedbackOperationReceipt, FeedbackReceipt, FeedbackSubmissionError, FrameBoundsSource,
    FrameFeedback, FrameMaterializationBudget, FrameMaterializationError,
    FrameMaterializationOverflow, FrameNode, FramePort, FrameRequest, GeometrySnapshot,
    HandlePosition, LayoutNodeStatus, LayoutPublicationReceipt, NodeInstanceKey, NodeLayout,
    NodeLayoutBuilder, NodeLayoutBuilderError, NodeLayoutTarget, PortLayoutOutcome,
    PortLayoutTarget, PublishedEdgeRoute, PublishedNodeGeometry, PublishedPortGeometry,
    PublishedPortOutcome, RenderedPortLayout, RendererStamp, StaleFrameReason, ViewPoint, ViewRect,
    ViewSize, side_anchor,
};
pub use runtime::commands::{
    ConnectionDecision, ConnectionEdit, ConnectionEvaluation, GraphCommand, GraphCommandConflict,
    GraphCommandNoop, GraphCommandOutcome, GraphCommandReceipt, GraphCommandRejection,
    IncidentEdgePolicy, NodeGeometryEdit, SymbolEdit, TopologyEdit, TopologyPort,
};
pub use runtime::graph_runtime::{
    CreateFromWireCandidates, CreateFromWireQueryError, DocumentEpoch, FactChange, FactChangeMask,
    GraphRevision, GraphRuntime, GraphRuntimeBuilder, GraphRuntimeError, GraphRuntimeEvent,
    GraphRuntimePolicy, GraphRuntimeStats, GraphRuntimeSubscriptionToken, GraphSnapshot,
    HistoryState, NodeIncarnation, PortConnectionDragFacts, PortConnectionDragIntent,
    PortConnectionFacts, PortIncidentConnectionFacts, RuntimeFactDelta, RuntimeFactKey,
    RuntimePatch, RuntimePatchKind,
};
pub use runtime::middleware::{GraphRuntimeMiddleware, GraphRuntimeMiddlewareSnapshot};
pub use runtime::view_session::{
    AttachViewError, NodeBindingEvent, NodeBindingKey, SelectedEdgeIds, ViewCloseOutcome,
    ViewCloseReason, ViewCloseReceipt, ViewCloseRequest, ViewCreateFromWireQueryError,
    ViewGeneration, ViewId, ViewSession, ViewSessionError, ViewSessionEvent, ViewSessionOptions,
    ViewSessionStats, ViewSubscriptionToken, WeakViewSession,
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
                "jellyflow-runtime must stay headless; forbidden dependency `{forbidden}` found",
            );
        }
    }
}
