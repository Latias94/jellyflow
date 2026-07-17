//! Renderer-neutral viewport pan and zoom helpers.
//!
//! Adapters own platform input and frame scheduling. The runtime only provides deterministic
//! canvas/screen transform math without depending on renderer, windowing, or gesture APIs.

mod transform;

pub use transform::{
    ViewportConstraints, ViewportPanRequest, ViewportTransform, ViewportZoomRequest,
    constrain_viewport, pan_viewport, zoom_viewport,
};
