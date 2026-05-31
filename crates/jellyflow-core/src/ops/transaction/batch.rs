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

    /// Creates an unlabeled transaction from ops.
    pub fn from_ops(ops: impl IntoIterator<Item = GraphOp>) -> Self {
        Self::new().with_ops(ops)
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

    /// Sets or clears the optional label.
    pub fn with_optional_label(mut self, label: Option<String>) -> Self {
        self.label = label;
        self
    }

    /// Adds ops in order and returns this transaction.
    pub fn with_ops(mut self, ops: impl IntoIterator<Item = GraphOp>) -> Self {
        self.extend(ops);
        self
    }

    /// Transforms this transaction's ops while preserving metadata.
    pub fn map_ops(mut self, f: impl FnOnce(Vec<GraphOp>) -> Vec<GraphOp>) -> Self {
        self.ops = f(self.ops);
        self
    }

    /// Pushes an op.
    pub fn push(&mut self, op: GraphOp) {
        self.ops.push(op);
    }

    /// Extends this transaction with ops in order.
    pub fn extend(&mut self, ops: impl IntoIterator<Item = GraphOp>) {
        self.ops.extend(ops);
    }

    /// Retains ops that match `f`.
    pub fn retain_ops(&mut self, f: impl FnMut(&GraphOp) -> bool) {
        self.ops.retain(f);
    }

    /// Removes all ops while preserving transaction metadata.
    pub fn clear_ops(&mut self) {
        self.ops.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// Returns the optional human-readable label.
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    /// Returns ops in order.
    pub fn ops(&self) -> &[GraphOp] {
        &self.ops
    }

    /// Returns the number of ops.
    pub fn len(&self) -> usize {
        self.ops.len()
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
