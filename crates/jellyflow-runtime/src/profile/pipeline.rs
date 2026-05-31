//! Edit pipeline for profiles (apply -> concretize -> validate).
//!
//! This is the headless counterpart of an editor interaction loop. UI code can drive edits by
//! producing `GraphTransaction`s, then passing them through this pipeline to get deterministic
//! derived edits (dynamic ports, autofixes) and diagnostics.

use crate::rules::{ConnectPlan, Diagnostic, DiagnosticSeverity};
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

#[cfg(test)]
mod tests {
    use jellyflow_core::core::{CanvasPoint, Graph, Node, NodeId, NodeKindKey, PortId};

    use super::*;

    #[test]
    fn apply_transaction_with_profile_preserves_label_and_appends_derived_ops() {
        let node = NodeId::new();
        let mut graph = Graph::default();
        graph.nodes.insert(node, make_node());

        let tx = GraphTransaction::from_ops([GraphOp::SetNodePos {
            id: node,
            from: CanvasPoint { x: 0.0, y: 0.0 },
            to: CanvasPoint { x: 10.0, y: 20.0 },
        }])
        .with_label("Move");

        let committed =
            apply_transaction_with_profile(&mut graph, &mut OneDerivedOp::new(node), &tx)
                .expect("profile apply");

        assert_eq!(committed.label(), Some("Move"));
        assert_eq!(committed.ops().len(), 2);
        assert!(matches!(committed.ops()[0], GraphOp::SetNodePos { id, .. } if id == node));
        assert!(matches!(
            committed.ops()[1],
            GraphOp::SetNodeHidden {
                id,
                from: false,
                to: true
            } if id == node
        ));
        assert_eq!(
            graph.nodes.get(&node).expect("node").pos,
            CanvasPoint { x: 10.0, y: 20.0 }
        );
        assert!(graph.nodes.get(&node).expect("node").hidden);
    }

    struct OneDerivedOp {
        node: NodeId,
        emitted: bool,
    }

    impl OneDerivedOp {
        fn new(node: NodeId) -> Self {
            Self {
                node,
                emitted: false,
            }
        }
    }

    impl GraphProfile for OneDerivedOp {
        fn type_of_port(
            &mut self,
            _graph: &Graph,
            _port: PortId,
        ) -> Option<jellyflow_core::types::TypeDesc> {
            None
        }

        fn validate_graph(&mut self, _graph: &Graph) -> Vec<Diagnostic> {
            Vec::new()
        }

        fn concretize(&mut self, _graph: &Graph) -> Vec<GraphOp> {
            if self.emitted {
                return Vec::new();
            }

            self.emitted = true;
            vec![GraphOp::SetNodeHidden {
                id: self.node,
                from: false,
                to: true,
            }]
        }
    }

    fn make_node() -> Node {
        Node {
            kind: NodeKindKey::new("demo.node"),
            kind_version: 1,
            pos: CanvasPoint { x: 0.0, y: 0.0 },
            selectable: None,
            draggable: None,
            connectable: None,
            deletable: None,
            parent: None,
            extent: None,
            expand_parent: None,
            size: None,
            hidden: false,
            collapsed: false,
            ports: Vec::new(),
            data: serde_json::Value::Null,
        }
    }
}
