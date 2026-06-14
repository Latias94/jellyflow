use std::borrow::Borrow;
use std::collections::{BTreeMap, btree_map};
use std::iter::FusedIterator;
use std::ops::Index;

use serde::{Deserialize, Serialize};

use crate::core::ids::{
    BindingId, EdgeId, GraphId, GroupId, NodeId, PortId, StickyNoteId, SymbolId,
};
use crate::core::imports::GraphImport;

use super::binding::Binding;
use super::edge::Edge;
use super::node::Node;
use super::port::Port;
use super::resources::{Group, StickyNote, Symbol};

/// Graph schema version (v1).
pub const GRAPH_VERSION: u32 = 1;

/// Read-only view over one graph element collection.
///
/// This keeps `Graph`'s public API from committing callers to its storage type while preserving the
/// common map-like read operations adapters need for deterministic traversal and lookup.
#[derive(Debug, Clone, Copy)]
pub struct GraphElements<'a, K, V> {
    entries: &'a BTreeMap<K, V>,
}

/// Iterator over graph element ids and values.
#[derive(Debug, Clone)]
pub struct GraphElementIter<'a, K, V> {
    inner: btree_map::Iter<'a, K, V>,
}

/// Iterator over graph element ids.
#[derive(Debug, Clone)]
pub struct GraphElementKeys<'a, K, V> {
    inner: btree_map::Keys<'a, K, V>,
}

/// Iterator over graph element values.
#[derive(Debug, Clone)]
pub struct GraphElementValues<'a, K, V> {
    inner: btree_map::Values<'a, K, V>,
}

impl<'a, K, V> GraphElements<'a, K, V> {
    pub(crate) fn new(entries: &'a BTreeMap<K, V>) -> Self {
        Self { entries }
    }

    /// Returns the number of elements.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` when this collection has no elements.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns `true` when the collection contains `key`.
    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q> + Ord,
        Q: Ord + ?Sized,
    {
        self.entries.contains_key(key)
    }

    /// Returns an element by id.
    pub fn get<Q>(&self, key: &Q) -> Option<&'a V>
    where
        K: Borrow<Q> + Ord,
        Q: Ord + ?Sized,
    {
        self.entries.get(key)
    }

    /// Returns key-value pairs in deterministic id order.
    pub fn iter(&self) -> GraphElementIter<'a, K, V> {
        GraphElementIter {
            inner: self.entries.iter(),
        }
    }

    /// Returns ids in deterministic order.
    pub fn keys(&self) -> GraphElementKeys<'a, K, V> {
        GraphElementKeys {
            inner: self.entries.keys(),
        }
    }

    /// Returns element values in deterministic id order.
    pub fn values(&self) -> GraphElementValues<'a, K, V> {
        GraphElementValues {
            inner: self.entries.values(),
        }
    }
}

impl<'a, 'b, K, V> PartialEq<GraphElements<'b, K, V>> for GraphElements<'a, K, V>
where
    K: Ord + PartialEq,
    V: PartialEq,
{
    fn eq(&self, other: &GraphElements<'b, K, V>) -> bool {
        self.entries == other.entries
    }
}

impl<'a, K, V> Eq for GraphElements<'a, K, V>
where
    K: Ord + Eq,
    V: Eq,
{
}

impl<'a, K, V> Index<&K> for GraphElements<'a, K, V>
where
    K: Ord,
{
    type Output = V;

    fn index(&self, key: &K) -> &Self::Output {
        self.entries.get(key).expect("no entry found for key")
    }
}

impl<'a, K, V> IntoIterator for GraphElements<'a, K, V> {
    type Item = (&'a K, &'a V);
    type IntoIter = GraphElementIter<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, K, V> Iterator for GraphElementIter<'a, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<'a, K, V> DoubleEndedIterator for GraphElementIter<'a, K, V> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.inner.next_back()
    }
}

impl<'a, K, V> ExactSizeIterator for GraphElementIter<'a, K, V> {}
impl<'a, K, V> FusedIterator for GraphElementIter<'a, K, V> {}

impl<'a, K, V> Iterator for GraphElementKeys<'a, K, V> {
    type Item = &'a K;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<'a, K, V> DoubleEndedIterator for GraphElementKeys<'a, K, V> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.inner.next_back()
    }
}

impl<'a, K, V> ExactSizeIterator for GraphElementKeys<'a, K, V> {}
impl<'a, K, V> FusedIterator for GraphElementKeys<'a, K, V> {}

impl<'a, K, V> Iterator for GraphElementValues<'a, K, V> {
    type Item = &'a V;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<'a, K, V> DoubleEndedIterator for GraphElementValues<'a, K, V> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.inner.next_back()
    }
}

