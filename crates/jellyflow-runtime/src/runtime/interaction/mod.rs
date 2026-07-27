//! Renderer-neutral graph interaction state and intent types.

mod connection;
mod intent;
mod navigation;
mod presentation;
mod session;

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
    CanvasInteractionEffect, CanvasInteractionError, CanvasInteractionQuery,
    CanvasInteractionSession, CanvasInteractionUpdate, CanvasPressFallback, CanvasPressTarget,
    CommandSettlement, ConnectionInteractionStart, ConnectionSearchRequest,
    ConnectionSearchSelection, InteractionDamage, InteractionGeneration,
    InteractionGraphSubmission, InteractionGraphSubmissionBindError, InteractionOwner,
    InteractionQueryId, InteractionSessionId, InteractionTickDemand, InteractionTicketId,
    KeyboardConnectionFeedback, KeyboardConnectionPort, NodeDragStart, NodeNudgeDirection,
    SelectionMode,
};

#[cfg(test)]
pub(crate) use session::CanvasInteractionQueryKind;
