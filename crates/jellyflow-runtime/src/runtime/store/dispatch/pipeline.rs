use crate::profile::ApplyPipelineError;
use crate::rules::{Diagnostic, DiagnosticSeverity, DiagnosticTarget};
use jellyflow_core::core::Graph;
use jellyflow_core::ops::{GraphTransaction, normalize_transaction};

use super::super::{NodeGraphStore, dispatch_profile::DispatchProfile};

pub(super) struct DispatchPipeline<'store, 'profile> {
    store: &'store mut NodeGraphStore,
    dispatch_profile: DispatchProfile<'profile>,
}

pub(super) enum DispatchPipelineResult {
    Empty(GraphTransaction),
    Commit {
        graph: Graph,
        committed: GraphTransaction,
    },
}

impl<'store, 'profile> DispatchPipeline<'store, 'profile> {
    pub(super) fn new(
        store: &'store mut NodeGraphStore,
        dispatch_profile: DispatchProfile<'profile>,
    ) -> Self {
        Self {
            store,
            dispatch_profile,
        }
    }

    pub(super) fn run(
        mut self,
        tx: &GraphTransaction,
    ) -> Result<DispatchPipelineResult, ApplyPipelineError> {
        let mut tx = Self::normalize_and_validate(tx.clone())?;
        if tx.is_empty() {
            return Ok(DispatchPipelineResult::Empty(tx));
        }

        self.store.run_before_dispatch_middleware(&mut tx)?;
        tx = Self::normalize_and_validate(tx)?;
        if tx.is_empty() {
            return Ok(DispatchPipelineResult::Empty(tx));
        }

        let (graph, committed) = self.apply_to_scratch(&tx)?;
        let committed = Self::normalize_and_validate(committed)?;
        Ok(DispatchPipelineResult::Commit { graph, committed })
    }

    fn apply_to_scratch(
        &mut self,
        tx: &GraphTransaction,
    ) -> Result<(Graph, GraphTransaction), ApplyPipelineError> {
        let mut scratch = self.store.graph.clone();
        let committed = self
            .dispatch_profile
            .apply_to_graph(self.store, &mut scratch, tx)?;
        Ok((scratch, committed))
    }

    fn normalize_and_validate(
        tx: GraphTransaction,
    ) -> Result<GraphTransaction, ApplyPipelineError> {
        let tx = normalize_transaction(tx);
        Self::validate_transaction(&tx)?;
        Ok(tx)
    }

    fn validate_transaction(tx: &GraphTransaction) -> Result<(), ApplyPipelineError> {
        if let Some((key, message)) = jellyflow_core::ops::find_non_finite_in_tx(tx) {
            return Err(Self::reject_tx(key, message));
        }
        if let Some((key, message)) = jellyflow_core::ops::find_invalid_size_in_tx(tx) {
            return Err(Self::reject_tx(key, message));
        }
        Ok(())
    }

    fn reject_tx(key: String, message: String) -> ApplyPipelineError {
        ApplyPipelineError::Rejected {
            message: message.clone(),
            diagnostics: vec![Diagnostic {
                key,
                severity: DiagnosticSeverity::Error,
                target: DiagnosticTarget::Graph,
                message,
                fixes: Vec::new(),
            }],
        }
    }
}
