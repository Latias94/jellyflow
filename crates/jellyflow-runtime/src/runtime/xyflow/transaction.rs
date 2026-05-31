//! Transaction planner for XyFlow-style node/edge changes.

use crate::runtime::xyflow::changes::{
    ChangesToTransactionError, EdgeChange, NodeChange, NodeGraphChanges,
};
use jellyflow_core::core::{Edge, EdgeId, Graph, Node, NodeId};
use jellyflow_core::ops::{EdgeEndpoints, GraphMutationPlanner, GraphOp, GraphTransaction};

pub(super) fn changes_to_transaction(
    changes: &NodeGraphChanges,
    graph: &Graph,
) -> Result<GraphTransaction, ChangesToTransactionError> {
    ChangesTransactionPlanner::new(graph).finish(changes)
}

struct ChangesTransactionPlanner<'a> {
    graph: &'a Graph,
    tx: GraphTransaction,
}

impl<'a> ChangesTransactionPlanner<'a> {
    fn new(graph: &'a Graph) -> Self {
        Self {
            graph,
            tx: GraphTransaction::new(),
        }
    }

    fn finish(
        mut self,
        changes: &NodeGraphChanges,
    ) -> Result<GraphTransaction, ChangesToTransactionError> {
        for change in &changes.nodes {
            self.push_node_change(change)?;
        }
        for change in &changes.edges {
            self.push_edge_change(change)?;
        }

        Ok(self.tx)
    }

    fn push_node_change(&mut self, change: &NodeChange) -> Result<(), ChangesToTransactionError> {
        match change {
            NodeChange::Add { id, node } => self.tx.push(GraphOp::AddNode {
                id: *id,
                node: node.clone(),
            }),
            NodeChange::Remove { id } => {
                let op = GraphMutationPlanner::new(self.graph)
                    .remove_node_op(*id)
                    .map_err(|_| ChangesToTransactionError::MissingNode(*id))?;
                self.tx.push(op);
            }
            NodeChange::Position { id, position } => {
                let from = self.existing_node(*id)?.pos;
                self.tx.push(GraphOp::SetNodePos {
                    id: *id,
                    from,
                    to: *position,
                });
            }
            NodeChange::Kind { id, kind } => {
                let from = self.existing_node(*id)?.kind.clone();
                self.tx.push(GraphOp::SetNodeKind {
                    id: *id,
                    from,
                    to: kind.clone(),
                });
            }
            NodeChange::KindVersion { id, kind_version } => {
                let from = self.existing_node(*id)?.kind_version;
                self.tx.push(GraphOp::SetNodeKindVersion {
                    id: *id,
                    from,
                    to: *kind_version,
                });
            }
            NodeChange::Selectable { id, selectable } => {
                let from = self.existing_node(*id)?.selectable;
                self.tx.push(GraphOp::SetNodeSelectable {
                    id: *id,
                    from,
                    to: *selectable,
                });
            }
            NodeChange::Draggable { id, draggable } => {
                let from = self.existing_node(*id)?.draggable;
                self.tx.push(GraphOp::SetNodeDraggable {
                    id: *id,
                    from,
                    to: *draggable,
                });
            }
            NodeChange::Connectable { id, connectable } => {
                let from = self.existing_node(*id)?.connectable;
                self.tx.push(GraphOp::SetNodeConnectable {
                    id: *id,
                    from,
                    to: *connectable,
                });
            }
            NodeChange::Deletable { id, deletable } => {
                let from = self.existing_node(*id)?.deletable;
                self.tx.push(GraphOp::SetNodeDeletable {
                    id: *id,
                    from,
                    to: *deletable,
                });
            }
            NodeChange::Parent { id, parent } => {
                let from = self.existing_node(*id)?.parent;
                self.tx.push(GraphOp::SetNodeParent {
                    id: *id,
                    from,
                    to: *parent,
                });
            }
            NodeChange::Extent { id, extent } => {
                let from = self.existing_node(*id)?.extent;
                self.tx.push(GraphOp::SetNodeExtent {
                    id: *id,
                    from,
                    to: *extent,
                });
            }
            NodeChange::ExpandParent { id, expand_parent } => {
                let from = self.existing_node(*id)?.expand_parent;
                self.tx.push(GraphOp::SetNodeExpandParent {
                    id: *id,
                    from,
                    to: *expand_parent,
                });
            }
            NodeChange::Size { id, size } => {
                let from = self.existing_node(*id)?.size;
                self.tx.push(GraphOp::SetNodeSize {
                    id: *id,
                    from,
                    to: *size,
                });
            }
            NodeChange::Hidden { id, hidden } => {
                let from = self.existing_node(*id)?.hidden;
                self.tx.push(GraphOp::SetNodeHidden {
                    id: *id,
                    from,
                    to: *hidden,
                });
            }
            NodeChange::Collapsed { id, collapsed } => {
                let from = self.existing_node(*id)?.collapsed;
                self.tx.push(GraphOp::SetNodeCollapsed {
                    id: *id,
                    from,
                    to: *collapsed,
                });
            }
            NodeChange::Data { id, data } => {
                let from = self.existing_node(*id)?.data.clone();
                self.tx.push(GraphOp::SetNodeData {
                    id: *id,
                    from,
                    to: data.clone(),
                });
            }
            NodeChange::Ports { id, ports } => {
                let from = self.existing_node(*id)?.ports.clone();
                self.tx.push(GraphOp::SetNodePorts {
                    id: *id,
                    from,
                    to: ports.clone(),
                });
            }
        }
        Ok(())
    }

