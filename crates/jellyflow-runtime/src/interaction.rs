//! Renderer-neutral graph interaction state, queries, and effects.

pub use crate::runtime::interaction::{
    AlignmentGuide, AlignmentGuideAxis, AlignmentSnapConfig, AlignmentSnapQuery,
    AlignmentSnapQueryOutcome, AlignmentSnapResult, AlignmentSnapWork, CanvasInteractionConfig,
    CanvasInteractionDispatch, CanvasInteractionEffect, CanvasInteractionError,
    CanvasInteractionPhase, CanvasInteractionPresentation, CanvasInteractionQuery,
    CanvasInteractionSession, CanvasInteractionUpdate, CanvasPressFallback, CanvasPressTarget,
    CommandSettlement, ConnectionGestureBinding, ConnectionInteractionStart,
    ConnectionPresentation, ConnectionProbe, ConnectionProbeTarget, ConnectionProbeTargetIdentity,
    ConnectionProbeValidity, ConnectionSearchRequest, ConnectionSearchSelection,
    ConnectionSearchStatus, ConnectionWireProjection, FreshGraphMutationBindError,
    InteractionDamage, InteractionGeneration, InteractionGraphSubmission,
    InteractionGraphSubmissionBindError, InteractionOwner, InteractionQueryId,
    InteractionSessionId, InteractionTerminalReason, InteractionTickDemand, InteractionTicketId,
    KeyboardConnectionFeedback, KeyboardConnectionPort, MAX_NODE_MOVE_COHORT_SIZE,
    NodeDragPresentation, NodeDragStart, NodeNudgeDirection, SelectionBaseline, SelectionMode,
    fit_viewport,
};
