use std::borrow::Borrow;
use std::ops::Index;

use rpds::RedBlackTreeMapSync;

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

type GraphMap<K, V> = RedBlackTreeMapSync<K, V>;
type GraphMapIter<'a, K, V> = <&'a GraphMap<K, V> as IntoIterator>::IntoIter;

/// Read-only view over one graph element collection.
///
/// This keeps `Graph`'s public API from committing callers to its storage type while preserving the
/// common map-like read operations adapters need for deterministic traversal and lookup.
#[derive(Debug, Clone, Copy)]
pub struct GraphElements<'a, K: Ord, V> {
    entries: &'a GraphMap<K, V>,
}

/// Iterator over graph element ids and values.
pub struct GraphElementIter<'a, K: Ord, V> {
    inner: GraphMapIter<'a, K, V>,
}

/// Iterator over graph element ids.
pub struct GraphElementKeys<'a, K: Ord, V> {
    inner: GraphElementIter<'a, K, V>,
}

/// Iterator over graph element values.
pub struct GraphElementValues<'a, K: Ord, V> {
    inner: GraphElementIter<'a, K, V>,
}

impl<'a, K: Ord, V> GraphElements<'a, K, V> {
    pub(crate) fn new(entries: &'a GraphMap<K, V>) -> Self {
        Self { entries }
    }

    /// Returns the number of elements.
    pub fn len(&self) -> usize {
        self.entries.size()
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
            inner: self.entries.into_iter(),
        }
    }

    /// Returns ids in deterministic order.
    pub fn keys(&self) -> GraphElementKeys<'a, K, V> {
        GraphElementKeys { inner: self.iter() }
    }

    /// Returns element values in deterministic id order.
    pub fn values(&self) -> GraphElementValues<'a, K, V> {
        GraphElementValues { inner: self.iter() }
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

impl<'a, K: Ord, V> IntoIterator for GraphElements<'a, K, V> {
    type Item = (&'a K, &'a V);
    type IntoIter = GraphElementIter<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, K: Ord, V> Iterator for GraphElementIter<'a, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<'a, K: Ord, V> DoubleEndedIterator for GraphElementIter<'a, K, V> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.inner.next_back()
    }
}

impl<K: Ord, V> ExactSizeIterator for GraphElementIter<'_, K, V> {}

impl<'a, K: Ord, V> Iterator for GraphElementKeys<'a, K, V> {
    type Item = &'a K;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|(key, _)| key)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<'a, K: Ord, V> DoubleEndedIterator for GraphElementKeys<'a, K, V> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.inner.next_back().map(|(key, _)| key)
    }
}

impl<K: Ord, V> ExactSizeIterator for GraphElementKeys<'_, K, V> {}

impl<'a, K: Ord, V> Iterator for GraphElementValues<'a, K, V> {
    type Item = &'a V;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|(_, value)| value)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<'a, K: Ord, V> DoubleEndedIterator for GraphElementValues<'a, K, V> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.inner.next_back().map(|(_, value)| value)
    }
}

impl<K: Ord, V> ExactSizeIterator for GraphElementValues<'_, K, V> {}

/// Node graph document.
///
/// `Graph` intentionally has no raw serde representation. Persisted documents must cross the
/// frozen DTO and canonical loading path owned by `jellyflow-runtime`.
///
/// ```compile_fail
/// use jellyflow_core::Graph;
///
/// fn require_serialize<T: serde::Serialize>() {}
/// require_serialize::<Graph>();
/// ```
///
/// ```compile_fail
/// use jellyflow_core::Graph;
///
/// fn require_deserialize<T: for<'de> serde::Deserialize<'de>>() {}
/// require_deserialize::<Graph>();
/// ```
#[derive(Debug, Clone)]
pub struct Graph {
    /// Stable identity for editor-state lookup and cross-graph references.
    graph_id: GraphId,
    /// Schema version for migrations.
    graph_version: u32,

    /// Transitive graph dependencies (semantic subgraphs / libraries).
    pub(crate) imports: GraphMap<GraphId, GraphImport>,

    /// Graph-scoped symbols (blackboard/variables).
    pub(crate) symbols: GraphMap<SymbolId, Symbol>,

