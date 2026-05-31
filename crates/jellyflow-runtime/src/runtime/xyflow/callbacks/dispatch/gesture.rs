use super::super::traits::NodeGraphCallbacks;
use crate::runtime::events::{ConnectDragKind, ConnectEnd, ConnectStart, NodeGraphGestureEvent};

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
