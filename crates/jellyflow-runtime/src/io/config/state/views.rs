use jellyflow_core::core::{CanvasRect, CanvasSize};
use jellyflow_core::interaction::{
    NodeGraphConnectionMode, NodeGraphDragHandleMode, NodeGraphModifierKey,
};

use crate::io::config::keys::{NodeGraphDeleteKey, NodeGraphKeyCode};
use crate::io::config::types::{
    NodeGraphBoxSelectEdges, NodeGraphNodeOrigin, NodeGraphNudgeStepMode,
    NodeGraphPanOnDragButtons, NodeGraphPanOnScrollMode, NodeGraphSelectionMode,
    NodeGraphViewportEase, NodeGraphViewportInterpolate,
};
use crate::io::tuning::{
    NodeGraphAutoPanTuning, NodeGraphPaintCachePruneTuning, NodeGraphPanInertiaTuning,
    NodeGraphSpatialIndexTuning,
};
use crate::runtime::geometry::EdgeHitTestOptions;

use super::NodeGraphInteractionState;

/// Connection gesture and edge interaction settings resolved for runtime use.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeGraphConnectionInteraction<'a> {
    pub nodes_connectable: bool,
    pub edges_reconnectable: bool,
    pub connection_mode: NodeGraphConnectionMode,
    pub connection_radius: f32,
    pub reconnect_radius: f32,
    pub reconnect_on_drop_empty: bool,
    pub connection_drag_threshold: f32,
    pub connect_on_click: bool,
    pub reroute_on_edge_double_click: bool,
    pub edge_insert_on_alt_drag: bool,
    pub edge_hit_test: EdgeHitTestOptions,
    pub auto_pan: &'a NodeGraphAutoPanTuning,
}

/// Selection behaviour resolved for runtime use.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeGraphSelectionInteraction {
    pub elements_selectable: bool,
    pub edges_selectable: bool,
    pub selection_on_drag: bool,
    pub selection_mode: NodeGraphSelectionMode,
    pub box_select_edges: NodeGraphBoxSelectEdges,
    pub selection_key: NodeGraphModifierKey,
    pub multi_selection_key: NodeGraphModifierKey,
}

/// Delete policy and keyboard binding resolved for runtime use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeGraphDeleteInteraction {
    pub nodes_deletable: bool,
    pub edges_deletable: bool,
    pub delete_key: NodeGraphDeleteKey,
}

/// Canvas pan behaviour resolved for runtime use.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeGraphPanInteraction<'a> {
    pub pan_on_scroll: bool,
    pub pan_on_drag: NodeGraphPanOnDragButtons,
    pub pane_click_distance: f32,
    pub pan_activation_key_code: Option<NodeGraphKeyCode>,
    pub space_to_pan: bool,
    pub pan_on_scroll_speed: f32,
    pub pan_on_scroll_mode: NodeGraphPanOnScrollMode,
    pub pan_inertia: &'a NodeGraphPanInertiaTuning,
    pub translate_extent: Option<CanvasRect>,
}

/// Viewport zoom behaviour resolved for runtime use.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeGraphZoomInteraction {
    pub zoom_on_scroll: bool,
    pub zoom_on_scroll_speed: f32,
    pub zoom_on_pinch: bool,
    pub zoom_on_pinch_speed: f32,
    pub zoom_on_double_click: bool,
    pub zoom_activation_key: NodeGraphModifierKey,
}

/// Programmatic viewport framing behaviour resolved for runtime use.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeGraphFrameViewInteraction {
    pub duration_ms: u32,
    pub interpolate: NodeGraphViewportInterpolate,
    pub ease: Option<NodeGraphViewportEase>,
    pub padding: f32,
}

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

/// Keyboard accessibility and nudge settings resolved for runtime use.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeGraphKeyboardInteraction {
    pub delete_key: NodeGraphDeleteKey,
    pub nudge_step_mode: NodeGraphNudgeStepMode,
    pub nudge_step_px: f32,
    pub nudge_fast_step_px: f32,
    pub disable_keyboard_a11y: bool,
}

/// Rendering and spatial-query tuning resolved for runtime use.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeGraphRenderingInteraction {
    pub spatial_index: NodeGraphSpatialIndexTuning,
    pub only_render_visible_elements: bool,
    pub paint_cache_prune: NodeGraphPaintCachePruneTuning,
    pub elevate_nodes_on_select: bool,
    pub elevate_edges_on_select: bool,
}

