use serde::{Deserialize, Serialize};

use crate::runtime::events::{
    ConnectEnd, ConnectStart, NodeDragEnd, NodeDragStart, NodeDragUpdate, NodeGraphGestureEvent,
    ViewportMove, ViewportMoveEnd, ViewportMoveStart,
};
use crate::runtime::xyflow::callbacks::{ConnectionChange, EdgeConnection};
use jellyflow_core::core::{CanvasPoint, EdgeId, GroupId, NodeId};
use jellyflow_core::ops::EdgeEndpoints;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum ConformanceTraceEvent {
    DocumentReplaced {
        before_revision: u64,
        after_revision: u64,
    },
    GraphCommitted {
        label: Option<String>,
        op_kinds: Vec<String>,
    },
    ViewChanged {
        changes: Vec<ConformanceViewChange>,
    },
    Gesture(NodeGraphGestureEvent),
    Callback(ConformanceCallbackEvent),
}

impl ConformanceTraceEvent {
    pub fn graph_commit(
        label: Option<impl Into<String>>,
        op_kinds: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self::GraphCommitted {
            label: label.map(Into::into),
            op_kinds: op_kinds.into_iter().map(Into::into).collect(),
        }
    }

    pub fn viewport(pan: CanvasPoint, zoom: f32) -> Self {
        Self::ViewChanged {
            changes: vec![ConformanceViewChange::Viewport { pan, zoom }],
        }
    }

    pub fn selection(
        nodes: impl IntoIterator<Item = NodeId>,
        edges: impl IntoIterator<Item = EdgeId>,
        groups: impl IntoIterator<Item = GroupId>,
    ) -> Self {
        Self::ViewChanged {
            changes: vec![ConformanceViewChange::Selection {
                nodes: nodes.into_iter().collect(),
                edges: edges.into_iter().collect(),
                groups: groups.into_iter().collect(),
            }],
        }
    }

    pub fn gesture(event: NodeGraphGestureEvent) -> Self {
        Self::Gesture(event)
    }

    pub fn callback(event: ConformanceCallbackEvent) -> Self {
        Self::Callback(event)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConformanceViewChange {
    Viewport {
        pan: CanvasPoint,
        zoom: f32,
    },
    Selection {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        nodes: Vec<NodeId>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        edges: Vec<EdgeId>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        groups: Vec<GroupId>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum ConformanceCallbackEvent {
    ViewChange {
        changes: Vec<ConformanceViewChange>,
    },
    ViewportChange {
        pan: CanvasPoint,
        zoom: f32,
    },
    SelectionChange {
        nodes: Vec<NodeId>,
        edges: Vec<EdgeId>,
        groups: Vec<GroupId>,
    },
    GraphCommit {
        label: Option<String>,
    },
    NodeEdgeChanges {
        nodes: usize,
        edges: usize,
    },
    NodesChange {
        count: usize,
    },
    EdgesChange {
        count: usize,
    },
    ConnectionChange(ConnectionChange),
    Connect(EdgeConnection),
    Disconnect(EdgeConnection),
    Reconnect {
        edge: EdgeId,
        from: EdgeEndpoints,
        to: EdgeEndpoints,
    },
    NodeDragStart(NodeDragStart),
    NodeDrag(NodeDragUpdate),
    NodeDragEnd(NodeDragEnd),
    ViewportMoveStart(ViewportMoveStart),
    ViewportMove(ViewportMove),
    ViewportMoveEnd(ViewportMoveEnd),
    ConnectStart(ConnectStart),
    ConnectEnd(ConnectEnd),
}
