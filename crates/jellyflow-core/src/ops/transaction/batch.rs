use serde::{Deserialize, Serialize};

use crate::core::Graph;
use crate::ops::apply::ApplyError;
use crate::ops::transaction::GraphOp;

/// A batch of edit operations that should be applied and undone as one unit.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GraphTransaction {
    /// Optional human-readable label for history UI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Operations in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ops: Vec<GraphOp>,
}

impl GraphTransaction {
    /// Creates an empty transaction.
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds a deterministic transaction that transforms `from` into `to`.
    pub fn diff(from: &Graph, to: &Graph) -> Self {
        crate::ops::diff::graph_diff(from, to)
    }

    /// Sets a label.
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Pushes an op.
    pub fn push(&mut self, op: GraphOp) {
        self.ops.push(op);
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// Applies this transaction atomically to `graph`.
    pub fn apply_to(&self, graph: &mut Graph) -> Result<(), ApplyError> {
        crate::ops::apply::apply_transaction(graph, self)
    }

    /// Builds the inverse transaction for undo/redo.
    pub fn inverse(&self) -> Self {
        crate::ops::history::invert_transaction(self)
    }
}
