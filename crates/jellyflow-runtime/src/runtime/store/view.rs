//! Document, view-state, editor-config, and history accessors for `NodeGraphStore`.

use crate::io::{
    NodeGraphEditorConfig, NodeGraphInteractionConfig, NodeGraphInteractionState,
    NodeGraphRuntimeTuning, NodeGraphViewState,
};
use crate::runtime::events::ViewChange;
use crate::runtime::lookups::NodeGraphLookups;
use jellyflow_core::core::Graph;
use jellyflow_core::ops::GraphHistory;

use super::NodeGraphStore;

impl NodeGraphStore {
    pub fn graph(&self) -> &Graph {
        &self.graph
    }

    pub fn graph_revision(&self) -> u64 {
        self.graph_revision
    }

    pub fn lookups(&self) -> &NodeGraphLookups {
        &self.lookups
    }

    /// Replaces the entire graph document.
    ///
    /// This is a controlled-mode helper: callers that own graph state can swap the document
    /// without going through transactions (e.g. loading a file, switching tabs).
    ///
    /// This emits a document replacement event, not a graph commit. Selection is sanitized against
    /// the new graph.
    pub fn replace_graph(&mut self, graph: Graph) {
        let before = self.capture_document_snapshot();

        self.graph = graph;
        self.bump_graph_revision();
        self.view_state.sanitize_for_graph(&self.graph);
        self.lookups.rebuild_from(&self.graph);
        self.publish_document_replaced(before);
    }

    /// Replaces the entire document snapshot in one atomic store update.
    ///
    /// This is the full document reset path: graph, view state, editor config, lookups, revision,
    /// and undo/redo history are updated together, then one `DocumentReplaced` event is emitted.
    pub fn replace_document(
        &mut self,
        graph: Graph,
        mut view_state: NodeGraphViewState,
        editor_config: NodeGraphEditorConfig,
    ) {
        let before = self.capture_document_snapshot();

        view_state.sanitize_for_graph(&graph);
        self.graph = graph;
        self.bump_graph_revision();
        self.view_state = view_state;
        self.interaction = editor_config.interaction;
        self.runtime_tuning = editor_config.runtime_tuning;
        self.history = GraphHistory::default();
        self.lookups.rebuild_from(&self.graph);
        self.publish_document_replaced(before);
    }

    pub fn view_state(&self) -> &NodeGraphViewState {
        &self.view_state
    }

    pub fn interaction(&self) -> &NodeGraphInteractionConfig {
        &self.interaction
    }

    pub fn runtime_tuning(&self) -> &NodeGraphRuntimeTuning {
        &self.runtime_tuning
    }

    pub fn editor_config(&self) -> NodeGraphEditorConfig {
        NodeGraphEditorConfig {
            interaction: self.interaction.clone(),
            runtime_tuning: self.runtime_tuning,
        }
    }

    pub fn resolved_interaction_state(&self) -> NodeGraphInteractionState {
        NodeGraphInteractionState::from_parts(&self.interaction, &self.runtime_tuning)
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

    pub fn replace_editor_config(&mut self, editor_config: NodeGraphEditorConfig) {
        self.install_editor_config_if_changed(editor_config);
    }

    pub fn update_editor_config(&mut self, f: impl FnOnce(&mut NodeGraphEditorConfig)) {
        let mut next = self.editor_config();
        f(&mut next);
        self.install_editor_config_if_changed(next);
    }

    fn install_editor_config_if_changed(&mut self, editor_config: NodeGraphEditorConfig) {
        if self.interaction == editor_config.interaction
            && self.runtime_tuning == editor_config.runtime_tuning
        {
            return;
        }

        self.interaction = editor_config.interaction;
        self.runtime_tuning = editor_config.runtime_tuning;
        self.notify_selectors();
    }

    /// Sets the viewport (pan/zoom) and notifies subscribers.
    pub fn set_viewport(&mut self, pan: jellyflow_core::core::CanvasPoint, zoom: f32) {
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
    pub fn set_selection(
        &mut self,
        nodes: Vec<jellyflow_core::core::NodeId>,
        edges: Vec<jellyflow_core::core::EdgeId>,
        groups: Vec<jellyflow_core::core::GroupId>,
    ) {
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

    pub fn history(&self) -> &GraphHistory {
        &self.history
    }

    pub fn clear_history(&mut self) {
        self.history = GraphHistory::default();
        self.notify_selectors();
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }
}

#[derive(Clone, Copy)]
enum ViewStateMutationKind {
    FullState,
    Viewport,
    Selection,
}

impl ViewStateMutationKind {
    fn sanitize(self, graph: &Graph, view_state: &mut NodeGraphViewState) {
        match self {
            Self::FullState | Self::Selection => view_state.sanitize_for_graph(graph),
            Self::Viewport => {}
        }
    }

    fn changed(self, before: &NodeGraphViewState, after: &NodeGraphViewState) -> bool {
        match self {
            Self::FullState => !view_state_eq(before, after),
            Self::Viewport => before.pan != after.pan || before.zoom != after.zoom,
            Self::Selection => {
                before.selected_nodes != after.selected_nodes
                    || before.selected_edges != after.selected_edges
                    || before.selected_groups != after.selected_groups
            }
        }
    }

    fn collect_changes(
        self,
        before: &NodeGraphViewState,
        after: &NodeGraphViewState,
    ) -> Vec<ViewChange> {
        match self {
            Self::FullState => collect_view_projection_changes(before, after),
            Self::Viewport => vec![ViewChange::Viewport {
                pan: after.pan,
                zoom: after.zoom,
            }],
            Self::Selection => vec![ViewChange::Selection {
                nodes: after.selected_nodes.clone(),
                edges: after.selected_edges.clone(),
                groups: after.selected_groups.clone(),
            }],
        }
    }
}

fn view_state_eq(a: &NodeGraphViewState, b: &NodeGraphViewState) -> bool {
    a.pan == b.pan
        && a.zoom == b.zoom
        && a.selected_nodes == b.selected_nodes
        && a.selected_edges == b.selected_edges
        && a.selected_groups == b.selected_groups
        && a.draw_order == b.draw_order
        && a.group_draw_order == b.group_draw_order
}

fn collect_view_projection_changes(
    before: &NodeGraphViewState,
    after: &NodeGraphViewState,
) -> Vec<ViewChange> {
    let mut changes: Vec<ViewChange> = Vec::new();
    if before.pan != after.pan || (before.zoom - after.zoom).abs() > 1.0e-6 {
        changes.push(ViewChange::Viewport {
            pan: after.pan,
            zoom: after.zoom,
        });
    }
    if before.selected_nodes != after.selected_nodes
        || before.selected_edges != after.selected_edges
        || before.selected_groups != after.selected_groups
    {
        changes.push(ViewChange::Selection {
            nodes: after.selected_nodes.clone(),
            edges: after.selected_edges.clone(),
            groups: after.selected_groups.clone(),
        });
    }
    changes
}
