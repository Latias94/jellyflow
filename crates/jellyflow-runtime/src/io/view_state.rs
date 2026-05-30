//! Persisted and in-memory editor view state.

use serde::{Deserialize, Serialize};

use jellyflow_core::core::{EdgeId, Graph, GroupId, NodeId};

/// Pure persisted view-state payload.
///
/// This excludes interaction policy and runtime tuning so persistence boundaries can evolve without
/// forcing every in-memory/runtime consumer to change in the same step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeGraphPureViewState {
    #[serde(default)]
    pub pan: jellyflow_core::core::CanvasPoint,
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
    pub group_draw_order: Vec<GroupId>,
}

impl Default for NodeGraphPureViewState {
    fn default() -> Self {
        Self {
            pan: jellyflow_core::core::CanvasPoint::default(),
            zoom: default_zoom(),
            selected_nodes: Vec::new(),
            selected_edges: Vec::new(),
            selected_groups: Vec::new(),
            draw_order: Vec::new(),
            group_draw_order: Vec::new(),
        }
    }
}

/// Node graph editor view-state.
///
/// This is intentionally separate from graph semantics and may be stored per-user/per-project.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeGraphViewState {
    /// Canvas pan in graph space.
    #[serde(default)]
    pub pan: jellyflow_core::core::CanvasPoint,
    /// Zoom factor.
    #[serde(default = "default_zoom")]
    pub zoom: f32,
    /// Selected nodes (optional).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub selected_nodes: Vec<NodeId>,
    /// Selected edges (optional).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub selected_edges: Vec<EdgeId>,
    /// Selected groups (optional).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub selected_groups: Vec<GroupId>,
    /// Explicit draw order (optional).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub draw_order: Vec<NodeId>,
    /// Explicit group draw order (optional).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub group_draw_order: Vec<GroupId>,
}

impl Default for NodeGraphViewState {
    fn default() -> Self {
        Self {
            pan: jellyflow_core::core::CanvasPoint::default(),
            zoom: default_zoom(),
            selected_nodes: Vec::new(),
            selected_edges: Vec::new(),
            selected_groups: Vec::new(),
            draw_order: Vec::new(),
            group_draw_order: Vec::new(),
        }
    }
}

impl NodeGraphViewState {
    /// Removes stale IDs (selection / draw order) that no longer exist in the target graph.
    pub fn sanitize_for_graph(&mut self, graph: &Graph) {
        let visible_node = |id: &NodeId| graph.nodes.get(id).is_some_and(|n| !n.hidden);

        self.selected_nodes.retain(visible_node);
        self.selected_edges.retain(|id| {
            let Some(edge) = graph.edges.get(id) else {
                return false;
            };
            let Some(from) = graph.ports.get(&edge.from) else {
                return false;
            };
            let Some(to) = graph.ports.get(&edge.to) else {
                return false;
            };
            visible_node(&from.node) && visible_node(&to.node)
        });
        self.selected_groups
            .retain(|id| graph.groups.contains_key(id));
        self.draw_order.retain(visible_node);
        self.group_draw_order
            .retain(|id| graph.groups.contains_key(id));
    }
}

impl From<NodeGraphPureViewState> for NodeGraphViewState {
    fn from(value: NodeGraphPureViewState) -> Self {
        Self {
            pan: value.pan,
            zoom: value.zoom,
            selected_nodes: value.selected_nodes,
            selected_edges: value.selected_edges,
            selected_groups: value.selected_groups,
            draw_order: value.draw_order,
            group_draw_order: value.group_draw_order,
        }
    }
}

impl From<NodeGraphViewState> for NodeGraphPureViewState {
    fn from(value: NodeGraphViewState) -> Self {
        Self {
            pan: value.pan,
            zoom: value.zoom,
            selected_nodes: value.selected_nodes,
            selected_edges: value.selected_edges,
            selected_groups: value.selected_groups,
            draw_order: value.draw_order,
            group_draw_order: value.group_draw_order,
        }
    }
}

impl From<&NodeGraphViewState> for NodeGraphPureViewState {
    fn from(value: &NodeGraphViewState) -> Self {
        Self {
            pan: value.pan,
            zoom: value.zoom,
            selected_nodes: value.selected_nodes.clone(),
            selected_edges: value.selected_edges.clone(),
            selected_groups: value.selected_groups.clone(),
            draw_order: value.draw_order.clone(),
            group_draw_order: value.group_draw_order.clone(),
        }
    }
}

fn default_zoom() -> f32 {
    1.0
}
