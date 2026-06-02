use crate::runtime::connection::resolve_connection_target;
use crate::runtime::drag::NodeDragRequest;
use crate::runtime::keyboard::KeyboardIntent;
use crate::runtime::store::NodeGraphStore;
use crate::runtime::viewport::{
    ViewportAnimationPlan, ViewportGestureIntent, ViewportGestureRejection,
    plan_viewport_animation_with_options, resolve_viewport_double_click_zoom,
    resolve_viewport_drag_pan_gesture, resolve_viewport_scroll_gesture,
};

use super::super::scenario::ConformanceAction;

pub(super) fn execute_action(
    store: &mut NodeGraphStore,
    action: &ConformanceAction,
) -> Result<(), String> {
    match action {
        ConformanceAction::DispatchTransaction { transaction } => store
            .dispatch_transaction(transaction)
            .map(|_| ())
            .map_err(|err| err.to_string()),
        ConformanceAction::ApplyNodeDrag { node, to } => require_commit(
            store.apply_node_drag(NodeDragRequest {
                node: *node,
                to: *to,
            }),
            "apply_node_drag",
        ),
        ConformanceAction::ApplyNodePointerDown { input } => {
            store.apply_node_pointer_down(input.into_runtime());
            Ok(())
        }
        ConformanceAction::ApplySelectionBox { input } => {
            store.apply_selection_box(*input);
            Ok(())
        }
        ConformanceAction::AssertConnectionTarget { input, expected } => {
            let actual = resolve_connection_target(*input);
            if actual == *expected {
                Ok(())
            } else {
                Err(format!(
                    "connection target resolved to {actual:?}, expected {expected:?}"
                ))
            }
        }
        ConformanceAction::ApplyConnectEdge { request } => {
            require_commit(store.apply_connect_edge(*request), "apply_connect_edge")
        }
        ConformanceAction::ApplyReconnectEdge { request } => {
            require_commit(store.apply_reconnect_edge(*request), "apply_reconnect_edge")
        }
        ConformanceAction::ApplyNodeNudge { request } => require_commit(
            store.apply_keyboard_intent(KeyboardIntent::NudgeSelection(request.into_runtime())),
            "apply_node_nudge",
        ),
        ConformanceAction::ApplyDeleteSelection => require_commit(
            store.apply_keyboard_intent(KeyboardIntent::DeleteSelection),
            "apply_delete_selection",
        ),
        ConformanceAction::ApplyDeleteSelectionForKey { key } => require_commit(
            store.apply_keyboard_intent(KeyboardIntent::DeleteSelectionForKey(key.0)),
            "apply_delete_selection_for_key",
        ),
        ConformanceAction::ApplyAutoPan { request } => store
            .apply_auto_pan(*request)
            .map(|_| ())
            .ok_or_else(|| "auto-pan request was rejected".to_owned()),
        ConformanceAction::ApplyViewportPan { request } => store
            .apply_viewport_pan(*request)
            .map(|_| ())
            .ok_or_else(|| "viewport pan request was rejected".to_owned()),
        ConformanceAction::ApplyViewportZoom { request } => store
            .apply_viewport_zoom(*request)
            .map(|_| ())
            .ok_or_else(|| "viewport zoom request was rejected".to_owned()),
        ConformanceAction::ApplyViewportAnimationFrame {
            request,
            elapsed_seconds,
        } => {
            let plan = plan_viewport_animation_with_options(*request)
                .ok_or_else(|| "viewport animation request was rejected".to_owned())?;
            apply_viewport_animation_frame(store, plan, *elapsed_seconds)?;
            Ok(())
        }
        ConformanceAction::ApplyViewportAnimationFrames {
            request,
            elapsed_seconds,
        } => {
            if elapsed_seconds.is_empty() {
                return Err("viewport animation frame list was empty".to_owned());
            }
            let plan = plan_viewport_animation_with_options(*request)
                .ok_or_else(|| "viewport animation request was rejected".to_owned())?;
            for elapsed_seconds in elapsed_seconds {
                apply_viewport_animation_frame(store, plan, *elapsed_seconds)?;
            }
            Ok(())
        }
        ConformanceAction::AssertViewportAnimationFrame {
            request,
            elapsed_seconds,
            expected,
        } => {
            let plan = plan_viewport_animation_with_options(*request)
                .ok_or_else(|| "viewport animation request was rejected".to_owned())?;
            let actual = plan
                .frame_at(*elapsed_seconds)
                .ok_or_else(|| "viewport animation frame was rejected".to_owned())?;
            if actual == *expected {
                Ok(())
            } else {
                Err(format!(
                    "viewport animation frame resolved to {actual:?}, expected {expected:?}"
                ))
            }
        }
        ConformanceAction::AssertViewportDoubleClickZoom {
            input,
            expected,
            expect_rejection,
        } => {
            let interaction = store.resolved_interaction_state();
            let result =
                resolve_viewport_double_click_zoom(&interaction.zoom_interaction(), *input);
            assert_viewport_double_click_zoom_result(result, *expected, *expect_rejection)
        }
        ConformanceAction::ApplyViewportScrollGesture {
            context,
            input,
            expect_rejection,
        } => {
            let interaction = store.resolved_interaction_state();
            let result = resolve_viewport_scroll_gesture(
                &interaction.pan_interaction(),
                &interaction.zoom_interaction(),
                *context,
                *input,
            );
            apply_viewport_gesture_result(store, result, *expect_rejection)
        }
        ConformanceAction::ApplyViewportDragPanGesture {
            context,
            input,
            expect_rejection,
        } => {
            let interaction = store.resolved_interaction_state();
            let result =
                resolve_viewport_drag_pan_gesture(&interaction.pan_interaction(), *context, *input);
            apply_viewport_gesture_result(store, result, *expect_rejection)
        }
        ConformanceAction::SetViewport { pan, zoom } => {
            store.set_viewport(*pan, *zoom);
            Ok(())
        }
        ConformanceAction::SetSelection {
            nodes,
            edges,
            groups,
        } => {
            store.set_selection(nodes.clone(), edges.clone(), groups.clone());
            Ok(())
        }
        ConformanceAction::EmitGesture { event } => {
            store.emit_gesture(event.clone());
            Ok(())
        }
    }
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

fn require_commit<T, E: ToString>(
    result: Result<Option<T>, E>,
    action: &'static str,
) -> Result<(), String> {
    match result {
        Ok(Some(_)) => Ok(()),
        Ok(None) => Err(format!("{action} produced no commit")),
        Err(err) => Err(err.to_string()),
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
