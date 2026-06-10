use super::super::super::traits::NodeGraphCallbacks;
use super::super::super::types::DeleteChange;

pub(super) fn dispatch_delete_callbacks(
    callbacks: &mut dyn NodeGraphCallbacks,
    deleted: &DeleteChange,
) {
    if !deleted.nodes().is_empty() {
        callbacks.on_nodes_delete(deleted.nodes());
    }
    if !deleted.edges().is_empty() {
        callbacks.on_edges_delete(deleted.edges());
    }
    if !deleted.groups().is_empty() {
        callbacks.on_groups_delete(deleted.groups());
    }
    if !deleted.sticky_notes().is_empty() {
        callbacks.on_sticky_notes_delete(deleted.sticky_notes());
    }
    if !deleted.is_empty() {
        callbacks.on_delete(deleted.clone());
    }
}
