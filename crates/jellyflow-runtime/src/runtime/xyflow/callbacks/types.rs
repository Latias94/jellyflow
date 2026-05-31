use jellyflow_core::core::{CanvasPoint, EdgeId, EdgeKind, GroupId, NodeId, PortId, StickyNoteId};
use jellyflow_core::ops::EdgeEndpoints;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeleteChange {
    pub nodes: Vec<NodeId>,
    pub edges: Vec<EdgeId>,
    pub groups: Vec<GroupId>,
    pub sticky_notes: Vec<StickyNoteId>,
}

impl DeleteChange {
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
            && self.edges.is_empty()
            && self.groups.is_empty()
            && self.sticky_notes.is_empty()
    }
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
