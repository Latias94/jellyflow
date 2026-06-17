use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::core::{
    Binding, BindingEndpoint, BindingId, Edge, EdgeId, GraphId, GraphLocalBindingTarget, GroupId,
    Node, NodeId, Port, PortId, StickyNoteId, SymbolId,
};

use super::{batch::GraphTransaction, endpoints::EdgeEndpoints, op::GraphOp};

fn is_false(value: &bool) -> bool {
    !*value
}

/// Model ids touched by one graph mutation.
///
/// This is intentionally separate from graph storage. Hosts can use it to invalidate indexes,
/// identify collaboration conflict/dependency boundaries, or derive more specific downstream work.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphMutationFootprint {
    /// The graph document itself was touched.
    #[serde(default, skip_serializing_if = "is_false")]
    pub graph: bool,
    /// Import records or import references touched by the mutation.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub imports: BTreeSet<GraphId>,
    /// Symbol records touched by the mutation.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub symbols: BTreeSet<SymbolId>,
    /// Node records or node references touched by the mutation.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub nodes: BTreeSet<NodeId>,
    /// Port records or port references touched by the mutation.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub ports: BTreeSet<PortId>,
    /// Edge records or edge references touched by the mutation.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub edges: BTreeSet<EdgeId>,
    /// Group records or group references touched by the mutation.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub groups: BTreeSet<GroupId>,
    /// Sticky note records or sticky note references touched by the mutation.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub sticky_notes: BTreeSet<StickyNoteId>,
    /// Binding records touched by the mutation.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub bindings: BTreeSet<BindingId>,
}

impl GraphMutationFootprint {
    /// Creates an empty footprint.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns `true` when this footprint does not touch any graph-local object.
    pub fn is_empty(&self) -> bool {
        !self.graph
            && self.imports.is_empty()
            && self.symbols.is_empty()
            && self.nodes.is_empty()
            && self.ports.is_empty()
            && self.edges.is_empty()
            && self.groups.is_empty()
            && self.sticky_notes.is_empty()
            && self.bindings.is_empty()
    }

    /// Merges another footprint into this one.
    pub fn extend(&mut self, other: Self) {
        self.graph |= other.graph;
        self.imports.extend(other.imports);
        self.symbols.extend(other.symbols);
        self.nodes.extend(other.nodes);
        self.ports.extend(other.ports);
        self.edges.extend(other.edges);
        self.groups.extend(other.groups);
        self.sticky_notes.extend(other.sticky_notes);
        self.bindings.extend(other.bindings);
    }

    /// Marks the whole graph document as touched.
    pub fn touch_graph(&mut self) {
        self.graph = true;
    }

    /// Marks a node id as touched.
    pub fn touch_node(&mut self, id: NodeId) {
        self.nodes.insert(id);
    }

    /// Marks a port id as touched.
    pub fn touch_port(&mut self, id: PortId) {
        self.ports.insert(id);
    }

    /// Marks an edge id as touched.
    pub fn touch_edge(&mut self, id: EdgeId) {
        self.edges.insert(id);
    }

    /// Marks a group id as touched.
    pub fn touch_group(&mut self, id: GroupId) {
        self.groups.insert(id);
    }

    /// Marks a sticky note id as touched.
    pub fn touch_sticky_note(&mut self, id: StickyNoteId) {
        self.sticky_notes.insert(id);
    }

    /// Marks a binding id as touched.
    pub fn touch_binding(&mut self, id: BindingId) {
        self.bindings.insert(id);
    }

    /// Marks an import id as touched.
    pub fn touch_import(&mut self, id: GraphId) {
        self.imports.insert(id);
    }

    /// Marks a symbol id as touched.
    pub fn touch_symbol(&mut self, id: SymbolId) {
        self.symbols.insert(id);
    }

    /// Marks ids referenced by a node snapshot.
    pub fn touch_node_snapshot(&mut self, id: NodeId, node: &Node) {
        self.touch_node(id);
        self.ports.extend(node.ports.iter().copied());
        if let Some(parent) = node.parent {
            self.touch_group(parent);
        }
    }

