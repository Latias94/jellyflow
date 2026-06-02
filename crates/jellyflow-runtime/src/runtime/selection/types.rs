use serde::{Deserialize, Serialize};

use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize, EdgeId, GroupId, NodeId};

/// Modifier state that controls whether a selection gesture adds to existing selection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct SelectionBoxOptions {
    /// Whether the box result replaces or unions with the current selection.
    #[serde(default)]
    pub modifier: SelectionModifier,
    /// Fallback size for nodes that do not have an explicit measured size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_size: Option<CanvasSize>,
}

/// Renderer-neutral input for a canvas-space marquee selection gesture.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SelectionBoxInput {
    pub rect: CanvasRect,
    #[serde(default)]
    pub options: SelectionBoxOptions,
}

impl SelectionBoxInput {
    pub fn new(rect: CanvasRect, options: SelectionBoxOptions) -> Self {
        Self { rect, options }
    }

    pub fn from_drag(
        start: CanvasPoint,
        current: CanvasPoint,
        options: SelectionBoxOptions,
    ) -> Self {
        Self::new(normalized_drag_rect(start, current), options)
    }

    pub fn replace(rect: CanvasRect) -> Self {
        Self::new(rect, SelectionBoxOptions::default())
    }

    pub fn replace_from_drag(start: CanvasPoint, current: CanvasPoint) -> Self {
        Self::from_drag(start, current, SelectionBoxOptions::default())
    }

    pub fn additive(rect: CanvasRect) -> Self {
        Self::new(
            rect,
            SelectionBoxOptions {
                modifier: SelectionModifier::Additive,
                ..SelectionBoxOptions::default()
            },
        )
    }

    pub fn additive_from_drag(start: CanvasPoint, current: CanvasPoint) -> Self {
        Self::from_drag(
            start,
            current,
            SelectionBoxOptions {
                modifier: SelectionModifier::Additive,
                ..SelectionBoxOptions::default()
            },
        )
    }
}

fn normalized_drag_rect(start: CanvasPoint, current: CanvasPoint) -> CanvasRect {
    let min_x = start.x.min(current.x);
    let min_y = start.y.min(current.y);
    CanvasRect {
        origin: CanvasPoint { x: min_x, y: min_y },
        size: CanvasSize {
            width: (start.x - current.x).abs(),
            height: (start.y - current.y).abs(),
        },
    }
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

/// Resolved selection-box outcome ready to be applied by a store or inspected by tests.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SelectionBoxDecision {
    result: SelectionBoxResult,
}

impl SelectionBoxDecision {
    pub fn new(result: SelectionBoxResult) -> Self {
        Self { result }
    }

    pub fn result(&self) -> &SelectionBoxResult {
        &self.result
    }

    pub fn into_result(self) -> SelectionBoxResult {
        self.result
    }
}
