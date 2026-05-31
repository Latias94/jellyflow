use crate::runtime::xyflow::changes::{ChangesToTransactionError, NodeChange};
use jellyflow_core::core::{Node, NodeId};
use jellyflow_core::ops::{GraphMutationPlanner, GraphOp};

use super::ChangesTransactionPlanner;

impl<'a> ChangesTransactionPlanner<'a> {
    pub(super) fn push_node_change(
        &mut self,
        change: &NodeChange,
    ) -> Result<(), ChangesToTransactionError> {
        match change {
            NodeChange::Add { id, node } => {
                self.push_op(GraphOp::AddNode {
                    id: *id,
                    node: node.clone(),
                });
            }
            NodeChange::Remove { id } => {
                self.push_remove_node_change(*id)?;
            }
            NodeChange::Position { id, position } => {
                self.push_node_update(*id, |node| GraphOp::SetNodePos {
                    id: *id,
                    from: node.pos,
                    to: *position,
                })?;
            }
            NodeChange::Kind { id, kind } => {
                self.push_node_update(*id, |node| GraphOp::SetNodeKind {
                    id: *id,
                    from: node.kind.clone(),
                    to: kind.clone(),
                })?;
            }
            NodeChange::KindVersion { id, kind_version } => {
                self.push_node_update(*id, |node| GraphOp::SetNodeKindVersion {
                    id: *id,
                    from: node.kind_version,
                    to: *kind_version,
                })?;
            }
            NodeChange::Selectable { id, selectable } => {
                self.push_node_update(*id, |node| GraphOp::SetNodeSelectable {
                    id: *id,
                    from: node.selectable,
                    to: *selectable,
                })?;
            }
            NodeChange::Draggable { id, draggable } => {
                self.push_node_update(*id, |node| GraphOp::SetNodeDraggable {
                    id: *id,
                    from: node.draggable,
                    to: *draggable,
                })?;
            }
            NodeChange::Connectable { id, connectable } => {
                self.push_node_update(*id, |node| GraphOp::SetNodeConnectable {
                    id: *id,
                    from: node.connectable,
                    to: *connectable,
                })?;
            }
            NodeChange::Deletable { id, deletable } => {
                self.push_node_update(*id, |node| GraphOp::SetNodeDeletable {
                    id: *id,
                    from: node.deletable,
                    to: *deletable,
                })?;
            }
            NodeChange::Parent { id, parent } => {
                self.push_node_update(*id, |node| GraphOp::SetNodeParent {
                    id: *id,
                    from: node.parent,
                    to: *parent,
                })?;
            }
            NodeChange::Extent { id, extent } => {
                self.push_node_update(*id, |node| GraphOp::SetNodeExtent {
                    id: *id,
                    from: node.extent,
                    to: *extent,
                })?;
            }
            NodeChange::ExpandParent { id, expand_parent } => {
                self.push_node_update(*id, |node| GraphOp::SetNodeExpandParent {
                    id: *id,
                    from: node.expand_parent,
                    to: *expand_parent,
                })?;
            }
            NodeChange::Size { id, size } => {
                self.push_node_update(*id, |node| GraphOp::SetNodeSize {
                    id: *id,
                    from: node.size,
                    to: *size,
                })?;
            }
            NodeChange::Hidden { id, hidden } => {
                self.push_node_update(*id, |node| GraphOp::SetNodeHidden {
                    id: *id,
                    from: node.hidden,
                    to: *hidden,
                })?;
            }
            NodeChange::Collapsed { id, collapsed } => {
                self.push_node_update(*id, |node| GraphOp::SetNodeCollapsed {
                    id: *id,
                    from: node.collapsed,
                    to: *collapsed,
                })?;
            }
            NodeChange::Data { id, data } => {
                self.push_node_update(*id, |node| GraphOp::SetNodeData {
                    id: *id,
                    from: node.data.clone(),
                    to: data.clone(),
                })?;
            }
            NodeChange::Ports { id, ports } => {
                self.push_node_update(*id, |node| GraphOp::SetNodePorts {
                    id: *id,
                    from: node.ports.clone(),
                    to: ports.clone(),
                })?;
            }
        }
        Ok(())
    }

    fn push_remove_node_change(&mut self, id: NodeId) -> Result<(), ChangesToTransactionError> {
        let op = GraphMutationPlanner::new(self.graph)
            .remove_node_op(id)
            .map_err(|_| ChangesToTransactionError::MissingNode(id))?;
        self.push_op(op);
        Ok(())
    }

    fn push_node_update(
        &mut self,
        id: NodeId,
        build: impl FnOnce(&Node) -> GraphOp,
    ) -> Result<(), ChangesToTransactionError> {
        let op = {
            let node = self.existing_node(id)?;
            build(node)
        };
        self.push_op(op);
        Ok(())
    }
}
