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
