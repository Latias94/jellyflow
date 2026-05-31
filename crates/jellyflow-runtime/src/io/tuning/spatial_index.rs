use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NodeGraphSpatialIndexTuning {
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
            cell_size_screen_px: Self::default_cell_size_screen_px(),
            min_cell_size_screen_px: Self::default_min_cell_size_screen_px(),
            edge_aabb_pad_screen_px: Self::default_edge_aabb_pad_screen_px(),
        }
    }
}