impl<'a, K, V> ExactSizeIterator for GraphElementValues<'a, K, V> {}
impl<'a, K, V> FusedIterator for GraphElementValues<'a, K, V> {}

/// Node graph document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Graph {
    /// Stable identity for editor-state lookup and cross-graph references.
    graph_id: GraphId,
    /// Schema version for migrations.
    graph_version: u32,

    /// Transitive graph dependencies (semantic subgraphs / libraries).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) imports: BTreeMap<GraphId, GraphImport>,

    /// Graph-scoped symbols (blackboard/variables).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) symbols: BTreeMap<SymbolId, Symbol>,

    /// Node instances.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) nodes: BTreeMap<NodeId, Node>,

    /// Port instances (owned by nodes, but stored in a flat map for stable lookup).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) ports: BTreeMap<PortId, Port>,

    /// Edges between ports.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) edges: BTreeMap<EdgeId, Edge>,

    /// Optional groups.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) groups: BTreeMap<GroupId, Group>,

    /// Optional sticky notes.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) sticky_notes: BTreeMap<StickyNoteId, StickyNote>,

    /// Optional knowledge-canvas bindings.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) bindings: BTreeMap<BindingId, Binding>,
}

impl Default for Graph {
    fn default() -> Self {
        Self::new(GraphId::new())
    }
}

impl Graph {
    /// Creates a new, empty graph with the given id.
    pub fn new(graph_id: GraphId) -> Self {
        Self {
            graph_id,
            graph_version: GRAPH_VERSION,
            imports: BTreeMap::new(),
            symbols: BTreeMap::new(),
            nodes: BTreeMap::new(),
            ports: BTreeMap::new(),
            edges: BTreeMap::new(),
            groups: BTreeMap::new(),
            sticky_notes: BTreeMap::new(),
            bindings: BTreeMap::new(),
        }
    }

    /// Returns the stable graph identity.
    pub fn graph_id(&self) -> GraphId {
        self.graph_id
    }

    /// Returns the schema version.
    pub fn graph_version(&self) -> u32 {
        self.graph_version
    }

