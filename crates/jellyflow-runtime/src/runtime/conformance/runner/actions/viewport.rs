use crate::runtime::auto_pan::{AutoPanRequest, SelectionAutoPanRequest};
use crate::runtime::store::NodeGraphStore;
use crate::runtime::viewport::{
    ViewportAnimationFrame, ViewportAnimationPlan, ViewportAnimationRequest,
    ViewportDoubleClickZoomInput, ViewportDragPanInput, ViewportGestureContext,
    ViewportGestureIntent, ViewportGestureRejection, ViewportPanInertiaFrame,
    ViewportPanInertiaPlan, ViewportPanInertiaRequest, ViewportPanRequest, ViewportScrollInput,
    ViewportZoomRequest, plan_viewport_animation_with_options, plan_viewport_pan_inertia,
    resolve_viewport_double_click_zoom, resolve_viewport_drag_pan_gesture,
    resolve_viewport_scroll_gesture,
};
use jellyflow_core::core::CanvasSize;

use super::super::super::scenario::ConformanceAction;

pub(super) fn execute_action(
    store: &mut NodeGraphStore,
    action: &ConformanceAction,
) -> Option<Result<(), String>> {
    Some(match action {
        ConformanceAction::ApplyAutoPan { request } => apply_auto_pan(store, *request),
        ConformanceAction::ApplySelectionAutoPan { request } => {
            apply_selection_auto_pan(store, *request)
        }
        ConformanceAction::ApplyViewportPan { request } => apply_pan(store, *request),
        ConformanceAction::ApplyViewportPanConstrained {
            request,
            viewport_size,
        } => apply_pan_constrained(store, *request, *viewport_size),
        ConformanceAction::ApplyViewportZoom { request } => apply_zoom(store, *request),
        ConformanceAction::ApplyViewportZoomConstrained {
            request,
            viewport_size,
        } => apply_zoom_constrained(store, *request, *viewport_size),
        ConformanceAction::ApplyViewportAnimationFrame {
            request,
            elapsed_seconds,
        } => apply_animation_frame(store, *request, *elapsed_seconds),
        ConformanceAction::ApplyViewportAnimationFrames {
            request,
            elapsed_seconds,
        } => apply_animation_frames(store, *request, elapsed_seconds),
        ConformanceAction::AssertViewportAnimationFrame {
            request,
            elapsed_seconds,
            expected,
        } => assert_animation_frame(*request, *elapsed_seconds, *expected),
        ConformanceAction::ApplyViewportPanInertiaFrame {
            request,
            elapsed_seconds,
        } => apply_pan_inertia_frame(store, request, *elapsed_seconds),
        ConformanceAction::ApplyViewportPanInertiaFrames {
            request,
            elapsed_seconds,
        } => apply_pan_inertia_frames(store, request, elapsed_seconds),
        ConformanceAction::AssertViewportPanInertiaFrame {
            request,
            elapsed_seconds,
            expected,
        } => assert_pan_inertia_frame(request, *elapsed_seconds, *expected),
        ConformanceAction::ExpectViewportPanInertiaRejected { request } => {
            expect_pan_inertia_rejected(request)
        }
        ConformanceAction::AssertViewportDoubleClickZoom {
            input,
            expected,
            expect_rejection,
        } => assert_double_click_zoom(store, *input, *expected, *expect_rejection),
        ConformanceAction::ApplyViewportScrollGesture {
            context,
            input,
            expect_rejection,
        } => apply_scroll_gesture(store, *context, *input, *expect_rejection),
        ConformanceAction::ApplyViewportDragPanGesture {
            context,
            input,
            expect_rejection,
        } => apply_drag_pan_gesture(store, *context, *input, *expect_rejection),
        _ => return None,
    })
}

pub(super) fn apply_auto_pan(
    store: &mut NodeGraphStore,
    request: AutoPanRequest,
) -> Result<(), String> {
    store
        .apply_auto_pan(request)
        .map(|_| ())
        .ok_or_else(|| "auto-pan request was rejected".to_owned())
}

pub(super) fn apply_selection_auto_pan(
    store: &mut NodeGraphStore,
    request: SelectionAutoPanRequest,
) -> Result<(), String> {
    store
        .apply_selection_auto_pan(request)
        .map(|_| ())
        .ok_or_else(|| "selection auto-pan request was rejected".to_owned())
}

pub(super) fn apply_pan(
    store: &mut NodeGraphStore,
    request: ViewportPanRequest,
) -> Result<(), String> {
    store
        .apply_viewport_pan(request)
        .map(|_| ())
        .ok_or_else(|| "viewport pan request was rejected".to_owned())
}

pub(super) fn apply_pan_constrained(
    store: &mut NodeGraphStore,
    request: ViewportPanRequest,
    viewport_size: CanvasSize,
) -> Result<(), String> {
    store
        .apply_viewport_pan_constrained(request, viewport_size)
        .map(|_| ())
        .ok_or_else(|| "viewport constrained pan request was rejected".to_owned())
}

