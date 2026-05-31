use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeGraphPanOnDragButtons {
    /// Pan the canvas by dragging on empty background with the left mouse button.
    #[serde(default)]
    pub left: bool,
    /// Pan the canvas by dragging with the middle mouse button.
    #[serde(default)]
    pub middle: bool,
    /// Pan the canvas by dragging with the right mouse button.
    ///
    /// When enabled, apps should provide an alternate way to open context menus (or make context
    /// menus conditional on "click without pan"), matching XyFlow's `panOnDrag={[2]}` patterns.
    #[serde(default)]
    pub right: bool,
}

pub(super) fn default_pan_on_drag_buttons() -> NodeGraphPanOnDragButtons {
    NodeGraphPanOnDragButtons {
        left: true,
        middle: true,
        right: false,
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeGraphPanOnScrollMode {
    #[default]
    Free,
    Horizontal,
    Vertical,
}
