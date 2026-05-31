use super::{EdgeChange, NodeChange, NodeGraphPatch};

use jellyflow_core::core::{EdgeId, Graph, NodeId};
use jellyflow_core::ops::GraphTransaction;

/// XyFlow-style node/edge projection of a graph patch.
///
/// This adapter is intentionally lossy: it only contains node and edge changes. Use
/// [`crate::runtime::commit::NodeGraphPatch`] when a consumer must observe full graph resources
/// such as ports, groups, sticky notes, imports, or symbols.
#[derive(Debug, Default, Clone)]
pub struct NodeGraphChanges {
    pub nodes: Vec<NodeChange>,
    pub edges: Vec<EdgeChange>,
}

#[derive(Debug, thiserror::Error)]
pub enum ChangesToTransactionError {
    #[error("node not found: {0:?}")]
    MissingNode(NodeId),
    #[error("edge not found: {0:?}")]
    MissingEdge(EdgeId),
}

impl NodeGraphChanges {
    pub fn from_parts(nodes: Vec<NodeChange>, edges: Vec<EdgeChange>) -> Self {
        Self { nodes, edges }
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty() && self.edges.is_empty()
    }

    pub fn nodes(&self) -> &[NodeChange] {
        &self.nodes
    }

    pub fn edges(&self) -> &[EdgeChange] {
        &self.edges
    }

    pub(in crate::runtime::xyflow) fn push_node(&mut self, change: NodeChange) {
        self.nodes.push(change);
    }

    pub(in crate::runtime::xyflow) fn push_edge(&mut self, change: EdgeChange) {
        self.edges.push(change);
    }

    pub fn from_patch(patch: &NodeGraphPatch) -> Self {
        Self::from_transaction(patch.transaction())
    }

    /// Derives change events from a reversible graph transaction.
    ///
    /// This is intended for XyFlow-style callbacks such as "on_nodes_change".
    pub fn from_transaction(tx: &GraphTransaction) -> Self {
        crate::runtime::xyflow::projection::node_graph_changes_from_transaction(tx)
    }

    /// Converts change events into a reversible transaction by looking up "from" values in the
    /// current graph.
    ///
    /// This enables an XyFlow-like runtime to accept `(NodeChange, EdgeChange)` and still keep
    /// `GraphHistory` undo/redo semantics.
    pub fn to_transaction(
        &self,
        graph: &Graph,
    ) -> Result<GraphTransaction, ChangesToTransactionError> {
        crate::runtime::xyflow::transaction::changes_to_transaction(self, graph)
    }
}
