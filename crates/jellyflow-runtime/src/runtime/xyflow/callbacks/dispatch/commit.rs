use super::super::traits::NodeGraphCallbacks;
use super::super::types::ConnectionChange;
use crate::runtime::commit::NodeGraphPatch;
use crate::runtime::xyflow::changes::NodeGraphChanges;
use jellyflow_core::ops::GraphTransaction;

use super::{connection_changes_from_transaction, delete_changes_from_transaction};

pub(super) fn dispatch_graph_commit_callbacks(
    callbacks: &mut dyn NodeGraphCallbacks,
    patch: &NodeGraphPatch,
) {
    let node_edge_changes = NodeGraphChanges::from_patch(patch);
    callbacks.on_graph_commit(patch);
    callbacks.on_node_edge_changes(&node_edge_changes);
    if !node_edge_changes.nodes().is_empty() {
        callbacks.on_nodes_change(node_edge_changes.nodes());
    }
    if !node_edge_changes.edges().is_empty() {
        callbacks.on_edges_change(node_edge_changes.edges());
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
    if !deleted.is_empty() {
        callbacks.on_delete(deleted);
    }
}
