use super::types::{
    ConnectionChange, DeleteChange, EdgeConnection, NodeDragEnd, NodeDragStart, NodeDragUpdate,
    SelectionChange, ViewportMove, ViewportMoveEnd, ViewportMoveStart,
};
use crate::runtime::commit::NodeGraphPatch;
use crate::runtime::xyflow::changes::{EdgeChange, NodeChange, NodeGraphChanges};
use jellyflow_core::core::{CanvasPoint, EdgeId, GroupId, NodeId, StickyNoteId};
use jellyflow_core::ops::EdgeEndpoints;

/// Headless/store commit callbacks for B-layer consumers.
///
/// Use this layer for controlled graph synchronization, analytics, and transaction-driven
/// integrations. `NodeGraphPatch` is the full-fidelity primary payload; node/edge changes are a
/// lossy XyFlow-style projection.
///
/// Ordering guarantees (per `GraphCommitted` store event):
///
/// 1) `on_graph_commit`
/// 2) `on_node_edge_changes`
/// 3) `on_nodes_change` (if non-empty)
/// 4) `on_edges_change` (if non-empty)
/// 5) `on_connection_change` for each derived `ConnectionChange`
/// 6) `on_connect`/`on_disconnect`/`on_reconnect` for each derived `ConnectionChange`
pub trait NodeGraphCommitCallbacks: 'static {
    fn on_graph_commit(&mut self, _patch: &NodeGraphPatch) {}

    fn on_node_edge_changes(&mut self, _changes: &NodeGraphChanges) {}

    fn on_nodes_change(&mut self, _changes: &[NodeChange]) {}
    fn on_edges_change(&mut self, _changes: &[EdgeChange]) {}

    fn on_connection_change(&mut self, _change: ConnectionChange) {}

    fn on_connect(&mut self, _conn: EdgeConnection) {}
    fn on_disconnect(&mut self, _conn: EdgeConnection) {}
    fn on_reconnect(&mut self, _edge: EdgeId, _from: EdgeEndpoints, _to: EdgeEndpoints) {}

    /// ReactFlow-style delete hook (`onNodesDelete`).
    fn on_nodes_delete(&mut self, _nodes: &[NodeId]) {}
    /// ReactFlow-style delete hook (`onEdgesDelete`).
    fn on_edges_delete(&mut self, _edges: &[EdgeId]) {}
    /// Delete hook for group containers.
    fn on_groups_delete(&mut self, _groups: &[GroupId]) {}
    /// Delete hook for sticky notes.
    fn on_sticky_notes_delete(&mut self, _notes: &[StickyNoteId]) {}
    /// Combined delete hook (ReactFlow `onDelete`-like).
    fn on_delete(&mut self, _change: DeleteChange) {}
}

/// Headless/store view callbacks for B-layer consumers.
///
/// Use this layer for app-owned viewport/selection synchronization. These hooks are derived from
/// `ViewChange` and remain headless-safe.
///
/// Ordering guarantees (per `ViewChanged` store event):
///
/// 1) `on_view_change`
/// 2) `on_viewport_change` / `on_selection_change` for each derived `ViewChange`
pub trait NodeGraphViewCallbacks: 'static {
    fn on_view_change(&mut self, _changes: &[crate::runtime::events::ViewChange]) {}

    fn on_viewport_change(&mut self, _pan: CanvasPoint, _zoom: f32) {}
    fn on_selection_change(&mut self, _sel: SelectionChange) {}
}

/// UI gesture lifecycle callbacks for retained/editor shells.
///
/// Use this layer for canvas-owned transient gesture observation. App-facing controlled
/// integrations usually only need commit/view callbacks unless they intentionally react to
/// pointer-driven lifecycle events.
pub trait NodeGraphGestureCallbacks: 'static {
    /// UI-driven hook: viewport pan/zoom gesture start (ReactFlow `onMoveStart`).
    fn on_move_start(&mut self, _ev: ViewportMoveStart) {}
    /// UI-driven hook: viewport pan/zoom gesture update (ReactFlow `onMove`).
    fn on_move(&mut self, _ev: ViewportMove) {}
    /// UI-driven hook: viewport pan/zoom gesture end (ReactFlow `onMoveEnd`).
    fn on_move_end(&mut self, _ev: ViewportMoveEnd) {}

    /// UI-driven hook: node drag gesture start (ReactFlow `onNodeDragStart`).
    fn on_node_drag_start(&mut self, _ev: NodeDragStart) {}
    /// UI-driven hook: node drag gesture end (ReactFlow `onNodeDragStop`).
    fn on_node_drag_end(&mut self, _ev: NodeDragEnd) {}
    /// UI-driven hook: node drag gesture move (ReactFlow `onNodeDrag`).
    fn on_node_drag(&mut self, _ev: NodeDragUpdate) {}

    /// UI-driven hook: called when a connection gesture starts (after drag threshold / click-to-connect).
    fn on_connect_start(&mut self, _ev: crate::runtime::events::ConnectStart) {}
    /// UI-driven hook: called when a connection gesture ends (commit/reject/cancel/picker).
    fn on_connect_end(&mut self, _ev: crate::runtime::events::ConnectEnd) {}
}

/// Composite callback surface consumed by store/canvas adapters.
///
/// Prefer implementing the smallest concern traits:
///
/// - `NodeGraphCommitCallbacks` for committed graph patches,
/// - `NodeGraphViewCallbacks` for viewport/selection synchronization,
/// - `NodeGraphGestureCallbacks` for transient UI gesture lifecycle.
///
/// `NodeGraphCallbacks` itself is only the composition boundary used by `install_callbacks` and
/// retained canvas wiring.
pub trait NodeGraphCallbacks:
    NodeGraphCommitCallbacks + NodeGraphViewCallbacks + NodeGraphGestureCallbacks
{
}

impl<T> NodeGraphCallbacks for T where
    T: NodeGraphCommitCallbacks + NodeGraphViewCallbacks + NodeGraphGestureCallbacks
{
}
