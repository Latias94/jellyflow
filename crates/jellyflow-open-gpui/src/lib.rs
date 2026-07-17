//! Native retained Open GPUI adapter for Jellyflow.
//!
//! The headless runtime owns graph and view facts. This crate owns retained
//! Open GPUI entities, native input, measurement, lifecycle, and scene order.

#![deny(unsafe_code)]

pub mod accessibility;
pub mod activity;
pub mod binding;
mod command_journal;
mod connection_controller;
pub mod draft;
mod draft_store;
pub mod host;
pub mod interaction;
mod port_handle;
pub mod registry;
mod semantic_zoom;

pub use accessibility::{
    OPEN_GPUI_DIAGNOSTIC_ROLE, OPEN_GPUI_INTERACTIVE_PORT_ROLE, OPEN_GPUI_NODE_ROLE,
    OPEN_GPUI_SCENE_ROLE, OPEN_GPUI_STATIC_PORT_ROLE, open_gpui_interaction_element_id,
    open_gpui_node_element_id, open_gpui_node_measurement_element_id, open_gpui_port_element_id,
    open_gpui_port_measurement_element_id, open_gpui_scene_element_id,
    open_gpui_scene_measurement_element_id,
};
pub use activity::{
    OpenGpuiActivityAcquireError, OpenGpuiActivityKind, OpenGpuiActivityLease,
    OpenGpuiNodeActivityRegistry,
};
pub use binding::{
    OpenGpuiBoundPort, OpenGpuiNodeBinding, OpenGpuiNodeBindingRead, OpenGpuiNodeBindingReadError,
    OpenGpuiNodeBindingSnapshot,
};
pub use command_journal::{
    OPEN_GPUI_COMMAND_JOURNAL_CAPACITY, OpenGpuiCommandEnqueueError, OpenGpuiCommandTicket,
    OpenGpuiCommandTicketError, OpenGpuiCommandTicketResult,
};
pub use connection_controller::{
    OpenGpuiConnectionCancelReason, OpenGpuiConnectionCommitRejection,
    OpenGpuiConnectionGeneration, OpenGpuiConnectionPhase, OpenGpuiConnectionPoint,
    OpenGpuiCreateFromWireRequest,
};
pub use draft::{
    DraftAttempt, DraftCompositionDecision, DraftConflict, DraftConflictResolution,
    DraftEditDecision, DraftIntentRejection, DraftPhase, DraftReceiptDecision,
    DraftReconcileDecision, DraftRecovery, DraftResolutionDecision, DraftRestore,
    DraftStaleRejectionDecision, DraftSubmissionDecision, DraftSubmissionIntent,
    DraftSubmissionKind, DraftSubmissionRejectionDecision, RevisionedDraft,
};
pub use draft_store::ExternalizedDraftStore;
pub use host::{
    OPEN_GPUI_DEFAULT_EDGE_LAYER_BUDGET, OPEN_GPUI_DEFAULT_EDGE_ROUTE_SCAN_LIMIT,
    OPEN_GPUI_DEFAULT_ELIGIBLE_PARKED_CAPACITY, OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY,
    OpenGpuiConnectionCommitEvent, OpenGpuiConnectionCommitFailure,
    OpenGpuiConnectionCommitOutcome, OpenGpuiConnectionCommitStage,
    OpenGpuiCreateFromWireCandidates, OpenGpuiCreateFromWireError,
    OpenGpuiCreateFromWireSubmission, OpenGpuiEdgeLayerOverflow, OpenGpuiSceneHost,
    OpenGpuiSceneHostCloseError, OpenGpuiSceneHostDiagnostic, OpenGpuiSceneHostDiagnosticKind,
    OpenGpuiSceneHostStats,
};
pub use interaction::{
    OpenGpuiNodeInteractionGuard, OpenGpuiPortActivationEvent, OpenGpuiPortActivationSource,
    OpenGpuiPortDragCancelReason, OpenGpuiPortDragEvent, OpenGpuiPortDragOwner,
    OpenGpuiPortDragPhase, OpenGpuiPortEvent,
};
pub use port_handle::{OpenGpuiPortHandle, OpenGpuiPortSide};
pub use registry::{
    OpenGpuiNodeMountError, OpenGpuiNodeRegistry, OpenGpuiNodeRegistryBuilder,
    OpenGpuiNodeRegistryConfigError, OpenGpuiNodeRegistryIssue, OpenGpuiNodeRegistryIssueKind,
    OpenGpuiRendererRegistrationStatus,
};
