use std::cell::RefCell;
use std::rc::Rc;

use crate::runtime::events::{
    ConnectEnd, ConnectStart, NodeDragEnd, NodeDragStart, NodeDragUpdate, ViewChange, ViewportMove,
    ViewportMoveEnd, ViewportMoveStart,
};
use crate::runtime::xyflow::callbacks::{
    ConnectionChange, EdgeConnection, NodeGraphCommitCallbacks, NodeGraphGestureCallbacks,
    NodeGraphViewCallbacks, SelectionChange,
};
use crate::runtime::xyflow::changes::{EdgeChange, NodeChange, NodeGraphChanges};
use jellyflow_core::core::{CanvasPoint, EdgeId};
use jellyflow_core::ops::EdgeEndpoints;

use super::super::scenario::{
    ConformanceCallbackEvent, ConformanceTraceEvent, ConformanceViewChange,
};

#[derive(Clone)]
pub(super) struct CallbackTraceRecorder {
    trace: Rc<RefCell<Vec<ConformanceTraceEvent>>>,
}

impl CallbackTraceRecorder {
    pub(super) fn new(trace: Rc<RefCell<Vec<ConformanceTraceEvent>>>) -> Self {
        Self { trace }
    }

    fn push(&self, event: ConformanceCallbackEvent) {
        self.trace
            .borrow_mut()
            .push(ConformanceTraceEvent::Callback(event));
    }
}

impl NodeGraphCommitCallbacks for CallbackTraceRecorder {
    fn on_graph_commit(&mut self, patch: &crate::runtime::commit::NodeGraphPatch) {
        self.push(ConformanceCallbackEvent::GraphCommit {
            label: patch.transaction().label().map(str::to_owned),
        });
    }

    fn on_node_edge_changes(&mut self, changes: &NodeGraphChanges) {
        self.push(ConformanceCallbackEvent::NodeEdgeChanges {
            nodes: changes.nodes().len(),
            edges: changes.edges().len(),
        });
    }

    fn on_nodes_change(&mut self, changes: &[NodeChange]) {
        self.push(ConformanceCallbackEvent::NodesChange {
            count: changes.len(),
        });
    }

    fn on_edges_change(&mut self, changes: &[EdgeChange]) {
        self.push(ConformanceCallbackEvent::EdgesChange {
            count: changes.len(),
        });
    }

    fn on_connection_change(&mut self, change: ConnectionChange) {
        self.push(ConformanceCallbackEvent::ConnectionChange(change));
    }

    fn on_connect(&mut self, conn: EdgeConnection) {
        self.push(ConformanceCallbackEvent::Connect(conn));
    }

    fn on_disconnect(&mut self, conn: EdgeConnection) {
        self.push(ConformanceCallbackEvent::Disconnect(conn));
    }

    fn on_reconnect(&mut self, edge: EdgeId, from: EdgeEndpoints, to: EdgeEndpoints) {
        self.push(ConformanceCallbackEvent::Reconnect { edge, from, to });
    }
}

impl NodeGraphViewCallbacks for CallbackTraceRecorder {
    fn on_view_change(&mut self, changes: &[ViewChange]) {
        self.push(ConformanceCallbackEvent::ViewChange {
            changes: changes
                .iter()
                .map(ConformanceViewChange::from_view_change)
                .collect(),
        });
    }

    fn on_viewport_change(&mut self, pan: CanvasPoint, zoom: f32) {
        self.push(ConformanceCallbackEvent::ViewportChange { pan, zoom });
    }

    fn on_selection_change(&mut self, sel: SelectionChange) {
        let (nodes, edges, groups) = sel.into_parts();
        self.push(ConformanceCallbackEvent::SelectionChange {
            nodes,
            edges,
            groups,
        });
    }
}

impl NodeGraphGestureCallbacks for CallbackTraceRecorder {
    fn on_move_start(&mut self, ev: ViewportMoveStart) {
        self.push(ConformanceCallbackEvent::ViewportMoveStart(ev));
    }

    fn on_move(&mut self, ev: ViewportMove) {
        self.push(ConformanceCallbackEvent::ViewportMove(ev));
    }

    fn on_move_end(&mut self, ev: ViewportMoveEnd) {
        self.push(ConformanceCallbackEvent::ViewportMoveEnd(ev));
    }

    fn on_node_drag_start(&mut self, ev: NodeDragStart) {
        self.push(ConformanceCallbackEvent::NodeDragStart(ev));
    }

    fn on_node_drag(&mut self, ev: NodeDragUpdate) {
        self.push(ConformanceCallbackEvent::NodeDrag(ev));
    }

    fn on_node_drag_end(&mut self, ev: NodeDragEnd) {
        self.push(ConformanceCallbackEvent::NodeDragEnd(ev));
    }

    fn on_connect_start(&mut self, ev: ConnectStart) {
        self.push(ConformanceCallbackEvent::ConnectStart(ev));
    }

    fn on_connect_end(&mut self, ev: ConnectEnd) {
        self.push(ConformanceCallbackEvent::ConnectEnd(ev));
    }
}
