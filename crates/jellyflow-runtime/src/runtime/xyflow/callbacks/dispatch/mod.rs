mod commit;
mod gesture;
mod view;

use super::traits::NodeGraphCallbacks;
use crate::runtime::events::{NodeGraphGestureEvent, NodeGraphStoreEvent};

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
