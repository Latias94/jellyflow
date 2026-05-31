//! Interaction configuration and resolved editor policy.

mod keys;
mod types;

use keyboard_types::Code as KeyCode;
use serde::{Deserialize, Serialize};

use jellyflow_core::core::{CanvasRect, CanvasSize};
use jellyflow_core::interaction::{
    NodeGraphConnectionMode, NodeGraphDragHandleMode, NodeGraphModifierKey,
    NodeGraphZoomActivationKey,
};

use super::tuning::{
    NodeGraphAutoPanTuning, NodeGraphPaintCachePruneTuning, NodeGraphPanInertiaTuning,
    NodeGraphRuntimeTuning, NodeGraphSpatialIndexTuning, default_only_render_visible_elements,
};

pub use self::keys::{NodeGraphDeleteKey, NodeGraphKeyCode};
pub use self::types::{
    NodeGraphBoxSelectEdges, NodeGraphNodeOrigin, NodeGraphNudgeStepMode,
    NodeGraphPanOnDragButtons, NodeGraphPanOnScrollMode, NodeGraphSelectionMode,
    NodeGraphViewportEase, NodeGraphViewportInterpolate,
};

use self::types::{default_box_select_edges, default_pan_on_drag_buttons};

fn default_nodes_draggable() -> bool {
    true
}

fn default_nodes_connectable() -> bool {
    true
}

fn default_nodes_deletable() -> bool {
    true
}

fn default_edges_deletable() -> bool {
    true
}

fn default_bezier_hit_test_steps() -> u8 {
    24
}

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
    #[serde(
        default = "default_box_select_edges",
        alias = "box_select_connected_edges"
    )]
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
    pub zoom_activation_key: NodeGraphZoomActivationKey,
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

/// Persisted editor configuration stored alongside pure view state.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NodeGraphEditorConfig {
    #[serde(
        default,
        skip_serializing_if = "NodeGraphInteractionConfig::is_default"
    )]
    pub interaction: NodeGraphInteractionConfig,
    #[serde(default, skip_serializing_if = "NodeGraphRuntimeTuning::is_default")]
    pub runtime_tuning: NodeGraphRuntimeTuning,
}

impl NodeGraphEditorConfig {
    pub fn is_default(this: &Self) -> bool {
        this == &Self::default()
    }

