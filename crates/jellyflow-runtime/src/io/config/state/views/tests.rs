use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize};
use jellyflow_core::interaction::{
    NodeGraphConnectionMode, NodeGraphDragHandleMode, NodeGraphModifierKey,
};

use crate::io::config::keys::{NodeGraphDeleteKey, NodeGraphKeyCode};
use crate::io::config::types::{
    NodeGraphBoxSelectEdges, NodeGraphNodeOrigin, NodeGraphNudgeStepMode,
    NodeGraphPanOnDragButtons, NodeGraphPanOnScrollMode, NodeGraphSelectionMode,
    NodeGraphViewportEase,
};
use crate::io::tuning::{
    NodeGraphAutoPanTuning, NodeGraphPaintCachePruneTuning, NodeGraphPanInertiaTuning,
    NodeGraphSpatialIndexTuning,
};
use crate::runtime::geometry::EdgeHitTestOptions;

use super::super::NodeGraphInteractionState;

#[test]
fn connection_and_selection_views_group_related_fields() {
    let state = NodeGraphInteractionState {
        elements_selectable: false,
        nodes_connectable: false,
        nodes_deletable: false,
        nodes_focusable: false,
        edges_selectable: false,
        edges_deletable: false,
        edges_reconnectable: false,
        connection_mode: NodeGraphConnectionMode::Loose,
        connection_radius: 42.0,
        reconnect_radius: 13.0,
        reconnect_on_drop_empty: true,
        edge_interaction_width: 18.0,
        bezier_hit_test_steps: 32,
        selection_on_drag: true,
        select_nodes_on_drag: false,
        selection_mode: NodeGraphSelectionMode::Partial,
        box_select_edges: NodeGraphBoxSelectEdges::BothEndpoints,
        selection_key: NodeGraphModifierKey::Alt,
        multi_selection_key: NodeGraphModifierKey::Shift,
        reroute_on_edge_double_click: true,
        edge_insert_on_alt_drag: true,
        connection_drag_threshold: 7.0,
        connect_on_click: true,
        auto_pan: NodeGraphAutoPanTuning {
            on_connect: false,
            ..NodeGraphAutoPanTuning::default()
        },
        ..NodeGraphInteractionState::default()
    };

    let connection = state.connection_interaction();
    assert!(!connection.nodes_connectable);
    assert!(!connection.edges_reconnectable);
    assert_eq!(connection.connection_mode, NodeGraphConnectionMode::Loose);
    assert_eq!(connection.connection_radius, 42.0);
    assert_eq!(connection.reconnect_radius, 13.0);
    assert!(connection.reconnect_on_drop_empty);
    assert_eq!(connection.connection_drag_threshold, 7.0);
    assert!(connection.connect_on_click);
    assert!(connection.reroute_on_edge_double_click);
    assert!(connection.edge_insert_on_alt_drag);
    assert_eq!(connection.edge_hit_test, EdgeHitTestOptions::new(18.0, 32));
    assert!(!connection.auto_pan.on_connect);

    let selection = state.selection_interaction();
    assert!(!selection.elements_selectable);
    assert!(!selection.edges_selectable);
    assert!(selection.selection_on_drag);
    assert!(!selection.select_nodes_on_drag);
    assert_eq!(selection.selection_mode, NodeGraphSelectionMode::Partial);
    assert_eq!(
        selection.box_select_edges,
        NodeGraphBoxSelectEdges::BothEndpoints
    );
    assert_eq!(selection.selection_key, NodeGraphModifierKey::Alt);
    assert_eq!(selection.multi_selection_key, NodeGraphModifierKey::Shift);

    let delete = state.delete_interaction();
    assert!(!delete.nodes_deletable);
    assert!(!delete.edges_deletable);
    assert_eq!(delete.delete_key, NodeGraphDeleteKey::Backspace);
}

