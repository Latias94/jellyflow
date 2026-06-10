use crate::runtime::drag::NodeDragRequest;
use crate::runtime::keyboard::KeyboardIntent;
use crate::runtime::store::NodeGraphStore;
use jellyflow_core::core::{CanvasPoint, NodeId};

use super::super::super::scenario::{
    ConformanceNodeNudgeRequest, ConformanceNodePointerDownInput,
    ConformanceNodePointerResizeRequest, ConformanceNodeResizeRequest,
};
use super::require_commit;

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

pub(super) fn apply_node_resize(
    store: &mut NodeGraphStore,
    request: ConformanceNodeResizeRequest,
) -> Result<(), String> {
    require_commit(
        store.apply_node_resize(request.into_runtime()),
        "apply_node_resize",
    )
}

pub(super) fn apply_node_pointer_resize(
    store: &mut NodeGraphStore,
    request: ConformanceNodePointerResizeRequest,
) -> Result<(), String> {
    require_commit(
        store.apply_node_pointer_resize(request.into_runtime()),
        "apply_node_pointer_resize",
    )
}

pub(super) fn apply_node_pointer_resize_session(
    store: &mut NodeGraphStore,
    request: ConformanceNodePointerResizeRequest,
) -> Result<(), String> {
    let (session, update_request) = request.into_runtime_session();
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
    input: ConformanceNodePointerDownInput,
) {
    store.apply_node_pointer_down(input.into_runtime());
}

pub(super) fn apply_node_nudge(
    store: &mut NodeGraphStore,
    request: ConformanceNodeNudgeRequest,
) -> Result<(), String> {
    require_commit(
        store.apply_keyboard_intent(KeyboardIntent::NudgeSelection(request.into_runtime())),
        "apply_node_nudge",
    )
}
