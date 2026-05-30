//! XyFlow-compatible projections and callback adapters.
//!
//! Jellyflow's canonical runtime payload is a reversible graph transaction wrapped in a
//! full-fidelity graph patch. This module contains the compatibility surface for integrations that
//! want XyFlow/ReactFlow-style node/edge changes and callback naming.

pub mod apply;
pub mod callbacks;
pub mod changes;

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