pub(super) fn apply_zoom(
    store: &mut NodeGraphStore,
    request: ViewportZoomRequest,
) -> Result<(), String> {
    store
        .apply_viewport_zoom(request)
        .map(|_| ())
        .ok_or_else(|| "viewport zoom request was rejected".to_owned())
}

pub(super) fn apply_zoom_constrained(
    store: &mut NodeGraphStore,
    request: ViewportZoomRequest,
    viewport_size: CanvasSize,
) -> Result<(), String> {
    store
        .apply_viewport_zoom_constrained(request, viewport_size)
        .map(|_| ())
        .ok_or_else(|| "viewport constrained zoom request was rejected".to_owned())
}

pub(super) fn apply_animation_frame(
    store: &mut NodeGraphStore,
    request: ViewportAnimationRequest,
    elapsed_seconds: f32,
) -> Result<(), String> {
    let plan = plan_viewport_animation_with_options(request)
        .ok_or_else(|| "viewport animation request was rejected".to_owned())?;
    apply_viewport_animation_frame(store, plan, elapsed_seconds)
}

pub(super) fn apply_animation_frames(
    store: &mut NodeGraphStore,
    request: ViewportAnimationRequest,
    elapsed_seconds: &[f32],
) -> Result<(), String> {
    if elapsed_seconds.is_empty() {
        return Err("viewport animation frame list was empty".to_owned());
    }
    let plan = plan_viewport_animation_with_options(request)
        .ok_or_else(|| "viewport animation request was rejected".to_owned())?;
    for elapsed_seconds in elapsed_seconds {
        apply_viewport_animation_frame(store, plan, *elapsed_seconds)?;
    }
    Ok(())
}

pub(super) fn assert_animation_frame(
    request: ViewportAnimationRequest,
    elapsed_seconds: f32,
    expected: ViewportAnimationFrame,
) -> Result<(), String> {
    let plan = plan_viewport_animation_with_options(request)
        .ok_or_else(|| "viewport animation request was rejected".to_owned())?;
    let actual = plan
        .frame_at(elapsed_seconds)
        .ok_or_else(|| "viewport animation frame was rejected".to_owned())?;
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "viewport animation frame resolved to {actual:?}, expected {expected:?}"
        ))
    }
}

pub(super) fn apply_pan_inertia_frame(
    store: &mut NodeGraphStore,
    request: &ViewportPanInertiaRequest,
    elapsed_seconds: f32,
) -> Result<(), String> {
    let plan = plan_viewport_pan_inertia(request.clone())
        .ok_or_else(|| "viewport pan inertia request was rejected".to_owned())?;
    apply_viewport_pan_inertia_frame(store, plan, elapsed_seconds)
}

pub(super) fn apply_pan_inertia_frames(
    store: &mut NodeGraphStore,
    request: &ViewportPanInertiaRequest,
    elapsed_seconds: &[f32],
) -> Result<(), String> {
    if elapsed_seconds.is_empty() {
        return Err("viewport pan inertia frame list was empty".to_owned());
    }
    let plan = plan_viewport_pan_inertia(request.clone())
        .ok_or_else(|| "viewport pan inertia request was rejected".to_owned())?;
    for elapsed_seconds in elapsed_seconds {
        apply_viewport_pan_inertia_frame(store, plan, *elapsed_seconds)?;
    }
    Ok(())
}

pub(super) fn assert_pan_inertia_frame(
    request: &ViewportPanInertiaRequest,
    elapsed_seconds: f32,
    expected: ViewportPanInertiaFrame,
) -> Result<(), String> {
    let plan = plan_viewport_pan_inertia(request.clone())
        .ok_or_else(|| "viewport pan inertia request was rejected".to_owned())?;
    let actual = plan
        .frame_at(elapsed_seconds)
        .ok_or_else(|| "viewport pan inertia frame was rejected".to_owned())?;
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "viewport pan inertia frame resolved to {actual:?}, expected {expected:?}"
        ))
    }
}

pub(super) fn expect_pan_inertia_rejected(
    request: &ViewportPanInertiaRequest,
) -> Result<(), String> {
    match plan_viewport_pan_inertia(request.clone()) {
        Some(actual) => Err(format!(
            "viewport pan inertia was accepted as {actual:?}, expected rejection"
        )),
        None => Ok(()),
    }
}

pub(super) fn assert_double_click_zoom(
    store: &NodeGraphStore,
    input: ViewportDoubleClickZoomInput,
    expected: Option<ViewportAnimationPlan>,
    expect_rejection: Option<ViewportGestureRejection>,
) -> Result<(), String> {
    let interaction = store.resolved_interaction_state();
    let result = resolve_viewport_double_click_zoom(&interaction.zoom_interaction(), input);
    assert_viewport_double_click_zoom_result(result, expected, expect_rejection)
}

