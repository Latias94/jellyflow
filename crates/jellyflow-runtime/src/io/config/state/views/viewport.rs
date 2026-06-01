use jellyflow_core::core::CanvasRect;
use jellyflow_core::interaction::NodeGraphModifierKey;

use crate::io::config::keys::NodeGraphKeyCode;
use crate::io::config::types::{
    NodeGraphPanOnDragButtons, NodeGraphPanOnScrollMode, NodeGraphViewportEase,
};
use crate::io::tuning::NodeGraphPanInertiaTuning;

use super::super::NodeGraphInteractionState;

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
    pub ease: NodeGraphViewportEase,
    pub padding: f32,
}

impl NodeGraphInteractionState {
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
            ease: self.frame_view_ease,
            padding: self.frame_view_padding,
        }
    }
}
