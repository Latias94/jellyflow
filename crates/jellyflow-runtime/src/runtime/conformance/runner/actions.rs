use crate::runtime::drag::NodeDragRequest;
use crate::runtime::store::NodeGraphStore;
use crate::runtime::viewport::{
    ViewportGestureIntent, ViewportGestureRejection, resolve_viewport_drag_pan_gesture,
    resolve_viewport_scroll_gesture,
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
        ConformanceAction::ApplyNodeDrag { node, to } => store
            .apply_node_drag(NodeDragRequest {
                node: *node,
                to: *to,
            })
            .map(|_| ())
            .map_err(|err| err.to_string()),
        ConformanceAction::ApplyDeleteSelection => store
            .apply_delete_selection()
            .map(|_| ())
            .map_err(|err| err.to_string()),
        ConformanceAction::ApplyDeleteSelectionForKey { key } => store
            .apply_delete_selection_for_key(key.0)
            .map(|_| ())
            .map_err(|err| err.to_string()),
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
    match intent {
        ViewportGestureIntent::Pan { request, .. } => store
            .apply_viewport_pan(request)
            .map(|_| ())
            .ok_or_else(|| "viewport pan gesture request was rejected".to_owned()),
        ViewportGestureIntent::Zoom { request, .. } => store
            .apply_viewport_zoom(request)
            .map(|_| ())
            .ok_or_else(|| "viewport zoom gesture request was rejected".to_owned()),
    }
}
