use serde::{Deserialize, Serialize};

use jellyflow_core::core::{CanvasRect, CanvasSize};
use jellyflow_core::interaction::{
    NodeGraphConnectionMode, NodeGraphDragHandleMode, NodeGraphModifierKey,
};

use crate::io::tuning::{NodeGraphAutoPanTuning, NodeGraphPanInertiaTuning};

use super::super::defaults::*;
use super::super::keys::{NodeGraphDeleteKey, NodeGraphKeyCode};
use super::super::state::NodeGraphInteractionState;
use super::super::types::{
    NodeGraphBoxSelectEdges, NodeGraphNodeOrigin, NodeGraphNudgeStepMode,
    NodeGraphPanOnDragButtons, NodeGraphPanOnScrollMode, NodeGraphSelectionMode,
    NodeGraphViewportEase, NodeGraphViewportInterpolate, default_box_select_edges,
    default_pan_on_drag_buttons,
};

/// Persisted interaction configuration stored alongside view state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeGraphInteractionConfig {
    #[serde(default = "default_elements_selectable")]
    pub elements_selectable: bool,
    #[serde(default = "default_nodes_draggable")]
    pub nodes_draggable: bool,
    #[serde(default = "default_nodes_connectable")]
    pub nodes_connectable: bool,
    #[serde(default = "default_nodes_deletable")]
    pub nodes_deletable: bool,
    #[serde(default = "default_edges_selectable")]
    pub edges_selectable: bool,
    #[serde(default = "default_edges_deletable")]
    pub edges_deletable: bool,
    #[serde(default = "default_edges_focusable")]
    pub edges_focusable: bool,
    #[serde(default = "default_edges_reconnectable")]
    pub edges_reconnectable: bool,
    #[serde(default)]
    pub connection_mode: NodeGraphConnectionMode,
    #[serde(default = "default_connection_radius")]
    pub connection_radius: f32,
    #[serde(default = "default_reconnect_radius")]
    pub reconnect_radius: f32,
    #[serde(default)]
    pub reconnect_on_drop_empty: bool,
    #[serde(default = "default_edge_interaction_width")]
    pub edge_interaction_width: f32,
    #[serde(default = "default_bezier_hit_test_steps")]
    pub bezier_hit_test_steps: u8,
    #[serde(default = "default_elevate_nodes_on_select")]
    pub elevate_nodes_on_select: bool,
    #[serde(default = "default_elevate_edges_on_select")]
    pub elevate_edges_on_select: bool,
    #[serde(default)]
    pub snap_to_grid: bool,
    #[serde(default = "default_snap_grid")]
    pub snap_grid: CanvasSize,
    #[serde(default = "default_snaplines")]
    pub snaplines: bool,
    #[serde(default = "default_snaplines_threshold")]
    pub snaplines_threshold: f32,
    #[serde(default = "default_pan_on_scroll")]
    pub pan_on_scroll: bool,
    #[serde(default = "default_pan_on_drag_buttons")]
    pub pan_on_drag: NodeGraphPanOnDragButtons,
    #[serde(default)]
    pub selection_on_drag: bool,
    #[serde(default)]
    pub selection_mode: NodeGraphSelectionMode,
    #[serde(default = "default_box_select_edges")]
    pub box_select_edges: NodeGraphBoxSelectEdges,
    #[serde(default = "default_selection_key")]
    pub selection_key: NodeGraphModifierKey,
    #[serde(default = "default_multi_selection_key")]
    pub multi_selection_key: NodeGraphModifierKey,
    #[serde(default)]
    pub delete_key: NodeGraphDeleteKey,
    #[serde(default)]
    pub nudge_step_mode: NodeGraphNudgeStepMode,
    #[serde(default = "default_nudge_step_px")]
    pub nudge_step_px: f32,
    #[serde(default = "default_nudge_fast_step_px")]
    pub nudge_fast_step_px: f32,
    #[serde(default)]
    pub disable_keyboard_a11y: bool,
    #[serde(default = "default_pane_click_distance")]
    pub pane_click_distance: f32,
    #[serde(
        default = "default_pan_activation_key_code",
        skip_serializing_if = "Option::is_none"
    )]
    pub pan_activation_key_code: Option<NodeGraphKeyCode>,
    #[serde(default = "default_space_to_pan")]
    pub space_to_pan: bool,
    #[serde(default = "default_pan_on_scroll_speed")]
    pub pan_on_scroll_speed: f32,
    #[serde(default)]
    pub pan_on_scroll_mode: NodeGraphPanOnScrollMode,
    #[serde(default)]
    pub pan_inertia: NodeGraphPanInertiaTuning,
    #[serde(default = "default_zoom_on_scroll")]
    pub zoom_on_scroll: bool,
    #[serde(default = "default_zoom_on_scroll_speed")]
    pub zoom_on_scroll_speed: f32,
    #[serde(default = "default_zoom_on_pinch")]
    pub zoom_on_pinch: bool,
    #[serde(default = "default_zoom_on_pinch_speed")]
    pub zoom_on_pinch_speed: f32,
    #[serde(default = "default_zoom_on_double_click")]
    pub zoom_on_double_click: bool,
    #[serde(default = "default_frame_view_duration_ms")]
    pub frame_view_duration_ms: u32,
    #[serde(default)]
    pub frame_view_interpolate: NodeGraphViewportInterpolate,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame_view_ease: Option<NodeGraphViewportEase>,
    #[serde(default = "default_frame_view_padding")]
    pub frame_view_padding: f32,
    #[serde(default = "default_reroute_on_edge_double_click")]
    pub reroute_on_edge_double_click: bool,
    #[serde(default = "default_edge_insert_on_alt_drag")]
    pub edge_insert_on_alt_drag: bool,
    #[serde(default)]
    pub zoom_activation_key: NodeGraphModifierKey,
    #[serde(default = "default_node_drag_threshold")]
    pub node_drag_threshold: f32,
    #[serde(default)]
    pub node_drag_handle_mode: NodeGraphDragHandleMode,
    #[serde(default = "default_node_click_distance")]
    pub node_click_distance: f32,
    #[serde(default = "default_connection_drag_threshold")]
    pub connection_drag_threshold: f32,
    #[serde(default)]
    pub connect_on_click: bool,
    #[serde(default)]
    pub auto_pan: NodeGraphAutoPanTuning,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub translate_extent: Option<CanvasRect>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_extent: Option<CanvasRect>,
    #[serde(default)]
    pub node_origin: NodeGraphNodeOrigin,
}

impl NodeGraphInteractionConfig {
    pub fn is_default(this: &Self) -> bool {
        this == &Self::default()
    }
}

impl Default for NodeGraphInteractionConfig {
    fn default() -> Self {
        NodeGraphInteractionState::default().config()
    }
}
