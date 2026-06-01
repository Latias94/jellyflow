use serde::{Deserialize, Serialize};

use jellyflow_core::core::{CanvasRect, CanvasSize};
use jellyflow_core::interaction::{
    NodeGraphConnectionMode, NodeGraphDragHandleMode, NodeGraphModifierKey,
    NodeGraphZoomActivationKey,
};

use crate::io::tuning::{
    NodeGraphAutoPanTuning, NodeGraphPaintCachePruneTuning, NodeGraphPanInertiaTuning,
    NodeGraphSpatialIndexTuning,
};
use crate::runtime::geometry::EdgeHitTestOptions;

use super::keys::{NodeGraphDeleteKey, NodeGraphKeyCode};
use super::types::{
    NodeGraphBoxSelectEdges, NodeGraphNodeOrigin, NodeGraphNudgeStepMode,
    NodeGraphPanOnDragButtons, NodeGraphPanOnScrollMode, NodeGraphSelectionMode,
    NodeGraphViewportEase, NodeGraphViewportInterpolate,
};

mod default_impl;
mod split;

/// Resolved runtime interaction state assembled from persisted config and runtime tuning.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeGraphInteractionState {
    pub elements_selectable: bool,
    pub nodes_draggable: bool,
    pub nodes_connectable: bool,
    pub nodes_deletable: bool,
    pub edges_selectable: bool,
    pub edges_deletable: bool,
    pub edges_focusable: bool,
    pub edges_reconnectable: bool,
    pub connection_mode: NodeGraphConnectionMode,
    pub connection_radius: f32,
    pub reconnect_radius: f32,
    pub reconnect_on_drop_empty: bool,
    pub edge_interaction_width: f32,
    pub bezier_hit_test_steps: u8,
    pub spatial_index: NodeGraphSpatialIndexTuning,
    pub only_render_visible_elements: bool,
    pub elevate_nodes_on_select: bool,
    pub elevate_edges_on_select: bool,
    pub paint_cache_prune: NodeGraphPaintCachePruneTuning,
    pub snap_to_grid: bool,
    pub snap_grid: CanvasSize,
    pub snaplines: bool,
    pub snaplines_threshold: f32,
    pub pan_on_scroll: bool,
    pub pan_on_drag: NodeGraphPanOnDragButtons,
    pub selection_on_drag: bool,
    pub selection_mode: NodeGraphSelectionMode,
    pub box_select_edges: NodeGraphBoxSelectEdges,
    pub selection_key: NodeGraphModifierKey,
    pub multi_selection_key: NodeGraphModifierKey,
    pub delete_key: NodeGraphDeleteKey,
    pub nudge_step_mode: NodeGraphNudgeStepMode,
    pub nudge_step_px: f32,
    pub nudge_fast_step_px: f32,
    pub disable_keyboard_a11y: bool,
    pub pane_click_distance: f32,
    pub pan_activation_key_code: Option<NodeGraphKeyCode>,
    pub space_to_pan: bool,
    pub pan_on_scroll_speed: f32,
    pub pan_on_scroll_mode: NodeGraphPanOnScrollMode,
    pub pan_inertia: NodeGraphPanInertiaTuning,
    pub zoom_on_scroll: bool,
    pub zoom_on_scroll_speed: f32,
    pub zoom_on_pinch: bool,
    pub zoom_on_pinch_speed: f32,
    pub zoom_on_double_click: bool,
    pub frame_view_duration_ms: u32,
    pub frame_view_interpolate: NodeGraphViewportInterpolate,
    pub frame_view_ease: Option<NodeGraphViewportEase>,
    pub frame_view_padding: f32,
    pub reroute_on_edge_double_click: bool,
    pub edge_insert_on_alt_drag: bool,
    pub zoom_activation_key: NodeGraphZoomActivationKey,
    pub node_drag_threshold: f32,
    pub node_drag_handle_mode: NodeGraphDragHandleMode,
    pub node_click_distance: f32,
    pub connection_drag_threshold: f32,
    pub connect_on_click: bool,
    pub auto_pan: NodeGraphAutoPanTuning,
    pub translate_extent: Option<CanvasRect>,
    pub node_extent: Option<CanvasRect>,
    pub node_origin: NodeGraphNodeOrigin,
}

impl NodeGraphInteractionState {
    pub fn edge_hit_test_options(&self) -> EdgeHitTestOptions {
        EdgeHitTestOptions::new(
            self.edge_interaction_width,
            usize::from(self.bezier_hit_test_steps),
        )
    }
}
