use serde::{Deserialize, Serialize};

use super::{NodeGraphPaintCachePruneTuning, NodeGraphSpatialIndexTuning};

/// Persisted runtime-heavy tuning for the node graph editor.
///
/// Backend-specific payloads may be reserved before the current linear/headless implementation
/// consumes them. They are configuration compatibility commitments, not proof that a backend is
/// active.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NodeGraphRuntimeTuning {
    /// Reserved spatial-query backend tuning. Current visibility reads use deterministic scans.
    #[serde(default = "default_spatial_index_tuning")]
    pub spatial_index: NodeGraphSpatialIndexTuning,
    /// Cull renderer-facing visibility lists to the current viewport when possible.
    #[serde(default = "default_only_render_visible_elements")]
    pub only_render_visible_elements: bool,
    /// Reserved paint-cache pruning tuning for adapters or future runtime-owned cache plumbing.
    #[serde(default = "default_paint_cache_prune_tuning")]
    pub paint_cache_prune: NodeGraphPaintCachePruneTuning,
}

impl NodeGraphRuntimeTuning {
    pub fn is_default(this: &Self) -> bool {
        this == &Self::default()
    }

    pub fn with_spatial_index_enabled(mut self, enabled: bool) -> Self {
        self.spatial_index.enabled = enabled;
        self
    }

    pub fn with_only_render_visible_elements(mut self, enabled: bool) -> Self {
        self.only_render_visible_elements = enabled;
        self
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
