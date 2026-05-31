use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NodeGraphPaintCachePruneTuning {
    /// Remove cache entries not used within this many frames.
    #[serde(default = "NodeGraphPaintCachePruneTuning::default_max_age_frames")]
    pub max_age_frames: u64,
    /// Hard cap on total cache entries (paths + markers + text blobs + text metrics).
    #[serde(default = "NodeGraphPaintCachePruneTuning::default_max_entries")]
    pub max_entries: usize,
}

impl NodeGraphPaintCachePruneTuning {
    fn default_max_age_frames() -> u64 {
        300
    }

    fn default_max_entries() -> usize {
        30_000
    }
}

impl Default for NodeGraphPaintCachePruneTuning {
    fn default() -> Self {
        Self {
            max_age_frames: Self::default_max_age_frames(),
            max_entries: Self::default_max_entries(),
        }
    }
}
