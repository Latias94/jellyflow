use crate::graph_kernel::apply::ApplyError;
use crate::graph_kernel::transaction::GraphOp;
use jellyflow_core::core::Graph;

/// A batch of edit operations that should be applied and undone as one unit.
#[derive(Debug, Clone, Default)]
pub struct GraphTransaction {
    /// Optional human-readable label for history UI.
    label: Option<String>,
    /// Operations in order.
    ops: Vec<GraphOp>,
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

    /// Creates a transaction from metadata and ops.
    pub fn from_parts(label: Option<String>, ops: impl IntoIterator<Item = GraphOp>) -> Self {
        Self {
            label,
            ops: ops.into_iter().collect(),
        }
    }

    /// Sets a label.
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Adds ops in order and returns this transaction.
    pub fn with_ops(mut self, ops: impl IntoIterator<Item = GraphOp>) -> Self {
        self.extend(ops);
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

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// Returns the optional human-readable label.
    #[cfg(test)]
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    /// Returns ops in order.
    pub fn ops(&self) -> &[GraphOp] {
        &self.ops
    }

    /// Consumes this transaction and returns its ops in order.
    pub fn into_ops(self) -> Vec<GraphOp> {
        self.ops
    }

    /// Consumes this transaction and returns its metadata and ops.
    pub fn into_parts(self) -> (Option<String>, Vec<GraphOp>) {
        let Self { label, ops } = self;
        (label, ops)
    }

    /// Applies this transaction atomically to `graph`.
    pub fn apply_to(&self, graph: &mut Graph) -> Result<(), ApplyError> {
        crate::graph_kernel::apply::apply_transaction(graph, self)
    }

    /// Applies an already-validated batch without a whole-graph storage scan.
    ///
    /// Callers must establish that the ordered operations preserve graph storage invariants. The
    /// batch remains atomic if an individual operation fails.
    pub fn apply_prevalidated_batch(&self, graph: &mut Graph) -> Result<(), ApplyError> {
        crate::graph_kernel::apply::apply_prevalidated_transaction_batch(graph, self)
    }

    /// Builds the inverse transaction for undo/redo.
    pub fn inverse(&self) -> Self {
        crate::graph_kernel::history::invert_transaction(self)
    }
}
