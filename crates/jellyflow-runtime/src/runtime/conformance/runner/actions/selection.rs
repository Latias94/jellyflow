use crate::io::NodeGraphKeyCode;
use crate::runtime::keyboard::KeyboardIntent;
use crate::runtime::selection::SelectionBoxInput;
use crate::runtime::store::NodeGraphStore;

use super::super::super::scenario::ConformanceAction;
use super::require_commit;

pub(super) fn execute_action(
    store: &mut NodeGraphStore,
    action: &ConformanceAction,
) -> Option<Result<(), String>> {
    Some(match action {
        ConformanceAction::ApplySelectionBox { input } => {
            apply_selection_box(store, *input);
            Ok(())
        }
        ConformanceAction::ApplyDeleteSelection => apply_delete_selection(store),
        ConformanceAction::ApplyDeleteSelectionForKey { key } => {
            apply_delete_selection_for_key(store, *key)
        }
        _ => return None,
    })
}

pub(super) fn apply_selection_box(store: &mut NodeGraphStore, input: SelectionBoxInput) {
    store.apply_selection_box(input);
}

pub(super) fn apply_delete_selection(store: &mut NodeGraphStore) -> Result<(), String> {
    require_commit(
        store.apply_keyboard_intent(KeyboardIntent::DeleteSelection),
        "apply_delete_selection",
    )
}

pub(super) fn apply_delete_selection_for_key(
    store: &mut NodeGraphStore,
    key: NodeGraphKeyCode,
) -> Result<(), String> {
    require_commit(
        store.apply_keyboard_intent(KeyboardIntent::DeleteSelectionForKey(key.0)),
        "apply_delete_selection_for_key",
    )
}
