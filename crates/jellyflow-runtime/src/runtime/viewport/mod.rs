//! Renderer-neutral viewport pan and zoom helpers.
//!
//! Adapters normalize platform input into these request types. The runtime owns deterministic
//! canvas/screen transform math and gesture policy without depending on renderer, windowing, or
//! gesture APIs.

mod animation;
mod gesture;
mod inertia;
mod transform;

pub use animation::{
    ViewportAnimationEasing, ViewportAnimationFrame, ViewportAnimationOptions,
    ViewportAnimationPlan, ViewportAnimationRequest, plan_viewport_animation,
    plan_viewport_animation_with_options,
};
pub use gesture::{
    PaneClickDistanceInput, ViewportDoubleClickZoomInput, ViewportDragPanInput,
    ViewportGestureContext, ViewportGestureIntent, ViewportGestureRejection, ViewportPointerButton,
    ViewportScrollInput, resolve_pane_click_distance, resolve_viewport_double_click_zoom,
    resolve_viewport_drag_pan_gesture, resolve_viewport_scroll_gesture,
};
pub use inertia::{
    ViewportPanInertiaFrame, ViewportPanInertiaPlan, ViewportPanInertiaRequest,
    plan_viewport_pan_inertia,
};
pub use transform::{
    ViewportConstraints, ViewportPanRequest, ViewportTransform, ViewportZoomRequest,
    constrain_viewport, pan_viewport, zoom_viewport,
};
