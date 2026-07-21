use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use jellyflow_core::core::{
    BindingId, CanvasPoint, EdgeId, Graph, GraphId, GroupId, NodeId, PortId, StickyNoteId, SymbolId,
};

use super::{DocumentEpoch, GraphRevision, GraphSnapshot, NodeIncarnation};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimePatchKind {
    Incremental,
    DocumentReset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactChange {
    Added,
    Updated,
    Removed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RuntimeFactKey {
    Import(GraphId),
    Symbol(SymbolId),
    Node(NodeId),
    Port(PortId),
    Edge(EdgeId),
    Group(GroupId),
    StickyNote(StickyNoteId),
    Binding(BindingId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeFactDelta {
    key: RuntimeFactKey,
    change: FactChange,
    change_mask: FactChangeMask,
}

impl RuntimeFactDelta {
    pub(super) fn new(
        key: RuntimeFactKey,
        change: FactChange,
        change_mask: FactChangeMask,
    ) -> Self {
        Self {
            key,
            change,
            change_mask,
        }
    }

    pub fn key(self) -> RuntimeFactKey {
        self.key
    }

    pub fn change(self) -> FactChange {
        self.change
    }

    pub fn change_mask(self) -> FactChangeMask {
        self.change_mask
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FactChangeMask(u64);

impl FactChangeMask {
    pub const IDENTITY: Self = Self(1 << 0);
    pub const POSITION: Self = Self(1 << 1);
    pub const LAYOUT: Self = Self(1 << 2);
    pub const VISIBILITY: Self = Self(1 << 3);
    pub const STRUCTURE: Self = Self(1 << 4);
    pub const PAYLOAD: Self = Self(1 << 5);
    pub const PRESENTATION: Self = Self(1 << 6);

    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub fn is_only(self, other: Self) -> bool {
        self == other
    }

    pub(super) fn merge(&mut self, other: Self) {
        self.0 |= other.0;
    }
}

/// Conservative graph facts that can invalidate native node geometry measured from an older frame.
///
/// The runtime derives this value once while the authoritative before/after graphs, transaction,
/// and indexes are available. Native adapters must consume this value instead of reconstructing
/// geometry dependencies from patch deltas.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GeometryImpact {
    global: bool,
    nodes: BTreeSet<NodeId>,
    ports: BTreeSet<PortId>,
    incident_edges: BTreeSet<EdgeId>,
    translation_only_nodes: BTreeSet<NodeId>,
}

impl GeometryImpact {
    /// Returns whether every layout candidate must be treated as affected.
    pub fn is_global(&self) -> bool {
        self.global
    }

    /// Returns node identities whose frame-local layout facts changed.
    pub fn nodes(&self) -> &BTreeSet<NodeId> {
        &self.nodes
    }

    /// Returns port identities whose layout or connection facts changed.
    pub fn ports(&self) -> &BTreeSet<PortId> {
        &self.ports
    }

    /// Returns edge identities whose incident facts changed.
    pub fn incident_edges(&self) -> &BTreeSet<EdgeId> {
        &self.incident_edges
    }

    /// Returns whether facts captured for one layout candidate intersect this impact.
    pub fn affects_layout(
        &self,
        node: NodeId,
        ports: impl IntoIterator<Item = PortId>,
        incident_edges: impl IntoIterator<Item = EdgeId>,
    ) -> bool {
        self.global
            || self.nodes.contains(&node)
            || ports.into_iter().any(|port| self.ports.contains(&port))
            || incident_edges
                .into_iter()
                .any(|edge| self.incident_edges.contains(&edge))
    }

    pub(crate) fn preserves_translated_layout(
        &self,
        node: NodeId,
        ports: impl IntoIterator<Item = PortId>,
        incident_edges: impl IntoIterator<Item = EdgeId>,
    ) -> bool {
        !self.global
            && self.translation_only_nodes.contains(&node)
            && !ports.into_iter().any(|port| self.ports.contains(&port))
            && !incident_edges
                .into_iter()
                .any(|edge| self.incident_edges.contains(&edge))
    }

    pub(crate) fn merge(&mut self, other: &Self) -> u64 {
        if self.global {
            return 0;
        }
        if other.global {
            self.make_global();
            return 0;
        }
        for node in other.nodes.iter().copied() {
            let inserted = self.nodes.insert(node);
            if other.translation_only_nodes.contains(&node) {
                if inserted {
                    self.translation_only_nodes.insert(node);
                }
            } else {
                self.translation_only_nodes.remove(&node);
            }
        }
        self.ports.extend(other.ports.iter().copied());
        self.incident_edges
            .extend(other.incident_edges.iter().copied());
        other
            .nodes
            .len()
            .saturating_add(other.ports.len())
            .saturating_add(other.incident_edges.len()) as u64
    }

    pub(crate) fn make_global(&mut self) {
        self.global = true;
        self.nodes.clear();
        self.ports.clear();
        self.incident_edges.clear();
        self.translation_only_nodes.clear();
    }

    pub(super) fn include_nodes(&mut self, nodes: impl IntoIterator<Item = NodeId>) {
        self.nodes.extend(nodes);
    }

    pub(super) fn include_port(&mut self, port: PortId) {
        self.ports.insert(port);
    }

    pub(super) fn include_incident_edge(&mut self, edge: EdgeId) {
        self.incident_edges.insert(edge);
    }

    pub(super) fn mark_translation_only(&mut self, nodes: impl IntoIterator<Item = NodeId>) {
        self.translation_only_nodes.extend(nodes);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct NodeTranslation {
    pub(crate) node: NodeId,
    pub(crate) delta: CanvasPoint,
}

#[derive(Debug, Default)]
pub(crate) struct SessionImpact {
    pub(crate) affected_nodes: Vec<NodeId>,
    pub(crate) retired_nodes: Vec<NodeId>,
    pub(crate) geometry_withdrawals: Vec<NodeId>,
    pub(crate) layout_dirty_nodes: Vec<NodeId>,
    pub(crate) translated_nodes: Vec<NodeTranslation>,
    pub(crate) route_edges: Vec<EdgeId>,
    pub(crate) sanitize_view_state: bool,
    pub(crate) geometry: GeometryImpact,
}

#[derive(Debug)]
struct RuntimePatchInner {
    from_revision: GraphRevision,
    to_revision: GraphRevision,
    document_epoch: DocumentEpoch,
    kind: RuntimePatchKind,
    snapshot: GraphSnapshot,
    facts: Vec<RuntimeFactDelta>,
    impact: SessionImpact,
    incarnations: BTreeMap<NodeId, Option<NodeIncarnation>>,
}

#[derive(Debug, Clone)]
pub struct RuntimePatch(Arc<RuntimePatchInner>);

impl RuntimePatch {
    pub fn from_revision(&self) -> GraphRevision {
        self.0.from_revision
    }

    pub fn to_revision(&self) -> GraphRevision {
        self.0.to_revision
    }

    pub fn document_epoch(&self) -> DocumentEpoch {
        self.0.document_epoch
    }

    pub fn kind(&self) -> RuntimePatchKind {
        self.0.kind
    }

    pub fn snapshot(&self) -> &GraphSnapshot {
        &self.0.snapshot
    }

    pub fn facts(&self) -> &[RuntimeFactDelta] {
        &self.0.facts
    }

    pub fn affected_nodes(&self) -> &[NodeId] {
        &self.0.impact.affected_nodes
    }

    /// Returns the authoritative geometry invalidation footprint for this patch.
    pub fn geometry_impact(&self) -> &GeometryImpact {
        &self.0.impact.geometry
    }

    pub fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    /// Returns the new incarnation when this patch retired or replaced one node identity.
    ///
    /// `None` means the patch did not change that node's identity. `Some(None)` means the node was
    /// retired, while `Some(Some(_))` identifies its new incarnation.
    pub fn node_incarnation_change(&self, node: NodeId) -> Option<Option<NodeIncarnation>> {
        self.0.incarnations.get(&node).copied()
    }

    /// Iterates only node identities retired or replaced by this patch.
    pub fn node_incarnation_changes(
        &self,
    ) -> impl Iterator<Item = (NodeId, Option<NodeIncarnation>)> + '_ {
        self.0
            .incarnations
            .iter()
            .map(|(node, incarnation)| (*node, *incarnation))
    }

    pub(crate) fn impact(&self) -> &SessionImpact {
        &self.0.impact
    }

    pub(super) fn incremental(
        from_revision: GraphRevision,
        to_revision: GraphRevision,
        document_epoch: DocumentEpoch,
        snapshot: GraphSnapshot,
        facts: Vec<RuntimeFactDelta>,
        impact: SessionImpact,
        incarnations: BTreeMap<NodeId, Option<NodeIncarnation>>,
    ) -> Self {
        Self(Arc::new(RuntimePatchInner {
            from_revision,
            to_revision,
            document_epoch,
            kind: RuntimePatchKind::Incremental,
            snapshot,
            facts,
            impact,
            incarnations,
        }))
    }

    pub(super) fn document_reset(
        before: &Graph,
        from_revision: GraphRevision,
        to_revision: GraphRevision,
        document_epoch: DocumentEpoch,
        snapshot: GraphSnapshot,
        incarnations: BTreeMap<NodeId, Option<NodeIncarnation>>,
    ) -> Self {
        let affected_nodes = incarnations.keys().copied().collect();
        Self(Arc::new(RuntimePatchInner {
            from_revision,
            to_revision,
            document_epoch,
            kind: RuntimePatchKind::DocumentReset,
            snapshot,
            facts: Vec::new(),
            impact: SessionImpact {
                affected_nodes,
                retired_nodes: before.nodes().keys().copied().collect(),
                geometry: GeometryImpact {
                    global: true,
                    ..GeometryImpact::default()
                },
                ..SessionImpact::default()
            },
            incarnations,
        }))
    }
}
