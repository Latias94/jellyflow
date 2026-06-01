use serde::{Deserialize, Serialize};

/// Easing curve for animated viewport changes (XyFlow `fitViewOptions.ease`).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeGraphViewportEase {
    Linear,
    /// Smoothstep `t*t*(3-2*t)` (close to common editor defaults).
    #[default]
    Smoothstep,
    /// Cubic ease-in-out.
    CubicInOut,
}
