use jellyflow_core::GraphMutationFootprint;

use super::{batch::GraphTransaction, endpoints::EdgeEndpoints, op::GraphOp};

impl GraphOp {
    /// Appends this operation's touched ids to an existing footprint.
    pub fn append_footprint(&self, footprint: &mut GraphMutationFootprint) {
        match self {
            Self::AddNode { id, node } => {
                footprint.touch_node_snapshot(*id, node);
            }
            Self::RemoveNode {
                id,
                node,
                ports,
                edges,
                bindings,
            } => {
                footprint.touch_node_snapshot(*id, node);
                for (port_id, port) in ports {
                    footprint.touch_port_snapshot(*port_id, port);
                }
                for (edge_id, edge) in edges {
                    footprint.touch_edge_snapshot(*edge_id, edge);
                }
                for (binding_id, binding) in bindings {
                    footprint.touch_binding_snapshot(*binding_id, binding);
                }
            }
            Self::SetNodePos { id, .. }
            | Self::SetNodeOrigin { id, .. }
            | Self::SetNodeKind { id, .. }
            | Self::SetNodeKindVersion { id, .. }
            | Self::SetNodeSelectable { id, .. }
            | Self::SetNodeDraggable { id, .. }
            | Self::SetNodeConnectable { id, .. }
            | Self::SetNodeDeletable { id, .. }
            | Self::SetNodeExtent { id, .. }
            | Self::SetNodeExpandParent { id, .. }
            | Self::SetNodeSize { id, .. }
            | Self::SetNodeHidden { id, .. }
            | Self::SetNodeCollapsed { id, .. }
            | Self::SetNodeData { id, .. } => {
                footprint.touch_node(*id);
            }
            Self::SetNodeParent { id, from, to } => {
                footprint.touch_node(*id);
                if let Some(parent) = from {
                    footprint.touch_group(*parent);
                }
                if let Some(parent) = to {
                    footprint.touch_group(*parent);
                }
            }
            Self::SetNodePorts { id, from, to } => {
                footprint.touch_node(*id);
                footprint.ports.extend(from.iter().copied());
                footprint.ports.extend(to.iter().copied());
            }

            Self::AddPort { id, port } => {
                footprint.touch_port_snapshot(*id, port);
            }
            Self::RemovePort {
                id,
                port,
                edges,
                bindings,
            } => {
                footprint.touch_port_snapshot(*id, port);
                for (edge_id, edge) in edges {
                    footprint.touch_edge_snapshot(*edge_id, edge);
                }
                for (binding_id, binding) in bindings {
                    footprint.touch_binding_snapshot(*binding_id, binding);
                }
            }
            Self::SetPortConnectable { id, .. }
            | Self::SetPortConnectableStart { id, .. }
            | Self::SetPortConnectableEnd { id, .. }
            | Self::SetPortType { id, .. }
            | Self::SetPortData { id, .. } => {
                footprint.touch_port(*id);
            }

            Self::AddEdge { id, edge } => {
                footprint.touch_edge_snapshot(*id, edge);
            }
            Self::RemoveEdge { id, edge, bindings } => {
                footprint.touch_edge_snapshot(*id, edge);
                for (binding_id, binding) in bindings {
                    footprint.touch_binding_snapshot(*binding_id, binding);
                }
            }
            Self::SetEdgeKind { id, .. }
            | Self::SetEdgeSelectable { id, .. }
            | Self::SetEdgeHidden { id, .. }
            | Self::SetEdgeInteractionWidth { id, .. }
            | Self::SetEdgeDeletable { id, .. }
            | Self::SetEdgeReconnectable { id, .. }
            | Self::SetEdgeData { id, .. }
            | Self::SetEdgeView { id, .. } => {
                footprint.touch_edge(*id);
            }
            Self::SetEdgeEndpoints { id, from, to } => {
                footprint.touch_edge(*id);
                touch_edge_endpoints(footprint, *from);
                touch_edge_endpoints(footprint, *to);
            }

            Self::AddImport { id, .. }
            | Self::RemoveImport { id, .. }
            | Self::SetImportAlias { id, .. } => {
                footprint.touch_import(*id);
            }

            Self::AddSymbol { id, .. }
            | Self::RemoveSymbol { id, .. }
            | Self::SetSymbolName { id, .. }
            | Self::SetSymbolType { id, .. }
            | Self::SetSymbolDefaultValue { id, .. }
            | Self::SetSymbolMeta { id, .. } => {
                footprint.touch_symbol(*id);
            }

            Self::AddGroup { id, .. } | Self::SetGroupRect { id, .. } => {
                footprint.touch_group(*id);
            }
            Self::RemoveGroup {
                id,
                detached,
                bindings,
                ..
            } => {
                footprint.touch_group(*id);
                for (node, previous_parent) in detached {
                    footprint.touch_node(*node);
                    if let Some(previous_parent) = previous_parent {
                        footprint.touch_group(*previous_parent);
                    }
                }
                for (binding_id, binding) in bindings {
                    footprint.touch_binding_snapshot(*binding_id, binding);
                }
            }
            Self::SetGroupTitle { id, .. } | Self::SetGroupColor { id, .. } => {
                footprint.touch_group(*id);
            }

            Self::AddStickyNote { id, .. }
            | Self::SetStickyNoteText { id, .. }
            | Self::SetStickyNoteRect { id, .. }
            | Self::SetStickyNoteColor { id, .. } => {
                footprint.touch_sticky_note(*id);
            }
            Self::RemoveStickyNote { id, bindings, .. } => {
                footprint.touch_sticky_note(*id);
                for (binding_id, binding) in bindings {
                    footprint.touch_binding_snapshot(*binding_id, binding);
                }
            }

            Self::AddBinding { id, binding } | Self::RemoveBinding { id, binding } => {
                footprint.touch_binding_snapshot(*id, binding);
            }
            Self::SetBindingSubject { id, from, to } | Self::SetBindingTarget { id, from, to } => {
                footprint.touch_binding(*id);
                footprint.touch_binding_endpoint(from);
                footprint.touch_binding_endpoint(to);
            }
            Self::SetBindingKind { id, .. } | Self::SetBindingMeta { id, .. } => {
                footprint.touch_binding(*id);
            }
        }
    }
}

fn touch_edge_endpoints(footprint: &mut GraphMutationFootprint, endpoints: EdgeEndpoints) {
    footprint.touch_port(endpoints.from);
    footprint.touch_port(endpoints.to);
}

impl GraphTransaction {
    /// Returns the graph-local ids touched by all operations in this transaction.
    pub fn footprint(&self) -> GraphMutationFootprint {
        let mut footprint = GraphMutationFootprint::new();
        self.append_footprint(&mut footprint);
        footprint
    }

    /// Appends this transaction's touched ids to an existing footprint.
    pub fn append_footprint(&self, footprint: &mut GraphMutationFootprint) {
        for op in self.ops() {
            op.append_footprint(footprint);
        }
    }
}
