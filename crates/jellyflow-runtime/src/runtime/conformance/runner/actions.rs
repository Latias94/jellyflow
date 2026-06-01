use crate::runtime::drag::NodeDragRequest;
use crate::runtime::store::NodeGraphStore;

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