    pub fn resolved_interaction_state(&self) -> NodeGraphInteractionState {
        NodeGraphInteractionState::from_parts(&self.interaction, &self.runtime_tuning)
    }
}

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
    pub fn from_parts(
        config: &NodeGraphInteractionConfig,
        runtime_tuning: &NodeGraphRuntimeTuning,
    ) -> Self {
        Self {
            elements_selectable: config.elements_selectable,
            nodes_draggable: config.nodes_draggable,
            nodes_connectable: config.nodes_connectable,
            nodes_deletable: config.nodes_deletable,
            edges_selectable: config.edges_selectable,
            edges_deletable: config.edges_deletable,
            edges_focusable: config.edges_focusable,
            edges_reconnectable: config.edges_reconnectable,
            connection_mode: config.connection_mode,
            connection_radius: config.connection_radius,
            reconnect_radius: config.reconnect_radius,
            reconnect_on_drop_empty: config.reconnect_on_drop_empty,
            edge_interaction_width: config.edge_interaction_width,
            bezier_hit_test_steps: config.bezier_hit_test_steps,
            spatial_index: runtime_tuning.spatial_index,
            only_render_visible_elements: runtime_tuning.only_render_visible_elements,
            elevate_nodes_on_select: config.elevate_nodes_on_select,
            elevate_edges_on_select: config.elevate_edges_on_select,
            paint_cache_prune: runtime_tuning.paint_cache_prune,
            snap_to_grid: config.snap_to_grid,
            snap_grid: config.snap_grid,
            snaplines: config.snaplines,
            snaplines_threshold: config.snaplines_threshold,
            pan_on_scroll: config.pan_on_scroll,
            pan_on_drag: config.pan_on_drag,
            selection_on_drag: config.selection_on_drag,
            selection_mode: config.selection_mode,
            box_select_edges: config.box_select_edges,
            selection_key: config.selection_key,
            multi_selection_key: config.multi_selection_key,
            delete_key: config.delete_key,
            nudge_step_mode: config.nudge_step_mode,
            nudge_step_px: config.nudge_step_px,
            nudge_fast_step_px: config.nudge_fast_step_px,
            disable_keyboard_a11y: config.disable_keyboard_a11y,
            pane_click_distance: config.pane_click_distance,
            pan_activation_key_code: config.pan_activation_key_code,
            space_to_pan: config.space_to_pan,
            pan_on_scroll_speed: config.pan_on_scroll_speed,
            pan_on_scroll_mode: config.pan_on_scroll_mode,
            pan_inertia: config.pan_inertia.clone(),
            zoom_on_scroll: config.zoom_on_scroll,
            zoom_on_scroll_speed: config.zoom_on_scroll_speed,
            zoom_on_pinch: config.zoom_on_pinch,
            zoom_on_pinch_speed: config.zoom_on_pinch_speed,
            zoom_on_double_click: config.zoom_on_double_click,
            frame_view_duration_ms: config.frame_view_duration_ms,
            frame_view_interpolate: config.frame_view_interpolate,
            frame_view_ease: config.frame_view_ease,
            frame_view_padding: config.frame_view_padding,
            reroute_on_edge_double_click: config.reroute_on_edge_double_click,
            edge_insert_on_alt_drag: config.edge_insert_on_alt_drag,
            zoom_activation_key: config.zoom_activation_key,
            node_drag_threshold: config.node_drag_threshold,
            node_drag_handle_mode: config.node_drag_handle_mode,
            node_click_distance: config.node_click_distance,
            connection_drag_threshold: config.connection_drag_threshold,
            connect_on_click: config.connect_on_click,
            auto_pan: config.auto_pan.clone(),
            translate_extent: config.translate_extent,
            node_extent: config.node_extent,
            node_origin: config.node_origin,
        }
    }

    pub fn config(&self) -> NodeGraphInteractionConfig {
        NodeGraphInteractionConfig {
            elements_selectable: self.elements_selectable,
            nodes_draggable: self.nodes_draggable,
            nodes_connectable: self.nodes_connectable,
            nodes_deletable: self.nodes_deletable,
            edges_selectable: self.edges_selectable,
            edges_deletable: self.edges_deletable,
            edges_focusable: self.edges_focusable,
            edges_reconnectable: self.edges_reconnectable,
            connection_mode: self.connection_mode,
            connection_radius: self.connection_radius,
            reconnect_radius: self.reconnect_radius,
            reconnect_on_drop_empty: self.reconnect_on_drop_empty,
            edge_interaction_width: self.edge_interaction_width,
            bezier_hit_test_steps: self.bezier_hit_test_steps,
            elevate_nodes_on_select: self.elevate_nodes_on_select,
            elevate_edges_on_select: self.elevate_edges_on_select,
            snap_to_grid: self.snap_to_grid,
            snap_grid: self.snap_grid,
            snaplines: self.snaplines,
            snaplines_threshold: self.snaplines_threshold,
            pan_on_scroll: self.pan_on_scroll,
            pan_on_drag: self.pan_on_drag,
            selection_on_drag: self.selection_on_drag,
            selection_mode: self.selection_mode,
            box_select_edges: self.box_select_edges,
            selection_key: self.selection_key,
            multi_selection_key: self.multi_selection_key,
            delete_key: self.delete_key,
            nudge_step_mode: self.nudge_step_mode,
            nudge_step_px: self.nudge_step_px,
            nudge_fast_step_px: self.nudge_fast_step_px,
            disable_keyboard_a11y: self.disable_keyboard_a11y,
            pane_click_distance: self.pane_click_distance,
            pan_activation_key_code: self.pan_activation_key_code,
            space_to_pan: self.space_to_pan,
            pan_on_scroll_speed: self.pan_on_scroll_speed,
            pan_on_scroll_mode: self.pan_on_scroll_mode,
            pan_inertia: self.pan_inertia.clone(),
            zoom_on_scroll: self.zoom_on_scroll,
            zoom_on_scroll_speed: self.zoom_on_scroll_speed,
            zoom_on_pinch: self.zoom_on_pinch,
            zoom_on_pinch_speed: self.zoom_on_pinch_speed,
            zoom_on_double_click: self.zoom_on_double_click,
            frame_view_duration_ms: self.frame_view_duration_ms,
            frame_view_interpolate: self.frame_view_interpolate,
            frame_view_ease: self.frame_view_ease,
            frame_view_padding: self.frame_view_padding,
            reroute_on_edge_double_click: self.reroute_on_edge_double_click,
            edge_insert_on_alt_drag: self.edge_insert_on_alt_drag,
            zoom_activation_key: self.zoom_activation_key,
            node_drag_threshold: self.node_drag_threshold,
            node_drag_handle_mode: self.node_drag_handle_mode,
            node_click_distance: self.node_click_distance,
            connection_drag_threshold: self.connection_drag_threshold,
            connect_on_click: self.connect_on_click,
            auto_pan: self.auto_pan.clone(),
            translate_extent: self.translate_extent,
            node_extent: self.node_extent,
            node_origin: self.node_origin,
        }
    }

    pub fn runtime_tuning(&self) -> NodeGraphRuntimeTuning {
        NodeGraphRuntimeTuning {
            spatial_index: self.spatial_index,
            only_render_visible_elements: self.only_render_visible_elements,
            paint_cache_prune: self.paint_cache_prune,
        }
    }

    pub fn split(&self) -> (NodeGraphInteractionConfig, NodeGraphRuntimeTuning) {
        (self.config(), self.runtime_tuning())
    }
}

