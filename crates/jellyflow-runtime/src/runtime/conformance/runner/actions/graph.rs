use crate::runtime::events::NodeGraphGestureEvent;
use crate::runtime::store::NodeGraphStore;
use jellyflow_core::core::{CanvasPoint, EdgeId, GroupId, NodeId};
use jellyflow_core::ops::GraphTransaction;

pub(super) fn dispatch_transaction(
    store: &mut NodeGraphStore,
    transaction: &GraphTransaction,
) -> Result<(), String> {
    store
        .dispatch_transaction(transaction)
        .map(|_| ())
        .map_err(|err| err.to_string())
}

pub(super) fn set_viewport(store: &mut NodeGraphStore, pan: CanvasPoint, zoom: f32) {
    store.set_viewport(pan, zoom);
}

pub(super) fn set_selection(
    store: &mut NodeGraphStore,
    nodes: &[NodeId],
    edges: &[EdgeId],
    groups: &[GroupId],
) {
    store.set_selection(nodes.to_vec(), edges.to_vec(), groups.to_vec());
}

pub(super) fn emit_gesture(store: &mut NodeGraphStore, event: &NodeGraphGestureEvent) {
    store.emit_gesture(event.clone());
}
