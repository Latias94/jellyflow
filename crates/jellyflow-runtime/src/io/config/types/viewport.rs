use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeGraphViewportInterpolate {
    Linear,
    #[default]
    Smooth,
}

/// Easing curve for animated viewport changes (XyFlow `fitViewOptions.ease`).
///
/// Note: this is an optional override. When unset, the legacy behavior is derived from
/// `frame_view_interpolate` for backward compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeGraphViewportEase {
    Linear,
    /// Smoothstep `t*t*(3-2*t)` (close to common editor defaults).
    Smoothstep,
    /// Cubic ease-in-out.
    CubicInOut,
}
