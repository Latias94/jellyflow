use crate::io::NodeGraphKeyCode;
use crate::runtime::selection::SelectionBoxInput;
use keyboard_types::Code as KeyCode;

use super::ConformanceAction;

impl ConformanceAction {
    pub fn apply_selection_box(input: SelectionBoxInput) -> Self {
        Self::ApplySelectionBox { input }
    }

    pub fn apply_delete_selection() -> Self {
        Self::ApplyDeleteSelection
    }

    pub fn apply_delete_selection_for_key(key: KeyCode) -> Self {
        Self::ApplyDeleteSelectionForKey {
            key: NodeGraphKeyCode(key),
        }
    }
}