    /// Node instances.
    pub(crate) nodes: GraphMap<NodeId, Node>,

    /// Port instances (owned by nodes, but stored in a flat map for stable lookup).
    pub(crate) ports: GraphMap<PortId, Port>,

    /// Edges between ports.
    pub(crate) edges: GraphMap<EdgeId, Edge>,

    /// Optional groups.
    pub(crate) groups: GraphMap<GroupId, Group>,

    /// Optional sticky notes.
    pub(crate) sticky_notes: GraphMap<StickyNoteId, StickyNote>,

    /// Optional graph or external-source bindings.
    pub(crate) bindings: GraphMap<BindingId, Binding>,
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
            imports: GraphMap::new_sync(),
            symbols: GraphMap::new_sync(),
            nodes: GraphMap::new_sync(),
            ports: GraphMap::new_sync(),
            edges: GraphMap::new_sync(),
            groups: GraphMap::new_sync(),
            sticky_notes: GraphMap::new_sync(),
            bindings: GraphMap::new_sync(),
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

    /// Inserts or replaces a graph import.
    pub(crate) fn insert_import(&mut self, id: GraphId, value: GraphImport) -> Option<GraphImport> {
        insert_map(&mut self.imports, id, value)
    }

    /// Removes a graph import.
    pub(crate) fn remove_import(&mut self, id: &GraphId) -> Option<GraphImport> {
        remove_map(&mut self.imports, id)
    }

    /// Clears all graph imports.
    pub(crate) fn clear_imports(&mut self) {
        clear_map(&mut self.imports);
    }

