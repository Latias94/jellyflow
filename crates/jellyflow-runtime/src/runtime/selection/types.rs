use jellyflow_core::core::{CanvasSize, EdgeId, GroupId, NodeId};

/// Modifier state that controls whether a selection gesture adds to existing selection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SelectionModifier {
    #[default]
    Replace,
    Additive,
}

impl SelectionModifier {
    pub fn additive(self) -> bool {
        matches!(self, Self::Additive)
    }
}

/// Options for applying a marquee selection box.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SelectionBoxOptions {
    /// Whether the box result replaces or unions with the current selection.
    pub modifier: SelectionModifier,
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
