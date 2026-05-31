//! Transaction projections for XyFlow-style callback payloads.

use std::collections::BTreeSet;

use crate::runtime::xyflow::callbacks::{ConnectionChange, DeleteChange, EdgeConnection};
use crate::runtime::xyflow::changes::{EdgeChange, NodeChange, NodeGraphChanges};
use jellyflow_core::core::EdgeId;
use jellyflow_core::ops::{GraphOp, GraphTransaction};

pub(super) fn node_graph_changes_from_transaction(tx: &GraphTransaction) -> NodeGraphChanges {
    TransactionProjectionPlanner::new(tx).node_graph_changes()
}

pub(super) fn connection_changes_from_transaction(tx: &GraphTransaction) -> Vec<ConnectionChange> {
    TransactionProjectionPlanner::new(tx).connection_changes()
}

pub(super) fn delete_changes_from_transaction(tx: &GraphTransaction) -> DeleteChange {
    TransactionProjectionPlanner::new(tx).delete_changes()
}

struct TransactionProjectionPlanner<'a> {
    tx: &'a GraphTransaction,
}

impl<'a> TransactionProjectionPlanner<'a> {
    fn new(tx: &'a GraphTransaction) -> Self {
        Self { tx }
    }

    fn node_graph_changes(&self) -> NodeGraphChanges {
        let mut out = NodeGraphChanges::default();
        for op in &self.tx.ops {
            self.push_node_graph_change(op, &mut out);
        }
        out
    }

    fn push_node_graph_change(&self, op: &GraphOp, out: &mut NodeGraphChanges) {
        match op {
            GraphOp::AddNode { id, node } => out.nodes.push(NodeChange::Add {
                id: *id,
                node: node.clone(),
            }),
            GraphOp::RemoveNode { id, edges, .. } => {
                out.nodes.push(NodeChange::Remove { id: *id });
                for (edge_id, _edge) in edges {
                    out.edges.push(EdgeChange::Remove { id: *edge_id });
                }
            }
            GraphOp::SetNodePos { id, to, .. } => out.nodes.push(NodeChange::Position {
                id: *id,
                position: *to,
            }),
            GraphOp::SetNodeKind { id, to, .. } => out.nodes.push(NodeChange::Kind {
                id: *id,
                kind: to.clone(),
            }),
            GraphOp::SetNodeKindVersion { id, to, .. } => out.nodes.push(NodeChange::KindVersion {
                id: *id,
                kind_version: *to,
            }),
            GraphOp::SetNodeSelectable { id, to, .. } => out.nodes.push(NodeChange::Selectable {
                id: *id,
                selectable: *to,
            }),
            GraphOp::SetNodeDraggable { id, to, .. } => out.nodes.push(NodeChange::Draggable {
                id: *id,
                draggable: *to,
            }),
            GraphOp::SetNodeConnectable { id, to, .. } => out.nodes.push(NodeChange::Connectable {
                id: *id,
                connectable: *to,
            }),
            GraphOp::SetNodeDeletable { id, to, .. } => out.nodes.push(NodeChange::Deletable {
                id: *id,
                deletable: *to,
            }),
            GraphOp::SetNodeParent { id, to, .. } => out.nodes.push(NodeChange::Parent {
                id: *id,
                parent: *to,
            }),
            GraphOp::SetNodeExtent { id, to, .. } => out.nodes.push(NodeChange::Extent {
                id: *id,
                extent: *to,
            }),
            GraphOp::SetNodeExpandParent { id, to, .. } => {
                out.nodes.push(NodeChange::ExpandParent {
                    id: *id,
                    expand_parent: *to,
                })
            }
            GraphOp::SetNodeSize { id, to, .. } => {
                out.nodes.push(NodeChange::Size { id: *id, size: *to })
            }
            GraphOp::SetNodeHidden { id, to, .. } => out.nodes.push(NodeChange::Hidden {
                id: *id,
                hidden: *to,
            }),
            GraphOp::SetNodeCollapsed { id, to, .. } => out.nodes.push(NodeChange::Collapsed {
                id: *id,
                collapsed: *to,
            }),
            GraphOp::SetNodeData { id, to, .. } => out.nodes.push(NodeChange::Data {
                id: *id,
                data: to.clone(),
            }),
            GraphOp::SetNodePorts { id, to, .. } => out.nodes.push(NodeChange::Ports {
                id: *id,
                ports: to.clone(),
            }),
            GraphOp::RemovePort { edges, .. } => {
                for (edge_id, _edge) in edges {
                    out.edges.push(EdgeChange::Remove { id: *edge_id });
                }
            }
            GraphOp::AddEdge { id, edge } => out.edges.push(EdgeChange::Add {
                id: *id,
                edge: edge.clone(),
            }),
            GraphOp::RemoveEdge { id, .. } => out.edges.push(EdgeChange::Remove { id: *id }),
            GraphOp::SetEdgeKind { id, to, .. } => {
                out.edges.push(EdgeChange::Kind { id: *id, kind: *to })
            }
            GraphOp::SetEdgeSelectable { id, to, .. } => out.edges.push(EdgeChange::Selectable {
                id: *id,
                selectable: *to,
            }),
            GraphOp::SetEdgeDeletable { id, to, .. } => out.edges.push(EdgeChange::Deletable {
                id: *id,
                deletable: *to,
            }),
            GraphOp::SetEdgeReconnectable { id, to, .. } => {
                out.edges.push(EdgeChange::Reconnectable {
                    id: *id,
                    reconnectable: *to,
                })
            }
            GraphOp::SetEdgeEndpoints { id, to, .. } => out.edges.push(EdgeChange::Endpoints {
                id: *id,
                from: to.from,
                to: to.to,
            }),
            GraphOp::RemoveGroup { detached, .. } => {
                for (node_id, _previous_parent) in detached {
                    out.nodes.push(NodeChange::Parent {
                        id: *node_id,
                        parent: None,
                    });
                }
            }

            // These variants mutate graph resources that are outside the XyFlow-style
            // node/edge change-array contract. Full-fidelity controlled integrations should
            // apply the committed GraphTransaction from on_graph_commit.
            GraphOp::AddPort { .. }
            | GraphOp::SetPortConnectable { .. }
            | GraphOp::SetPortConnectableStart { .. }
            | GraphOp::SetPortConnectableEnd { .. }
            | GraphOp::SetPortType { .. }
            | GraphOp::SetPortData { .. }
            | GraphOp::AddImport { .. }
            | GraphOp::RemoveImport { .. }
            | GraphOp::SetImportAlias { .. }
            | GraphOp::AddSymbol { .. }
            | GraphOp::RemoveSymbol { .. }
            | GraphOp::SetSymbolName { .. }
            | GraphOp::SetSymbolType { .. }
            | GraphOp::SetSymbolDefaultValue { .. }
            | GraphOp::SetSymbolMeta { .. }
            | GraphOp::AddGroup { .. }
            | GraphOp::SetGroupRect { .. }
            | GraphOp::SetGroupTitle { .. }
            | GraphOp::SetGroupColor { .. }
            | GraphOp::AddStickyNote { .. }
            | GraphOp::RemoveStickyNote { .. }
            | GraphOp::SetStickyNoteText { .. }
            | GraphOp::SetStickyNoteRect { .. }
            | GraphOp::SetStickyNoteColor { .. } => {}
        }
    }

