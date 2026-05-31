use serde::{Deserialize, Serialize};

use jellyflow_core::core::{EdgeId, NodeId, PortId, SymbolId};
use jellyflow_core::ops::GraphOp;

/// Diagnostic severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    /// Informational note.
    Info,
    /// Warning.
    Warning,
    /// Error.
    Error,
}

/// Diagnostic target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DiagnosticTarget {
    /// Graph-level diagnostic.
    Graph,
    /// Node-level diagnostic.
    Node { id: NodeId },
    /// Port-level diagnostic.
    Port { id: PortId },
    /// Edge-level diagnostic.
    Edge { id: EdgeId },
    /// Symbol-level diagnostic.
    Symbol { id: SymbolId },
}

/// A diagnostic produced by validation or connection planning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    /// Stable machine key (for filtering/suppression).
    pub key: String,
    /// Severity.
    pub severity: DiagnosticSeverity,
    /// Target element.
    pub target: DiagnosticTarget,
    /// Human-readable message.
    pub message: String,
    /// Optional suggested fixes expressed as graph ops.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fixes: Vec<GraphOp>,
}
