use jellyflow_core::core::{NodeId, NodeKindKey};
use jellyflow_core::ops::GraphTransaction;

#[derive(Debug, Clone)]
pub struct NodeKindRewrite {
    pub node: NodeId,
    pub from: NodeKindKey,
    pub to: NodeKindKey,
}

impl NodeKindRewrite {
    pub fn node(&self) -> NodeId {
        self.node
    }

    pub fn from(&self) -> &NodeKindKey {
        &self.from
    }

    pub fn to(&self) -> &NodeKindKey {
        &self.to
    }
}

#[derive(Debug, Clone)]
pub struct CanonicalizeKindsPlan {
    pub tx: GraphTransaction,
    pub rewrites: Vec<NodeKindRewrite>,
}

impl CanonicalizeKindsPlan {
    pub(in crate::schema) fn from_parts(
        tx: GraphTransaction,
        rewrites: Vec<NodeKindRewrite>,
    ) -> Self {
        Self { tx, rewrites }
    }

    pub fn transaction(&self) -> &GraphTransaction {
        &self.tx
    }

    pub fn rewrites(&self) -> &[NodeKindRewrite] {
        &self.rewrites
    }

    pub fn into_parts(self) -> (GraphTransaction, Vec<NodeKindRewrite>) {
        (self.tx, self.rewrites)
    }
}
