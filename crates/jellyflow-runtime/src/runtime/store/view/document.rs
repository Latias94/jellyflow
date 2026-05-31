use crate::io::{NodeGraphEditorConfig, NodeGraphViewState};
use crate::runtime::lookups::NodeGraphLookups;
use jellyflow_core::core::Graph;
use jellyflow_core::ops::GraphHistory;

use super::super::NodeGraphStore;

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
}
