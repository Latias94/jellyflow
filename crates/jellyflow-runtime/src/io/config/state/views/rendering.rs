use crate::io::tuning::{NodeGraphPaintCachePruneTuning, NodeGraphSpatialIndexTuning};

use super::super::NodeGraphInteractionState;

/// Rendering and cache tuning resolved for runtime use.
///
/// `spatial_index` selects the optional cached spatial query backend when enabled; renderer-facing
/// reads otherwise keep the deterministic linear path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeGraphRenderingInteraction {
    pub spatial_index: NodeGraphSpatialIndexTuning,
    pub only_render_visible_elements: bool,
    pub paint_cache_prune: NodeGraphPaintCachePruneTuning,
    pub elevate_nodes_on_select: bool,
    pub elevate_edges_on_select: bool,
}

impl NodeGraphInteractionState {
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
