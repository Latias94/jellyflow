use crate::rules::Diagnostic;
use crate::runtime::store::DispatchError;

/// Default transaction label used for committed delete-selection updates.
pub const DELETE_SELECTION_TRANSACTION_LABEL: &str = "delete selection";

/// Error returned when a delete-selection request could not be committed.
#[derive(Debug, thiserror::Error)]
pub enum DeleteSelectionError {
    /// Rules rejected the selected elements.
    #[error("delete selection was rejected")]
    Rejected {
        /// Diagnostics produced by the delete rules.
        diagnostics: Vec<Diagnostic>,
    },
    /// Store dispatch failed after rules accepted the delete plan.
    #[error(transparent)]
    Dispatch(#[from] DispatchError),
}

impl DeleteSelectionError {
    /// Returns rule diagnostics when the request was rejected by delete policy.
    pub fn diagnostics(&self) -> Option<&[Diagnostic]> {
        match self {
            Self::Rejected { diagnostics } => Some(diagnostics),
            Self::Dispatch(_) => None,
        }
    }
}
