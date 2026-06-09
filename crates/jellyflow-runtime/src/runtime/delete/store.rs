use keyboard_types::Code as KeyCode;

use crate::rules::DeletePlan;
use crate::runtime::store::{DispatchOutcome, NodeGraphStore};

use super::planner::{
    delete_selection_transaction_from_plan, plan_delete_elements, plan_delete_selection,
    plan_delete_selection_for_key, prepare_delete_selection, prepare_delete_selection_for_key,
};
use super::types::{DeleteElements, DeleteSelectionError, PreDeleteRequest, PreDeleteResolution};

impl NodeGraphStore {
    /// Plans deletion for the store's current node/edge selection.
    pub fn plan_delete_selection(&self) -> DeletePlan {
        let interaction = self.resolved_interaction_state();
        plan_delete_selection(self.graph(), self.view_state(), &interaction)
    }

    /// Plans deletion for the current selection when the configured delete key matches.
    pub fn plan_delete_selection_for_key(&self, key: KeyCode) -> Option<DeletePlan> {
        let interaction = self.resolved_interaction_state();
        plan_delete_selection_for_key(self.graph(), self.view_state(), &interaction, key)
    }

    /// Plans deletion for explicit node/edge ids through the store's resolved interaction policy.
    pub fn plan_delete_elements(&self, elements: &DeleteElements) -> DeletePlan {
        let interaction = self.resolved_interaction_state();
        plan_delete_elements(self.graph(), &interaction, elements)
    }

    /// Prepares the current selection for an adapter-owned async pre-delete hook.
    pub fn prepare_delete_selection(
        &self,
    ) -> Result<Option<PreDeleteRequest>, DeleteSelectionError> {
        let interaction = self.resolved_interaction_state();
        prepare_delete_selection(self.graph(), self.view_state(), &interaction)
    }

    /// Prepares a key-gated selection delete for an adapter-owned async pre-delete hook.
    pub fn prepare_delete_selection_for_key(
        &self,
        key: KeyCode,
    ) -> Result<Option<PreDeleteRequest>, DeleteSelectionError> {
        let interaction = self.resolved_interaction_state();
        prepare_delete_selection_for_key(self.graph(), self.view_state(), &interaction, key)
    }

    /// Commits deletion for the current node/edge selection through normal store dispatch.
    pub fn apply_delete_selection(
        &mut self,
    ) -> Result<Option<DispatchOutcome>, DeleteSelectionError> {
        let plan = self.plan_delete_selection();
        self.apply_delete_selection_plan(plan)
    }

    /// Commits deletion for the current selection when the configured delete key matches.
    pub fn apply_delete_selection_for_key(
        &mut self,
        key: KeyCode,
    ) -> Result<Option<DispatchOutcome>, DeleteSelectionError> {
        let Some(plan) = self.plan_delete_selection_for_key(key) else {
            return Ok(None);
        };

        self.apply_delete_selection_plan(plan)
    }

    /// Applies an adapter-owned pre-delete hook result through normal delete policy and dispatch.
    pub fn apply_pre_delete_resolution(
        &mut self,
        request: &PreDeleteRequest,
        resolution: PreDeleteResolution,
    ) -> Result<Option<DispatchOutcome>, DeleteSelectionError> {
        let elements = match resolution {
            PreDeleteResolution::Accept => request.planned().clone(),
            PreDeleteResolution::Veto => return Ok(None),
            PreDeleteResolution::Replace { elements } => elements,
        };

        self.apply_delete_elements(elements)
    }

    /// Commits explicit node/edge ids through normal delete policy and store dispatch.
    pub fn apply_delete_elements(
        &mut self,
        elements: DeleteElements,
    ) -> Result<Option<DispatchOutcome>, DeleteSelectionError> {
        let plan = self.plan_delete_elements(&elements);
        self.apply_delete_selection_plan(plan)
    }

    fn apply_delete_selection_plan(
        &mut self,
        plan: DeletePlan,
    ) -> Result<Option<DispatchOutcome>, DeleteSelectionError> {
        if plan.is_reject() {
            return Err(DeleteSelectionError::Rejected {
                diagnostics: plan.diagnostics,
            });
        }

        let Some(transaction) = delete_selection_transaction_from_plan(plan) else {
            return Ok(None);
        };

        self.dispatch_transaction(&transaction)
            .map(Some)
            .map_err(DeleteSelectionError::from)
    }
}
