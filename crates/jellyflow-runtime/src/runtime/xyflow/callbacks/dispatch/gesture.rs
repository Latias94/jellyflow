use super::super::traits::NodeGraphCallbacks;
use crate::runtime::events::{ConnectEnd, ConnectStart, NodeGraphGestureEvent};

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
    callbacks.on_connect_start(ev);
}

fn dispatch_connect_end_callbacks(callbacks: &mut dyn NodeGraphCallbacks, ev: ConnectEnd) {
    callbacks.on_connect_end(ev);
}
