use jellyflow_core::core::{Node, NodeId, NodeKindKey};
use jellyflow_core::ops::{GraphOp, GraphTransaction};
use serde_json::Value;

pub(super) struct NodeKindTxWriter {
    tx: GraphTransaction,
}

impl NodeKindTxWriter {
    pub(super) fn new(label: &str) -> Self {
        Self {
            tx: GraphTransaction::new().with_label(label),
        }
    }

    pub(super) fn rewrite_node_kind(
        &mut self,
        id: NodeId,
        node: &Node,
        canonical: &NodeKindKey,
    ) -> bool {
        if canonical == &node.kind {
            return false;
        }

        self.tx.push(GraphOp::SetNodeKind {
            id,
            from: node.kind.clone(),
            to: canonical.clone(),
        });
        true
    }

    pub(super) fn update_node_data(&mut self, id: NodeId, node: &Node, new_data: Value) {
        if node.data == new_data {
            return;
        }

        self.tx.push(GraphOp::SetNodeData {
            id,
            from: node.data.clone(),
            to: new_data,
        });
    }

    pub(super) fn update_node_kind_version(
        &mut self,
        id: NodeId,
        node: &Node,
        latest_kind_version: u32,
    ) {
        self.tx.push(GraphOp::SetNodeKindVersion {
            id,
            from: node.kind_version,
            to: latest_kind_version,
        });
    }

    pub(super) fn into_tx(self) -> GraphTransaction {
        self.tx
    }
}
