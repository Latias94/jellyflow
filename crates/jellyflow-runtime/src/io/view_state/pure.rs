use serde::{Deserialize, Serialize};

use jellyflow_core::core::{CanvasPoint, EdgeId, GroupId, NodeId};

use super::default_zoom;

/// Pure persisted view-state payload.
///
/// This excludes interaction policy and runtime tuning so persistence boundaries can evolve without
/// forcing every in-memory/runtime consumer to change in the same step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeGraphPureViewState {
    #[serde(default)]
    pub pan: CanvasPoint,
    #[serde(default = "default_zoom")]
    pub zoom: f32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub selected_nodes: Vec<NodeId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub selected_edges: Vec<EdgeId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub selected_groups: Vec<GroupId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub draw_order: Vec<NodeId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edge_draw_order: Vec<EdgeId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub group_draw_order: Vec<GroupId>,
}

impl Default for NodeGraphPureViewState {
    fn default() -> Self {
        Self {
            pan: CanvasPoint::default(),
            zoom: default_zoom(),
            selected_nodes: Vec::new(),
            selected_edges: Vec::new(),
            selected_groups: Vec::new(),
            draw_order: Vec::new(),
            edge_draw_order: Vec::new(),
            group_draw_order: Vec::new(),
        }
    }
}
