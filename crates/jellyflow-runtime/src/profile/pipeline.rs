//! Edit pipeline for profiles (apply -> concretize -> validate).
//!
//! This is the headless counterpart of an editor interaction loop. UI code can drive edits by
//! producing `GraphTransaction`s, then passing them through this pipeline to get deterministic
//! derived edits (dynamic ports, autofixes) and diagnostics.

use crate::rules::{ConnectDecision, ConnectPlan, Diagnostic, DiagnosticSeverity};
use jellyflow_core::core::Graph;
use jellyflow_core::ops::{ApplyError, GraphOp, GraphTransaction};

use super::GraphProfile;

#[derive(Debug, thiserror::Error)]
pub enum ApplyPipelineError {
    #[error("failed to apply transaction ops")]
    Apply(#[from] ApplyError),
    #[error("concretization did not converge within bound={bound}")]
    ConcretizeNonConvergent { bound: usize },
    #[error("transaction rejected by diagnostics: {message}")]
    Rejected {
        message: String,
        diagnostics: Vec<Diagnostic>,
    },
}

impl ApplyPipelineError {
    pub fn diagnostics(&self) -> Option<&[Diagnostic]> {
        match self {
            Self::Rejected { diagnostics, .. } => Some(diagnostics),
            _ => None,
        }
    }
}

/// Applies a transaction and runs profile-driven concretization/validation to a fixed point.
///
/// Returns the committed transaction (original ops + derived concretization ops).
pub fn apply_transaction_with_profile(
    graph: &mut Graph,
    profile: &mut dyn GraphProfile,
    tx: &GraphTransaction,
) -> Result<GraphTransaction, ApplyPipelineError> {
    let mut committed = GraphTransaction::new();
    committed.label = tx.label.clone();
    apply_original_transaction(graph, tx, &mut committed)?;

    concretize_to_fixed_point(graph, profile, committed)
}

fn apply_original_transaction(
    graph: &mut Graph,
    tx: &GraphTransaction,
    committed: &mut GraphTransaction,
) -> Result<(), ApplyPipelineError> {
    tx.apply_to(graph)?;
    committed.extend(tx.ops.iter().cloned());
    Ok(())
}

fn concretize_to_fixed_point(
    graph: &mut Graph,
    profile: &mut dyn GraphProfile,
    mut committed: GraphTransaction,
) -> Result<GraphTransaction, ApplyPipelineError> {
    let bound = profile.concretize_bound();
    for _ in 0..bound {
        let derived_ops: Vec<GraphOp> = profile.concretize(graph);
        if derived_ops.is_empty() {
            validate_profile_graph(profile, graph)?;
            return Ok(committed);
        }

        apply_derived_ops(graph, &derived_ops)?;
        committed.extend(derived_ops);
    }

    Err(ApplyPipelineError::ConcretizeNonConvergent { bound })
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
    match plan.decision {
        ConnectDecision::Accept => {
            let tx = GraphTransaction::from_ops(plan.ops.clone());
            apply_transaction_with_profile(graph, profile, &tx)
        }
        ConnectDecision::Reject => Err(rejected_diagnostics(
            plan.diagnostics.clone(),
            "connect rejected",
        )),
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
