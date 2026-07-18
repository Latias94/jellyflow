//! Native retained Open GPUI adapter for Jellyflow.
//!
//! The headless runtime owns graph and view facts. This crate owns retained
//! Open GPUI entities, native input, measurement, lifecycle, and scene order.

#![deny(unsafe_code)]

pub mod accessibility;
pub mod activity;
pub mod binding;
pub mod chrome;
mod command_journal;
pub mod draft;
mod draft_store;
pub mod host;
pub mod interaction;
mod navigation;
mod node_drag_handle;
mod port_handle;
pub mod presentation;
pub mod registry;
mod scale_policy;
mod semantic_zoom;
mod size_policy;
pub mod theme;

pub use accessibility::{
    OPEN_GPUI_DIAGNOSTIC_ROLE, OPEN_GPUI_EDGE_ROLE, OPEN_GPUI_INTERACTIVE_PORT_ROLE,
    OPEN_GPUI_NODE_ROLE, OPEN_GPUI_RECONNECT_HANDLE_ROLE, OPEN_GPUI_SCENE_ROLE,
    OPEN_GPUI_STATIC_PORT_ROLE, open_gpui_edge_element_id, open_gpui_interaction_element_id,
    open_gpui_node_element_id, open_gpui_node_measurement_element_id, open_gpui_port_element_id,
    open_gpui_port_measurement_element_id, open_gpui_reconnect_element_id,
    open_gpui_scene_element_id, open_gpui_scene_measurement_element_id,
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
pub use draft::{
    DraftAttempt, DraftCompositionDecision, DraftConflict, DraftConflictResolution,
    DraftEditDecision, DraftIntentRejection, DraftPhase, DraftReceiptDecision,
    DraftReconcileDecision, DraftRecovery, DraftResolutionDecision, DraftRestore,
    DraftStaleRejectionDecision, DraftSubmissionDecision, DraftSubmissionIntent,
    DraftSubmissionKind, DraftSubmissionRejectionDecision, RevisionedDraft,
};
pub use draft_store::ExternalizedDraftStore;
pub use host::{
    OPEN_GPUI_DEFAULT_EDGE_LAYER_BUDGET, OPEN_GPUI_DEFAULT_ELIGIBLE_PARKED_CAPACITY,
    OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY, OPEN_GPUI_DEFAULT_VISIBLE_ROUTE_QUERY_BUDGET,
    OPEN_GPUI_MAX_EDGE_LAYER_BUDGET, OpenGpuiConnectionOptions, OpenGpuiConnectionSearchError,
    OpenGpuiConnectionSearchSubmission, OpenGpuiEdgeLayerOverflow, OpenGpuiSceneHost,
    OpenGpuiSceneHostCloseError, OpenGpuiSceneHostDiagnostic, OpenGpuiSceneHostDiagnosticKind,
    OpenGpuiSceneHostStats,
};
pub use interaction::{
    OpenGpuiOverlayInteractionBoundary, OpenGpuiPortActivationEvent, OpenGpuiPortActivationSource,
    OpenGpuiPortDragCancelReason, OpenGpuiPortDragEvent, OpenGpuiPortDragOwner,
    OpenGpuiPortDragPhase, OpenGpuiPortEvent,
};
pub use navigation::{OpenGpuiNavigationConfig, OpenGpuiNavigationProfile};
pub use node_drag_handle::OpenGpuiNodeDragHandle;
pub use port_handle::{OpenGpuiPortAnchor, OpenGpuiPortHandle, OpenGpuiPortSide};
pub use presentation::{
    OpenGpuiNodeInteractionState, OpenGpuiNodeLod, OpenGpuiNodePresentationSnapshot,
    OpenGpuiPortCandidateState, OpenGpuiPortInteractionState, OpenGpuiSettlementState,
};
pub use registry::{
    OpenGpuiNodeMountError, OpenGpuiNodeRegistrationPolicy, OpenGpuiNodeRegistry,
    OpenGpuiNodeRegistryBuilder, OpenGpuiNodeRegistryConfigError, OpenGpuiNodeRegistryIssue,
    OpenGpuiNodeRegistryIssueKind, OpenGpuiRendererRegistrationStatus,
};
pub use scale_policy::{
    OPEN_GPUI_DEFAULT_DETAIL_ENTER_ZOOM, OPEN_GPUI_DEFAULT_OVERVIEW_ENTER_ZOOM,
    OpenGpuiNodeScalePolicy, OpenGpuiSemanticLodConfigError, OpenGpuiSemanticLodPolicy,
};
pub use size_policy::{
    OPEN_GPUI_DEFAULT_MAX_CONTENT_SIZE, OPEN_GPUI_DEFAULT_MIN_CONTENT_SIZE,
    OPEN_GPUI_DEFAULT_SIZE_ATTEMPT_BOUND, OPEN_GPUI_DEFAULT_SIZE_EPSILON,
    OPEN_GPUI_DEFAULT_STABLE_SIZE_SAMPLES, OpenGpuiContentHeightPolicy, OpenGpuiContentSizePolicy,
    OpenGpuiNodeSizeConfigError, OpenGpuiNodeSizeConvergence, OpenGpuiNodeSizePolicy,
};
pub use theme::{
    OPEN_GPUI_DEFAULT_EDGE_EMPHASIZED_WIDTH, OPEN_GPUI_DEFAULT_EDGE_HOVERED_WIDTH,
    OPEN_GPUI_DEFAULT_EDGE_LABEL_CORNER_RADIUS, OPEN_GPUI_DEFAULT_EDGE_LABEL_FONT_SIZE,
    OPEN_GPUI_DEFAULT_EDGE_LABEL_HORIZONTAL_PADDING, OPEN_GPUI_DEFAULT_EDGE_LABEL_MAX_WIDTH,
    OPEN_GPUI_DEFAULT_EDGE_LABEL_VERTICAL_PADDING, OPEN_GPUI_DEFAULT_EDGE_MARKER_DIAMETER,
    OPEN_GPUI_DEFAULT_EDGE_ORDINARY_WIDTH, OPEN_GPUI_DEFAULT_PORT_HIT_DIAMETER,
    OPEN_GPUI_DEFAULT_PORT_HOVER_MARKER_DIAMETER, OPEN_GPUI_DEFAULT_PORT_MARKER_DIAMETER,
    OPEN_GPUI_DEFAULT_RECONNECT_HANDLE_DIAMETER, OPEN_GPUI_DEFAULT_RECONNECT_HIT_DIAMETER,
    OpenGpuiGraphMetrics, OpenGpuiGraphPalette, OpenGpuiGraphTheme,
};
