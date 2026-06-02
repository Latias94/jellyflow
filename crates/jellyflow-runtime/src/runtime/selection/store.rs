use crate::runtime::store::NodeGraphStore;

use super::compute::resolve_selection_box;
use super::types::{SelectionBoxDecision, SelectionBoxInput, SelectionBoxResult};

impl SelectionBoxDecision {
    pub fn apply_to_store(self, store: &mut NodeGraphStore) -> SelectionBoxResult {
        let result = self.into_result();
        store.set_selection(
            result.nodes.clone(),
            result.edges.clone(),
            result.groups.clone(),
        );
        result
    }
}

impl NodeGraphStore {
    /// Applies a canvas-space marquee selection box to the store view-state.
    pub fn apply_selection_box(&mut self, input: SelectionBoxInput) -> SelectionBoxResult {
        let interaction = self.resolved_interaction_state();
        let decision = resolve_selection_box(
            self.graph(),
            self.lookups(),
            self.view_state(),
            &interaction,
            input,
        );
        decision.apply_to_store(self)
    }
}
