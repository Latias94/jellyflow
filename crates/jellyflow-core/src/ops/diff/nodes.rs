use super::GraphDiffPlanner;
use crate::core::{Node, NodeId};
use crate::ops::{GraphMutationPlanner, GraphOp};

impl<'a> GraphDiffPlanner<'a> {
    pub(super) fn diff_nodes(&mut self) {
        let from = self.from;
        let to = self.to;

        for (id, node_to) in &to.nodes {
            if let Some(node_from) = from.nodes.get(id) {
                self.diff_existing_node(*id, node_from, node_to);
            } else {
                self.push_op(GraphOp::AddNode {
                    id: *id,
                    node: node_to.clone(),
                });
            }
        }

        for (id, node_from) in &from.nodes {
            if !to.nodes.contains_key(id) {
                self.diff_removed_node(*id, node_from);
            }
        }
    }

    fn diff_existing_node(&mut self, id: NodeId, node_from: &Node, node_to: &Node) {
        self.diff_node_identity_fields(id, node_from, node_to);
        self.diff_node_policy_fields(id, node_from, node_to);
        self.diff_node_layout_fields(id, node_from, node_to);
        self.diff_node_ports(id, node_from, node_to);
        self.diff_node_data(id, node_from, node_to);
    }

    fn diff_node_identity_fields(&mut self, id: NodeId, node_from: &Node, node_to: &Node) {
        if node_from.kind != node_to.kind {
            self.push_op(GraphOp::SetNodeKind {
                id,
                from: node_from.kind.clone(),
                to: node_to.kind.clone(),
            });
        }
        if node_from.kind_version != node_to.kind_version {
            self.push_op(GraphOp::SetNodeKindVersion {
                id,
                from: node_from.kind_version,
                to: node_to.kind_version,
            });
        }
    }

    fn diff_node_policy_fields(&mut self, id: NodeId, node_from: &Node, node_to: &Node) {
        if node_from.selectable != node_to.selectable {
            self.push_op(GraphOp::SetNodeSelectable {
                id,
                from: node_from.selectable,
                to: node_to.selectable,
            });
        }
        if node_from.focusable != node_to.focusable {
            self.push_op(GraphOp::SetNodeFocusable {
                id,
                from: node_from.focusable,
                to: node_to.focusable,
            });
        }
        if node_from.draggable != node_to.draggable {
            self.push_op(GraphOp::SetNodeDraggable {
                id,
                from: node_from.draggable,
                to: node_to.draggable,
            });
        }
        if node_from.connectable != node_to.connectable {
            self.push_op(GraphOp::SetNodeConnectable {
                id,
                from: node_from.connectable,
                to: node_to.connectable,
            });
        }
        if node_from.deletable != node_to.deletable {
            self.push_op(GraphOp::SetNodeDeletable {
                id,
                from: node_from.deletable,
                to: node_to.deletable,
            });
        }
    }

    fn diff_node_layout_fields(&mut self, id: NodeId, node_from: &Node, node_to: &Node) {
        if node_from.pos != node_to.pos {
            self.push_op(GraphOp::SetNodePos {
                id,
                from: node_from.pos,
                to: node_to.pos,
            });
        }
        if node_from.origin != node_to.origin {
            self.push_op(GraphOp::SetNodeOrigin {
                id,
                from: node_from.origin,
                to: node_to.origin,
            });
        }
        self.diff_node_parent(id, node_from, node_to);
        if node_from.extent != node_to.extent {
            self.push_op(GraphOp::SetNodeExtent {
                id,
                from: node_from.extent,
                to: node_to.extent,
            });
        }
        if node_from.expand_parent != node_to.expand_parent {
            self.push_op(GraphOp::SetNodeExpandParent {
                id,
                from: node_from.expand_parent,
                to: node_to.expand_parent,
            });
        }
        if node_from.size != node_to.size {
            self.push_op(GraphOp::SetNodeSize {
                id,
                from: node_from.size,
                to: node_to.size,
            });
        }
        if node_from.hidden != node_to.hidden {
            self.push_op(GraphOp::SetNodeHidden {
                id,
                from: node_from.hidden,
                to: node_to.hidden,
            });
        }
        if node_from.collapsed != node_to.collapsed {
            self.push_op(GraphOp::SetNodeCollapsed {
                id,
                from: node_from.collapsed,
                to: node_to.collapsed,
            });
        }
    }

    fn diff_node_ports(&mut self, id: NodeId, node_from: &Node, node_to: &Node) {
        if node_from.ports == node_to.ports {
            return;
        }

        if self.node_port_order_needs_post_restore(id, &node_to.ports) {
            let stable_ports = self.stable_existing_port_order(&node_from.ports);
            if node_from.ports != stable_ports {
                self.push_op(GraphOp::SetNodePorts {
                    id,
                    from: node_from.ports.clone(),
                    to: stable_ports,
                });
            }
            self.nodes_requiring_port_order_restore.insert(id);
            return;
        }

        self.push_op(GraphOp::SetNodePorts {
            id,
            from: node_from.ports.clone(),
            to: node_to.ports.clone(),
        });
    }

    fn diff_node_data(&mut self, id: NodeId, node_from: &Node, node_to: &Node) {
        if node_from.data != node_to.data {
            self.push_op(GraphOp::SetNodeData {
                id,
                from: node_from.data.clone(),
                to: node_to.data.clone(),
            });
        }
    }

    fn diff_node_parent(&mut self, id: NodeId, node_from: &Node, node_to: &Node) {
        if node_from.parent == node_to.parent {
            return;
        }

        let parent_from = if self.parent_group_was_removed(node_from) {
            // Group diffs are emitted before node diffs. When a parent group is removed,
            // `RemoveGroup` detaches the child node to `None`, so the node parent change
            // must be expressed relative to that intermediate state.
            None
        } else {
            node_from.parent
        };

        if parent_from != node_to.parent {
            self.push_op(GraphOp::SetNodeParent {
                id,
                from: parent_from,
                to: node_to.parent,
            });
        }
    }

    fn parent_group_was_removed(&self, node: &Node) -> bool {
        let Some(group_id) = node.parent else {
            return false;
        };

        self.from.groups.contains_key(&group_id) && !self.to.groups.contains_key(&group_id)
    }

    fn diff_removed_node(&mut self, id: NodeId, node_from: &Node) {
        // Prefer the reversible removal op with captured ports/edges.
        if let Ok(op) = GraphMutationPlanner::new(self.from).remove_node_op(id) {
            self.record_removed_node_cascade(&op);
            self.push_op(op);
        } else {
            // Fallback: remove node only (should not happen if graph is consistent).
            self.push_op(GraphOp::RemoveNode {
                id,
                node: node_from.clone(),
                ports: Vec::new(),
                edges: Vec::new(),
            });
        }
    }

    fn record_removed_node_cascade(&mut self, op: &GraphOp) {
        if let GraphOp::RemoveNode { ports, edges, .. } = op {
            self.removed_ports_by_cascade
                .extend(ports.iter().map(|(id, _)| *id));
            self.removed_edges_by_cascade
                .extend(edges.iter().map(|(id, _)| *id));
        }
    }
}
