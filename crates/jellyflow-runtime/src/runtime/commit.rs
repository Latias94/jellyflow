//! Canonical runtime commit payloads.
//!
//! Store commits are represented as reversible graph transactions. Adapter-specific projections,
//! such as XyFlow-style node/edge changes, are derived outside this module.

use serde::{Deserialize, Serialize};

use jellyflow_core::ops::{GraphOp, GraphTransaction};

/// Full-fidelity committed graph patch.
///
/// This is the primary commit payload for controlled integrations. It preserves every
/// `GraphOp`, including ports, groups, sticky notes, imports, symbols, and other resources.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NodeGraphPatch {
    /// Reversible transaction committed by the store.
    pub transaction: GraphTransaction,
}

impl NodeGraphPatch {
    pub fn new(transaction: GraphTransaction) -> Self {
        Self { transaction }
    }

    pub fn transaction(&self) -> &GraphTransaction {
        &self.transaction
    }

    pub fn into_transaction(self) -> GraphTransaction {
        self.transaction
    }

    pub fn ops(&self) -> &[GraphOp] {
        &self.transaction.ops
    }

    pub fn is_empty(&self) -> bool {
        self.transaction.is_empty()
    }
}

impl From<GraphTransaction> for NodeGraphPatch {
    fn from(transaction: GraphTransaction) -> Self {
        Self::new(transaction)
    }
}
