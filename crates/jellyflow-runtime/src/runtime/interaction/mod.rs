//! Renderer-neutral graph interaction state and intent types.

mod connection;
mod intent;
mod navigation;
mod presentation;
mod session;

pub use connection::{ConnectionGestureIntent, ConnectionProbe, ConnectionProbeTarget};
pub use intent::{
    ConnectionTopology, EdgeDeletionIntent, FreshGraphMutation, FreshGraphMutationBindError,
    RelativeNodeMoveIntent, SelectionBaseline,
};
pub(crate) use navigation::resolve_alignment_snap;
pub use navigation::{
    AlignmentSnapQuery, AlignmentSnapQueryOutcome, AlignmentSnapResult, AlignmentSnapWork,
    fit_viewport,
};
pub use presentation::{
    AlignmentGuide, AlignmentGuideAxis, CanvasInteractionPhase, CanvasInteractionPresentation,
    ConnectionPresentation, ConnectionSearchStatus, InteractionPosition, InteractionTerminalReason,
    NodeDragPresentation,
};
pub use session::{
    AlignmentSnapConfig, CanvasInteractionConfig, CanvasInteractionDispatch,
    CanvasInteractionEffect, CanvasInteractionError, CanvasInteractionQuery,
    CanvasInteractionSession, CanvasInteractionUpdate, CanvasPressFallback, CanvasPressTarget,
    CommandSettlement, ConnectionInteractionStart, ConnectionSearchRequest,
    ConnectionSearchSelection, InteractionDamage, InteractionGeneration, InteractionOwner,
    InteractionQueryId, InteractionTickDemand, InteractionTicketId, NodeDragStart, SelectionMode,
    SelectionMutation, ViewportMutation,
};
