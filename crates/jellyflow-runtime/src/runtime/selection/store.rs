use jellyflow_core::core::CanvasRect;

use crate::runtime::store::NodeGraphStore;

use super::compute::compute_selection_box;
use super::types::{SelectionBoxOptions, SelectionBoxResult};

impl NodeGraphStore {
    /// Applies a canvas-space marquee selection box to the store view-state.
    pub fn apply_selection_box(
        &mut self,
        rect: CanvasRect,
        options: SelectionBoxOptions,
    ) -> SelectionBoxResult {
        let interaction = self.resolved_interaction_state();
        let result = compute_selection_box(
            self.graph(),
            self.lookups(),
            self.view_state(),
            &interaction,
            rect,
            options,
        );
        self.set_selection(
            result.nodes.clone(),
            result.edges.clone(),
            result.groups.clone(),
        );
        result
    }
}