    /// Marks ids referenced by a port snapshot.
    pub fn touch_port_snapshot(&mut self, id: PortId, port: &Port) {
        self.touch_port(id);
        self.touch_node(port.node);
    }

    /// Marks ids referenced by an edge snapshot.
    pub fn touch_edge_snapshot(&mut self, id: EdgeId, edge: &Edge) {
        self.touch_edge(id);
        self.touch_edge_endpoints(EdgeEndpoints::from_edge(edge));
    }

    /// Marks ids referenced by edge endpoint ports.
    pub fn touch_edge_endpoints(&mut self, endpoints: EdgeEndpoints) {
        self.touch_port(endpoints.from);
        self.touch_port(endpoints.to);
    }

    /// Marks ids referenced by a binding snapshot.
    pub fn touch_binding_snapshot(&mut self, id: BindingId, binding: &Binding) {
        self.touch_binding(id);
        self.touch_binding_endpoint(&binding.subject);
        self.touch_binding_endpoint(&binding.target);
    }

    /// Marks ids referenced by one binding endpoint.
    pub fn touch_binding_endpoint(&mut self, endpoint: &BindingEndpoint) {
        let Some(target) = endpoint.graph_local_target() else {
            return;
        };

        match target {
            GraphLocalBindingTarget::Graph => self.touch_graph(),
            GraphLocalBindingTarget::Node { id } => self.touch_node(id),
            GraphLocalBindingTarget::Port { id } => self.touch_port(id),
            GraphLocalBindingTarget::Edge { id } => self.touch_edge(id),
            GraphLocalBindingTarget::Group { id } => self.touch_group(id),
            GraphLocalBindingTarget::StickyNote { id } => self.touch_sticky_note(id),
        }
    }
}

impl GraphOp {
    /// Returns the graph-local ids touched by this operation.
    pub fn footprint(&self) -> GraphMutationFootprint {
        let mut footprint = GraphMutationFootprint::new();
        self.append_footprint(&mut footprint);
        footprint
    }

    /// Appends this operation's touched ids to an existing footprint.
    pub fn append_footprint(&self, footprint: &mut GraphMutationFootprint) {
        match self {
            Self::AddNode { id, node } | Self::RemoveNode { id, node, .. } => {
                footprint.touch_node_snapshot(*id, node);
            }
            Self::SetNodePos { id, .. }
            | Self::SetNodeOrigin { id, .. }
            | Self::SetNodeKind { id, .. }
            | Self::SetNodeKindVersion { id, .. }
            | Self::SetNodeSelectable { id, .. }
            | Self::SetNodeFocusable { id, .. }
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

            Self::AddPort { id, port } | Self::RemovePort { id, port, .. } => {
                footprint.touch_port_snapshot(*id, port);
            }
            Self::SetPortConnectable { id, .. }
            | Self::SetPortConnectableStart { id, .. }
            | Self::SetPortConnectableEnd { id, .. }
            | Self::SetPortType { id, .. }
            | Self::SetPortData { id, .. } => {
                footprint.touch_port(*id);
            }

            Self::AddEdge { id, edge } | Self::RemoveEdge { id, edge, .. } => {
                footprint.touch_edge_snapshot(*id, edge);
            }
            Self::SetEdgeKind { id, .. }
            | Self::SetEdgeSelectable { id, .. }
            | Self::SetEdgeFocusable { id, .. }
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
                footprint.touch_edge_endpoints(*from);
                footprint.touch_edge_endpoints(*to);
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

        self.append_cascaded_footprint(footprint);
    }

    fn append_cascaded_footprint(&self, footprint: &mut GraphMutationFootprint) {
        match self {
            Self::RemoveNode {
                ports,
                edges,
                bindings,
                ..
            } => {
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
            Self::RemovePort {
                edges, bindings, ..
            } => {
                for (edge_id, edge) in edges {
                    footprint.touch_edge_snapshot(*edge_id, edge);
                }
                for (binding_id, binding) in bindings {
                    footprint.touch_binding_snapshot(*binding_id, binding);
                }
            }
            Self::RemoveEdge { bindings, .. } => {
                for (binding_id, binding) in bindings {
                    footprint.touch_binding_snapshot(*binding_id, binding);
                }
            }
            _ => {}
        }
    }
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
