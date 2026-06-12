use std::collections::BTreeMap;

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

/// Node graph document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Graph {
    /// Stable identity for editor-state lookup and cross-graph references.
    pub graph_id: GraphId,
    /// Schema version for migrations.
    pub graph_version: u32,

    /// Transitive graph dependencies (semantic subgraphs / libraries).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub imports: BTreeMap<GraphId, GraphImport>,

    /// Graph-scoped symbols (blackboard/variables).
    pub symbols: BTreeMap<SymbolId, Symbol>,

    /// Node instances.
    pub nodes: BTreeMap<NodeId, Node>,

    /// Port instances (owned by nodes, but stored in a flat map for stable lookup).
    pub ports: BTreeMap<PortId, Port>,

    /// Edges between ports.
    pub edges: BTreeMap<EdgeId, Edge>,

    /// Optional groups.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub groups: BTreeMap<GroupId, Group>,

    /// Optional sticky notes.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub sticky_notes: BTreeMap<StickyNoteId, StickyNote>,

    /// Optional knowledge-canvas bindings.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub bindings: BTreeMap<BindingId, Binding>,
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
}
