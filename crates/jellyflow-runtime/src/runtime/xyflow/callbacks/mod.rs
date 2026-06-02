//! ReactFlow-style callback surface for B-layer integrations.
//!
//! `NodeGraphStore` already emits a low-level event stream (`NodeGraphStoreEvent`), but users
//! typically want a higher-level contract similar to ReactFlow:
//! - `onNodesChange`
//! - `onEdgesChange`
//! - `onConnect` / `onReconnect` / `onDisconnect`
//! - `onViewportChange` / `onSelectionChange`
//!
//! This module provides an object-safe callback trait and an adapter that can be installed into
//! a store subscription.

mod dispatch;
mod install;
mod traits;
mod types;

pub use crate::runtime::events::{ConnectDragKind, ConnectEnd, ConnectEndOutcome, ConnectStart};
pub use dispatch::{connection_changes_from_transaction, delete_changes_from_transaction};
pub use install::install_callbacks;
pub use traits::{
    NodeGraphCallbacks, NodeGraphCommitCallbacks, NodeGraphGestureCallbacks, NodeGraphViewCallbacks,
};
pub use types::{
    ConnectionChange, DeleteChange, EdgeConnection, NodeDragEnd, NodeDragEndOutcome, NodeDragStart,
    NodeDragUpdate, NodeResizeEnd, NodeResizeEndOutcome, NodeResizeStart, NodeResizeUpdate,
    SelectionChange, ViewportMove, ViewportMoveEnd, ViewportMoveEndOutcome, ViewportMoveKind,
    ViewportMoveStart,
};
