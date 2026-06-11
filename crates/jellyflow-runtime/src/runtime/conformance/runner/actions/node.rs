use crate::runtime::drag::{NodeDragRequest, NodeNudgeRequest, PointerGestureClaim};
use crate::runtime::gesture::NodeDragSession;
use crate::runtime::keyboard::KeyboardIntent;
use crate::runtime::resize::{NodePointerResizeRequest, NodeResizeRequest};
use crate::runtime::selection::NodePointerDownInput;
use crate::runtime::store::NodeGraphStore;
use jellyflow_core::core::{CanvasPoint, NodeId};

use super::super::super::scenario::ConformanceAction;
use super::require_commit;

pub(super) fn execute_action(
    store: &mut NodeGraphStore,
    action: &ConformanceAction,
) -> Option<Result<(), String>> {
    Some(match action {
        ConformanceAction::ApplyNodeDrag { node, to } => apply_node_drag(store, *node, *to),
        ConformanceAction::ApplyNodeDragSession { node, start, to } => {
            apply_node_drag_session(store, *node, *start, *to)
        }
        ConformanceAction::ApplyNodeResize { request } => apply_node_resize(store, *request),
        ConformanceAction::ApplyNodePointerResize { request } => {
            apply_node_pointer_resize(store, *request)
        }
        ConformanceAction::ApplyNodePointerResizeSession { request } => {
            apply_node_pointer_resize_session(store, *request)
        }
        ConformanceAction::ApplyNodePointerDown {
            input,
            expected_claim,
        } => apply_node_pointer_down(store, *input, *expected_claim),
        ConformanceAction::ApplyNodeNudge { request } => apply_node_nudge(store, *request),
        _ => return None,
    })
}

pub(super) fn apply_node_drag(
    store: &mut NodeGraphStore,
    node: NodeId,
    to: CanvasPoint,
) -> Result<(), String> {
    require_commit(
        store.apply_node_drag(NodeDragRequest { node, to }),
        "apply_node_drag",
    )
}

pub(super) fn apply_node_drag_session(
    store: &mut NodeGraphStore,
    node: NodeId,
    start: CanvasPoint,
    to: CanvasPoint,
) -> Result<(), String> {
    let outcome = store
        .apply_node_drag_session(NodeDragSession::new(node, start, to))
        .map_err(|err| err.to_string())?;
    if outcome.committed_update().is_some() {
        Ok(())
    } else {
        Err("apply_node_drag_session produced no commit".to_owned())
    }
}

pub(super) fn apply_node_resize(
    store: &mut NodeGraphStore,
    request: NodeResizeRequest,
) -> Result<(), String> {
    require_commit(store.apply_node_resize(request), "apply_node_resize")
}

pub(super) fn apply_node_pointer_resize(
    store: &mut NodeGraphStore,
    request: NodePointerResizeRequest,
) -> Result<(), String> {
    require_commit(
        store.apply_node_pointer_resize(request),
        "apply_node_pointer_resize",
    )
}

pub(super) fn apply_node_pointer_resize_session(
    store: &mut NodeGraphStore,
    request: NodePointerResizeRequest,
) -> Result<(), String> {
    let session = crate::runtime::resize::NodeResizeSession::new(
        request.node,
        request.start,
        request.direction,
    );
    let update_request =
        crate::runtime::resize::NodeResizeSessionUpdateRequest::new(request.current)
            .with_constraints(request.constraints)
            .with_keep_aspect_ratio(request.keep_aspect_ratio)
            .with_axis(request.axis);
    let outcome = store
        .apply_node_resize_session(session, update_request)
        .map_err(|err| err.to_string())?;
    if outcome.committed_update().is_some() {
        Ok(())
    } else {
        Err("apply_node_pointer_resize_session produced no commit".to_owned())
    }
}

pub(super) fn apply_node_pointer_down(
    store: &mut NodeGraphStore,
    input: NodePointerDownInput,
    expected_claim: Option<PointerGestureClaim>,
) -> Result<(), String> {
    let decision = store.apply_node_pointer_down(input);
    if let Some(expected_claim) = expected_claim
        && decision.drag_claim != expected_claim
    {
        return Err(format!(
            "apply_node_pointer_down expected drag claim {expected_claim:?}, got {:?}",
            decision.drag_claim
        ));
    }

    Ok(())
}

pub(super) fn apply_node_nudge(
    store: &mut NodeGraphStore,
    request: NodeNudgeRequest,
) -> Result<(), String> {
    require_commit(
        store.apply_keyboard_intent(KeyboardIntent::NudgeSelection(request)),
        "apply_node_nudge",
    )
}