    fn connection_changes(&self) -> Vec<ConnectionChange> {
        let mut out = Vec::new();
        out.reserve(self.tx.ops.len().min(8));

        let mut removed_edges: BTreeSet<EdgeId> = BTreeSet::new();
        for op in &self.tx.ops {
            match op {
                GraphOp::AddEdge { id, edge } => {
                    out.push(ConnectionChange::Connected(EdgeConnection {
                        edge: *id,
                        from: edge.from,
                        to: edge.to,
                        kind: edge.kind,
                    }))
                }
                GraphOp::RemoveNode { edges, .. } => {
                    Self::push_disconnected_edges(edges, &mut removed_edges, &mut out);
                }
                GraphOp::RemovePort { edges, .. } => {
                    Self::push_disconnected_edges(edges, &mut removed_edges, &mut out);
                }
                GraphOp::RemoveEdge { id, edge } => {
                    let _ = removed_edges.insert(*id);
                    out.push(ConnectionChange::Disconnected(EdgeConnection {
                        edge: *id,
                        from: edge.from,
                        to: edge.to,
                        kind: edge.kind,
                    }))
                }
                GraphOp::SetEdgeEndpoints { id, from, to } => {
                    out.push(ConnectionChange::Reconnected {
                        edge: *id,
                        from: *from,
                        to: *to,
                    })
                }
                _ => {}
            }
        }

        out
    }

    fn push_disconnected_edges(
        edges: &[(EdgeId, jellyflow_core::core::Edge)],
        removed_edges: &mut BTreeSet<EdgeId>,
        out: &mut Vec<ConnectionChange>,
    ) {
        for (id, edge) in edges {
            if !removed_edges.insert(*id) {
                continue;
            }
            out.push(ConnectionChange::Disconnected(EdgeConnection {
                edge: *id,
                from: edge.from,
                to: edge.to,
                kind: edge.kind,
            }))
        }
    }

    fn delete_changes(&self) -> DeleteChange {
        let mut out = DeleteChange::default();

        for op in &self.tx.ops {
            match op {
                GraphOp::RemoveNode { id, edges, .. } => {
                    out.nodes.push(*id);
                    for (edge_id, _edge) in edges {
                        out.edges.push(*edge_id);
                    }
                }
                GraphOp::RemoveEdge { id, .. } => out.edges.push(*id),
                GraphOp::RemoveGroup { id, .. } => out.groups.push(*id),
                GraphOp::RemoveStickyNote { id, .. } => out.sticky_notes.push(*id),
                GraphOp::RemovePort { edges, .. } => {
                    for (edge_id, _edge) in edges {
                        out.edges.push(*edge_id);
                    }
                }
                _ => {}
            }
        }

        out.nodes.sort_unstable();
        out.nodes.dedup();
        out.edges.sort_unstable();
        out.edges.dedup();
        out.groups.sort_unstable();
        out.groups.dedup();
        out.sticky_notes.sort_unstable();
        out.sticky_notes.dedup();

        out
    }
}
