use serde::{Deserialize, Serialize};

use crate::graph_kernel::GraphOp;

use super::Diagnostic;

/// Connection decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectDecision {
    /// Accept the connection.
    Accept,
    /// Reject the connection.
    Reject,
}

/// Which endpoint of an existing edge is being reconnected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeEndpoint {
    /// The source endpoint (`edge.from`).
    From,
    /// The target endpoint (`edge.to`).
    To,
}

/// A rules-driven plan for connecting two ports.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectPlan {
    /// Decision.
    pub decision: ConnectDecision,
    /// Diagnostics explaining the decision.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<Diagnostic>,
    /// Optional edits to apply if accepted (disconnect existing edges, insert conversion nodes, etc.).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ops: Vec<GraphOp>,
}

impl ConnectPlan {
    pub fn is_reject(&self) -> bool {
        self.decision == ConnectDecision::Reject
    }

    pub fn into_ops(self) -> Vec<GraphOp> {
        self.ops
    }

    /// Creates an accepted plan with no side effects.
    pub fn accept() -> Self {
        Self {
            decision: ConnectDecision::Accept,
            diagnostics: Vec::new(),
            ops: Vec::new(),
        }
    }

    /// Creates an accepted plan with planned connection ops.
    pub fn from_ops(ops: impl IntoIterator<Item = GraphOp>) -> Self {
        Self {
            decision: ConnectDecision::Accept,
            diagnostics: Vec::new(),
            ops: ops.into_iter().collect(),
        }
    }

    pub fn reject_with_diagnostic(diagnostic: Diagnostic) -> Self {
        Self::reject_with_diagnostics(vec![diagnostic])
    }

    pub fn reject_with_diagnostics(diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            decision: ConnectDecision::Reject,
            diagnostics,
            ops: Vec::new(),
        }
    }
}