#[test]
fn viewport_drag_keyboard_and_rendering_views_group_related_fields() {
    let translate_extent = CanvasRect {
        origin: CanvasPoint { x: -10.0, y: -20.0 },
        size: CanvasSize {
            width: 500.0,
            height: 300.0,
        },
    };
    let node_extent = CanvasRect {
        origin: CanvasPoint { x: 0.0, y: 0.0 },
        size: CanvasSize {
            width: 400.0,
            height: 200.0,
        },
    };
    let state = NodeGraphInteractionState {
        pan_on_scroll: false,
        pan_on_drag: NodeGraphPanOnDragButtons {
            left: false,
            middle: true,
            right: true,
        },
        pane_click_distance: 4.0,
        pan_activation_key_code: Some(NodeGraphKeyCode(keyboard_types::Code::Space)),
        space_to_pan: false,
        pan_on_scroll_speed: 2.0,
        pan_on_scroll_mode: NodeGraphPanOnScrollMode::Horizontal,
        pan_inertia: NodeGraphPanInertiaTuning {
            enabled: true,
            ..NodeGraphPanInertiaTuning::default()
        },
        zoom_on_scroll: false,
        zoom_on_scroll_speed: 3.0,
        zoom_on_pinch: false,
        zoom_on_pinch_speed: 4.0,
        zoom_on_double_click: false,
        zoom_activation_key: NodeGraphModifierKey::Alt,
        frame_view_duration_ms: 123,
        frame_view_ease: NodeGraphViewportEase::CubicInOut,
        frame_view_padding: 0.25,
        nodes_draggable: false,
        snap_to_grid: true,
        snap_grid: CanvasSize {
            width: 10.0,
            height: 20.0,
        },
        snaplines: false,
        snaplines_threshold: 6.0,
        node_drag_threshold: 8.0,
        node_drag_handle_mode: NodeGraphDragHandleMode::Header,
        node_click_distance: 9.0,
        node_extent: Some(node_extent),
        node_origin: NodeGraphNodeOrigin { x: 0.5, y: 0.5 },
        nodes_focusable: false,
        edges_focusable: false,
        delete_key: NodeGraphDeleteKey::Delete,
        nudge_step_mode: NodeGraphNudgeStepMode::Grid,
        nudge_step_px: 12.0,
        nudge_fast_step_px: 120.0,
        disable_keyboard_a11y: true,
        spatial_index: NodeGraphSpatialIndexTuning {
            enabled: true,
            cell_size_screen_px: 300.0,
            min_cell_size_screen_px: 20.0,
            edge_aabb_pad_screen_px: 70.0,
        },
        only_render_visible_elements: false,
        paint_cache_prune: NodeGraphPaintCachePruneTuning {
            max_age_frames: 10,
            max_entries: 20,
        },
        elevate_nodes_on_select: true,
        elevate_edges_on_select: true,
        auto_pan: NodeGraphAutoPanTuning {
            on_node_drag: false,
            ..NodeGraphAutoPanTuning::default()
        },
        translate_extent: Some(translate_extent),
        ..NodeGraphInteractionState::default()
    };

    let pan = state.pan_interaction();
    assert!(!pan.pan_on_scroll);
    assert!(pan.pan_on_drag.right);
    assert_eq!(pan.pane_click_distance, 4.0);
    assert_eq!(
        pan.pan_activation_key_code,
        Some(NodeGraphKeyCode(keyboard_types::Code::Space))
    );
    assert!(!pan.space_to_pan);
    assert_eq!(pan.pan_on_scroll_speed, 2.0);
    assert_eq!(pan.pan_on_scroll_mode, NodeGraphPanOnScrollMode::Horizontal);
    assert!(pan.pan_inertia.enabled);
    assert_eq!(pan.translate_extent, Some(translate_extent));

    let zoom = state.zoom_interaction();
    assert!(!zoom.zoom_on_scroll);
    assert_eq!(zoom.zoom_on_scroll_speed, 3.0);
    assert!(!zoom.zoom_on_pinch);
    assert_eq!(zoom.zoom_on_pinch_speed, 4.0);
    assert!(!zoom.zoom_on_double_click);
    assert_eq!(zoom.zoom_activation_key, NodeGraphModifierKey::Alt);

    let frame = state.frame_view_interaction();
    assert_eq!(frame.duration_ms, 123);
    assert_eq!(frame.ease, NodeGraphViewportEase::CubicInOut);
    assert_eq!(frame.padding, 0.25);

    let node_drag = state.node_drag_interaction();
    assert!(!node_drag.nodes_draggable);
    assert!(node_drag.snap_to_grid);
    assert_eq!(node_drag.snap_grid.width, 10.0);
    assert!(!node_drag.snaplines);
    assert_eq!(node_drag.snaplines_threshold, 6.0);
    assert_eq!(node_drag.node_drag_threshold, 8.0);
    assert_eq!(
        node_drag.node_drag_handle_mode,
        NodeGraphDragHandleMode::Header
    );
    assert_eq!(node_drag.node_click_distance, 9.0);
    assert_eq!(node_drag.node_extent, Some(node_extent));
    assert_eq!(
        node_drag.node_origin,
        NodeGraphNodeOrigin { x: 0.5, y: 0.5 }
    );
    assert!(!node_drag.auto_pan.on_node_drag);

    let keyboard = state.keyboard_interaction();
    assert!(!keyboard.nodes_focusable);
    assert!(!keyboard.edges_focusable);
    assert_eq!(keyboard.delete_key, NodeGraphDeleteKey::Delete);
    assert_eq!(keyboard.nudge_step_mode, NodeGraphNudgeStepMode::Grid);
    assert_eq!(keyboard.nudge_step_px, 12.0);
    assert_eq!(keyboard.nudge_fast_step_px, 120.0);
    assert!(keyboard.disable_keyboard_a11y);

    let rendering = state.rendering_interaction();
    assert!(rendering.spatial_index.enabled);
    assert_eq!(rendering.spatial_index.cell_size_screen_px, 300.0);
    assert!(!rendering.only_render_visible_elements);
    assert_eq!(rendering.paint_cache_prune.max_entries, 20);
    assert!(rendering.elevate_nodes_on_select);
    assert!(rendering.elevate_edges_on_select);
}
