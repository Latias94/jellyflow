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
