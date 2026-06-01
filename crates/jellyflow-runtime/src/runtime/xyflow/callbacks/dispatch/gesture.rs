use super::super::traits::NodeGraphCallbacks;
use crate::runtime::events::{
    ConnectEnd, ConnectStart, NodeDragEnd, NodeDragStart, NodeDragUpdate, NodeGraphGestureEvent,
    ViewportMove, ViewportMoveEnd, ViewportMoveStart,
};

pub(super) fn dispatch_gesture_callbacks(
    callbacks: &mut dyn NodeGraphCallbacks,
    ev: NodeGraphGestureEvent,
) {
    match ev {
        NodeGraphGestureEvent::ConnectStart(ev) => dispatch_connect_start_callbacks(callbacks, ev),
        NodeGraphGestureEvent::ConnectEnd(ev) => dispatch_connect_end_callbacks(callbacks, ev),
        NodeGraphGestureEvent::NodeDragStart(ev) => {
            dispatch_node_drag_start_callbacks(callbacks, ev);
        }
        NodeGraphGestureEvent::NodeDragUpdate(ev) => dispatch_node_drag_callbacks(callbacks, ev),
        NodeGraphGestureEvent::NodeDragEnd(ev) => dispatch_node_drag_end_callbacks(callbacks, ev),
        NodeGraphGestureEvent::ViewportMoveStart(ev) => {
            dispatch_viewport_move_start_callbacks(callbacks, ev);
        }
        NodeGraphGestureEvent::ViewportMove(ev) => dispatch_viewport_move_callbacks(callbacks, ev),
        NodeGraphGestureEvent::ViewportMoveEnd(ev) => {
            dispatch_viewport_move_end_callbacks(callbacks, ev);
        }
    }
}

fn dispatch_connect_start_callbacks(callbacks: &mut dyn NodeGraphCallbacks, ev: ConnectStart) {
    callbacks.on_connect_start(ev);
}

fn dispatch_connect_end_callbacks(callbacks: &mut dyn NodeGraphCallbacks, ev: ConnectEnd) {
    callbacks.on_connect_end(ev);
}

fn dispatch_node_drag_start_callbacks(callbacks: &mut dyn NodeGraphCallbacks, ev: NodeDragStart) {
    callbacks.on_node_drag_start(ev);
}

fn dispatch_node_drag_callbacks(callbacks: &mut dyn NodeGraphCallbacks, ev: NodeDragUpdate) {
    callbacks.on_node_drag(ev);
}

fn dispatch_node_drag_end_callbacks(callbacks: &mut dyn NodeGraphCallbacks, ev: NodeDragEnd) {
    callbacks.on_node_drag_end(ev);
}

fn dispatch_viewport_move_start_callbacks(
    callbacks: &mut dyn NodeGraphCallbacks,
    ev: ViewportMoveStart,
) {
    callbacks.on_move_start(ev);
}

fn dispatch_viewport_move_callbacks(callbacks: &mut dyn NodeGraphCallbacks, ev: ViewportMove) {
    callbacks.on_move(ev);
}

fn dispatch_viewport_move_end_callbacks(
    callbacks: &mut dyn NodeGraphCallbacks,
    ev: ViewportMoveEnd,
) {
    callbacks.on_move_end(ev);
}
