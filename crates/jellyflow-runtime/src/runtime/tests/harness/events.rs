use crate::runtime::events::{
    ConnectEnd, ConnectStart, NodeDragEnd, NodeDragStart, NodeDragUpdate, NodeGraphGestureEvent,
    NodeGraphStoreEvent, ViewChange, ViewportMove, ViewportMoveEnd, ViewportMoveStart,
};
use crate::runtime::xyflow::callbacks::{ConnectionChange, EdgeConnection};
use jellyflow_core::core::{CanvasPoint, EdgeId, GroupId, NodeId};
use jellyflow_core::ops::EdgeEndpoints;

#[derive(Debug, Clone, PartialEq)]
pub(in crate::runtime::tests) enum HarnessEvent {
    DocumentReplaced {
        before_revision: u64,
        after_revision: u64,
    },
    GraphCommitted {
        label: Option<String>,
        op_kinds: Vec<String>,
    },
    ViewChanged {
        changes: Vec<HarnessViewChange>,
    },
    Gesture(NodeGraphGestureEvent),
    Callback(HarnessCallbackEvent),
}

impl HarnessEvent {
    pub(in crate::runtime::tests) fn graph_commit(label: Option<&str>, op_kinds: &[&str]) -> Self {
        Self::GraphCommitted {
            label: label.map(str::to_owned),
            op_kinds: op_kinds.iter().map(|kind| (*kind).to_owned()).collect(),
        }
    }

    pub(in crate::runtime::tests) fn viewport(pan: CanvasPoint, zoom: f32) -> Self {
        Self::ViewChanged {
            changes: vec![HarnessViewChange::Viewport { pan, zoom }],
        }
    }

    pub(in crate::runtime::tests) fn selection(
        nodes: Vec<NodeId>,
        edges: Vec<EdgeId>,
        groups: Vec<GroupId>,
    ) -> Self {
        Self::ViewChanged {
            changes: vec![HarnessViewChange::Selection {
                nodes,
                edges,
                groups,
            }],
        }
    }

    pub(in crate::runtime::tests) fn gesture(event: NodeGraphGestureEvent) -> Self {
        Self::Gesture(event)
    }

    pub(in crate::runtime::tests) fn callback(event: HarnessCallbackEvent) -> Self {
        Self::Callback(event)
    }

    pub(super) fn from_store_event(event: NodeGraphStoreEvent<'_>) -> Self {
        match event {
            NodeGraphStoreEvent::DocumentReplaced { before, after } => Self::DocumentReplaced {
                before_revision: before.graph_revision,
                after_revision: after.graph_revision,
            },
            NodeGraphStoreEvent::GraphCommitted { patch } => Self::GraphCommitted {
                label: patch.transaction().label().map(str::to_owned),
                op_kinds: patch
                    .transaction()
                    .ops()
                    .iter()
                    .map(serialized_graph_op_kind)
                    .collect(),
            },
            NodeGraphStoreEvent::ViewChanged { changes, .. } => Self::ViewChanged {
                changes: changes
                    .iter()
                    .map(HarnessViewChange::from_view_change)
                    .collect(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(in crate::runtime::tests) enum HarnessCallbackEvent {
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

#[derive(Debug, Clone, PartialEq)]
pub(in crate::runtime::tests) enum HarnessViewChange {
    Viewport {
        pan: CanvasPoint,
        zoom: f32,
    },
    Selection {
        nodes: Vec<NodeId>,
        edges: Vec<EdgeId>,
        groups: Vec<GroupId>,
    },
}

impl HarnessViewChange {
    fn from_view_change(change: &ViewChange) -> Self {
        match change {
            ViewChange::Viewport { pan, zoom } => Self::Viewport {
                pan: *pan,
                zoom: *zoom,
            },
            ViewChange::Selection {
                nodes,
                edges,
                groups,
            } => Self::Selection {
                nodes: nodes.clone(),
                edges: edges.clone(),
                groups: groups.clone(),
            },
        }
    }
}

fn serialized_graph_op_kind(op: &jellyflow_core::ops::GraphOp) -> String {
    serde_json::to_value(op)
        .ok()
        .and_then(|value| {
            value
                .get("op")
                .and_then(|op| op.as_str())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "unknown".to_owned())
}
