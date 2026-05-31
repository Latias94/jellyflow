use serde::{Deserialize, Serialize};

use super::{NodeGraphPaintCachePruneTuning, NodeGraphSpatialIndexTuning};

/// Persisted runtime-heavy tuning for the node graph editor.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NodeGraphRuntimeTuning {
    #[serde(default = "default_spatial_index_tuning")]
    pub spatial_index: NodeGraphSpatialIndexTuning,
    #[serde(default = "default_only_render_visible_elements")]
    pub only_render_visible_elements: bool,
    #[serde(default = "default_paint_cache_prune_tuning")]
    pub paint_cache_prune: NodeGraphPaintCachePruneTuning,
}

impl NodeGraphRuntimeTuning {
    pub fn is_default(this: &Self) -> bool {
        this == &Self::default()
    }
}

impl Default for NodeGraphRuntimeTuning {
    fn default() -> Self {
        Self {
            spatial_index: default_spatial_index_tuning(),
            only_render_visible_elements: default_only_render_visible_elements(),
            paint_cache_prune: default_paint_cache_prune_tuning(),
        }
    }
}

fn default_spatial_index_tuning() -> NodeGraphSpatialIndexTuning {
    NodeGraphSpatialIndexTuning::default()
}

fn default_paint_cache_prune_tuning() -> NodeGraphPaintCachePruneTuning {
    NodeGraphPaintCachePruneTuning::default()
}

pub(crate) fn default_only_render_visible_elements() -> bool {
    true
}
