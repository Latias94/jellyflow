use crate::rules::{ConnectPlan, Diagnostic, DiagnosticSeverity};
use jellyflow_core::core::Graph;
use jellyflow_core::ops::{GraphOp, GraphTransaction};

use super::super::GraphProfile;
use super::error::ApplyPipelineError;

/// Applies a transaction and runs profile-driven concretization/validation to a fixed point.
///
/// Returns the committed transaction (original ops + derived concretization ops).
pub fn apply_transaction_with_profile(
    graph: &mut Graph,
    profile: &mut dyn GraphProfile,
    tx: &GraphTransaction,
) -> Result<GraphTransaction, ApplyPipelineError> {
    let mut scratch = graph.clone();
    let committed = apply_transaction_with_profile_in_place(&mut scratch, profile, tx)?;
    *graph = scratch;
    Ok(committed)
}

pub(crate) fn apply_transaction_with_profile_in_place(
    graph: &mut Graph,
    profile: &mut dyn GraphProfile,
    tx: &GraphTransaction,
) -> Result<GraphTransaction, ApplyPipelineError> {
    let mut committed = CommittedTransactionBuilder::new(tx);
    apply_original_transaction(graph, tx, &mut committed)?;

    concretize_to_fixed_point(graph, profile, committed)
}

fn apply_original_transaction(
    graph: &mut Graph,
    tx: &GraphTransaction,
    committed: &mut CommittedTransactionBuilder,
) -> Result<(), ApplyPipelineError> {
    tx.apply_to(graph)?;
    committed.extend_original(tx);
    Ok(())
}

fn concretize_to_fixed_point(
    graph: &mut Graph,
    profile: &mut dyn GraphProfile,
    mut committed: CommittedTransactionBuilder,
) -> Result<GraphTransaction, ApplyPipelineError> {
    let bound = profile.concretize_bound();
    for _ in 0..bound {
        let derived_ops: Vec<GraphOp> = profile.concretize(graph);
        if derived_ops.is_empty() {
            validate_profile_graph(profile, graph)?;
            return Ok(committed.finish());
        }

        apply_derived_ops(graph, &derived_ops)?;
        committed.extend_derived(derived_ops);
    }

    Err(ApplyPipelineError::ConcretizeNonConvergent { bound })
}

struct CommittedTransactionBuilder {
    tx: GraphTransaction,
}

impl CommittedTransactionBuilder {
    fn new(source: &GraphTransaction) -> Self {
        Self {
            tx: GraphTransaction::new().with_optional_label(source.label().map(str::to_owned)),
        }
    }

    fn extend_original(&mut self, source: &GraphTransaction) {
        self.tx.extend(source.ops().iter().cloned());
    }

    fn extend_derived(&mut self, ops: impl IntoIterator<Item = GraphOp>) {
        self.tx.extend(ops);
    }

    fn finish(self) -> GraphTransaction {
        self.tx
    }
}

fn apply_derived_ops(graph: &mut Graph, ops: &[GraphOp]) -> Result<(), ApplyPipelineError> {
    let derived_tx = GraphTransaction::from_ops(ops.iter().cloned());
    derived_tx.apply_to(graph)?;
    Ok(())
}

fn validate_profile_graph(
    profile: &mut dyn GraphProfile,
    graph: &Graph,
) -> Result<(), ApplyPipelineError> {
    let diagnostics = profile.validate_graph(graph);
    if diagnostics
        .iter()
        .any(|d| d.severity == DiagnosticSeverity::Error)
    {
        return Err(rejected_diagnostics(diagnostics, "transaction rejected"));
    }
    Ok(())
}

/// Helper for UI loops: apply a `ConnectPlan` as a transaction via the profile pipeline.
pub fn apply_connect_plan_with_profile(
    graph: &mut Graph,
    profile: &mut dyn GraphProfile,
    plan: &ConnectPlan,
) -> Result<GraphTransaction, ApplyPipelineError> {
    if plan.is_accept() {
        let tx = GraphTransaction::from_ops(plan.ops().iter().cloned());
        apply_transaction_with_profile(graph, profile, &tx)
    } else {
        Err(rejected_diagnostics(
            plan.diagnostics().to_vec(),
            "connect rejected",
        ))
    }
}

fn rejected_diagnostics(
    diagnostics: Vec<Diagnostic>,
    fallback_message: &str,
) -> ApplyPipelineError {
    let message = diagnostics
        .first()
        .map(|d| d.message.clone())
        .unwrap_or_else(|| fallback_message.to_string());
    ApplyPipelineError::Rejected {
        message,
        diagnostics,
    }
}
