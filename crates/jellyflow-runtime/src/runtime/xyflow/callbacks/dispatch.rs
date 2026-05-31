use super::traits::NodeGraphCallbacks;
use super::types::{ConnectionChange, DeleteChange, SelectionChange};
use crate::runtime::commit::NodeGraphPatch;
use crate::runtime::events::{
    ConnectDragKind, ConnectEnd, ConnectStart, NodeGraphGestureEvent, NodeGraphStoreEvent,
    ViewChange,
};
use crate::runtime::xyflow::changes::NodeGraphChanges;
use crate::runtime::xyflow::projection;
use jellyflow_core::ops::GraphTransaction;

pub fn dispatch_store_event_callbacks(
    callbacks: &mut dyn NodeGraphCallbacks,
    ev: NodeGraphStoreEvent<'_>,
) {
    match ev {
        NodeGraphStoreEvent::DocumentReplaced { .. } => {}
        NodeGraphStoreEvent::GraphCommitted { patch } => {
            dispatch_graph_commit_callbacks(callbacks, patch);
        }
        NodeGraphStoreEvent::ViewChanged { changes, .. } => {
            dispatch_view_callbacks(callbacks, changes);
        }
    }
}

fn dispatch_graph_commit_callbacks(callbacks: &mut dyn NodeGraphCallbacks, patch: &NodeGraphPatch) {
    let node_edge_changes = NodeGraphChanges::from_patch(patch);
    callbacks.on_graph_commit(patch);
    callbacks.on_node_edge_changes(&node_edge_changes);
    if !node_edge_changes.nodes.is_empty() {
        callbacks.on_nodes_change(&node_edge_changes.nodes);
    }
    if !node_edge_changes.edges.is_empty() {
        callbacks.on_edges_change(&node_edge_changes.edges);
    }

    dispatch_connection_callbacks(callbacks, patch.transaction());
    dispatch_delete_callbacks(callbacks, patch.transaction());
}

fn dispatch_connection_callbacks(callbacks: &mut dyn NodeGraphCallbacks, tx: &GraphTransaction) {
    for change in connection_changes_from_transaction(tx) {
        callbacks.on_connection_change(change);
        match change {
            ConnectionChange::Connected(conn) => callbacks.on_connect(conn),
            ConnectionChange::Disconnected(conn) => callbacks.on_disconnect(conn),
            ConnectionChange::Reconnected { edge, from, to } => {
                callbacks.on_reconnect(edge, from, to);
                callbacks.on_edge_update(edge, from, to);
            }
        }
    }
}

fn dispatch_delete_callbacks(callbacks: &mut dyn NodeGraphCallbacks, tx: &GraphTransaction) {
    let deleted = delete_changes_from_transaction(tx);
    if !deleted.nodes.is_empty() {
        callbacks.on_nodes_delete(&deleted.nodes);
    }
    if !deleted.edges.is_empty() {
        callbacks.on_edges_delete(&deleted.edges);
    }
    if !deleted.groups.is_empty() {
        callbacks.on_groups_delete(&deleted.groups);
    }
    if !deleted.sticky_notes.is_empty() {
        callbacks.on_sticky_notes_delete(&deleted.sticky_notes);
    }
    if has_delete_changes(&deleted) {
        callbacks.on_delete(deleted);
    }
}

fn has_delete_changes(deleted: &DeleteChange) -> bool {
    !deleted.nodes.is_empty()
        || !deleted.edges.is_empty()
        || !deleted.groups.is_empty()
        || !deleted.sticky_notes.is_empty()
}

fn dispatch_view_callbacks(callbacks: &mut dyn NodeGraphCallbacks, changes: &[ViewChange]) {
    callbacks.on_view_change(changes);
    for change in changes.iter() {
        match change {
            ViewChange::Viewport { pan, zoom } => {
                callbacks.on_viewport_change(*pan, *zoom);
                callbacks.on_move(*pan, *zoom);
            }
            ViewChange::Selection {
                nodes,
                edges,
                groups,
            } => callbacks.on_selection_change(SelectionChange {
                nodes: nodes.clone(),
                edges: edges.clone(),
                groups: groups.clone(),
            }),
        }
    }
}

pub(super) fn dispatch_gesture_callbacks(
    callbacks: &mut dyn NodeGraphCallbacks,
    ev: NodeGraphGestureEvent,
) {
    match ev {
        NodeGraphGestureEvent::ConnectStart(ev) => dispatch_connect_start_callbacks(callbacks, ev),
        NodeGraphGestureEvent::ConnectEnd(ev) => dispatch_connect_end_callbacks(callbacks, ev),
    }
}

fn dispatch_connect_start_callbacks(callbacks: &mut dyn NodeGraphCallbacks, ev: ConnectStart) {
    let is_reconnect = is_reconnect_drag(&ev.kind);
    callbacks.on_connect_start(ev.clone());
    if is_reconnect {
        callbacks.on_reconnect_start(ev.clone());
        callbacks.on_edge_update_start(ev);
    }
}

fn dispatch_connect_end_callbacks(callbacks: &mut dyn NodeGraphCallbacks, ev: ConnectEnd) {
    let is_reconnect = is_reconnect_drag(&ev.kind);
    callbacks.on_connect_end(ev.clone());
    if is_reconnect {
        callbacks.on_reconnect_end(ev.clone());
        callbacks.on_edge_update_end(ev);
    }
}

fn is_reconnect_drag(kind: &ConnectDragKind) -> bool {
    matches!(
        kind,
        ConnectDragKind::Reconnect { .. } | ConnectDragKind::ReconnectMany { .. }
    )
}

pub fn connection_changes_from_transaction(tx: &GraphTransaction) -> Vec<ConnectionChange> {
    projection::connection_changes_from_transaction(tx)
}

pub fn delete_changes_from_transaction(tx: &GraphTransaction) -> DeleteChange {
    projection::delete_changes_from_transaction(tx)
}
