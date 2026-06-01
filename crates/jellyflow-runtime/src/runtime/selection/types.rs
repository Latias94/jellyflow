use jellyflow_core::core::{CanvasSize, EdgeId, GroupId, NodeId};

/// Options for applying a marquee selection box.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SelectionBoxOptions {
    /// Whether the box result is unioned with the current selection.
    pub additive: bool,
    /// Fallback size for nodes that do not have an explicit measured size.
    pub fallback_size: Option<CanvasSize>,
}

/// Ordered selection result produced by a marquee selection box.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SelectionBoxResult {
    pub nodes: Vec<NodeId>,
    pub edges: Vec<EdgeId>,
    pub groups: Vec<GroupId>,
}

impl SelectionBoxResult {
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty() && self.edges.is_empty() && self.groups.is_empty()
    }
}
