use std::ops::Deref;

use crate::core::ids::{
    BindingId, EdgeId, GraphId, GroupId, NodeId, PortId, StickyNoteId, SymbolId,
};
use crate::core::imports::GraphImport;
use crate::core::validate::{GraphValidationError, validate_graph};

use super::binding::Binding;
use super::edge::Edge;
use super::graph::Graph;
use super::node::Node;
use super::port::Port;
use super::resources::{Group, StickyNote, Symbol};

/// Builder for assembling an initial graph document before handing it to runtime mutation APIs.
///
/// The builder keeps direct collection writes out of `Graph`'s public API while still making
/// fixtures, importers, and graph generators straightforward. Use [`GraphBuilder::build`] when the
/// result should satisfy graph invariants, or [`GraphBuilder::build_unchecked`] only for tests and
/// migrations that intentionally inspect invalid graphs.
#[derive(Debug, Clone)]
pub struct GraphBuilder {
    graph: Graph,
}

impl GraphBuilder {
    /// Creates an empty builder for the given graph id.
    pub fn new(graph_id: GraphId) -> Self {
        Self {
            graph: Graph::new(graph_id),
        }
    }

    /// Starts from an existing graph.
    pub(crate) fn from_graph(graph: Graph) -> Self {
        Self { graph }
    }

    /// Returns the graph id being built.
    pub fn graph_id(&self) -> GraphId {
        self.graph.graph_id()
    }

    /// Borrows the current graph snapshot.
    pub fn graph(&self) -> &Graph {
        &self.graph
    }

    /// Adds or replaces an import.
    pub fn with_import(mut self, id: GraphId, import: GraphImport) -> Self {
        self.insert_import(id, import);
        self
    }

    /// Adds or replaces a symbol.
    pub fn with_symbol(mut self, id: SymbolId, symbol: Symbol) -> Self {
        self.insert_symbol(id, symbol);
        self
    }

    /// Adds or replaces a node.
    pub fn with_node(mut self, id: NodeId, node: Node) -> Self {
        self.insert_node(id, node);
        self
    }

    /// Adds or replaces a port.
    pub fn with_port(mut self, id: PortId, port: Port) -> Self {
        self.insert_port(id, port);
        self
    }

    /// Adds or replaces an edge.
    pub fn with_edge(mut self, id: EdgeId, edge: Edge) -> Self {
        self.insert_edge(id, edge);
        self
    }

    /// Adds or replaces a group.
    pub fn with_group(mut self, id: GroupId, group: Group) -> Self {
        self.insert_group(id, group);
        self
    }

    /// Adds or replaces a sticky note.
    pub fn with_sticky_note(mut self, id: StickyNoteId, note: StickyNote) -> Self {
        self.insert_sticky_note(id, note);
        self
    }

    /// Adds or replaces a binding.
    pub fn with_binding(mut self, id: BindingId, binding: Binding) -> Self {
        self.insert_binding(id, binding);
        self
    }

    /// Inserts or replaces an import in-place.
    pub fn insert_import(&mut self, id: GraphId, import: GraphImport) -> Option<GraphImport> {
        self.graph.insert_import(id, import)
    }

    /// Updates an import in-place.
    pub fn update_import<R>(
        &mut self,
        id: &GraphId,
        f: impl FnOnce(&mut GraphImport) -> R,
    ) -> Option<R> {
        self.graph.update_import(id, f)
    }

    /// Removes an import in-place.
    pub fn remove_import(&mut self, id: &GraphId) -> Option<GraphImport> {
        self.graph.remove_import(id)
    }

    /// Clears imports in-place.
    pub fn clear_imports(&mut self) {
        self.graph.clear_imports();
    }

    /// Retains imports matching `f`.
    pub fn retain_imports(&mut self, f: impl FnMut(&GraphId, &mut GraphImport) -> bool) {
        self.graph.retain_imports(f);
    }

    /// Inserts or replaces a symbol in-place.
    pub fn insert_symbol(&mut self, id: SymbolId, symbol: Symbol) -> Option<Symbol> {
        self.graph.insert_symbol(id, symbol)
    }

    /// Updates a symbol in-place.
    pub fn update_symbol<R>(
        &mut self,
        id: &SymbolId,
        f: impl FnOnce(&mut Symbol) -> R,
    ) -> Option<R> {
        self.graph.update_symbol(id, f)
    }

    /// Removes a symbol in-place.
    pub fn remove_symbol(&mut self, id: &SymbolId) -> Option<Symbol> {
        self.graph.remove_symbol(id)
    }

    /// Clears symbols in-place.
    pub fn clear_symbols(&mut self) {
        self.graph.clear_symbols();
    }

    /// Retains symbols matching `f`.
    pub fn retain_symbols(&mut self, f: impl FnMut(&SymbolId, &mut Symbol) -> bool) {
        self.graph.retain_symbols(f);
    }

    /// Inserts or replaces a node in-place.
    pub fn insert_node(&mut self, id: NodeId, node: Node) -> Option<Node> {
        self.graph.insert_node(id, node)
    }

    /// Updates a node in-place.
    pub fn update_node<R>(&mut self, id: &NodeId, f: impl FnOnce(&mut Node) -> R) -> Option<R> {
        self.graph.update_node(id, f)
    }

    /// Removes a node in-place.
    pub fn remove_node(&mut self, id: &NodeId) -> Option<Node> {
        self.graph.remove_node(id)
    }

    /// Clears nodes in-place.
    pub fn clear_nodes(&mut self) {
        self.graph.clear_nodes();
    }

    /// Retains nodes matching `f`.
    pub fn retain_nodes(&mut self, f: impl FnMut(&NodeId, &mut Node) -> bool) {
        self.graph.retain_nodes(f);
    }

