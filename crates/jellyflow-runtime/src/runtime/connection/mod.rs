//! Renderer-neutral connection gesture helpers.
//!
//! Adapters own pointer capture and platform hit testing. The runtime owns deterministic
//! connection activation and handle proximity policy that should feel like XyFlow across renderers.

mod activation;
mod handles;
mod indicator;
mod target;

pub use activation::{ConnectionDragActivationInput, connection_drag_threshold_met};
pub use handles::{
    ClosestConnectionHandle, ClosestConnectionHandleInput, ConnectionHandleCandidate,
    ConnectionHandleRef, ConnectionHandleValidity, closest_connection_handle,
    connection_handle_validity,
};
pub use indicator::{
    ConnectionHandleIndicator, ConnectionHandleIndicatorInput, resolve_connection_handle_indicator,
};
pub use target::{
    ConnectionHandleConnection, ConnectionTargetHandle, ConnectionTargetInput,
    ResolvedConnectionTarget, resolve_connection_target,
};
