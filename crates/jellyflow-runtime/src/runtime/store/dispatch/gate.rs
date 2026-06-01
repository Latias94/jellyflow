use crate::profile::ApplyPipelineError;
use crate::rules::{Diagnostic, DiagnosticTarget};
use jellyflow_core::ops::{GraphTransaction, normalize_transaction};

pub(super) struct DispatchTransactionGate;

impl DispatchTransactionGate {
    pub(super) fn normalize_and_validate(
        tx: GraphTransaction,
    ) -> Result<GraphTransaction, ApplyPipelineError> {
        let tx = normalize_transaction(tx);
        Self::validate(&tx)?;
        Ok(tx)
    }

    fn validate(tx: &GraphTransaction) -> Result<(), ApplyPipelineError> {
        if let Some((key, message)) = jellyflow_core::ops::find_non_finite_in_tx(tx) {
            return Err(Self::reject(key, message));
        }
        if let Some((key, message)) = jellyflow_core::ops::find_invalid_size_in_tx(tx) {
            return Err(Self::reject(key, message));
        }
        Ok(())
    }

    fn reject(key: String, message: String) -> ApplyPipelineError {
        ApplyPipelineError::Rejected {
            message: message.clone(),
            diagnostics: vec![Diagnostic::error(key, DiagnosticTarget::Graph, message)],
        }
    }
}
