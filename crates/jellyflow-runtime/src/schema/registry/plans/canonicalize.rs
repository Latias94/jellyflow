use jellyflow_core::core::{Node, NodeId, NodeKindKey};

use crate::schema::migration::{CanonicalizeKindsPlan, NodeKindRewrite};

use super::writer::NodeKindTxWriter;

pub(super) struct CanonicalizeKindsPlanner {
    tx: NodeKindTxWriter,
    rewrites: Vec<NodeKindRewrite>,
}

impl CanonicalizeKindsPlanner {
    pub(super) fn new() -> Self {
        Self {
            tx: NodeKindTxWriter::new("Canonicalize node kinds"),
            rewrites: Vec::new(),
        }
    }

    pub(super) fn rewrite_node_kind(&mut self, id: NodeId, node: &Node, canonical: &NodeKindKey) {
        if !self.tx.rewrite_node_kind(id, node, canonical) {
            return;
        }

        self.rewrites.push(NodeKindRewrite {
            node: id,
            from: node.kind.clone(),
            to: canonical.clone(),
        });
    }

    pub(super) fn finish(self) -> CanonicalizeKindsPlan {
        CanonicalizeKindsPlan::from_parts(self.tx.into_tx(), self.rewrites)
    }
}
