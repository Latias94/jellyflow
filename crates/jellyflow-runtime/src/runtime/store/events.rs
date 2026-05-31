//! Store event publication internals.

use crate::io::{NodeGraphEditorConfig, NodeGraphViewState};
use crate::runtime::commit::NodeGraphPatch;
use crate::runtime::events::{
    NodeGraphDocumentSnapshot, NodeGraphGestureEvent, NodeGraphStoreEvent, ViewChange,
};
use jellyflow_core::core::Graph;

use super::NodeGraphStore;

pub(super) struct DocumentSnapshotParts {
    graph: Graph,
    graph_revision: u64,
    view_state: NodeGraphViewState,
    editor_config: NodeGraphEditorConfig,
}

impl DocumentSnapshotParts {
    fn from_store(store: &NodeGraphStore) -> Self {
        Self {
            graph: store.graph.clone(),
            graph_revision: store.graph_revision,
            view_state: store.view_state.clone(),
            editor_config: store.editor_config(),
        }
    }

    fn event_snapshot(&self) -> NodeGraphDocumentSnapshot<'_> {
        NodeGraphDocumentSnapshot {
            graph: &self.graph,
            graph_revision: self.graph_revision,
            view_state: &self.view_state,
            editor_config: &self.editor_config,
        }
    }
}

impl NodeGraphStore {
    pub(super) fn bump_graph_revision(&mut self) {
        self.graph_revision = self.graph_revision.saturating_add(1);
    }

    pub(super) fn capture_document_snapshot(&self) -> DocumentSnapshotParts {
        DocumentSnapshotParts::from_store(self)
    }

    pub(super) fn publish_document_replaced(&mut self, before: DocumentSnapshotParts) {
        let after = self.capture_document_snapshot();

        self.emit(NodeGraphStoreEvent::DocumentReplaced {
            before: before.event_snapshot(),
            after: after.event_snapshot(),
        });
        self.notify_selectors();
    }

    pub(super) fn publish_graph_commit(&mut self, patch: &NodeGraphPatch) {
        self.emit(NodeGraphStoreEvent::GraphCommitted { patch });
        self.notify_selectors();
    }

    pub(super) fn publish_view_changed(
        &mut self,
        before: &NodeGraphViewState,
        after: &NodeGraphViewState,
        changes: &[ViewChange],
    ) {
        if !changes.is_empty() {
            self.emit(NodeGraphStoreEvent::ViewChanged {
                before,
                after,
                changes,
            });
        }
        self.notify_selectors();
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