    /// Retains graph imports matching `f`.
    pub(crate) fn retain_imports(&mut self, mut f: impl FnMut(&GraphId, &mut GraphImport) -> bool) {
        retain_map(&mut self.imports, &mut f);
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

    /// Inserts or replaces a symbol.
    pub(crate) fn insert_symbol(&mut self, id: SymbolId, value: Symbol) -> Option<Symbol> {
        insert_map(&mut self.symbols, id, value)
    }

    /// Removes a symbol.
    pub(crate) fn remove_symbol(&mut self, id: &SymbolId) -> Option<Symbol> {
        remove_map(&mut self.symbols, id)
    }

    /// Clears all graph symbols.
    pub(crate) fn clear_symbols(&mut self) {
        clear_map(&mut self.symbols);
    }

    /// Retains graph symbols matching `f`.
    pub(crate) fn retain_symbols(&mut self, mut f: impl FnMut(&SymbolId, &mut Symbol) -> bool) {
        retain_map(&mut self.symbols, &mut f);
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

    /// Inserts or replaces a node.
    pub(crate) fn insert_node(&mut self, id: NodeId, value: Node) -> Option<Node> {
        insert_map(&mut self.nodes, id, value)
    }

    /// Removes a node.
    pub(crate) fn remove_node(&mut self, id: &NodeId) -> Option<Node> {
        remove_map(&mut self.nodes, id)
    }

    /// Clears all graph nodes.
    pub(crate) fn clear_nodes(&mut self) {
        clear_map(&mut self.nodes);
    }

    /// Retains graph nodes matching `f`.
    pub(crate) fn retain_nodes(&mut self, mut f: impl FnMut(&NodeId, &mut Node) -> bool) {
        retain_map(&mut self.nodes, &mut f);
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

    /// Inserts or replaces a port.
    pub(crate) fn insert_port(&mut self, id: PortId, value: Port) -> Option<Port> {
        insert_map(&mut self.ports, id, value)
    }

    /// Removes a port.
    pub(crate) fn remove_port(&mut self, id: &PortId) -> Option<Port> {
        remove_map(&mut self.ports, id)
    }

    /// Clears all graph ports.
    pub(crate) fn clear_ports(&mut self) {
        clear_map(&mut self.ports);
    }

    /// Retains graph ports matching `f`.
    pub(crate) fn retain_ports(&mut self, mut f: impl FnMut(&PortId, &mut Port) -> bool) {
        retain_map(&mut self.ports, &mut f);
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

    /// Inserts or replaces an edge.
    pub(crate) fn insert_edge(&mut self, id: EdgeId, value: Edge) -> Option<Edge> {
        insert_map(&mut self.edges, id, value)
    }

    /// Removes an edge.
    pub(crate) fn remove_edge(&mut self, id: &EdgeId) -> Option<Edge> {
        remove_map(&mut self.edges, id)
    }

    /// Clears all graph edges.
    pub(crate) fn clear_edges(&mut self) {
        clear_map(&mut self.edges);
    }

    /// Retains graph edges matching `f`.
    pub(crate) fn retain_edges(&mut self, mut f: impl FnMut(&EdgeId, &mut Edge) -> bool) {
        retain_map(&mut self.edges, &mut f);
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

    /// Inserts or replaces a group.
    pub(crate) fn insert_group(&mut self, id: GroupId, value: Group) -> Option<Group> {
        insert_map(&mut self.groups, id, value)
    }

    /// Removes a group.
    pub(crate) fn remove_group(&mut self, id: &GroupId) -> Option<Group> {
        remove_map(&mut self.groups, id)
    }

    /// Clears all graph groups.
    pub(crate) fn clear_groups(&mut self) {
        clear_map(&mut self.groups);
    }

    /// Retains graph groups matching `f`.
    pub(crate) fn retain_groups(&mut self, mut f: impl FnMut(&GroupId, &mut Group) -> bool) {
        retain_map(&mut self.groups, &mut f);
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

    /// Inserts or replaces a sticky note.
    pub(crate) fn insert_sticky_note(
        &mut self,
        id: StickyNoteId,
        value: StickyNote,
    ) -> Option<StickyNote> {
        insert_map(&mut self.sticky_notes, id, value)
    }

    /// Removes a sticky note.
    pub(crate) fn remove_sticky_note(&mut self, id: &StickyNoteId) -> Option<StickyNote> {
        remove_map(&mut self.sticky_notes, id)
    }

    /// Clears all sticky notes.
    pub(crate) fn clear_sticky_notes(&mut self) {
        clear_map(&mut self.sticky_notes);
    }

    /// Retains sticky notes matching `f`.
    pub(crate) fn retain_sticky_notes(
        &mut self,
        mut f: impl FnMut(&StickyNoteId, &mut StickyNote) -> bool,
    ) {
        retain_map(&mut self.sticky_notes, &mut f);
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

    /// Inserts or replaces a binding.
    pub(crate) fn insert_binding(&mut self, id: BindingId, value: Binding) -> Option<Binding> {
        insert_map(&mut self.bindings, id, value)
    }

    /// Removes a binding.
    pub(crate) fn remove_binding(&mut self, id: &BindingId) -> Option<Binding> {
        remove_map(&mut self.bindings, id)
    }

    /// Clears all bindings.
    pub(crate) fn clear_bindings(&mut self) {
        clear_map(&mut self.bindings);
    }

    /// Retains bindings matching `f`.
    pub(crate) fn retain_bindings(&mut self, mut f: impl FnMut(&BindingId, &mut Binding) -> bool) {
        retain_map(&mut self.bindings, &mut f);
    }
}

fn insert_map<K, V>(map: &mut GraphMap<K, V>, key: K, value: V) -> Option<V>
where
    K: Ord,
    V: Clone,
{
    let previous = map.get(&key).cloned();
    map.insert_mut(key, value);
    previous
}

fn remove_map<K, V>(map: &mut GraphMap<K, V>, key: &K) -> Option<V>
where
    K: Ord,
    V: Clone,
{
    let previous = map.get(key).cloned();
    if previous.is_some() {
        map.remove_mut(key);
    }
    previous
}

fn clear_map<K: Ord, V>(map: &mut GraphMap<K, V>) {
    *map = GraphMap::new_sync();
}

fn retain_map<K, V>(map: &mut GraphMap<K, V>, f: &mut impl FnMut(&K, &mut V) -> bool)
where
    K: Ord + Clone,
    V: Clone,
{
    let mut retained = GraphMap::new_sync();
    for (key, value) in map.iter() {
        let mut value = value.clone();
        if f(key, &mut value) {
            retained.insert_mut(key.clone(), value);
        }
    }
    *map = retained;
}
