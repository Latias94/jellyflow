use crate::runtime::store::{DispatchError, DispatchOutcome, NodeGraphStore};

use super::planner::{plan_node_drag, plan_node_nudge};
use super::types::{NodeDragPlan, NodeDragRequest, NodeNudgePlan, NodeNudgeRequest};

impl NodeGraphStore {
    /// Plans a node drag update against the store's current selection and interaction state.
    pub fn plan_node_drag(&self, request: NodeDragRequest) -> Option<NodeDragPlan> {
        let interaction = self.resolved_interaction_state();
        plan_node_drag(self.graph(), self.view_state(), &interaction, request)
    }

    /// Plans a keyboard nudge update against the store's current selection and interaction state.
    pub fn plan_node_nudge(&self, request: NodeNudgeRequest) -> Option<NodeNudgePlan> {
        let interaction = self.resolved_interaction_state();
        plan_node_nudge(self.graph(), self.view_state(), &interaction, request)
    }

    /// Commits a node drag update through the normal store dispatch path.
    ///
    /// This records normal graph history for the committed update. Higher-level drag sessions that
    /// need preview/final-commit semantics should build on top of the planning API.
    pub fn apply_node_drag(
        &mut self,
        request: NodeDragRequest,
    ) -> Result<Option<DispatchOutcome>, DispatchError> {
        let Some(plan) = self.plan_node_drag(request) else {
            return Ok(None);
        };
        self.dispatch_transaction(plan.transaction()).map(Some)
    }

    /// Commits a keyboard nudge update through the normal store dispatch path.
    pub fn apply_node_nudge(
        &mut self,
        request: NodeNudgeRequest,
    ) -> Result<Option<DispatchOutcome>, DispatchError> {
        let Some(plan) = self.plan_node_nudge(request) else {
            return Ok(None);
        };
        self.dispatch_transaction(plan.transaction()).map(Some)
    }
}
