use crate::graph_kernel::{
    GraphTransaction, find_invalid_size_in_tx, find_non_finite_in_tx, normalize_transaction,
};
use crate::profile::ApplyPipelineError;
use crate::rules::{Diagnostic, DiagnosticTarget};

pub(crate) struct GraphTransactionGate;

impl GraphTransactionGate {
    pub(crate) fn normalize_and_validate(
        transaction: GraphTransaction,
    ) -> Result<GraphTransaction, ApplyPipelineError> {
        let transaction = normalize_transaction(transaction);
        Self::validate(&transaction)?;
        Ok(transaction)
    }

    pub(crate) fn validate(transaction: &GraphTransaction) -> Result<(), ApplyPipelineError> {
        if let Some((key, message)) = find_non_finite_in_tx(transaction) {
            return Err(Self::reject(key, message));
        }
        if let Some((key, message)) = find_invalid_size_in_tx(transaction) {
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