impl Default for NodeGraphInteractionState {
    fn default() -> Self {
        Self {
            elements_selectable: default_elements_selectable(),
            nodes_draggable: default_nodes_draggable(),
            nodes_connectable: default_nodes_connectable(),
            nodes_deletable: default_nodes_deletable(),
            edges_selectable: default_edges_selectable(),
            edges_deletable: default_edges_deletable(),
            edges_focusable: default_edges_focusable(),
            edges_reconnectable: default_edges_reconnectable(),
            connection_mode: NodeGraphConnectionMode::default(),
            connection_radius: default_connection_radius(),
            reconnect_radius: default_reconnect_radius(),
            reconnect_on_drop_empty: false,
            edge_interaction_width: default_edge_interaction_width(),
            bezier_hit_test_steps: default_bezier_hit_test_steps(),
            spatial_index: NodeGraphSpatialIndexTuning::default(),
            only_render_visible_elements: default_only_render_visible_elements(),
            elevate_nodes_on_select: default_elevate_nodes_on_select(),
            elevate_edges_on_select: default_elevate_edges_on_select(),
            paint_cache_prune: NodeGraphPaintCachePruneTuning::default(),
            snap_to_grid: false,
            snap_grid: default_snap_grid(),
            snaplines: default_snaplines(),
            snaplines_threshold: default_snaplines_threshold(),
            pan_on_scroll: default_pan_on_scroll(),
            pan_on_drag: default_pan_on_drag_buttons(),
            selection_on_drag: false,
            selection_mode: NodeGraphSelectionMode::default(),
            box_select_edges: default_box_select_edges(),
            selection_key: default_selection_key(),
            multi_selection_key: default_multi_selection_key(),
            delete_key: NodeGraphDeleteKey::default(),
            nudge_step_mode: NodeGraphNudgeStepMode::default(),
            nudge_step_px: default_nudge_step_px(),
            nudge_fast_step_px: default_nudge_fast_step_px(),
            disable_keyboard_a11y: false,
            pane_click_distance: default_pane_click_distance(),
            pan_activation_key_code: default_pan_activation_key_code(),
            space_to_pan: default_space_to_pan(),
            pan_on_scroll_speed: default_pan_on_scroll_speed(),
            pan_on_scroll_mode: NodeGraphPanOnScrollMode::default(),
            pan_inertia: NodeGraphPanInertiaTuning::default(),
            zoom_on_scroll: default_zoom_on_scroll(),
            zoom_on_scroll_speed: default_zoom_on_scroll_speed(),
            zoom_on_pinch: default_zoom_on_pinch(),
            zoom_on_pinch_speed: default_zoom_on_pinch_speed(),
            zoom_on_double_click: default_zoom_on_double_click(),
            frame_view_duration_ms: default_frame_view_duration_ms(),
            frame_view_interpolate: NodeGraphViewportInterpolate::default(),
            frame_view_ease: None,
            frame_view_padding: default_frame_view_padding(),
            reroute_on_edge_double_click: default_reroute_on_edge_double_click(),
            edge_insert_on_alt_drag: default_edge_insert_on_alt_drag(),
            zoom_activation_key: NodeGraphZoomActivationKey::default(),
            node_drag_threshold: default_node_drag_threshold(),
            node_drag_handle_mode: NodeGraphDragHandleMode::default(),
            node_click_distance: default_node_click_distance(),
            connection_drag_threshold: default_connection_drag_threshold(),
            connect_on_click: false,
            auto_pan: NodeGraphAutoPanTuning::default(),
            translate_extent: None,
            node_extent: None,
            node_origin: NodeGraphNodeOrigin::default(),
        }
    }
}

