//! Renderer-neutral graph interaction state and intent types.

mod connection;
mod intent;
mod presentation;
mod session;

pub use connection::{ConnectionGestureIntent, ConnectionProbe, ConnectionProbeTarget};
pub use intent::{FreshGraphMutation, FreshGraphMutationBindError, RelativeNodeMoveIntent};
pub use presentation::{
    AlignmentGuide, CanvasInteractionPhase, CanvasInteractionPresentation, ConnectionPresentation,
    ConnectionSearchStatus, InteractionPosition, InteractionTerminalReason, NodeDragPresentation,
};
pub use session::{
    CanvasInteractionConfig, CanvasInteractionEffect, CanvasInteractionError,
    CanvasInteractionQuery, CanvasInteractionSession, CanvasInteractionUpdate, CanvasPressTarget,
    CommandSettlement, ConnectionInteractionStart, ConnectionSearchRequest, InteractionDamage,
    InteractionGeneration, InteractionOwner, InteractionQueryId, InteractionTickDemand,
    InteractionTicketId, NodeDragStart, SelectionMode, SelectionMutation, ViewportMutation,
};
