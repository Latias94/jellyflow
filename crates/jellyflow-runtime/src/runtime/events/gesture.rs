use super::{ConnectEnd, ConnectStart, NodeDragEnd, NodeDragStart, NodeDragUpdate};

/// Transient UI gesture event emitted to gesture subscribers.
#[derive(Debug, Clone, PartialEq)]
pub enum NodeGraphGestureEvent {
    ConnectStart(ConnectStart),
    ConnectEnd(ConnectEnd),
    NodeDragStart(NodeDragStart),
    NodeDragUpdate(NodeDragUpdate),
    NodeDragEnd(NodeDragEnd),
}
