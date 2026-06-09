use crate::runtime::store::NodeGraphStore;
use crate::runtime::viewport::ViewportTransform;

use super::planner::{compute_auto_pan, compute_selection_auto_pan};
use super::types::{AutoPanOutcome, AutoPanPlan, AutoPanRequest, SelectionAutoPanRequest};

impl AutoPanPlan {
    pub fn apply_to_store(self, store: &mut NodeGraphStore) -> Option<ViewportTransform> {
        store.apply_viewport_pan(self.viewport_pan_request())
    }
}

impl NodeGraphStore {
    /// Applies one auto-pan frame through normal viewport view-state publication.
    pub fn apply_auto_pan(&mut self, request: AutoPanRequest) -> Option<AutoPanOutcome> {
        let interaction = self.resolved_interaction_state();
        let plan = compute_auto_pan(&interaction.auto_pan, request)?;
        let transform = plan.apply_to_store(self)?;
        Some(AutoPanOutcome { plan, transform })
    }

    /// Applies one selection-drag auto-pan frame through normal viewport publication.
    pub fn apply_selection_auto_pan(
        &mut self,
        request: SelectionAutoPanRequest,
    ) -> Option<AutoPanOutcome> {
        let interaction = self.resolved_interaction_state();
        let plan = compute_selection_auto_pan(&interaction.auto_pan, request)?;
        let transform = plan.apply_to_store(self)?;
        Some(AutoPanOutcome { plan, transform })
    }
}
