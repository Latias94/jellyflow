use crate::core::{EdgeId, Graph, GroupId, NodeId, PortId};
use crate::ops::{GraphMutationPlanner, GraphOp, GraphTransaction};

/// Builder helpers for constructing safe, reversible edit operations.
///
/// These helpers:
/// - snapshot the removed data needed for undo,
/// - produce ops in a valid apply order,
/// - use deterministic ordering for stability.
pub trait GraphOpBuilderExt {
    /// Builds a `RemoveEdge` op for an existing edge.
    fn build_remove_edge_op(&self, id: EdgeId) -> Option<GraphOp>;

    /// Builds a `RemovePort` op for an existing port, including incident edges.
    fn build_remove_port_op(&self, id: PortId) -> Option<GraphOp>;

    /// Builds ops that disconnect all edges incident to the port.
    ///
    /// The ops are returned in deterministic order.
    fn build_disconnect_port_ops(&self, id: PortId) -> Option<Vec<GraphOp>>;

    /// Builds a transaction that removes a port (and its incident edges).
    fn build_remove_port_tx(
        &self,
        id: PortId,
        label: impl Into<String>,
    ) -> Option<GraphTransaction>;

    /// Builds a `RemoveNode` op for an existing node, including owned ports and incident edges.
    fn build_remove_node_op(&self, id: NodeId) -> Option<GraphOp>;

    /// Builds a transaction that removes a node (and its ports/edges).
    fn build_remove_node_tx(
        &self,
        id: NodeId,
        label: impl Into<String>,
    ) -> Option<GraphTransaction>;

    /// Builds a `RemoveGroup` op for an existing group, including nodes detached from it.
    fn build_remove_group_op(&self, id: GroupId) -> Option<GraphOp>;

    /// Builds a transaction that removes a group (and detaches any child nodes).
    fn build_remove_group_tx(
        &self,
        id: GroupId,
        label: impl Into<String>,
    ) -> Option<GraphTransaction>;
}

impl GraphOpBuilderExt for Graph {
    fn build_remove_edge_op(&self, id: EdgeId) -> Option<GraphOp> {
        GraphMutationPlanner::new(self).remove_edge_op(id).ok()
    }

    fn build_remove_port_op(&self, id: PortId) -> Option<GraphOp> {
        GraphMutationPlanner::new(self).remove_port_op(id).ok()
    }

    fn build_disconnect_port_ops(&self, id: PortId) -> Option<Vec<GraphOp>> {
        GraphMutationPlanner::new(self).disconnect_port_ops(id).ok()
    }

    fn build_remove_port_tx(
        &self,
        id: PortId,
        label: impl Into<String>,
    ) -> Option<GraphTransaction> {
        GraphMutationPlanner::new(self)
            .remove_port_tx(id, label)
            .ok()
    }

    fn build_remove_node_op(&self, id: NodeId) -> Option<GraphOp> {
        GraphMutationPlanner::new(self).remove_node_op(id).ok()
    }

    fn build_remove_node_tx(
        &self,
        id: NodeId,
        label: impl Into<String>,
    ) -> Option<GraphTransaction> {
        GraphMutationPlanner::new(self)
            .remove_node_tx(id, label)
            .ok()
    }

    fn build_remove_group_op(&self, id: GroupId) -> Option<GraphOp> {
        GraphMutationPlanner::new(self).remove_group_op(id).ok()
    }

    fn build_remove_group_tx(
        &self,
        id: GroupId,
        label: impl Into<String>,
    ) -> Option<GraphTransaction> {
        GraphMutationPlanner::new(self)
            .remove_group_tx(id, label)
            .ok()
    }
}
