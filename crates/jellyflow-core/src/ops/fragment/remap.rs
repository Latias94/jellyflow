use uuid::Uuid;

use crate::core::{BindingId, EdgeId, GroupId, NodeId, PortId, StickyNoteId, SymbolId};

/// Seed used for deterministic ID remapping.
///
/// Using different seeds yields different IDs while keeping determinism for a given paste action.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct IdRemapSeed(pub Uuid);

impl IdRemapSeed {
    pub fn new_random() -> Self {
        Self(Uuid::new_v4())
    }
}

/// Deterministic ID remapper (UUID v5 under a caller-provided seed).
#[derive(Debug, Clone)]
pub struct IdRemapper {
    seed: IdRemapSeed,
}

impl IdRemapper {
    pub fn new(seed: IdRemapSeed) -> Self {
        Self { seed }
    }

    fn remap_uuid(&self, old: Uuid) -> Uuid {
        // Use UUID v5 in a deterministic namespace to avoid collisions across elements.
        // The caller should pick a new seed for each paste/duplicate action.
        Uuid::new_v5(&self.seed.0, old.as_bytes())
    }

    pub fn remap_node(&self, id: NodeId) -> NodeId {
        NodeId(self.remap_uuid(id.0))
    }

    pub fn remap_port(&self, id: PortId) -> PortId {
        PortId(self.remap_uuid(id.0))
    }

    pub fn remap_edge(&self, id: EdgeId) -> EdgeId {
        EdgeId(self.remap_uuid(id.0))
    }

    pub fn remap_group(&self, id: GroupId) -> GroupId {
        GroupId(self.remap_uuid(id.0))
    }

    pub fn remap_note(&self, id: StickyNoteId) -> StickyNoteId {
        StickyNoteId(self.remap_uuid(id.0))
    }

    pub fn remap_symbol(&self, id: SymbolId) -> SymbolId {
        SymbolId(self.remap_uuid(id.0))
    }

    pub fn remap_binding(&self, id: BindingId) -> BindingId {
        BindingId(self.remap_uuid(id.0))
    }
}
