//! Store helpers for XyFlow-style change arrays.

use crate::runtime::store::{DispatchError, DispatchOutcome, NodeGraphStore};
use crate::runtime::xyflow::changes::{ChangesToTransactionError, NodeGraphChanges};

#[derive(Debug, thiserror::Error)]
pub enum DispatchChangesError {
    #[error(transparent)]
    Changes(#[from] ChangesToTransactionError),
    #[error(transparent)]
    Dispatch(#[from] DispatchError),
}

impl NodeGraphStore {
    /// Applies XyFlow-style changes by converting them to a reversible transaction.
    pub fn dispatch_changes(
        &mut self,
        changes: &NodeGraphChanges,
    ) -> Result<DispatchOutcome, DispatchChangesError> {
        let tx = changes.to_transaction(self.graph())?;
        Ok(self.dispatch_transaction(&tx)?)
    }
}
