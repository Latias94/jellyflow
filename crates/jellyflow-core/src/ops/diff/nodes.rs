use super::GraphDiffPlanner;
use crate::ops::{GraphMutationPlanner, GraphOp};

impl<'a> GraphDiffPlanner<'a> {
    pub(super) fn diff_nodes(&mut self) {
        let from = self.from;
        let to = self.to;
        let tx = &mut self.tx;
        let removed_ports_by_cascade = &mut self.removed_ports_by_cascade;
        let removed_edges_by_cascade = &mut self.removed_edges_by_cascade;

        for (id, node_to) in &to.nodes {
            if let Some(node_from) = from.nodes.get(id) {
                if node_from.kind != node_to.kind {
                    tx.ops.push(GraphOp::SetNodeKind {
                        id: *id,
                        from: node_from.kind.clone(),
                        to: node_to.kind.clone(),
                    });
                }
                if node_from.kind_version != node_to.kind_version {
                    tx.ops.push(GraphOp::SetNodeKindVersion {
                        id: *id,
                        from: node_from.kind_version,
                        to: node_to.kind_version,
                    });
                }
                if node_from.selectable != node_to.selectable {
                    tx.ops.push(GraphOp::SetNodeSelectable {
                        id: *id,
                        from: node_from.selectable,
                        to: node_to.selectable,
                    });
                }
                if node_from.draggable != node_to.draggable {
                    tx.ops.push(GraphOp::SetNodeDraggable {
                        id: *id,
                        from: node_from.draggable,
                        to: node_to.draggable,
                    });
                }
                if node_from.connectable != node_to.connectable {
                    tx.ops.push(GraphOp::SetNodeConnectable {
                        id: *id,
                        from: node_from.connectable,
                        to: node_to.connectable,
                    });
                }
                if node_from.deletable != node_to.deletable {
                    tx.ops.push(GraphOp::SetNodeDeletable {
                        id: *id,
                        from: node_from.deletable,
                        to: node_to.deletable,
                    });
                }
                if node_from.pos != node_to.pos {
                    tx.ops.push(GraphOp::SetNodePos {
                        id: *id,
                        from: node_from.pos,
                        to: node_to.pos,
                    });
                }
                if node_from.parent != node_to.parent {
                    if let Some(group_id) = node_from.parent
                        && from.groups.contains_key(&group_id)
                        && !to.groups.contains_key(&group_id)
                    {
                        // Group diffs are emitted before node diffs. When a parent group is removed,
                        // `RemoveGroup` detaches the child node to `None`, so the node parent change
                        // must be expressed relative to that intermediate state.
                        if node_to.parent.is_some() {
                            tx.ops.push(GraphOp::SetNodeParent {
                                id: *id,
                                from: None,
                                to: node_to.parent,
                            });
                        }
                    } else {
                        tx.ops.push(GraphOp::SetNodeParent {
                            id: *id,
                            from: node_from.parent,
                            to: node_to.parent,
                        });
                    }
                }
                if node_from.extent != node_to.extent {
                    tx.ops.push(GraphOp::SetNodeExtent {
                        id: *id,
                        from: node_from.extent,
                        to: node_to.extent,
                    });
                }
                if node_from.expand_parent != node_to.expand_parent {
                    tx.ops.push(GraphOp::SetNodeExpandParent {
                        id: *id,
                        from: node_from.expand_parent,
                        to: node_to.expand_parent,
                    });
                }
                if node_from.size != node_to.size {
                    tx.ops.push(GraphOp::SetNodeSize {
                        id: *id,
                        from: node_from.size,
                        to: node_to.size,
                    });
                }
                if node_from.hidden != node_to.hidden {
                    tx.ops.push(GraphOp::SetNodeHidden {
                        id: *id,
                        from: node_from.hidden,
                        to: node_to.hidden,
                    });
                }
                if node_from.collapsed != node_to.collapsed {
                    tx.ops.push(GraphOp::SetNodeCollapsed {
                        id: *id,
                        from: node_from.collapsed,
                        to: node_to.collapsed,
                    });
                }
                if node_from.ports != node_to.ports {
                    tx.ops.push(GraphOp::SetNodePorts {
                        id: *id,
                        from: node_from.ports.clone(),
                        to: node_to.ports.clone(),
                    });
                }
                if node_from.data != node_to.data {
                    tx.ops.push(GraphOp::SetNodeData {
                        id: *id,
                        from: node_from.data.clone(),
                        to: node_to.data.clone(),
                    });
                }
            } else {
                tx.ops.push(GraphOp::AddNode {
                    id: *id,
                    node: node_to.clone(),
                });
            }
        }

        for (id, node_from) in &from.nodes {
            if !to.nodes.contains_key(id) {
                // Prefer the reversible removal op with captured ports/edges.
                if let Ok(op) = GraphMutationPlanner::new(from).remove_node_op(*id) {
                    if let GraphOp::RemoveNode { ports, edges, .. } = &op {
                        removed_ports_by_cascade.extend(ports.iter().map(|(id, _)| *id));
                        removed_edges_by_cascade.extend(edges.iter().map(|(id, _)| *id));
                    }
                    tx.ops.push(op);
                } else {
                    // Fallback: remove node only (should not happen if graph is consistent).
                    tx.ops.push(GraphOp::RemoveNode {
                        id: *id,
                        node: node_from.clone(),
                        ports: Vec::new(),
                        edges: Vec::new(),
                    });
                }
            }
        }
    }
}
