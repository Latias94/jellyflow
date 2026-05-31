use keyboard_types::Code as KeyCode;

use jellyflow_core::core::CanvasSize;
use jellyflow_core::interaction::NodeGraphModifierKey;

use super::keys::NodeGraphKeyCode;

pub(super) fn default_nodes_draggable() -> bool {
    true
}

pub(super) fn default_nodes_connectable() -> bool {
    true
}

pub(super) fn default_nodes_deletable() -> bool {
    true
}

pub(super) fn default_edges_deletable() -> bool {
    true
}

pub(super) fn default_bezier_hit_test_steps() -> u8 {
    24
}

pub(super) fn default_elevate_nodes_on_select() -> bool {
    false
}

pub(super) fn default_elevate_edges_on_select() -> bool {
    true
}

pub(super) fn default_nudge_step_px() -> f32 {
    1.0
}

pub(super) fn default_nudge_fast_step_px() -> f32 {
    10.0
}

pub(super) fn default_elements_selectable() -> bool {
    true
}

pub(super) fn default_edges_selectable() -> bool {
    true
}

pub(super) fn default_edges_focusable() -> bool {
    true
}

pub(super) fn default_edges_reconnectable() -> bool {
    true
}

pub(super) fn default_pan_on_scroll() -> bool {
    true
}

pub(super) fn default_space_to_pan() -> bool {
    true
}

pub(super) fn default_selection_key() -> NodeGraphModifierKey {
    NodeGraphModifierKey::Shift
}

pub(super) fn default_multi_selection_key() -> NodeGraphModifierKey {
    NodeGraphModifierKey::CtrlOrMeta
}

pub(super) fn default_pan_activation_key_code() -> Option<NodeGraphKeyCode> {
    Some(NodeGraphKeyCode(KeyCode::Space))
}

pub(super) fn default_pane_click_distance() -> f32 {
    1.0
}

pub(super) fn default_pan_on_scroll_speed() -> f32 {
    1.0
}

pub(super) fn default_zoom_on_scroll() -> bool {
    true
}

pub(super) fn default_zoom_on_scroll_speed() -> f32 {
    1.0
}

pub(super) fn default_zoom_on_pinch() -> bool {
    true
}

pub(super) fn default_zoom_on_pinch_speed() -> f32 {
    1.0
}

pub(super) fn default_zoom_on_double_click() -> bool {
    true
}

pub(super) fn default_frame_view_duration_ms() -> u32 {
    200
}

pub(super) fn default_frame_view_padding() -> f32 {
    0.0
}

pub(super) fn default_reroute_on_edge_double_click() -> bool {
    false
}

pub(super) fn default_edge_insert_on_alt_drag() -> bool {
    false
}

pub(super) fn default_connection_radius() -> f32 {
    16.0
}

pub(super) fn default_reconnect_radius() -> f32 {
    10.0
}

pub(super) fn default_edge_interaction_width() -> f32 {
    12.0
}

pub(super) fn default_snap_grid() -> CanvasSize {
    CanvasSize {
        width: 16.0,
        height: 16.0,
    }
}

pub(super) fn default_snaplines() -> bool {
    true
}

pub(super) fn default_snaplines_threshold() -> f32 {
    8.0
}

pub(super) fn default_node_drag_threshold() -> f32 {
    1.0
}

pub(super) fn default_node_click_distance() -> f32 {
    2.0
}

pub(super) fn default_connection_drag_threshold() -> f32 {
    2.0
}
