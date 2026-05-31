use crate::io::NodeGraphViewState;
use crate::runtime::events::ViewChange;
use jellyflow_core::core::{CanvasPoint, EdgeId, GroupId, NodeId};

use super::super::NodeGraphStore;
use super::changes::ViewStateMutationKind;

impl NodeGraphStore {
    pub fn view_state(&self) -> &NodeGraphViewState {
        &self.view_state
    }

    /// Replaces the full view-state payload.
    ///
    /// This is the controlled-mode counterpart of `set_viewport`/`set_selection`.
    pub fn replace_view_state(&mut self, view_state: NodeGraphViewState) {
        self.update_view_state_if_changed(
            |current| *current = view_state,
            ViewStateMutationKind::FullState,
        );
    }

    /// Mutates view-state in place and emits derived `ViewChange` events.
    pub fn update_view_state(&mut self, f: impl FnOnce(&mut NodeGraphViewState)) {
        self.update_view_state_if_changed(f, ViewStateMutationKind::FullState);
    }

    /// Sets the viewport (pan/zoom) and notifies subscribers.
    pub fn set_viewport(&mut self, pan: CanvasPoint, zoom: f32) {
        let z = if zoom.is_finite() && zoom > 0.0 {
            zoom
        } else {
            1.0
        };
        self.update_view_state_if_changed(
            |view_state| {
                view_state.pan = pan;
                view_state.zoom = z;
            },
            ViewStateMutationKind::Viewport,
        );
    }

    /// Sets selection state and notifies subscribers.
    pub fn set_selection(&mut self, nodes: Vec<NodeId>, edges: Vec<EdgeId>, groups: Vec<GroupId>) {
        self.update_view_state_if_changed(
            |view_state| {
                view_state.selected_nodes = nodes;
                view_state.selected_edges = edges;
                view_state.selected_groups = groups;
            },
            ViewStateMutationKind::Selection,
        );
    }

    fn update_view_state_if_changed(
        &mut self,
        mutate: impl FnOnce(&mut NodeGraphViewState),
        kind: ViewStateMutationKind,
    ) {
        let before = self.view_state.clone();
        mutate(&mut self.view_state);
        kind.sanitize(&self.graph, &mut self.view_state);
        let after = self.view_state.clone();

        if !kind.changed(&before, &after) {
            return;
        }

        let changes = kind.collect_changes(&before, &after);
        self.publish_view_state_change(before, after, changes);
    }

    fn publish_view_state_change(
        &mut self,
        before: NodeGraphViewState,
        after: NodeGraphViewState,
        changes: Vec<ViewChange>,
    ) {
        self.publish_view_changed(&before, &after, &changes);
    }
}
