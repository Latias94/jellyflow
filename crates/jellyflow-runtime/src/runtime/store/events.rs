//! Store event publication internals.

use crate::io::{NodeGraphEditorConfig, NodeGraphViewState};
use crate::runtime::events::{
    NodeGraphDocumentSnapshot, NodeGraphGestureEvent, NodeGraphStoreEvent,
};
use jellyflow_core::core::Graph;

use super::NodeGraphStore;

impl NodeGraphStore {
    pub(super) fn bump_graph_revision(&mut self) {
        self.graph_revision = self.graph_revision.saturating_add(1);
    }

    pub(super) fn emit_document_replaced(
        &mut self,
        before_graph: Graph,
        before_view_state: NodeGraphViewState,
        before_editor_config: NodeGraphEditorConfig,
        before_revision: u64,
    ) {
        let after_graph = self.graph.clone();
        let after_view_state = self.view_state.clone();
        let after_editor_config = self.editor_config();
        let after_revision = self.graph_revision;

        self.emit(NodeGraphStoreEvent::DocumentReplaced {
            before: NodeGraphDocumentSnapshot {
                graph: &before_graph,
                graph_revision: before_revision,
                view_state: &before_view_state,
                editor_config: &before_editor_config,
            },
            after: NodeGraphDocumentSnapshot {
                graph: &after_graph,
                graph_revision: after_revision,
                view_state: &after_view_state,
                editor_config: &after_editor_config,
            },
        });
    }

    pub(super) fn emit(&mut self, event: NodeGraphStoreEvent<'_>) {
        for (_, sub) in &mut self.event_subscriptions {
            sub(event);
        }
    }

    /// Emits a transient gesture event for adapter layers that own pointer/keyboard gestures.
    pub fn emit_gesture(&mut self, event: NodeGraphGestureEvent) {
        for (_, sub) in &mut self.gesture_subscriptions {
            sub(event.clone());
        }
    }
}
