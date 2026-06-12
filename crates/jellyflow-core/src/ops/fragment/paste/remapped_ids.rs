use std::collections::BTreeMap;

use crate::core::{BindingId, EdgeId, GroupId, NodeId, PortId, StickyNoteId, SymbolId};

use super::super::{model::GraphFragment, remap::IdRemapper};

pub(super) struct RemappedFragmentIds {
    groups: BTreeMap<GroupId, GroupId>,
    nodes: BTreeMap<NodeId, NodeId>,
    ports: BTreeMap<PortId, PortId>,
    edges: BTreeMap<EdgeId, EdgeId>,
    sticky_notes: BTreeMap<StickyNoteId, StickyNoteId>,
    symbols: BTreeMap<SymbolId, SymbolId>,
    bindings: BTreeMap<BindingId, BindingId>,
}

impl RemappedFragmentIds {
    pub(super) fn new(fragment: &GraphFragment, remapper: &IdRemapper) -> Self {
        Self {
            groups: remapped_groups(fragment, remapper),
            nodes: remapped_nodes(fragment, remapper),
            ports: remapped_ports(fragment, remapper),
            edges: remapped_edges(fragment, remapper),
            sticky_notes: remapped_sticky_notes(fragment, remapper),
            symbols: remapped_symbols(fragment, remapper),
            bindings: remapped_bindings(fragment, remapper),
        }
    }

    pub(super) fn group(&self, old_id: GroupId) -> GroupId {
        self.groups[&old_id]
    }

    pub(super) fn maybe_group(&self, old_id: GroupId) -> Option<GroupId> {
        self.groups.get(&old_id).copied()
    }

    pub(super) fn node(&self, old_id: NodeId) -> NodeId {
        self.nodes[&old_id]
    }

    pub(super) fn port(&self, old_id: PortId) -> PortId {
        self.ports[&old_id]
    }

    pub(super) fn edge(&self, old_id: EdgeId) -> EdgeId {
        self.edges[&old_id]
    }

    pub(super) fn sticky_note(&self, old_id: StickyNoteId) -> StickyNoteId {
        self.sticky_notes[&old_id]
    }

    pub(super) fn symbol(&self, old_id: SymbolId) -> SymbolId {
        self.symbols[&old_id]
    }

    pub(super) fn maybe_symbol(&self, old_id: SymbolId) -> Option<SymbolId> {
        self.symbols.get(&old_id).copied()
    }

    pub(super) fn binding(&self, old_id: BindingId) -> BindingId {
        self.bindings[&old_id]
    }

    pub(super) fn maybe_node(&self, old_id: NodeId) -> Option<NodeId> {
        self.nodes.get(&old_id).copied()
    }

    pub(super) fn maybe_port(&self, old_id: PortId) -> Option<PortId> {
        self.ports.get(&old_id).copied()
    }

    pub(super) fn maybe_edge(&self, old_id: EdgeId) -> Option<EdgeId> {
        self.edges.get(&old_id).copied()
    }

    pub(super) fn maybe_sticky_note(&self, old_id: StickyNoteId) -> Option<StickyNoteId> {
        self.sticky_notes.get(&old_id).copied()
    }
}

fn remapped_groups(fragment: &GraphFragment, remapper: &IdRemapper) -> BTreeMap<GroupId, GroupId> {
    let mut map = BTreeMap::new();
    for group_id in fragment.groups.keys() {
        map.insert(*group_id, remapper.remap_group(*group_id));
    }
    map
}

fn remapped_nodes(fragment: &GraphFragment, remapper: &IdRemapper) -> BTreeMap<NodeId, NodeId> {
    let mut map = BTreeMap::new();
    for node_id in fragment.nodes.keys() {
        map.insert(*node_id, remapper.remap_node(*node_id));
    }
    map
}

fn remapped_ports(fragment: &GraphFragment, remapper: &IdRemapper) -> BTreeMap<PortId, PortId> {
    let mut map = BTreeMap::new();
    for port_id in fragment.ports.keys() {
        map.insert(*port_id, remapper.remap_port(*port_id));
    }
    map
}

fn remapped_edges(fragment: &GraphFragment, remapper: &IdRemapper) -> BTreeMap<EdgeId, EdgeId> {
    let mut map = BTreeMap::new();
    for edge_id in fragment.edges.keys() {
        map.insert(*edge_id, remapper.remap_edge(*edge_id));
    }
    map
}

fn remapped_sticky_notes(
    fragment: &GraphFragment,
    remapper: &IdRemapper,
) -> BTreeMap<StickyNoteId, StickyNoteId> {
    let mut map = BTreeMap::new();
    for note_id in fragment.sticky_notes.keys() {
        map.insert(*note_id, remapper.remap_note(*note_id));
    }
    map
}

fn remapped_symbols(
    fragment: &GraphFragment,
    remapper: &IdRemapper,
) -> BTreeMap<SymbolId, SymbolId> {
    let mut map = BTreeMap::new();
    for symbol_id in fragment.symbols.keys() {
        map.insert(*symbol_id, remapper.remap_symbol(*symbol_id));
    }
    map
}

fn remapped_bindings(
    fragment: &GraphFragment,
    remapper: &IdRemapper,
) -> BTreeMap<BindingId, BindingId> {
    let mut map = BTreeMap::new();
    for binding_id in fragment.bindings.keys() {
        map.insert(*binding_id, remapper.remap_binding(*binding_id));
    }
    map
}