impl NodeGraphInteractionState {
    pub fn connection_interaction(&self) -> NodeGraphConnectionInteraction<'_> {
        NodeGraphConnectionInteraction {
            nodes_connectable: self.nodes_connectable,
            edges_reconnectable: self.edges_reconnectable,
            connection_mode: self.connection_mode,
            connection_radius: self.connection_radius,
            reconnect_radius: self.reconnect_radius,
            reconnect_on_drop_empty: self.reconnect_on_drop_empty,
            connection_drag_threshold: self.connection_drag_threshold,
            connect_on_click: self.connect_on_click,
            reroute_on_edge_double_click: self.reroute_on_edge_double_click,
            edge_insert_on_alt_drag: self.edge_insert_on_alt_drag,
            edge_hit_test: self.edge_hit_test_options(),
            auto_pan: &self.auto_pan,
        }
    }

    pub fn selection_interaction(&self) -> NodeGraphSelectionInteraction {
        NodeGraphSelectionInteraction {
            elements_selectable: self.elements_selectable,
            edges_selectable: self.edges_selectable,
            selection_on_drag: self.selection_on_drag,
            selection_mode: self.selection_mode,
            box_select_edges: self.box_select_edges,
            selection_key: self.selection_key,
            multi_selection_key: self.multi_selection_key,
        }
    }

    pub fn delete_interaction(&self) -> NodeGraphDeleteInteraction {
        NodeGraphDeleteInteraction {
            nodes_deletable: self.nodes_deletable,
            edges_deletable: self.edges_deletable,
            delete_key: self.delete_key,
        }
    }

    pub fn pan_interaction(&self) -> NodeGraphPanInteraction<'_> {
        NodeGraphPanInteraction {
            pan_on_scroll: self.pan_on_scroll,
            pan_on_drag: self.pan_on_drag,
            pane_click_distance: self.pane_click_distance,
            pan_activation_key_code: self.pan_activation_key_code,
            space_to_pan: self.space_to_pan,
            pan_on_scroll_speed: self.pan_on_scroll_speed,
            pan_on_scroll_mode: self.pan_on_scroll_mode,
            pan_inertia: &self.pan_inertia,
            translate_extent: self.translate_extent,
        }
    }

    pub fn zoom_interaction(&self) -> NodeGraphZoomInteraction {
        NodeGraphZoomInteraction {
            zoom_on_scroll: self.zoom_on_scroll,
            zoom_on_scroll_speed: self.zoom_on_scroll_speed,
            zoom_on_pinch: self.zoom_on_pinch,
            zoom_on_pinch_speed: self.zoom_on_pinch_speed,
            zoom_on_double_click: self.zoom_on_double_click,
            zoom_activation_key: self.zoom_activation_key,
        }
    }

    pub fn frame_view_interaction(&self) -> NodeGraphFrameViewInteraction {
        NodeGraphFrameViewInteraction {
            duration_ms: self.frame_view_duration_ms,
            interpolate: self.frame_view_interpolate,
            ease: self.frame_view_ease,
            padding: self.frame_view_padding,
        }
    }

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

    pub fn keyboard_interaction(&self) -> NodeGraphKeyboardInteraction {
        NodeGraphKeyboardInteraction {
            delete_key: self.delete_key,
            nudge_step_mode: self.nudge_step_mode,
            nudge_step_px: self.nudge_step_px,
            nudge_fast_step_px: self.nudge_fast_step_px,
            disable_keyboard_a11y: self.disable_keyboard_a11y,
        }
    }

    pub fn rendering_interaction(&self) -> NodeGraphRenderingInteraction {
        NodeGraphRenderingInteraction {
            spatial_index: self.spatial_index,
            only_render_visible_elements: self.only_render_visible_elements,
            paint_cache_prune: self.paint_cache_prune,
            elevate_nodes_on_select: self.elevate_nodes_on_select,
            elevate_edges_on_select: self.elevate_edges_on_select,
        }
    }
}

#[cfg(test)]
mod tests {
    use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize};
    use jellyflow_core::interaction::{
        NodeGraphConnectionMode, NodeGraphDragHandleMode, NodeGraphModifierKey,
    };

    use crate::io::config::keys::{NodeGraphDeleteKey, NodeGraphKeyCode};
    use crate::io::config::types::{
        NodeGraphBoxSelectEdges, NodeGraphNodeOrigin, NodeGraphNudgeStepMode,
        NodeGraphPanOnDragButtons, NodeGraphPanOnScrollMode, NodeGraphSelectionMode,
        NodeGraphViewportEase, NodeGraphViewportInterpolate,
    };
    use crate::io::tuning::{
        NodeGraphAutoPanTuning, NodeGraphPaintCachePruneTuning, NodeGraphPanInertiaTuning,
        NodeGraphSpatialIndexTuning,
    };
    use crate::runtime::geometry::EdgeHitTestOptions;

    use super::NodeGraphInteractionState;

    #[test]
    fn connection_and_selection_views_group_related_fields() {
        let state = NodeGraphInteractionState {
            elements_selectable: false,
            nodes_connectable: false,
            nodes_deletable: false,
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
            frame_view_interpolate: NodeGraphViewportInterpolate::Linear,
            frame_view_ease: Some(NodeGraphViewportEase::CubicInOut),
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
            delete_key: NodeGraphDeleteKey::Delete,
            nudge_step_mode: NodeGraphNudgeStepMode::Grid,
            nudge_step_px: 12.0,
            nudge_fast_step_px: 120.0,
            disable_keyboard_a11y: true,
            spatial_index: NodeGraphSpatialIndexTuning {
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
            elevate_edges_on_select: false,
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
        assert_eq!(frame.interpolate, NodeGraphViewportInterpolate::Linear);
        assert_eq!(frame.ease, Some(NodeGraphViewportEase::CubicInOut));
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
        assert_eq!(keyboard.delete_key, NodeGraphDeleteKey::Delete);
        assert_eq!(keyboard.nudge_step_mode, NodeGraphNudgeStepMode::Grid);
        assert_eq!(keyboard.nudge_step_px, 12.0);
        assert_eq!(keyboard.nudge_fast_step_px, 120.0);
        assert!(keyboard.disable_keyboard_a11y);

        let rendering = state.rendering_interaction();
        assert_eq!(rendering.spatial_index.cell_size_screen_px, 300.0);
        assert!(!rendering.only_render_visible_elements);
        assert_eq!(rendering.paint_cache_prune.max_entries, 20);
        assert!(rendering.elevate_nodes_on_select);
        assert!(!rendering.elevate_edges_on_select);
    }
}
