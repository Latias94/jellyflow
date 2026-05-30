//! Runtime-heavy tuning for headless editor adapters.

use serde::{Deserialize, Serialize};

/// Auto-pan tuning for drag/connect/focus workflows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeGraphAutoPanTuning {
    #[serde(default)]
    pub on_node_drag: bool,
    #[serde(default)]
    pub on_connect: bool,
    #[serde(default)]
    pub on_node_focus: bool,

    /// Speed in screen pixels per second (approximate).
    #[serde(default = "default_auto_pan_speed")]
    pub speed: f32,

    /// Margin from viewport edge in screen pixels that triggers auto-pan.
    #[serde(default = "default_auto_pan_margin")]
    pub margin: f32,
}

fn default_auto_pan_speed() -> f32 {
    900.0
}

fn default_auto_pan_margin() -> f32 {
    24.0
}

impl Default for NodeGraphAutoPanTuning {
    fn default() -> Self {
        Self {
            on_node_drag: true,
            on_connect: true,
            on_node_focus: false,
            speed: default_auto_pan_speed(),
            margin: default_auto_pan_margin(),
        }
    }
}

/// Momentum configuration for canvas panning.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeGraphPanInertiaTuning {
    /// Enables inertial panning after releasing the pan gesture.
    #[serde(default)]
    pub enabled: bool,

    /// Exponential damping factor applied to velocity (1 / seconds).
    #[serde(default = "default_pan_inertia_decay_per_s")]
    pub decay_per_s: f32,

    /// Minimum screen speed (px/s) required to keep inertia running.
    #[serde(default = "default_pan_inertia_min_speed")]
    pub min_speed: f32,

    /// Maximum screen speed (px/s) at inertia start (clamp).
    #[serde(default = "default_pan_inertia_max_speed")]
    pub max_speed: f32,
}

fn default_pan_inertia_decay_per_s() -> f32 {
    14.0
}

fn default_pan_inertia_min_speed() -> f32 {
    36.0
}

fn default_pan_inertia_max_speed() -> f32 {
    8000.0
}

impl Default for NodeGraphPanInertiaTuning {
    fn default() -> Self {
        Self {
            enabled: false,
            decay_per_s: default_pan_inertia_decay_per_s(),
            min_speed: default_pan_inertia_min_speed(),
            max_speed: default_pan_inertia_max_speed(),
        }
    }
}

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

pub(crate) fn default_only_render_visible_elements() -> bool {
    true
}
