use jellyflow_core::interaction::{
    NodeGraphConnectionMode, NodeGraphDragHandleMode, NodeGraphModifierKey,
};

use crate::io::tuning::{
    NodeGraphAutoPanTuning, NodeGraphPanInertiaTuning, NodeGraphRuntimeTuning,
};

use crate::io::config::defaults::*;
use crate::io::config::keys::NodeGraphDeleteKey;
use crate::io::config::types::{
    NodeGraphNodeOrigin, NodeGraphNudgeStepMode, NodeGraphPanOnScrollMode, NodeGraphSelectionMode,
    NodeGraphViewportEase, default_box_select_edges, default_pan_on_drag_buttons,
};

use super::NodeGraphInteractionState;

impl Default for NodeGraphInteractionState {
    fn default() -> Self {
        let runtime_tuning = NodeGraphRuntimeTuning::default();
        Self {
            elements_selectable: default_elements_selectable(),
            nodes_draggable: default_nodes_draggable(),
            nodes_connectable: default_nodes_connectable(),
            nodes_deletable: default_nodes_deletable(),
            nodes_focusable: default_nodes_focusable(),
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
            spatial_index: runtime_tuning.spatial_index,
            only_render_visible_elements: runtime_tuning.only_render_visible_elements,
            elevate_nodes_on_select: default_elevate_nodes_on_select(),
            elevate_edges_on_select: default_elevate_edges_on_select(),
            paint_cache_prune: runtime_tuning.paint_cache_prune,
            snap_to_grid: false,
            snap_grid: default_snap_grid(),
            snaplines: default_snaplines(),
            snaplines_threshold: default_snaplines_threshold(),
            pan_on_scroll: default_pan_on_scroll(),
            pan_on_drag: default_pan_on_drag_buttons(),
            selection_on_drag: false,
            select_nodes_on_drag: default_select_nodes_on_drag(),
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
            frame_view_ease: NodeGraphViewportEase::default(),
            frame_view_padding: default_frame_view_padding(),
            reroute_on_edge_double_click: default_reroute_on_edge_double_click(),
            edge_insert_on_alt_drag: default_edge_insert_on_alt_drag(),
            zoom_activation_key: NodeGraphModifierKey::default(),
            node_drag_threshold: default_node_drag_threshold(),
            node_drag_handle_mode: NodeGraphDragHandleMode::default(),
            node_click_distance: default_node_click_distance(),
            connection_drag_threshold: default_connection_drag_threshold(),
            connect_on_click: default_connect_on_click(),
            auto_pan: NodeGraphAutoPanTuning::default(),
            translate_extent: None,
            node_extent: None,
            node_origin: NodeGraphNodeOrigin::default(),
        }
    }
}
