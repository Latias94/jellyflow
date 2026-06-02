use keyboard_types::Code as KeyCode;

use crate::runtime::delete::DeleteSelectionError;
use crate::runtime::drag::NodeNudgeRequest;
use crate::runtime::store::{DispatchError, DispatchOutcome};

/// High-level keyboard intent handled by the headless runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardIntent {
    /// Delete the current selection without checking a key binding.
    DeleteSelection,
    /// Delete the current selection if the provided key matches the configured binding.
    DeleteSelectionForKey(KeyCode),
    /// Nudge the current selected nodes.
    NudgeSelection(NodeNudgeRequest),
}

/// Whether a keyboard delete action was explicit or key-bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardDeleteAction {
    ExplicitSelectionDelete,
    KeyBoundSelectionDelete(KeyCode),
}

/// Outcome returned by `NodeGraphStore::apply_keyboard_intent`.
#[derive(Debug, Clone)]
pub enum KeyboardActionOutcome {
    DeleteSelection {
        action: KeyboardDeleteAction,
        dispatch: DispatchOutcome,
    },
    NudgeSelection {
        request: NodeNudgeRequest,
        dispatch: DispatchOutcome,
    },
}

/// Error returned when a keyboard intent fails while routing to its runtime action.
#[derive(Debug, thiserror::Error)]
pub enum KeyboardActionError {
    #[error("delete selection keyboard action failed: {0}")]
    DeleteSelection(#[from] DeleteSelectionError),
    #[error("nudge selection keyboard action failed: {0}")]
    NudgeSelection(#[from] DispatchError),
}

impl KeyboardActionOutcome {
    pub fn dispatch(&self) -> &DispatchOutcome {
        match self {
            Self::DeleteSelection { dispatch, .. } | Self::NudgeSelection { dispatch, .. } => {
                dispatch
            }
        }
    }
}
