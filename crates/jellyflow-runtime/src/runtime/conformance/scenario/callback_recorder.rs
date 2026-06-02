use std::cell::RefCell;
use std::rc::Rc;

use crate::runtime::events::{
    ConnectEnd, ConnectStart, NodeDragEnd, NodeDragStart, NodeDragUpdate, NodeResizeEnd,
    NodeResizeStart, NodeResizeUpdate, ViewChange, ViewportMove, ViewportMoveEnd,
    ViewportMoveStart,
};
use crate::runtime::xyflow::callbacks::{
    ConnectionChange, DeleteChange, EdgeConnection, NodeGraphCommitCallbacks,
    NodeGraphGestureCallbacks, NodeGraphViewCallbacks, SelectionChange,
};
use crate::runtime::xyflow::changes::{EdgeChange, NodeChange, NodeGraphChanges};
use jellyflow_core::core::{CanvasPoint, EdgeId, GroupId, NodeId, StickyNoteId};
use jellyflow_core::ops::EdgeEndpoints;

use super::{ConformanceCallbackEvent, ConformanceTraceEvent, ConformanceViewChange};

pub(crate) trait ConformanceCallbackTraceSink: Clone + 'static {
    fn push_callback(&self, event: ConformanceCallbackEvent);
}

impl ConformanceCallbackTraceSink for Rc<RefCell<Vec<ConformanceTraceEvent>>> {
    fn push_callback(&self, event: ConformanceCallbackEvent) {
        self.borrow_mut()
            .push(ConformanceTraceEvent::Callback(event));
    }
}

#[derive(Clone)]
pub(crate) struct ConformanceCallbackTraceRecorder<S> {
    sink: S,
}

impl<S> ConformanceCallbackTraceRecorder<S>
where
    S: ConformanceCallbackTraceSink,
{
    pub(crate) fn new(sink: S) -> Self {
        Self { sink }
    }

    fn push(&self, event: ConformanceCallbackEvent) {
        self.sink.push_callback(event);
    }
}

impl<S> NodeGraphCommitCallbacks for ConformanceCallbackTraceRecorder<S>
where
    S: ConformanceCallbackTraceSink,
{
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

    fn on_nodes_delete(&mut self, nodes: &[NodeId]) {
        self.push(ConformanceCallbackEvent::NodesDelete { count: nodes.len() });
    }

    fn on_edges_delete(&mut self, edges: &[EdgeId]) {
        self.push(ConformanceCallbackEvent::EdgesDelete { count: edges.len() });
    }

    fn on_groups_delete(&mut self, groups: &[GroupId]) {
        self.push(ConformanceCallbackEvent::GroupsDelete {
            count: groups.len(),
        });
    }

    fn on_sticky_notes_delete(&mut self, notes: &[StickyNoteId]) {
        self.push(ConformanceCallbackEvent::StickyNotesDelete { count: notes.len() });
    }

    fn on_delete(&mut self, change: DeleteChange) {
        self.push(ConformanceCallbackEvent::Delete {
            nodes: change.nodes().len(),
            edges: change.edges().len(),
            groups: change.groups().len(),
            sticky_notes: change.sticky_notes().len(),
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

impl<S> NodeGraphViewCallbacks for ConformanceCallbackTraceRecorder<S>
where
    S: ConformanceCallbackTraceSink,
{
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

impl<S> NodeGraphGestureCallbacks for ConformanceCallbackTraceRecorder<S>
where
    S: ConformanceCallbackTraceSink,
{
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

    fn on_node_resize_start(&mut self, ev: NodeResizeStart) {
        self.push(ConformanceCallbackEvent::NodeResizeStart(ev));
    }

    fn on_node_resize(&mut self, ev: NodeResizeUpdate) {
        self.push(ConformanceCallbackEvent::NodeResize(ev));
    }

    fn on_node_resize_end(&mut self, ev: NodeResizeEnd) {
        self.push(ConformanceCallbackEvent::NodeResizeEnd(ev));
    }

    fn on_connect_start(&mut self, ev: ConnectStart) {
        self.push(ConformanceCallbackEvent::ConnectStart(ev));
    }

    fn on_connect_end(&mut self, ev: ConnectEnd) {
        self.push(ConformanceCallbackEvent::ConnectEnd(ev));
    }
}
