mod commit;
mod gesture;
mod view;

use super::traits::NodeGraphCallbacks;
use super::types::{ConnectionChange, DeleteChange};
use crate::runtime::events::{NodeGraphGestureEvent, NodeGraphStoreEvent};
use crate::runtime::xyflow::projection;
use jellyflow_core::ops::GraphTransaction;

pub fn dispatch_store_event_callbacks(
    callbacks: &mut dyn NodeGraphCallbacks,
    ev: NodeGraphStoreEvent<'_>,
) {
    match ev {
        NodeGraphStoreEvent::DocumentReplaced { .. } => {}
        NodeGraphStoreEvent::GraphCommitted { patch } => {
            commit::dispatch_graph_commit_callbacks(callbacks, patch);
        }
        NodeGraphStoreEvent::ViewChanged { changes, .. } => {
            view::dispatch_view_callbacks(callbacks, changes);
        }
    }
}

pub(super) fn dispatch_gesture_callbacks(
    callbacks: &mut dyn NodeGraphCallbacks,
    ev: NodeGraphGestureEvent,
) {
    gesture::dispatch_gesture_callbacks(callbacks, ev);
}

pub fn connection_changes_from_transaction(tx: &GraphTransaction) -> Vec<ConnectionChange> {
    projection::connection_changes_from_transaction(tx)
}

pub fn delete_changes_from_transaction(tx: &GraphTransaction) -> DeleteChange {
    projection::delete_changes_from_transaction(tx)
}