    fn push_edge_change(&mut self, change: &EdgeChange) -> Result<(), ChangesToTransactionError> {
        match change {
            EdgeChange::Add { id, edge } => self.tx.push(GraphOp::AddEdge {
                id: *id,
                edge: edge.clone(),
            }),
            EdgeChange::Remove { id } => {
                let op = GraphMutationPlanner::new(self.graph)
                    .remove_edge_op(*id)
                    .map_err(|_| ChangesToTransactionError::MissingEdge(*id))?;
                self.tx.push(op);
            }
            EdgeChange::Kind { id, kind } => {
                let from = self.existing_edge(*id)?.kind;
                self.tx.push(GraphOp::SetEdgeKind {
                    id: *id,
                    from,
                    to: *kind,
                });
            }
            EdgeChange::Selectable { id, selectable } => {
                let from = self.existing_edge(*id)?.selectable;
                self.tx.push(GraphOp::SetEdgeSelectable {
                    id: *id,
                    from,
                    to: *selectable,
                });
            }
            EdgeChange::Deletable { id, deletable } => {
                let from = self.existing_edge(*id)?.deletable;
                self.tx.push(GraphOp::SetEdgeDeletable {
                    id: *id,
                    from,
                    to: *deletable,
                });
            }
            EdgeChange::Reconnectable { id, reconnectable } => {
                let from = self.existing_edge(*id)?.reconnectable;
                self.tx.push(GraphOp::SetEdgeReconnectable {
                    id: *id,
                    from,
                    to: *reconnectable,
                });
            }
            EdgeChange::Endpoints { id, from, to } => {
                let edge = self.existing_edge(*id)?;
                self.tx.push(GraphOp::SetEdgeEndpoints {
                    id: *id,
                    from: edge_endpoints(edge),
                    to: EdgeEndpoints {
                        from: *from,
                        to: *to,
                    },
                });
            }
        }
        Ok(())
    }

    fn existing_node(&self, id: NodeId) -> Result<&'a Node, ChangesToTransactionError> {
        self.graph
            .nodes
            .get(&id)
            .ok_or(ChangesToTransactionError::MissingNode(id))
    }

    fn existing_edge(&self, id: EdgeId) -> Result<&'a Edge, ChangesToTransactionError> {
        self.graph
            .edges
            .get(&id)
            .ok_or(ChangesToTransactionError::MissingEdge(id))
    }
}

fn edge_endpoints(edge: &Edge) -> EdgeEndpoints {
    EdgeEndpoints {
        from: edge.from,
        to: edge.to,
    }
}
