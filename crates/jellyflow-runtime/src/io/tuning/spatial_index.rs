use serde::{Deserialize, Serialize};

/// Tuning for the optional indexed spatial-query backend.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NodeGraphSpatialIndexTuning {
    /// Enables the spatial backend for store-level query reads.
    #[serde(default)]
    pub enabled: bool,
    /// Preferred cell size in screen pixels (converted to canvas units by dividing by zoom).
    #[serde(default = "NodeGraphSpatialIndexTuning::default_cell_size_screen_px")]
    pub cell_size_screen_px: f32,
    /// Minimum cell size in screen pixels (converted to canvas units by dividing by zoom).
    #[serde(default = "NodeGraphSpatialIndexTuning::default_min_cell_size_screen_px")]
    pub min_cell_size_screen_px: f32,
    /// Extra padding (screen px) applied to edge wire AABBs to ensure stable hit-test candidate sets.
    #[serde(default = "NodeGraphSpatialIndexTuning::default_edge_aabb_pad_screen_px")]
    pub edge_aabb_pad_screen_px: f32,
}

impl NodeGraphSpatialIndexTuning {
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn with_cell_size_screen_px(mut self, cell_size_screen_px: f32) -> Self {
        self.cell_size_screen_px = cell_size_screen_px;
        self
    }

    pub fn with_min_cell_size_screen_px(mut self, min_cell_size_screen_px: f32) -> Self {
        self.min_cell_size_screen_px = min_cell_size_screen_px;
        self
    }

    pub fn with_edge_aabb_pad_screen_px(mut self, edge_aabb_pad_screen_px: f32) -> Self {
        self.edge_aabb_pad_screen_px = edge_aabb_pad_screen_px;
        self
    }

    fn default_cell_size_screen_px() -> f32 {
        256.0
    }

    fn default_min_cell_size_screen_px() -> f32 {
        16.0
    }

    fn default_edge_aabb_pad_screen_px() -> f32 {
        96.0
    }
}

impl Default for NodeGraphSpatialIndexTuning {
    fn default() -> Self {
        Self {
            enabled: false,
            cell_size_screen_px: Self::default_cell_size_screen_px(),
            min_cell_size_screen_px: Self::default_min_cell_size_screen_px(),
            edge_aabb_pad_screen_px: Self::default_edge_aabb_pad_screen_px(),
        }
    }
}