    /// Returns graph imports.
    pub fn imports(&self) -> GraphElements<'_, GraphId, GraphImport> {
        GraphElements::new(&self.imports)
    }

    /// Returns one graph import.
    pub fn import(&self, id: &GraphId) -> Option<&GraphImport> {
        self.imports.get(id)
    }

    /// Returns one mutable graph import.
    pub(crate) fn import_mut(&mut self, id: &GraphId) -> Option<&mut GraphImport> {
        self.imports.get_mut(id)
    }

    /// Updates one graph import through a controlled mutable callback.
    pub(crate) fn update_import<R>(
        &mut self,
        id: &GraphId,
        f: impl FnOnce(&mut GraphImport) -> R,
    ) -> Option<R> {
        self.imports.get_mut(id).map(f)
    }

    /// Inserts or replaces a graph import.
    pub(crate) fn insert_import(&mut self, id: GraphId, value: GraphImport) -> Option<GraphImport> {
        self.imports.insert(id, value)
    }

    /// Removes a graph import.
    pub(crate) fn remove_import(&mut self, id: &GraphId) -> Option<GraphImport> {
        self.imports.remove(id)
    }

    /// Clears all graph imports.
    pub(crate) fn clear_imports(&mut self) {
        self.imports.clear();
    }

    /// Retains graph imports matching `f`.
    pub(crate) fn retain_imports(&mut self, f: impl FnMut(&GraphId, &mut GraphImport) -> bool) {
        self.imports.retain(f);
    }

    /// Returns graph symbols.
    pub fn symbols(&self) -> GraphElements<'_, SymbolId, Symbol> {
        GraphElements::new(&self.symbols)
    }

    /// Returns one symbol.
    pub fn symbol(&self, id: &SymbolId) -> Option<&Symbol> {
        self.symbols.get(id)
    }

    /// Returns one mutable symbol.
    pub(crate) fn symbol_mut(&mut self, id: &SymbolId) -> Option<&mut Symbol> {
        self.symbols.get_mut(id)
    }

    /// Updates one symbol through a controlled mutable callback.
    pub(crate) fn update_symbol<R>(
        &mut self,
        id: &SymbolId,
        f: impl FnOnce(&mut Symbol) -> R,
    ) -> Option<R> {
        self.symbols.get_mut(id).map(f)
    }

    /// Inserts or replaces a symbol.
    pub(crate) fn insert_symbol(&mut self, id: SymbolId, value: Symbol) -> Option<Symbol> {
        self.symbols.insert(id, value)
    }

    /// Removes a symbol.
    pub(crate) fn remove_symbol(&mut self, id: &SymbolId) -> Option<Symbol> {
        self.symbols.remove(id)
    }

    /// Clears all graph symbols.
    pub(crate) fn clear_symbols(&mut self) {
        self.symbols.clear();
    }

    /// Retains graph symbols matching `f`.
    pub(crate) fn retain_symbols(&mut self, f: impl FnMut(&SymbolId, &mut Symbol) -> bool) {
        self.symbols.retain(f);
    }

    /// Returns graph nodes.
    pub fn nodes(&self) -> GraphElements<'_, NodeId, Node> {
        GraphElements::new(&self.nodes)
    }

    /// Returns one node.
    pub fn node(&self, id: &NodeId) -> Option<&Node> {
        self.nodes.get(id)
    }

    /// Returns one mutable node.
    pub(crate) fn node_mut(&mut self, id: &NodeId) -> Option<&mut Node> {
        self.nodes.get_mut(id)
    }

    /// Updates one node through a controlled mutable callback.
    pub(crate) fn update_node<R>(
        &mut self,
        id: &NodeId,
        f: impl FnOnce(&mut Node) -> R,
    ) -> Option<R> {
        self.nodes.get_mut(id).map(f)
    }

    /// Inserts or replaces a node.
    pub(crate) fn insert_node(&mut self, id: NodeId, value: Node) -> Option<Node> {
        self.nodes.insert(id, value)
    }

    /// Removes a node.
    pub(crate) fn remove_node(&mut self, id: &NodeId) -> Option<Node> {
        self.nodes.remove(id)
    }

    /// Clears all graph nodes.
    pub(crate) fn clear_nodes(&mut self) {
        self.nodes.clear();
    }

    /// Retains graph nodes matching `f`.
    pub(crate) fn retain_nodes(&mut self, f: impl FnMut(&NodeId, &mut Node) -> bool) {
        self.nodes.retain(f);
    }

    /// Returns graph ports.
    pub fn ports(&self) -> GraphElements<'_, PortId, Port> {
        GraphElements::new(&self.ports)
    }

    /// Returns one port.
    pub fn port(&self, id: &PortId) -> Option<&Port> {
        self.ports.get(id)
    }

    /// Returns one mutable port.
    pub(crate) fn port_mut(&mut self, id: &PortId) -> Option<&mut Port> {
        self.ports.get_mut(id)
    }

    /// Updates one port through a controlled mutable callback.
    pub(crate) fn update_port<R>(
        &mut self,
        id: &PortId,
        f: impl FnOnce(&mut Port) -> R,
    ) -> Option<R> {
        self.ports.get_mut(id).map(f)
    }

    /// Inserts or replaces a port.
    pub(crate) fn insert_port(&mut self, id: PortId, value: Port) -> Option<Port> {
        self.ports.insert(id, value)
    }

    /// Removes a port.
    pub(crate) fn remove_port(&mut self, id: &PortId) -> Option<Port> {
        self.ports.remove(id)
    }

    /// Clears all graph ports.
    pub(crate) fn clear_ports(&mut self) {
        self.ports.clear();
    }

    /// Retains graph ports matching `f`.
    pub(crate) fn retain_ports(&mut self, f: impl FnMut(&PortId, &mut Port) -> bool) {
        self.ports.retain(f);
    }

    /// Returns graph edges.
    pub fn edges(&self) -> GraphElements<'_, EdgeId, Edge> {
        GraphElements::new(&self.edges)
    }

    /// Returns one edge.
    pub fn edge(&self, id: &EdgeId) -> Option<&Edge> {
        self.edges.get(id)
    }

    /// Returns one mutable edge.
    pub(crate) fn edge_mut(&mut self, id: &EdgeId) -> Option<&mut Edge> {
        self.edges.get_mut(id)
    }

    /// Updates one edge through a controlled mutable callback.
    pub(crate) fn update_edge<R>(
        &mut self,
        id: &EdgeId,
        f: impl FnOnce(&mut Edge) -> R,
    ) -> Option<R> {
        self.edges.get_mut(id).map(f)
    }

    /// Inserts or replaces an edge.
    pub(crate) fn insert_edge(&mut self, id: EdgeId, value: Edge) -> Option<Edge> {
        self.edges.insert(id, value)
    }

    /// Removes an edge.
    pub(crate) fn remove_edge(&mut self, id: &EdgeId) -> Option<Edge> {
        self.edges.remove(id)
    }

    /// Clears all graph edges.
    pub(crate) fn clear_edges(&mut self) {
        self.edges.clear();
    }

    /// Retains graph edges matching `f`.
    pub(crate) fn retain_edges(&mut self, f: impl FnMut(&EdgeId, &mut Edge) -> bool) {
        self.edges.retain(f);
    }

    /// Returns graph groups.
    pub fn groups(&self) -> GraphElements<'_, GroupId, Group> {
        GraphElements::new(&self.groups)
    }

    /// Returns one group.
    pub fn group(&self, id: &GroupId) -> Option<&Group> {
        self.groups.get(id)
    }

    /// Returns one mutable group.
    pub(crate) fn group_mut(&mut self, id: &GroupId) -> Option<&mut Group> {
        self.groups.get_mut(id)
    }

    /// Updates one group through a controlled mutable callback.
    pub(crate) fn update_group<R>(
        &mut self,
        id: &GroupId,
        f: impl FnOnce(&mut Group) -> R,
    ) -> Option<R> {
        self.groups.get_mut(id).map(f)
    }

    /// Inserts or replaces a group.
    pub(crate) fn insert_group(&mut self, id: GroupId, value: Group) -> Option<Group> {
        self.groups.insert(id, value)
    }

    /// Removes a group.
    pub(crate) fn remove_group(&mut self, id: &GroupId) -> Option<Group> {
        self.groups.remove(id)
    }

    /// Clears all graph groups.
    pub(crate) fn clear_groups(&mut self) {
        self.groups.clear();
    }

    /// Retains graph groups matching `f`.
    pub(crate) fn retain_groups(&mut self, f: impl FnMut(&GroupId, &mut Group) -> bool) {
        self.groups.retain(f);
    }

    /// Returns sticky notes.
    pub fn sticky_notes(&self) -> GraphElements<'_, StickyNoteId, StickyNote> {
        GraphElements::new(&self.sticky_notes)
    }

    /// Returns one sticky note.
    pub fn sticky_note(&self, id: &StickyNoteId) -> Option<&StickyNote> {
        self.sticky_notes.get(id)
    }

    /// Returns one mutable sticky note.
    pub(crate) fn sticky_note_mut(&mut self, id: &StickyNoteId) -> Option<&mut StickyNote> {
        self.sticky_notes.get_mut(id)
    }

    /// Updates one sticky note through a controlled mutable callback.
    pub(crate) fn update_sticky_note<R>(
        &mut self,
        id: &StickyNoteId,
        f: impl FnOnce(&mut StickyNote) -> R,
    ) -> Option<R> {
        self.sticky_notes.get_mut(id).map(f)
    }

    /// Inserts or replaces a sticky note.
    pub(crate) fn insert_sticky_note(
        &mut self,
        id: StickyNoteId,
        value: StickyNote,
    ) -> Option<StickyNote> {
        self.sticky_notes.insert(id, value)
    }

    /// Removes a sticky note.
    pub(crate) fn remove_sticky_note(&mut self, id: &StickyNoteId) -> Option<StickyNote> {
        self.sticky_notes.remove(id)
    }

    /// Clears all sticky notes.
    pub(crate) fn clear_sticky_notes(&mut self) {
        self.sticky_notes.clear();
    }

    /// Retains sticky notes matching `f`.
    pub(crate) fn retain_sticky_notes(
        &mut self,
        f: impl FnMut(&StickyNoteId, &mut StickyNote) -> bool,
    ) {
        self.sticky_notes.retain(f);
    }

    /// Returns bindings.
    pub fn bindings(&self) -> GraphElements<'_, BindingId, Binding> {
        GraphElements::new(&self.bindings)
    }

    /// Returns one binding.
    pub fn binding(&self, id: &BindingId) -> Option<&Binding> {
        self.bindings.get(id)
    }

    /// Returns one mutable binding.
    pub(crate) fn binding_mut(&mut self, id: &BindingId) -> Option<&mut Binding> {
        self.bindings.get_mut(id)
    }

    /// Updates one binding through a controlled mutable callback.
    pub(crate) fn update_binding<R>(
        &mut self,
        id: &BindingId,
        f: impl FnOnce(&mut Binding) -> R,
    ) -> Option<R> {
        self.bindings.get_mut(id).map(f)
    }

    /// Inserts or replaces a binding.
    pub(crate) fn insert_binding(&mut self, id: BindingId, value: Binding) -> Option<Binding> {
        self.bindings.insert(id, value)
    }

    /// Removes a binding.
    pub(crate) fn remove_binding(&mut self, id: &BindingId) -> Option<Binding> {
        self.bindings.remove(id)
    }

    /// Clears all bindings.
    pub(crate) fn clear_bindings(&mut self) {
        self.bindings.clear();
    }

    /// Retains bindings matching `f`.
    pub(crate) fn retain_bindings(&mut self, f: impl FnMut(&BindingId, &mut Binding) -> bool) {
        self.bindings.retain(f);
    }
}