fn default_elevate_nodes_on_select() -> bool {
    false
}

fn default_elevate_edges_on_select() -> bool {
    true
}

fn default_nudge_step_px() -> f32 {
    1.0
}

fn default_nudge_fast_step_px() -> f32 {
    10.0
}

fn default_elements_selectable() -> bool {
    true
}

fn default_edges_selectable() -> bool {
    true
}

fn default_edges_focusable() -> bool {
    true
}

fn default_edges_reconnectable() -> bool {
    true
}

fn default_pan_on_scroll() -> bool {
    true
}

fn default_space_to_pan() -> bool {
    true
}

fn default_selection_key() -> NodeGraphModifierKey {
    NodeGraphModifierKey::Shift
}

fn default_multi_selection_key() -> NodeGraphModifierKey {
    NodeGraphModifierKey::CtrlOrMeta
}

fn default_pan_activation_key_code() -> Option<NodeGraphKeyCode> {
    Some(NodeGraphKeyCode(KeyCode::Space))
}

fn default_pane_click_distance() -> f32 {
    1.0
}

fn default_pan_on_scroll_speed() -> f32 {
    1.0
}

fn default_zoom_on_scroll() -> bool {
    true
}

fn default_zoom_on_scroll_speed() -> f32 {
    1.0
}

fn default_zoom_on_pinch() -> bool {
    true
}

fn default_zoom_on_pinch_speed() -> f32 {
    1.0
}

fn default_zoom_on_double_click() -> bool {
    true
}

fn default_frame_view_duration_ms() -> u32 {
    200
}

fn default_frame_view_padding() -> f32 {
    0.0
}

fn default_reroute_on_edge_double_click() -> bool {
    false
}

fn default_edge_insert_on_alt_drag() -> bool {
    false
}

fn default_connection_radius() -> f32 {
    16.0
}

fn default_reconnect_radius() -> f32 {
    10.0
}

fn default_edge_interaction_width() -> f32 {
    12.0
}

fn default_snap_grid() -> CanvasSize {
    CanvasSize {
        width: 16.0,
        height: 16.0,
    }
}

fn default_snaplines() -> bool {
    true
}

fn default_snaplines_threshold() -> f32 {
    8.0
}

fn default_node_drag_threshold() -> f32 {
    1.0
}

fn default_node_click_distance() -> f32 {
    2.0
}

fn default_connection_drag_threshold() -> f32 {
    2.0
}
