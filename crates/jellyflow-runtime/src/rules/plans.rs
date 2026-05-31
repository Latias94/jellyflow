use serde::{Deserialize, Serialize};

use jellyflow_core::ops::GraphOp;

use super::{Diagnostic, DiagnosticTarget};

/// Connection decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectDecision {
    /// Accept the connection.
    Accept,
    /// Reject the connection.
    Reject,
}

/// Delete decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeleteDecision {
    /// Accept the deletion.
    Accept,
    /// Reject the deletion.
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
    /// Creates an accepted plan with no side effects.
    pub fn accept() -> Self {
        Self {
            decision: ConnectDecision::Accept,
            diagnostics: Vec::new(),
            ops: Vec::new(),
        }
    }

    /// Creates an accepted plan with planned connection ops.
    pub fn from_ops(ops: Vec<GraphOp>) -> Self {
        Self {
            decision: ConnectDecision::Accept,
            diagnostics: Vec::new(),
            ops,
        }
    }

    /// Creates a rejected plan with a single error diagnostic.
    pub fn reject(message: impl Into<String>) -> Self {
        Self {
            decision: ConnectDecision::Reject,
            diagnostics: vec![Diagnostic::error(
                "connect.rejected",
                DiagnosticTarget::Graph,
                message,
            )],
            ops: Vec::new(),
        }
    }
}

/// A rules-driven plan for deleting graph elements.
///
/// Delete planning is atomic: if any explicitly requested element is missing or not deletable under
/// the effective interaction policy, the plan is rejected and contains no ops. Edges that are
/// removed as a consequence of deleting a node are treated as cascaded consistency edits rather
/// than separate direct edge deletions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeletePlan {
    /// Decision.
    pub decision: DeleteDecision,
    /// Diagnostics explaining the decision.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<Diagnostic>,
    /// Optional edits to apply if accepted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ops: Vec<GraphOp>,
}

impl DeletePlan {
    /// Creates an accepted plan with no side effects.
    pub fn accept() -> Self {
        Self {
            decision: DeleteDecision::Accept,
            diagnostics: Vec::new(),
            ops: Vec::new(),
        }
    }

    /// Creates an accepted plan with planned delete ops.
    pub fn from_ops(ops: Vec<GraphOp>) -> Self {
        Self {
            decision: DeleteDecision::Accept,
            diagnostics: Vec::new(),
            ops,
        }
    }

    /// Creates a rejected plan with a single graph-level error diagnostic.
    pub fn reject(message: impl Into<String>) -> Self {
        Self {
            decision: DeleteDecision::Reject,
            diagnostics: vec![Diagnostic::error(
                "delete.rejected",
                DiagnosticTarget::Graph,
                message,
            )],
            ops: Vec::new(),
        }
    }
}