pub(super) fn apply_scroll_gesture(
    store: &mut NodeGraphStore,
    context: ViewportGestureContext,
    input: ViewportScrollInput,
    expect_rejection: Option<ViewportGestureRejection>,
) -> Result<(), String> {
    let interaction = store.resolved_interaction_state();
    let result = resolve_viewport_scroll_gesture(
        &interaction.pan_interaction(),
        &interaction.zoom_interaction(),
        context,
        input,
    );
    apply_viewport_gesture_result(store, result, expect_rejection)
}

pub(super) fn apply_drag_pan_gesture(
    store: &mut NodeGraphStore,
    context: ViewportGestureContext,
    input: ViewportDragPanInput,
    expect_rejection: Option<ViewportGestureRejection>,
) -> Result<(), String> {
    let interaction = store.resolved_interaction_state();
    let result = resolve_viewport_drag_pan_gesture(&interaction.pan_interaction(), context, input);
    apply_viewport_gesture_result(store, result, expect_rejection)
}

fn apply_viewport_animation_frame(
    store: &mut NodeGraphStore,
    plan: ViewportAnimationPlan,
    elapsed_seconds: f32,
) -> Result<(), String> {
    let frame = plan
        .frame_at(elapsed_seconds)
        .ok_or_else(|| "viewport animation frame was rejected".to_owned())?;
    store.set_viewport(frame.transform.pan, frame.transform.zoom);
    Ok(())
}

fn apply_viewport_pan_inertia_frame(
    store: &mut NodeGraphStore,
    plan: ViewportPanInertiaPlan,
    elapsed_seconds: f32,
) -> Result<(), String> {
    let frame = plan
        .frame_at(elapsed_seconds)
        .ok_or_else(|| "viewport pan inertia frame was rejected".to_owned())?;
    store.set_viewport(frame.transform.pan, frame.transform.zoom);
    Ok(())
}

fn assert_viewport_double_click_zoom_result(
    result: Result<ViewportAnimationPlan, ViewportGestureRejection>,
    expected: Option<ViewportAnimationPlan>,
    expect_rejection: Option<ViewportGestureRejection>,
) -> Result<(), String> {
    match (result, expected, expect_rejection) {
        (Ok(actual), Some(expected), None) if actual == expected => Ok(()),
        (Ok(actual), Some(expected), None) => Err(format!(
            "viewport double-click zoom resolved to {actual:?}, expected {expected:?}"
        )),
        (Ok(actual), None, Some(expected_rejection)) => Err(format!(
            "viewport double-click zoom was accepted as {actual:?}, expected rejection {expected_rejection:?}"
        )),
        (Err(actual), None, Some(expected)) if actual == expected => Ok(()),
        (Err(actual), None, Some(expected)) => Err(format!(
            "viewport double-click zoom rejected with {actual:?}, expected {expected:?}"
        )),
        (Err(actual), Some(_), None) => Err(format!(
            "viewport double-click zoom was rejected: {actual:?}"
        )),
        (Ok(actual), None, None) => Err(format!(
            "viewport double-click zoom accepted as {actual:?}, but no expected plan was provided"
        )),
        (Err(actual), None, None) => Err(format!(
            "viewport double-click zoom was rejected: {actual:?}, but no expected rejection was provided"
        )),
        (_, Some(_), Some(_)) => Err(
            "viewport double-click zoom action cannot expect both a plan and a rejection"
                .to_owned(),
        ),
    }
}

fn apply_viewport_gesture_result(
    store: &mut NodeGraphStore,
    result: Result<ViewportGestureIntent, ViewportGestureRejection>,
    expect_rejection: Option<ViewportGestureRejection>,
) -> Result<(), String> {
    match (result, expect_rejection) {
        (Ok(intent), None) => apply_viewport_gesture_intent(store, intent),
        (Ok(intent), Some(expected)) => Err(format!(
            "viewport gesture was accepted as {:?}, expected rejection {:?}",
            intent.move_kind(),
            expected
        )),
        (Err(actual), Some(expected)) if actual == expected => Ok(()),
        (Err(actual), Some(expected)) => Err(format!(
            "viewport gesture rejected with {:?}, expected {:?}",
            actual, expected
        )),
        (Err(actual), None) => Err(format!("viewport gesture was rejected: {:?}", actual)),
    }
}

fn apply_viewport_gesture_intent(
    store: &mut NodeGraphStore,
    intent: ViewportGestureIntent,
) -> Result<(), String> {
    if intent.apply_to_store(store) {
        Ok(())
    } else {
        match intent {
            ViewportGestureIntent::Pan { .. } => {
                Err("viewport pan gesture request was rejected".to_owned())
            }
            ViewportGestureIntent::Zoom { .. } => {
                Err("viewport zoom gesture request was rejected".to_owned())
            }
        }
    }
}
