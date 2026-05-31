use super::super::traits::NodeGraphCallbacks;
use super::super::types::SelectionChange;
use crate::runtime::events::ViewChange;

pub(super) fn dispatch_view_callbacks(
    callbacks: &mut dyn NodeGraphCallbacks,
    changes: &[ViewChange],
) {
    callbacks.on_view_change(changes);
    for change in changes.iter() {
        match change {
            ViewChange::Viewport { pan, zoom } => {
                callbacks.on_viewport_change(*pan, *zoom);
                callbacks.on_move(*pan, *zoom);
            }
            ViewChange::Selection {
                nodes,
                edges,
                groups,
            } => callbacks.on_selection_change(SelectionChange::new(
                nodes.clone(),
                edges.clone(),
                groups.clone(),
            )),
        }
    }
}
