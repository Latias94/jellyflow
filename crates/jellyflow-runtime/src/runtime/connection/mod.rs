//! Renderer-neutral connection gesture helpers.
//!
//! Adapters own pointer capture and platform hit testing. The runtime owns deterministic
//! connection activation and handle proximity policy that should feel like XyFlow across renderers.

mod activation;
mod connect;
mod handles;
mod indicator;
mod reconnect;
mod target;

pub use activation::{ConnectionDragActivationInput, connection_drag_threshold_met};
pub use connect::{
    CONNECT_EDGE_TRANSACTION_LABEL, ConnectEdgeError, ConnectEdgeRequest, connect_edge_transaction,
    connect_edge_transaction_with_edge_id,
};
pub use handles::{
    ClosestConnectionHandle, ClosestConnectionHandleInput, ConnectionHandleCandidate,
    ConnectionHandleRef, ConnectionHandleValidity, closest_connection_handle,
    connection_handle_validity,
};
pub use indicator::{
    ConnectionHandleIndicator, ConnectionHandleIndicatorInput, resolve_connection_handle_indicator,
};
pub use reconnect::{
    RECONNECT_EDGE_TRANSACTION_LABEL, ReconnectEdgeError, ReconnectEdgeRequest,
    reconnect_edge_transaction,
};
pub use target::{
    ConnectionHandleConnection, ConnectionTargetHandle, ConnectionTargetInput,
    ResolvedConnectionTarget, resolve_connection_target,
};
