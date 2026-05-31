use serde::{Deserialize, Serialize};

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
