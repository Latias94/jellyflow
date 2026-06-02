use crate::runtime::drag::NodeNudgeRequest;
use crate::runtime::store::NodeGraphStore;

use super::types::{
    KeyboardActionError, KeyboardActionOutcome, KeyboardDeleteAction, KeyboardIntent,
};

impl NodeGraphStore {
    /// Applies a normalized keyboard intent through the store's headless runtime helpers.
    pub fn apply_keyboard_intent(
        &mut self,
        intent: KeyboardIntent,
    ) -> Result<Option<KeyboardActionOutcome>, KeyboardActionError> {
        match intent {
            KeyboardIntent::DeleteSelection => self
                .apply_delete_selection()
                .map(|result| {
                    result.map(|dispatch| KeyboardActionOutcome::DeleteSelection {
                        action: KeyboardDeleteAction::ExplicitSelectionDelete,
                        dispatch,
                    })
                })
                .map_err(KeyboardActionError::from),
            KeyboardIntent::DeleteSelectionForKey(key) => self
                .apply_delete_selection_for_key(key)
                .map(|result| {
                    result.map(|dispatch| KeyboardActionOutcome::DeleteSelection {
                        action: KeyboardDeleteAction::KeyBoundSelectionDelete(key),
                        dispatch,
                    })
                })
                .map_err(KeyboardActionError::from),
            KeyboardIntent::NudgeSelection(request) => {
                apply_keyboard_nudge(self, request).map(KeyboardActionOutcome::from_nudge)
            }
        }
    }
}

fn apply_keyboard_nudge(
    store: &mut NodeGraphStore,
    request: NodeNudgeRequest,
) -> Result<Option<(NodeNudgeRequest, crate::runtime::store::DispatchOutcome)>, KeyboardActionError>
{
    store
        .apply_node_nudge(request)
        .map(|result| result.map(|dispatch| (request, dispatch)))
        .map_err(KeyboardActionError::from)
}

impl KeyboardActionOutcome {
    fn from_nudge(
        value: Option<(NodeNudgeRequest, crate::runtime::store::DispatchOutcome)>,
    ) -> Option<Self> {
        value.map(|(request, dispatch)| Self::NudgeSelection { request, dispatch })
    }
}
