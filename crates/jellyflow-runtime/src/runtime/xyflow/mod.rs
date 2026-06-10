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
mod controlled;
mod dialect;
mod projection;
pub mod store;
mod transaction;

pub use apply::{
    ApplyChangesReport, XyFlowDimensionAttribute, XyFlowDimensionsSetAttributes, XyFlowEdgeChange,
    XyFlowEdgeElement, XyFlowNodeChange, XyFlowNodeElement, apply_edge_changes,
    apply_graph_changes, apply_node_changes, apply_xyflow_edge_changes, apply_xyflow_node_changes,
};
pub use callbacks::{
    ConnectionChange, DeleteChange, EdgeConnection, NodeDragEnd, NodeDragEndOutcome, NodeDragStart,
    NodeDragUpdate, NodeGraphCallbacks, NodeGraphCommitCallbacks, NodeGraphGestureCallbacks,
    NodeGraphViewCallbacks, NodeResizeEnd, NodeResizeEndOutcome, NodeResizeStart, NodeResizeUpdate,
    SelectionChange, ViewportMove, ViewportMoveEnd, ViewportMoveEndOutcome, ViewportMoveKind,
    ViewportMoveStart, install_callbacks,
};
pub use changes::{
    ChangesToTransactionError, EdgeChange, NodeChange, NodeGraphChanges, NodeGraphPatch,
};
pub use controlled::ControlledGraph;
pub use store::DispatchChangesError;
