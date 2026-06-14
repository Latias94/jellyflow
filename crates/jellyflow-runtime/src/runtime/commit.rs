//! Canonical runtime commit payloads.
//!
//! Store commits are represented as reversible graph transactions. Adapter-specific projections,
//! such as XyFlow-style node/edge changes, are derived outside this module.

use serde::{Deserialize, Deserializer, Serialize};

use jellyflow_core::ops::{GraphMutationFootprint, GraphOp, GraphTransaction};

/// Full-fidelity committed graph patch.
///
/// This is the primary commit payload for controlled integrations. It preserves every
/// `GraphOp`, including ports, groups, sticky notes, imports, symbols, and other resources.
#[derive(Debug, Clone, Default, Serialize)]
pub struct NodeGraphPatch {
    /// Reversible transaction committed by the store.
    transaction: GraphTransaction,
    /// Cached ids touched by `transaction`.
    #[serde(default, skip_serializing_if = "GraphMutationFootprint::is_empty")]
    footprint: GraphMutationFootprint,
}

impl NodeGraphPatch {
    pub fn new(transaction: GraphTransaction) -> Self {
        let footprint = transaction.footprint();
        Self {
            transaction,
            footprint,
        }
    }

    pub fn transaction(&self) -> &GraphTransaction {
        &self.transaction
    }

    pub fn footprint(&self) -> &GraphMutationFootprint {
        &self.footprint
    }

    pub fn into_transaction(self) -> GraphTransaction {
        self.transaction
    }

    pub fn into_parts(self) -> (GraphTransaction, GraphMutationFootprint) {
        (self.transaction, self.footprint)
    }

    pub fn ops(&self) -> &[GraphOp] {
        self.transaction.ops()
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

impl<'de> Deserialize<'de> for NodeGraphPatch {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct NodeGraphPatchWire {
            #[serde(default)]
            transaction: GraphTransaction,
            #[serde(default)]
            footprint: Option<GraphMutationFootprint>,
        }

        let wire = NodeGraphPatchWire::deserialize(deserializer)?;
        let _ = wire.footprint;
        Ok(Self::new(wire.transaction))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jellyflow_core::core::{CanvasPoint, NodeId};

    #[test]
    fn patch_caches_transaction_footprint() {
        let node = NodeId::from_u128(1);
        let tx = GraphTransaction::from_ops([GraphOp::SetNodePos {
            id: node,
            from: CanvasPoint { x: 0.0, y: 0.0 },
            to: CanvasPoint { x: 10.0, y: 20.0 },
        }]);

        let patch = NodeGraphPatch::new(tx.clone());

        assert_eq!(patch.footprint(), &tx.footprint());
        assert!(patch.footprint().nodes.contains(&node));
    }

    #[test]
    fn patch_deserialization_rebuilds_missing_or_stale_footprint() {
        let node = NodeId::from_u128(1);
        let tx = GraphTransaction::from_ops([GraphOp::SetNodePos {
            id: node,
            from: CanvasPoint { x: 0.0, y: 0.0 },
            to: CanvasPoint { x: 10.0, y: 20.0 },
        }]);
        let encoded = serde_json::json!({ "transaction": tx });

        let patch: NodeGraphPatch = serde_json::from_value(encoded).expect("deserialize patch");

        assert!(patch.footprint().nodes.contains(&node));
    }
}
