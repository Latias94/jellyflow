//! Renderer-neutral graph interaction state and intent types.

mod connection;
mod intent;
mod navigation;
mod presentation;
mod query_work;
mod session;

pub use crate::runtime::view_session::AlignmentSnapQueryReceipt;
pub use connection::{
    ConnectionGestureBinding, ConnectionProbe, ConnectionProbeTarget, ConnectionProbeTargetIdentity,
};
pub use intent::{FreshGraphMutationBindError, MAX_NODE_MOVE_COHORT_SIZE, SelectionBaseline};
pub(crate) use navigation::resolve_alignment_snap;
pub use navigation::{
    AlignmentSnapQuery, AlignmentSnapQueryOutcome, AlignmentSnapResult, AlignmentSnapWork,
    fit_viewport,
};
pub use presentation::{
    AlignmentGuide, AlignmentGuideAxis, CanvasInteractionPhase, CanvasInteractionPresentation,
    ConnectionPresentation, ConnectionProbeValidity, ConnectionSearchStatus,
    ConnectionWireProjection, InteractionTerminalReason, NodeDragPresentation,
};
pub(crate) use session::SelectionMutation;
pub use session::{
    AlignmentSnapConfig, CanvasInteractionConfig, CanvasInteractionDispatch,
    CanvasInteractionEffect, CanvasInteractionError, CanvasInteractionQueryAdvance,
    CanvasInteractionQueryAdvanceReceipt, CanvasInteractionQueryPageReceipt,
    CanvasInteractionSession, CanvasInteractionUpdate, CanvasPressFallback, CanvasPressTarget,
    ConnectionInteractionStart, ConnectionSearchRequest, ConnectionSearchSelection,
    InteractionDamage, InteractionGeneration, InteractionGraphSubmission,
    InteractionGraphSubmissionBindError, InteractionOwner, InteractionQueryId,
    InteractionSessionId, InteractionTickDemand, InteractionTicketId, KeyboardConnectionFeedback,
    KeyboardConnectionPort, NodeDragStart, NodeNudgeDirection, SelectionMode,
};
