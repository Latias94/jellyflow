use crate::io::NodeGraphViewState;
use crate::runtime::events::ViewChange;
use crate::runtime::viewport::{
    ViewportConstraints, ViewportPanRequest, ViewportTransform, ViewportZoomRequest,
    constrain_viewport, pan_viewport, zoom_viewport,
};
use jellyflow_core::core::{CanvasPoint, CanvasSize, EdgeId, GroupId, NodeId};

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
        self.update_view_state_if_changed(
            |view_state| view_state.set_viewport(pan, zoom),
            ViewStateMutationKind::Viewport,
        );
    }

    /// Applies a normalized drag-pan request through normal view-state publication.
    pub fn apply_viewport_pan(&mut self, request: ViewportPanRequest) -> Option<ViewportTransform> {
        let current = ViewportTransform::from_view_state(&self.view_state)?;
        let next = constrain_viewport(
            pan_viewport(current, request)?,
            ViewportConstraints::unconstrained(),
        )?;
        self.set_viewport(next.pan, next.zoom);
        Some(next)
    }

    /// Applies a drag-pan request while honoring configured translate extents.
    pub fn apply_viewport_pan_constrained(
        &mut self,
        request: ViewportPanRequest,
        viewport_size: CanvasSize,
    ) -> Option<ViewportTransform> {
        let current = ViewportTransform::from_view_state(&self.view_state)?;
        let next = constrain_viewport(
            pan_viewport(current, request)?,
            self.viewport_constraints(viewport_size),
        )?;
        self.set_viewport(next.pan, next.zoom);
        Some(next)
    }

    /// Applies a normalized anchored zoom request through normal view-state publication.
    pub fn apply_viewport_zoom(
        &mut self,
        request: ViewportZoomRequest,
    ) -> Option<ViewportTransform> {
        let current = ViewportTransform::from_view_state(&self.view_state)?;
        let next = constrain_viewport(
            zoom_viewport(current, request)?,
            ViewportConstraints::unconstrained(),
        )?;
        self.set_viewport(next.pan, next.zoom);
        Some(next)
    }

    /// Applies an anchored zoom request while honoring configured translate extents.
    pub fn apply_viewport_zoom_constrained(
        &mut self,
        request: ViewportZoomRequest,
        viewport_size: CanvasSize,
    ) -> Option<ViewportTransform> {
        let current = ViewportTransform::from_view_state(&self.view_state)?;
        let next = constrain_viewport(
            zoom_viewport(current, request)?,
            self.viewport_constraints(viewport_size),
        )?;
        self.set_viewport(next.pan, next.zoom);
        Some(next)
    }

    /// Sets selection state and notifies subscribers.
    pub fn set_selection(&mut self, nodes: Vec<NodeId>, edges: Vec<EdgeId>, groups: Vec<GroupId>) {
        self.update_view_state_if_changed(
            |view_state| view_state.set_selection(nodes, edges, groups),
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

    fn viewport_constraints(&self, viewport_size: CanvasSize) -> ViewportConstraints {
        match self
            .resolved_interaction_state()
            .pan_interaction()
            .translate_extent
        {
            Some(translate_extent) => {
                ViewportConstraints::with_translate_extent(viewport_size, translate_extent)
            }
            None => ViewportConstraints::unconstrained(),
        }
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
