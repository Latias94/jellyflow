use jellyflow_core::ops::GraphHistory;

use super::super::NodeGraphStore;

impl NodeGraphStore {
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
