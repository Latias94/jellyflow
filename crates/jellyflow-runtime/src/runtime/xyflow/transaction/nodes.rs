use crate::runtime::xyflow::changes::{ChangesToTransactionError, NodeChange};
use jellyflow_core::ops::{GraphMutationPlanner, GraphOp};

use super::ChangesTransactionPlanner;

impl<'a> ChangesTransactionPlanner<'a> {
    pub(super) fn push_node_change(
        &mut self,
        change: &NodeChange,
    ) -> Result<(), ChangesToTransactionError> {
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
}
