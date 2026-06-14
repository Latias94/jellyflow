use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::core::{
    Binding, BindingId, Edge, EdgeId, GraphId, GraphImport, Group, GroupId, Node, NodeId, Port,
    PortId, StickyNote, StickyNoteId, Symbol, SymbolId,
};

/// Wrapper version for `GraphFragment`.
pub const GRAPH_FRAGMENT_VERSION: u32 = 1;

/// A deterministic, serializable graph fragment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphFragment {
    pub version: u32,
    pub nodes: BTreeMap<NodeId, Node>,
    pub ports: BTreeMap<PortId, Port>,
    pub edges: BTreeMap<EdgeId, Edge>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub imports: BTreeMap<GraphId, GraphImport>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub groups: BTreeMap<GroupId, Group>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub sticky_notes: BTreeMap<StickyNoteId, StickyNote>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub symbols: BTreeMap<SymbolId, Symbol>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub bindings: BTreeMap<BindingId, Binding>,
}

impl Default for GraphFragment {
    fn default() -> Self {
        Self {
            version: GRAPH_FRAGMENT_VERSION,
            nodes: BTreeMap::new(),
            ports: BTreeMap::new(),
            edges: BTreeMap::new(),
            imports: BTreeMap::new(),
            groups: BTreeMap::new(),
            sticky_notes: BTreeMap::new(),
            symbols: BTreeMap::new(),
            bindings: BTreeMap::new(),
        }
    }
}

impl GraphFragment {
    pub fn nodes(&self) -> &BTreeMap<NodeId, Node> {
        &self.nodes
    }

    pub fn node_mut(&mut self, id: &NodeId) -> Option<&mut Node> {
        self.nodes.get_mut(id)
    }

    pub fn insert_node(&mut self, id: NodeId, value: Node) -> Option<Node> {
        self.nodes.insert(id, value)
    }

    pub fn ports(&self) -> &BTreeMap<PortId, Port> {
        &self.ports
    }

    pub fn port_mut(&mut self, id: &PortId) -> Option<&mut Port> {
        self.ports.get_mut(id)
    }

    pub fn insert_port(&mut self, id: PortId, value: Port) -> Option<Port> {
        self.ports.insert(id, value)
    }

    pub fn edges(&self) -> &BTreeMap<EdgeId, Edge> {
        &self.edges
    }

    pub fn edge_mut(&mut self, id: &EdgeId) -> Option<&mut Edge> {
        self.edges.get_mut(id)
    }

    pub fn insert_edge(&mut self, id: EdgeId, value: Edge) -> Option<Edge> {
        self.edges.insert(id, value)
    }

    pub fn imports(&self) -> &BTreeMap<GraphId, GraphImport> {
        &self.imports
    }

    pub fn insert_import(&mut self, id: GraphId, value: GraphImport) -> Option<GraphImport> {
        self.imports.insert(id, value)
    }

    pub fn groups(&self) -> &BTreeMap<GroupId, Group> {
        &self.groups
    }

    pub fn group_mut(&mut self, id: &GroupId) -> Option<&mut Group> {
        self.groups.get_mut(id)
    }

    pub fn insert_group(&mut self, id: GroupId, value: Group) -> Option<Group> {
        self.groups.insert(id, value)
    }

    pub fn sticky_notes(&self) -> &BTreeMap<StickyNoteId, StickyNote> {
        &self.sticky_notes
    }

    pub fn sticky_note_mut(&mut self, id: &StickyNoteId) -> Option<&mut StickyNote> {
        self.sticky_notes.get_mut(id)
    }

    pub fn insert_sticky_note(
        &mut self,
        id: StickyNoteId,
        value: StickyNote,
    ) -> Option<StickyNote> {
        self.sticky_notes.insert(id, value)
    }

    pub fn symbols(&self) -> &BTreeMap<SymbolId, Symbol> {
        &self.symbols
    }

    pub fn symbol_mut(&mut self, id: &SymbolId) -> Option<&mut Symbol> {
        self.symbols.get_mut(id)
    }

    pub fn insert_symbol(&mut self, id: SymbolId, value: Symbol) -> Option<Symbol> {
        self.symbols.insert(id, value)
    }

    pub fn bindings(&self) -> &BTreeMap<BindingId, Binding> {
        &self.bindings
    }

    pub fn binding_mut(&mut self, id: &BindingId) -> Option<&mut Binding> {
        self.bindings.get_mut(id)
    }

    pub fn insert_binding(&mut self, id: BindingId, value: Binding) -> Option<Binding> {
        self.bindings.insert(id, value)
    }
}
