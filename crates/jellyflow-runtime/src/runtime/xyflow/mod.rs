//! XyFlow-compatible projections and callback adapters.
//!
//! Jellyflow's canonical runtime payload lives in [`crate::runtime::commit`]. This module contains
//! the compatibility surface for integrations that want XyFlow/ReactFlow-style node/edge changes
//! and callback naming.
//!
//! Effective interaction policy resolution lives in [`crate::runtime::policy`]. Policy-shaped
//! fields exposed here keep XyFlow naming only for compatibility with node/edge change consumers.

pub mod apply;
pub mod callbacks;
pub mod changes;
pub mod store;

pub use apply::{ApplyChangesReport, apply_edge_changes, apply_graph_changes, apply_node_changes};
pub use callbacks::{
    ConnectionChange, DeleteChange, EdgeConnection, NodeDragEnd, NodeDragEndOutcome, NodeDragStart,
    NodeGraphCallbacks, NodeGraphCommitCallbacks, NodeGraphGestureCallbacks,
    NodeGraphViewCallbacks, SelectionChange, ViewportMoveEnd, ViewportMoveEndOutcome,
    ViewportMoveKind, ViewportMoveStart, connection_changes_from_transaction,
    delete_changes_from_transaction, install_callbacks,
};
pub use changes::{
    ChangesToTransactionError, EdgeChange, NodeChange, NodeGraphChanges, NodeGraphPatch,
};
pub use store::DispatchChangesError;
