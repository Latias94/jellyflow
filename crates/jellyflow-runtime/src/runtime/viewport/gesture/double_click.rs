use crate::io::NodeGraphZoomInteraction;

use super::super::{
    ViewportAnimationPlan, ViewportAnimationRequest, ViewportZoomRequest,
    plan_viewport_animation_with_options, zoom_viewport,
};
use super::types::{ViewportDoubleClickZoomInput, ViewportGestureRejection};

/// Resolves normalized double-click zoom input into an anchored viewport animation plan.
pub fn resolve_viewport_double_click_zoom(
    zoom: &NodeGraphZoomInteraction,
    input: ViewportDoubleClickZoomInput,
) -> Result<ViewportAnimationPlan, ViewportGestureRejection> {
    if !zoom.zoom_on_double_click {
        return Err(ViewportGestureRejection::DoubleClickZoomDisabled);
    }
    if !input.current.is_valid() || !input.anchor_screen.is_finite() {
        return Err(ViewportGestureRejection::InvalidInput);
    }
    if !input.zoom_factor.is_finite() || input.zoom_factor <= 0.0 {
        return Err(ViewportGestureRejection::InvalidInput);
    }
    let target_zoom = input.current.zoom * input.zoom_factor;
    if !target_zoom.is_finite() {
        return Err(ViewportGestureRejection::InvalidInput);
    }

    let target = zoom_viewport(
        input.current,
        ViewportZoomRequest::new(
            input.anchor_screen,
            target_zoom,
            input.min_zoom,
            input.max_zoom,
        ),
    )
    .ok_or(ViewportGestureRejection::InvalidInput)?;

    plan_viewport_animation_with_options(ViewportAnimationRequest::new(
        input.current,
        target,
        input.animation,
    ))
    .ok_or(ViewportGestureRejection::InvalidInput)
}
