use jellyflow_core::core::{
    CanvasPoint, Edge, EdgeId, EdgeKind, GroupId, NodeId, PortId, StickyNoteId,
};
use jellyflow_core::ops::EdgeEndpoints;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeleteChange {
    pub nodes: Vec<NodeId>,
    pub edges: Vec<EdgeId>,
    pub groups: Vec<GroupId>,
    pub sticky_notes: Vec<StickyNoteId>,
}

impl DeleteChange {
    pub fn from_parts(
        nodes: Vec<NodeId>,
        edges: Vec<EdgeId>,
        groups: Vec<GroupId>,
        sticky_notes: Vec<StickyNoteId>,
    ) -> Self {
        Self {
            nodes,
            edges,
            groups,
            sticky_notes,
        }
    }

    pub fn into_parts(self) -> (Vec<NodeId>, Vec<EdgeId>, Vec<GroupId>, Vec<StickyNoteId>) {
        (self.nodes, self.edges, self.groups, self.sticky_notes)
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
            && self.edges.is_empty()
            && self.groups.is_empty()
            && self.sticky_notes.is_empty()
    }

    pub fn nodes(&self) -> &[NodeId] {
        &self.nodes
    }

    pub fn edges(&self) -> &[EdgeId] {
        &self.edges
    }

    pub fn groups(&self) -> &[GroupId] {
        &self.groups
    }

    pub fn sticky_notes(&self) -> &[StickyNoteId] {
        &self.sticky_notes
    }

    pub(in crate::runtime::xyflow) fn push_node(&mut self, node: NodeId) {
        self.nodes.push(node);
    }

    pub(in crate::runtime::xyflow) fn push_edge(&mut self, edge: EdgeId) {
        self.edges.push(edge);
    }

    pub(in crate::runtime::xyflow) fn push_group(&mut self, group: GroupId) {
        self.groups.push(group);
    }

    pub(in crate::runtime::xyflow) fn push_sticky_note(&mut self, sticky_note: StickyNoteId) {
        self.sticky_notes.push(sticky_note);
    }

    pub(in crate::runtime::xyflow) fn extend_edges(
        &mut self,
        edges: impl IntoIterator<Item = EdgeId>,
    ) {
        self.edges.extend(edges);
    }

    pub(in crate::runtime::xyflow) fn sort_dedup(&mut self) {
        sort_dedup_items(&mut self.nodes);
        sort_dedup_items(&mut self.edges);
        sort_dedup_items(&mut self.groups);
        sort_dedup_items(&mut self.sticky_notes);
    }
}

fn sort_dedup_items<T: Ord>(items: &mut Vec<T>) {
    items.sort_unstable();
    items.dedup();
}

/// Viewport move gesture kind (UI-driven).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewportMoveKind {
    /// Pointer-drag panning (mouse/touch drag).
    PanDrag,
    /// Inertial/momentum panning after releasing a pan drag.
    PanInertia,
    /// Panning via scroll wheel / trackpad scroll when `pan_on_scroll` is enabled.
    PanScroll,
    /// Zooming via scroll wheel (e.g. Ctrl+wheel).
    ZoomWheel,
    /// Zooming via pinch gesture (trackpad pinch).
    ZoomPinch,
    /// Zooming via double-click gesture.
    ZoomDoubleClick,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewportMoveEndOutcome {
    Ended,
    Canceled,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewportMoveStart {
    pub kind: ViewportMoveKind,
    pub pan: CanvasPoint,
    pub zoom: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewportMoveEnd {
    pub kind: ViewportMoveKind,
    pub pan: CanvasPoint,
    pub zoom: f32,
    pub outcome: ViewportMoveEndOutcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeDragEndOutcome {
    Committed,
    Rejected,
    Canceled,
    NoOp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeDragStart {
    pub primary: NodeId,
    pub nodes: Vec<NodeId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeDragEnd {
    pub primary: NodeId,
    pub nodes: Vec<NodeId>,
    pub outcome: NodeDragEndOutcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EdgeConnection {
    pub edge: EdgeId,
    pub from: PortId,
    pub to: PortId,
    pub kind: EdgeKind,
}

impl EdgeConnection {
    pub fn new(edge: EdgeId, from: PortId, to: PortId, kind: EdgeKind) -> Self {
        Self {
            edge,
            from,
            to,
            kind,
        }
    }

    pub(in crate::runtime::xyflow) fn from_edge(edge_id: EdgeId, edge: &Edge) -> Self {
        Self::new(edge_id, edge.from, edge.to, edge.kind)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionChange {
    Connected(EdgeConnection),
    Disconnected(EdgeConnection),
    Reconnected {
        edge: EdgeId,
        from: EdgeEndpoints,
        to: EdgeEndpoints,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionChange {
    pub nodes: Vec<NodeId>,
    pub edges: Vec<EdgeId>,
    pub groups: Vec<GroupId>,
}

impl SelectionChange {
    pub fn new(nodes: Vec<NodeId>, edges: Vec<EdgeId>, groups: Vec<GroupId>) -> Self {
        Self {
            nodes,
            edges,
            groups,
        }
    }
}
