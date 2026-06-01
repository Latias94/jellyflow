use jellyflow_core::core::{CanvasRect, CanvasSize};
use jellyflow_core::interaction::NodeGraphDragHandleMode;

use crate::io::config::types::NodeGraphNodeOrigin;
use crate::io::tuning::NodeGraphAutoPanTuning;

use super::super::NodeGraphInteractionState;

/// Node dragging, snapping, and node-space settings resolved for runtime use.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeGraphNodeDragInteraction<'a> {
    pub nodes_draggable: bool,
    pub snap_to_grid: bool,
    pub snap_grid: CanvasSize,
    pub snaplines: bool,
    pub snaplines_threshold: f32,
    pub node_drag_threshold: f32,
    pub node_drag_handle_mode: NodeGraphDragHandleMode,
    pub node_click_distance: f32,
    pub node_extent: Option<CanvasRect>,
    pub node_origin: NodeGraphNodeOrigin,
    pub auto_pan: &'a NodeGraphAutoPanTuning,
}

impl NodeGraphInteractionState {
    pub fn node_drag_interaction(&self) -> NodeGraphNodeDragInteraction<'_> {
        NodeGraphNodeDragInteraction {
            nodes_draggable: self.nodes_draggable,
            snap_to_grid: self.snap_to_grid,
            snap_grid: self.snap_grid,
            snaplines: self.snaplines,
            snaplines_threshold: self.snaplines_threshold,
            node_drag_threshold: self.node_drag_threshold,
            node_drag_handle_mode: self.node_drag_handle_mode,
            node_click_distance: self.node_click_distance,
            node_extent: self.node_extent,
            node_origin: self.node_origin,
            auto_pan: &self.auto_pan,
        }
    }
}
