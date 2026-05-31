use super::{HandleConnection, NodeGraphLookups, NodeLookupEntry};
use jellyflow_core::core::{EdgeKind, Graph};
use jellyflow_core::ops::{EdgeEndpoints, GraphOp, GraphTransaction};

impl NodeGraphLookups {
    pub fn apply_transaction(&mut self, graph: &Graph, tx: &GraphTransaction) {
        for op in &tx.ops {
            if !self.apply_op(graph, op) {
                self.rebuild_from(graph);
                return;
            }
        }
    }

    fn apply_op(&mut self, graph: &Graph, op: &GraphOp) -> bool {
        match op {
            GraphOp::AddNode { id, node } => {
                self.node_lookup
                    .insert(*id, NodeLookupEntry::from_node(node));
                true
            }
            GraphOp::RemoveNode { id, edges, .. } => {
                for (edge_id, _edge) in edges {
                    if let Some(conn) = self.connection_from_edge_lookup(*edge_id) {
                        self.remove_edge_connection(conn);
                    } else {
                        self.slow_remove_edge_from_connection_lookup(*edge_id);
                    }
                    self.edge_lookup.remove(edge_id);
                }
                self.node_lookup.remove(id);
                true
            }
            GraphOp::SetNodePos { id, to, .. } => {
                if let Some(n) = self.node_lookup.get_mut(id) {
                    n.pos = *to;
                    return true;
                }
                self.insert_node_lookup_from_graph(graph, *id)
            }
            GraphOp::SetNodeKind { id, to, .. } => {
                if let Some(n) = self.node_lookup.get_mut(id) {
                    n.kind = to.clone();
                    return true;
                }
                self.insert_node_lookup_from_graph(graph, *id)
            }
            GraphOp::SetNodeKindVersion { id, to, .. } => {
                if let Some(n) = self.node_lookup.get_mut(id) {
                    n.kind_version = *to;
                    return true;
                }
                self.insert_node_lookup_from_graph(graph, *id)
            }
            GraphOp::SetNodeParent { id, to, .. } => {
                if let Some(n) = self.node_lookup.get_mut(id) {
                    n.parent = *to;
                    return true;
                }
                false
            }
            GraphOp::SetNodeSize { id, to, .. } => {
                if let Some(n) = self.node_lookup.get_mut(id) {
                    n.size = *to;
                    return true;
                }
                false
            }
            GraphOp::SetNodeHidden { id, to, .. } => {
                if let Some(n) = self.node_lookup.get_mut(id) {
                    n.hidden = *to;
                    return true;
                }
                false
            }
            GraphOp::SetNodeCollapsed { id, to, .. } => {
                if let Some(n) = self.node_lookup.get_mut(id) {
                    n.collapsed = *to;
                    return true;
                }
                false
            }
            GraphOp::SetNodePorts { id, to, .. } => {
                if let Some(n) = self.node_lookup.get_mut(id) {
                    n.ports = to.clone();
                    return true;
                }
                false
            }
            GraphOp::RemovePort { id, port, edges } => {
                if let Some(n) = self.node_lookup.get_mut(&port.node) {
                    n.ports.retain(|port_id| port_id != id);
                }
                for (edge_id, _edge) in edges {
                    if let Some(conn) = self.connection_from_edge_lookup(*edge_id) {
                        self.remove_edge_connection(conn);
                    } else {
                        self.slow_remove_edge_from_connection_lookup(*edge_id);
                    }
                    self.edge_lookup.remove(edge_id);
                }
                true
            }
            GraphOp::AddEdge { id, edge } => {
                let endpoints = EdgeEndpoints {
                    from: edge.from,
                    to: edge.to,
                };
                let Some((entry, conn)) = Self::edge_lookup_entry_from_graph(
                    graph,
                    *id,
                    edge.kind,
                    endpoints,
                    edge.reconnectable,
                ) else {
                    return false;
                };
                self.edge_lookup.insert(*id, entry);
                self.add_edge_connection(conn);
                true
            }
            GraphOp::RemoveEdge { id, .. } => {
                if let Some(conn) = self.connection_from_edge_lookup(*id) {
                    self.remove_edge_connection(conn);
                } else {
                    self.slow_remove_edge_from_connection_lookup(*id);
                }
                self.edge_lookup.remove(id);
                true
            }
            GraphOp::SetEdgeKind { id, to, .. } => {
                if let Some(e) = self.edge_lookup.get_mut(id) {
                    e.kind = *to;
                }
                let Some(conn) = self.connection_from_edge_lookup(*id) else {
                    self.slow_update_edge_kind_in_connection_lookup(*id, *to);
                    return true;
                };
                self.update_edge_kind_in_connection_lookup(conn, *to);
                true
            }
            GraphOp::SetEdgeReconnectable { id, to, .. } => {
                if let Some(e) = self.edge_lookup.get_mut(id) {
                    e.reconnectable = *to;
                    return true;
                }
                let Some(edge) = graph.edges.get(id) else {
                    return false;
                };
                let endpoints = EdgeEndpoints {
                    from: edge.from,
                    to: edge.to,
                };
                let Some((entry, _conn)) =
                    Self::edge_lookup_entry_from_graph(graph, *id, edge.kind, endpoints, *to)
                else {
                    return false;
                };
                self.edge_lookup.insert(*id, entry);
                true
            }
            GraphOp::SetEdgeEndpoints { id, from, to } => {
                if let Some(prev) = self.edge_lookup.get(id).copied() {
                    self.remove_edge_connection(HandleConnection::from_edge_lookup(*id, prev));
                } else {
                    // try best-effort removal based on old edge id
                    self.slow_remove_edge_from_connection_lookup(*id);
                }

                let kind = graph.edges.get(id).map(|e| e.kind).unwrap_or_else(|| {
                    self.edge_lookup
                        .get(id)
                        .map(|e| e.kind)
                        .unwrap_or(EdgeKind::Data)
                });
                let reconnectable = graph
                    .edges
                    .get(id)
                    .and_then(|e| e.reconnectable)
                    .or_else(|| self.edge_lookup.get(id).and_then(|e| e.reconnectable));

                let Some((entry, conn)) =
                    Self::edge_lookup_entry_from_graph(graph, *id, kind, *to, reconnectable)
                else {
                    // revert to full rebuild if we cannot compute endpoint owners
                    return false;
                };
                self.edge_lookup.insert(*id, entry);
                self.add_edge_connection(conn);
                let _ = from;
                true
            }
            GraphOp::RemoveGroup { detached, .. } => {
                for (node_id, _previous_parent) in detached {
                    if let Some(n) = self.node_lookup.get_mut(node_id) {
                        n.parent = None;
                    }
                }
                true
            }

            GraphOp::SetNodeSelectable { .. }
            | GraphOp::SetNodeDraggable { .. }
            | GraphOp::SetNodeConnectable { .. }
            | GraphOp::SetNodeDeletable { .. }
            | GraphOp::SetNodeExtent { .. }
            | GraphOp::SetNodeExpandParent { .. }
            | GraphOp::SetNodeData { .. }
            | GraphOp::AddPort { .. }
            | GraphOp::SetPortConnectable { .. }
            | GraphOp::SetPortConnectableStart { .. }
            | GraphOp::SetPortConnectableEnd { .. }
            | GraphOp::SetPortType { .. }
            | GraphOp::SetPortData { .. }
            | GraphOp::SetEdgeSelectable { .. }
            | GraphOp::SetEdgeDeletable { .. }
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
            | GraphOp::SetStickyNoteColor { .. } => true,
        }
    }
}
