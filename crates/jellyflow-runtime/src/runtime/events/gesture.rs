use serde::{Deserialize, Serialize};

use super::{
    ConnectEnd, ConnectStart, NodeDragEnd, NodeDragStart, NodeDragUpdate, ViewportMove,
    ViewportMoveEnd, ViewportMoveStart,
};

/// Transient UI gesture event emitted to gesture subscribers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum NodeGraphGestureEvent {
    ConnectStart(ConnectStart),
    ConnectEnd(ConnectEnd),
    NodeDragStart(NodeDragStart),
    NodeDragUpdate(NodeDragUpdate),
    NodeDragEnd(NodeDragEnd),
    ViewportMoveStart(ViewportMoveStart),
    ViewportMove(ViewportMove),
    ViewportMoveEnd(ViewportMoveEnd),
}