    /// Inserts or replaces a port in-place.
    pub fn insert_port(&mut self, id: PortId, port: Port) -> Option<Port> {
        self.graph.insert_port(id, port)
    }

    /// Updates a port in-place.
    pub fn update_port<R>(&mut self, id: &PortId, f: impl FnOnce(&mut Port) -> R) -> Option<R> {
        self.graph.update_port(id, f)
    }

    /// Removes a port in-place.
    pub fn remove_port(&mut self, id: &PortId) -> Option<Port> {
        self.graph.remove_port(id)
    }

    /// Clears ports in-place.
    pub fn clear_ports(&mut self) {
        self.graph.clear_ports();
    }

    /// Retains ports matching `f`.
    pub fn retain_ports(&mut self, f: impl FnMut(&PortId, &mut Port) -> bool) {
        self.graph.retain_ports(f);
    }

    /// Inserts or replaces an edge in-place.
    pub fn insert_edge(&mut self, id: EdgeId, edge: Edge) -> Option<Edge> {
        self.graph.insert_edge(id, edge)
    }

    /// Updates an edge in-place.
    pub fn update_edge<R>(&mut self, id: &EdgeId, f: impl FnOnce(&mut Edge) -> R) -> Option<R> {
        self.graph.update_edge(id, f)
    }

    /// Removes an edge in-place.
    pub fn remove_edge(&mut self, id: &EdgeId) -> Option<Edge> {
        self.graph.remove_edge(id)
    }

    /// Clears edges in-place.
    pub fn clear_edges(&mut self) {
        self.graph.clear_edges();
    }

    /// Retains edges matching `f`.
    pub fn retain_edges(&mut self, f: impl FnMut(&EdgeId, &mut Edge) -> bool) {
        self.graph.retain_edges(f);
    }

    /// Inserts or replaces a group in-place.
    pub fn insert_group(&mut self, id: GroupId, group: Group) -> Option<Group> {
        self.graph.insert_group(id, group)
    }

    /// Updates a group in-place.
    pub fn update_group<R>(&mut self, id: &GroupId, f: impl FnOnce(&mut Group) -> R) -> Option<R> {
        self.graph.update_group(id, f)
    }

    /// Removes a group in-place.
    pub fn remove_group(&mut self, id: &GroupId) -> Option<Group> {
        self.graph.remove_group(id)
    }

    /// Clears groups in-place.
    pub fn clear_groups(&mut self) {
        self.graph.clear_groups();
    }

    /// Retains groups matching `f`.
    pub fn retain_groups(&mut self, f: impl FnMut(&GroupId, &mut Group) -> bool) {
        self.graph.retain_groups(f);
    }

    /// Inserts or replaces a sticky note in-place.
    pub fn insert_sticky_note(&mut self, id: StickyNoteId, note: StickyNote) -> Option<StickyNote> {
        self.graph.insert_sticky_note(id, note)
    }

    /// Updates a sticky note in-place.
    pub fn update_sticky_note<R>(
        &mut self,
        id: &StickyNoteId,
        f: impl FnOnce(&mut StickyNote) -> R,
    ) -> Option<R> {
        self.graph.update_sticky_note(id, f)
    }

    /// Removes a sticky note in-place.
    pub fn remove_sticky_note(&mut self, id: &StickyNoteId) -> Option<StickyNote> {
        self.graph.remove_sticky_note(id)
    }

    /// Clears sticky notes in-place.
    pub fn clear_sticky_notes(&mut self) {
        self.graph.clear_sticky_notes();
    }

    /// Retains sticky notes matching `f`.
    pub fn retain_sticky_notes(&mut self, f: impl FnMut(&StickyNoteId, &mut StickyNote) -> bool) {
        self.graph.retain_sticky_notes(f);
    }

    /// Inserts or replaces a binding in-place.
    pub fn insert_binding(&mut self, id: BindingId, binding: Binding) -> Option<Binding> {
        self.graph.insert_binding(id, binding)
    }

    /// Updates a binding in-place.
    pub fn update_binding<R>(
        &mut self,
        id: &BindingId,
        f: impl FnOnce(&mut Binding) -> R,
    ) -> Option<R> {
        self.graph.update_binding(id, f)
    }

    /// Removes a binding in-place.
    pub fn remove_binding(&mut self, id: &BindingId) -> Option<Binding> {
        self.graph.remove_binding(id)
    }

    /// Clears bindings in-place.
    pub fn clear_bindings(&mut self) {
        self.graph.clear_bindings();
    }

    /// Retains bindings matching `f`.
    pub fn retain_bindings(&mut self, f: impl FnMut(&BindingId, &mut Binding) -> bool) {
        self.graph.retain_bindings(f);
    }

    /// Validates and returns the assembled graph.
    pub fn build(self) -> Result<Graph, Vec<GraphValidationError>> {
        let report = validate_graph(&self.graph);
        if report.is_ok() {
            Ok(self.graph)
        } else {
            Err(report.into_errors())
        }
    }

    /// Returns the graph without validation.
    ///
    /// Prefer [`GraphBuilder::build`] for normal construction. This is useful for tests and
    /// migration tooling that intentionally produce invalid graphs and then assert diagnostics.
    pub fn build_unchecked(self) -> Graph {
        self.graph
    }
}

impl Default for GraphBuilder {
    fn default() -> Self {
        Self {
            graph: Graph::default(),
        }
    }
}

impl Deref for GraphBuilder {
    type Target = Graph;

    fn deref(&self) -> &Self::Target {
        &self.graph
    }
}

impl AsRef<Graph> for GraphBuilder {
    fn as_ref(&self) -> &Graph {
        &self.graph
    }
}

impl From<GraphBuilder> for Graph {
    fn from(builder: GraphBuilder) -> Self {
        builder.build_unchecked()
    }
}
