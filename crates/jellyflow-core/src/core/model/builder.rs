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

/// Builder for assembling a detached graph document before handing it to runtime mutation APIs.
///
/// The builder keeps direct collection writes out of `Graph`'s public API while still making
/// fixtures, importers, and graph generators straightforward. Use [`GraphBuilder::build`] when the
/// result should satisfy graph invariants, or [`GraphBuilder::build_unchecked`] only for tests and
/// migrations that intentionally inspect invalid graphs. It is not a mutation API for a graph
/// owned by a live runtime.
///
/// Element mutation is exposed only through the `*_mut` accessors. The former callback wrappers
/// are intentionally absent from the public API:
///
/// ```compile_fail
/// use jellyflow_core::{GraphBuilder, GraphId};
///
/// let mut builder = GraphBuilder::new(GraphId::new());
/// builder.update_import(&GraphId::new(), |_| {});
/// ```
///
/// ```compile_fail
/// use jellyflow_core::{GraphBuilder, GraphId, SymbolId};
///
/// let mut builder = GraphBuilder::new(GraphId::new());
/// builder.update_symbol(&SymbolId::new(), |_| {});
/// ```
///
/// ```compile_fail
/// use jellyflow_core::{GraphBuilder, GraphId, NodeId};
///
/// let mut builder = GraphBuilder::new(GraphId::new());
/// builder.update_node(&NodeId::new(), |_| {});
/// ```
///
/// ```compile_fail
/// use jellyflow_core::{GraphBuilder, GraphId, PortId};
///
/// let mut builder = GraphBuilder::new(GraphId::new());
/// builder.update_port(&PortId::new(), |_| {});
/// ```
///
/// ```compile_fail
/// use jellyflow_core::{EdgeId, GraphBuilder, GraphId};
///
/// let mut builder = GraphBuilder::new(GraphId::new());
/// builder.update_edge(&EdgeId::new(), |_| {});
/// ```
///
/// ```compile_fail
/// use jellyflow_core::{GraphBuilder, GraphId, GroupId};
///
/// let mut builder = GraphBuilder::new(GraphId::new());
/// builder.update_group(&GroupId::new(), |_| {});
/// ```
///
/// ```compile_fail
/// use jellyflow_core::{GraphBuilder, GraphId, StickyNoteId};
///
/// let mut builder = GraphBuilder::new(GraphId::new());
/// builder.update_sticky_note(&StickyNoteId::new(), |_| {});
/// ```
///
/// ```compile_fail
/// use jellyflow_core::{BindingId, GraphBuilder, GraphId};
///
/// let mut builder = GraphBuilder::new(GraphId::new());
/// builder.update_binding(&BindingId::new(), |_| {});
/// ```
#[derive(Debug, Clone, Default)]
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

    /// Resumes detached assembly from an owned graph snapshot.
    ///
    /// The input must not still be installed in a live runtime. Runtime-owned graph edits belong
    /// in the runtime's transaction API; this entry point exists for apply kernels, migrations,
    /// importers, and other owners of a detached graph value.
    pub fn from_detached_graph(graph: Graph) -> Self {
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

    /// Returns a mutable import in the detached graph under assembly.
    pub fn import_mut(&mut self, id: &GraphId) -> Option<&mut GraphImport> {
        self.graph.import_mut(id)
    }

    /// Returns a mutable symbol in the detached graph under assembly.
    pub fn symbol_mut(&mut self, id: &SymbolId) -> Option<&mut Symbol> {
        self.graph.symbol_mut(id)
    }

    /// Returns a mutable node in the detached graph under assembly.
    pub fn node_mut(&mut self, id: &NodeId) -> Option<&mut Node> {
        self.graph.node_mut(id)
    }

    /// Returns a mutable port in the detached graph under assembly.
    pub fn port_mut(&mut self, id: &PortId) -> Option<&mut Port> {
        self.graph.port_mut(id)
    }

    /// Returns a mutable edge in the detached graph under assembly.
    pub fn edge_mut(&mut self, id: &EdgeId) -> Option<&mut Edge> {
        self.graph.edge_mut(id)
    }

    /// Returns a mutable group in the detached graph under assembly.
    pub fn group_mut(&mut self, id: &GroupId) -> Option<&mut Group> {
        self.graph.group_mut(id)
    }

    /// Returns a mutable sticky note in the detached graph under assembly.
    pub fn sticky_note_mut(&mut self, id: &StickyNoteId) -> Option<&mut StickyNote> {
        self.graph.sticky_note_mut(id)
    }

    /// Returns a mutable binding in the detached graph under assembly.
    pub fn binding_mut(&mut self, id: &BindingId) -> Option<&mut Binding> {
        self.graph.binding_mut(id)
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
