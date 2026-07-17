use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::ids::{BindingId, EdgeId, GraphId, GroupId, NodeId, PortId, StickyNoteId, SymbolId};
use super::model::{Binding, BindingEndpoint, Edge, GraphLocalBindingTarget, Node, Port};

fn is_false(value: &bool) -> bool {
    !*value
}

/// Model ids touched by one graph mutation.
///
/// This value is intentionally independent of any mutation or transaction kernel. Hosts can use
/// it to invalidate indexes, identify collaboration conflict/dependency boundaries, or derive
/// more specific downstream work.
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
        self.touch_edge_ports(edge.from, edge.to);
    }

    /// Marks the two ports referenced by an edge.
    pub fn touch_edge_ports(&mut self, from: PortId, to: PortId) {
        self.touch_port(from);
        self.touch_port(to);
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
