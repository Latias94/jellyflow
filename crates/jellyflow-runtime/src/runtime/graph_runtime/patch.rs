use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::graph_kernel::{GraphMutationFootprint, GraphOp, GraphTransaction};
use jellyflow_core::core::{
    Binding, BindingId, CanvasPoint, EdgeId, Graph, GraphId, GraphLocalBindingTarget, GroupId,
    NodeId, PortId, StickyNoteId, SymbolId, symbol_ref_target_symbol_id,
};

use super::index::GraphIndex;
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
    fn from_footprint(footprint: &GraphMutationFootprint) -> Self {
        Self {
            global: footprint.graph || !footprint.imports.is_empty(),
            nodes: footprint.nodes.clone(),
            ports: footprint.ports.clone(),
            incident_edges: footprint.edges.clone(),
            translation_only_nodes: BTreeSet::new(),
        }
    }

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

    pub(crate) fn merge(&mut self, other: &Self) {
        if self.global {
            return;
        }
        if other.global {
            self.make_global();
            return;
        }
        let non_translation_nodes = self
            .nodes
            .difference(&self.translation_only_nodes)
            .chain(other.nodes.difference(&other.translation_only_nodes))
            .copied()
            .collect::<BTreeSet<_>>();
        self.translation_only_nodes
            .extend(other.translation_only_nodes.iter().copied());
        self.translation_only_nodes
            .retain(|node| !non_translation_nodes.contains(node));
        self.nodes.extend(other.nodes.iter().copied());
        self.ports.extend(other.ports.iter().copied());
        self.incident_edges
            .extend(other.incident_edges.iter().copied());
    }

    pub(crate) fn make_global(&mut self) {
        self.global = true;
        self.nodes.clear();
        self.ports.clear();
        self.incident_edges.clear();
        self.translation_only_nodes.clear();
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

    #[allow(clippy::too_many_arguments)]
    pub(super) fn incremental(
        before: &Graph,
        after: &Graph,
        before_index: &GraphIndex,
        transaction: &GraphTransaction,
        from_revision: GraphRevision,
        to_revision: GraphRevision,
        document_epoch: DocumentEpoch,
        snapshot: GraphSnapshot,
        incarnations: BTreeMap<NodeId, Option<NodeIncarnation>>,
    ) -> Self {
        let mut masks = BTreeMap::<RuntimeFactKey, FactChangeMask>::new();
        for op in transaction.ops() {
            collect_direct_facts(op, &mut masks);
        }
        let facts = masks
            .into_iter()
            .map(|(key, change_mask)| RuntimeFactDelta {
                key,
                change: fact_change(before, after, key),
                change_mask,
            })
            .collect::<Vec<_>>();
        let impact = build_impact(before, after, before_index, transaction, &facts);

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

const CHANGE_IDENTITY: FactChangeMask = FactChangeMask::IDENTITY;
const CHANGE_POSITION: FactChangeMask = FactChangeMask::POSITION;
const CHANGE_LAYOUT: FactChangeMask = FactChangeMask::LAYOUT;
const CHANGE_VISIBILITY: FactChangeMask = FactChangeMask::VISIBILITY;
const CHANGE_STRUCTURE: FactChangeMask = FactChangeMask::STRUCTURE;
const CHANGE_PAYLOAD: FactChangeMask = FactChangeMask::PAYLOAD;
const CHANGE_PRESENTATION: FactChangeMask = FactChangeMask::PRESENTATION;

fn insert_mask(
    masks: &mut BTreeMap<RuntimeFactKey, FactChangeMask>,
    key: RuntimeFactKey,
    mask: FactChangeMask,
) {
    let entry = masks.entry(key).or_default();
    entry.0 |= mask.0;
}

fn collect_direct_facts(op: &GraphOp, masks: &mut BTreeMap<RuntimeFactKey, FactChangeMask>) {
    match op {
        GraphOp::AddNode { id, .. } => {
            insert_mask(masks, RuntimeFactKey::Node(*id), CHANGE_IDENTITY)
        }
        GraphOp::RemoveNode {
            id,
            ports,
            edges,
            bindings,
            ..
        } => {
            insert_mask(masks, RuntimeFactKey::Node(*id), CHANGE_IDENTITY);
            for (id, _) in ports {
                insert_mask(masks, RuntimeFactKey::Port(*id), CHANGE_IDENTITY);
            }
            for (id, _) in edges {
                insert_mask(masks, RuntimeFactKey::Edge(*id), CHANGE_IDENTITY);
            }
            for (id, _) in bindings {
                insert_mask(masks, RuntimeFactKey::Binding(*id), CHANGE_IDENTITY);
            }
        }
        GraphOp::SetNodePos { id, .. } | GraphOp::SetNodeOrigin { id, .. } => {
            insert_mask(masks, RuntimeFactKey::Node(*id), CHANGE_POSITION);
        }
        GraphOp::SetNodeKind { id, .. }
        | GraphOp::SetNodeKindVersion { id, .. }
        | GraphOp::SetNodeSize { id, .. }
        | GraphOp::SetNodeCollapsed { id, .. } => {
            insert_mask(masks, RuntimeFactKey::Node(*id), CHANGE_LAYOUT);
        }
        GraphOp::SetNodeHidden { id, .. } => {
            insert_mask(masks, RuntimeFactKey::Node(*id), CHANGE_VISIBILITY);
        }
        GraphOp::SetNodePorts { id, .. } | GraphOp::SetNodeParent { id, .. } => {
            insert_mask(masks, RuntimeFactKey::Node(*id), CHANGE_STRUCTURE);
        }
        GraphOp::SetNodeData { id, .. } => {
            insert_mask(masks, RuntimeFactKey::Node(*id), CHANGE_PAYLOAD);
        }
        GraphOp::SetNodeSelectable { id, .. }
        | GraphOp::SetNodeDraggable { id, .. }
        | GraphOp::SetNodeConnectable { id, .. }
        | GraphOp::SetNodeDeletable { id, .. }
        | GraphOp::SetNodeExtent { id, .. }
        | GraphOp::SetNodeExpandParent { id, .. } => {
            insert_mask(masks, RuntimeFactKey::Node(*id), CHANGE_PRESENTATION);
        }
        GraphOp::AddPort { id, .. } => {
            insert_mask(masks, RuntimeFactKey::Port(*id), CHANGE_IDENTITY)
        }
        GraphOp::RemovePort {
            id,
            edges,
            bindings,
            ..
        } => {
            insert_mask(masks, RuntimeFactKey::Port(*id), CHANGE_IDENTITY);
            for (id, _) in edges {
                insert_mask(masks, RuntimeFactKey::Edge(*id), CHANGE_IDENTITY);
            }
            for (id, _) in bindings {
                insert_mask(masks, RuntimeFactKey::Binding(*id), CHANGE_IDENTITY);
            }
        }
        GraphOp::SetPortType { id, .. } | GraphOp::SetPortData { id, .. } => {
            insert_mask(masks, RuntimeFactKey::Port(*id), CHANGE_PAYLOAD);
        }
        GraphOp::SetPortConnectable { id, .. }
        | GraphOp::SetPortConnectableStart { id, .. }
        | GraphOp::SetPortConnectableEnd { id, .. } => {
            insert_mask(masks, RuntimeFactKey::Port(*id), CHANGE_PRESENTATION);
        }
        GraphOp::AddEdge { id, .. } => {
            insert_mask(masks, RuntimeFactKey::Edge(*id), CHANGE_IDENTITY)
        }
        GraphOp::RemoveEdge { id, bindings, .. } => {
            insert_mask(masks, RuntimeFactKey::Edge(*id), CHANGE_IDENTITY);
            for (id, _) in bindings {
                insert_mask(masks, RuntimeFactKey::Binding(*id), CHANGE_IDENTITY);
            }
        }
        GraphOp::SetEdgeEndpoints { id, .. } => {
            insert_mask(masks, RuntimeFactKey::Edge(*id), CHANGE_STRUCTURE);
        }
        GraphOp::SetEdgeHidden { id, .. } => {
            insert_mask(masks, RuntimeFactKey::Edge(*id), CHANGE_VISIBILITY);
        }
        GraphOp::SetEdgeData { id, .. } => {
            insert_mask(masks, RuntimeFactKey::Edge(*id), CHANGE_PAYLOAD);
        }
        GraphOp::SetEdgeKind { id, .. }
        | GraphOp::SetEdgeSelectable { id, .. }
        | GraphOp::SetEdgeInteractionWidth { id, .. }
        | GraphOp::SetEdgeDeletable { id, .. }
        | GraphOp::SetEdgeReconnectable { id, .. }
        | GraphOp::SetEdgeView { id, .. } => {
            insert_mask(masks, RuntimeFactKey::Edge(*id), CHANGE_PRESENTATION);
        }
        GraphOp::AddImport { id, .. } | GraphOp::RemoveImport { id, .. } => {
            insert_mask(masks, RuntimeFactKey::Import(*id), CHANGE_IDENTITY);
        }
        GraphOp::SetImportAlias { id, .. } => {
            insert_mask(masks, RuntimeFactKey::Import(*id), CHANGE_PAYLOAD);
        }
        GraphOp::AddSymbol { id, .. } | GraphOp::RemoveSymbol { id, .. } => {
            insert_mask(masks, RuntimeFactKey::Symbol(*id), CHANGE_IDENTITY);
        }
        GraphOp::SetSymbolName { id, .. }
        | GraphOp::SetSymbolType { id, .. }
        | GraphOp::SetSymbolDefaultValue { id, .. }
        | GraphOp::SetSymbolMeta { id, .. } => {
            insert_mask(masks, RuntimeFactKey::Symbol(*id), CHANGE_PAYLOAD);
        }
        GraphOp::AddGroup { id, .. } => {
            insert_mask(masks, RuntimeFactKey::Group(*id), CHANGE_IDENTITY);
        }
        GraphOp::RemoveGroup {
            id,
            detached,
            bindings,
            ..
        } => {
            insert_mask(masks, RuntimeFactKey::Group(*id), CHANGE_IDENTITY);
            for (node, _) in detached {
                insert_mask(masks, RuntimeFactKey::Node(*node), CHANGE_STRUCTURE);
            }
            for (binding, _) in bindings {
                insert_mask(masks, RuntimeFactKey::Binding(*binding), CHANGE_IDENTITY);
            }
        }
        GraphOp::SetGroupRect { id, .. } => {
            insert_mask(masks, RuntimeFactKey::Group(*id), CHANGE_POSITION);
        }
        GraphOp::SetGroupTitle { id, .. } | GraphOp::SetGroupColor { id, .. } => {
            insert_mask(masks, RuntimeFactKey::Group(*id), CHANGE_PRESENTATION);
        }
        GraphOp::AddStickyNote { id, .. } => {
            insert_mask(masks, RuntimeFactKey::StickyNote(*id), CHANGE_IDENTITY);
        }
        GraphOp::RemoveStickyNote { id, bindings, .. } => {
            insert_mask(masks, RuntimeFactKey::StickyNote(*id), CHANGE_IDENTITY);
            for (binding, _) in bindings {
                insert_mask(masks, RuntimeFactKey::Binding(*binding), CHANGE_IDENTITY);
            }
        }
        GraphOp::SetStickyNoteRect { id, .. } => {
            insert_mask(masks, RuntimeFactKey::StickyNote(*id), CHANGE_POSITION);
        }
        GraphOp::SetStickyNoteText { id, .. } | GraphOp::SetStickyNoteColor { id, .. } => {
            insert_mask(masks, RuntimeFactKey::StickyNote(*id), CHANGE_PRESENTATION);
        }
        GraphOp::AddBinding { id, .. } | GraphOp::RemoveBinding { id, .. } => {
            insert_mask(masks, RuntimeFactKey::Binding(*id), CHANGE_IDENTITY);
        }
        GraphOp::SetBindingSubject { id, .. }
        | GraphOp::SetBindingTarget { id, .. }
        | GraphOp::SetBindingKind { id, .. }
        | GraphOp::SetBindingMeta { id, .. } => {
            insert_mask(masks, RuntimeFactKey::Binding(*id), CHANGE_PAYLOAD);
        }
    }
}

fn fact_change(before: &Graph, after: &Graph, key: RuntimeFactKey) -> FactChange {
    let (before, after) = match key {
        RuntimeFactKey::Import(id) => (before.import(&id).is_some(), after.import(&id).is_some()),
        RuntimeFactKey::Symbol(id) => (before.symbol(&id).is_some(), after.symbol(&id).is_some()),
        RuntimeFactKey::Node(id) => (before.node(&id).is_some(), after.node(&id).is_some()),
        RuntimeFactKey::Port(id) => (before.port(&id).is_some(), after.port(&id).is_some()),
        RuntimeFactKey::Edge(id) => (before.edge(&id).is_some(), after.edge(&id).is_some()),
        RuntimeFactKey::Group(id) => (before.group(&id).is_some(), after.group(&id).is_some()),
        RuntimeFactKey::StickyNote(id) => (
            before.sticky_note(&id).is_some(),
            after.sticky_note(&id).is_some(),
        ),
        RuntimeFactKey::Binding(id) => {
            (before.binding(&id).is_some(), after.binding(&id).is_some())
        }
    };
    match (before, after) {
        (false, true) => FactChange::Added,
        (true, false) => FactChange::Removed,
        _ => FactChange::Updated,
    }
}

fn build_impact(
    before: &Graph,
    after: &Graph,
    before_index: &GraphIndex,
    transaction: &GraphTransaction,
    facts: &[RuntimeFactDelta],
) -> SessionImpact {
    let footprint = transaction.footprint();
    let mut geometry = GeometryImpact::from_footprint(&footprint);
    let mut affected_nodes = BTreeSet::new();
    let mut retired_nodes = BTreeSet::new();
    let mut geometry_withdrawals = BTreeSet::new();
    let mut layout_dirty_nodes = BTreeSet::new();
    let mut translated_nodes = BTreeMap::new();
    let mut route_edges = BTreeSet::new();
    let mut sanitize_view_state = false;
    let staged_symbol_referrers = staged_symbol_referrers(after, transaction);

    for fact in facts {
        match fact.key {
            RuntimeFactKey::Node(node) => {
                affected_nodes.insert(node);
                if fact.change == FactChange::Removed {
                    retired_nodes.insert(node);
                }
            }
            RuntimeFactKey::Port(port) => {
                affected_nodes.extend(
                    [before.port(&port), after.port(&port)]
                        .into_iter()
                        .flatten()
                        .map(|port| port.node),
                );
            }
            RuntimeFactKey::Edge(edge) => {
                for current in [before.edge(&edge), after.edge(&edge)]
                    .into_iter()
                    .flatten()
                {
                    affected_nodes.extend(
                        [
                            before.port(&current.from),
                            before.port(&current.to),
                            after.port(&current.from),
                            after.port(&current.to),
                        ]
                        .into_iter()
                        .flatten()
                        .map(|port| port.node),
                    );
                }
                if fact.change != FactChange::Updated || !fact.change_mask.is_only(CHANGE_PAYLOAD) {
                    route_edges.insert(edge);
                }
            }
            RuntimeFactKey::Binding(binding) => {
                for current in [before.binding(&binding), after.binding(&binding)]
                    .into_iter()
                    .flatten()
                {
                    extend_binding_nodes(&mut affected_nodes, before, after, current);
                    extend_binding_geometry(&mut geometry, current);
                }
            }
            RuntimeFactKey::Symbol(symbol) => {
                let referrers = before_index
                    .symbol_referrers(symbol)
                    .chain(
                        staged_symbol_referrers
                            .get(&symbol)
                            .into_iter()
                            .flatten()
                            .copied(),
                    )
                    .collect::<BTreeSet<_>>();
                affected_nodes.extend(referrers.iter().copied());
                layout_dirty_nodes.extend(referrers);
            }
            _ => {}
        }
    }

    for op in transaction.ops() {
        sanitize_view_state |= matches!(
            op,
            GraphOp::RemoveNode { .. }
                | GraphOp::RemovePort { .. }
                | GraphOp::RemoveEdge { .. }
                | GraphOp::RemoveGroup { .. }
                | GraphOp::SetNodeHidden { .. }
                | GraphOp::SetEdgeHidden { .. }
                | GraphOp::SetEdgeEndpoints { .. }
        );
        match op {
            GraphOp::SetNodePos { id, from, to } => {
                let Some(node) = after.node(id) else {
                    continue;
                };
                translated_nodes.insert(
                    *id,
                    CanvasPoint {
                        x: to.x - from.x,
                        y: to.y - from.y,
                    },
                );
                debug_assert_eq!(node.pos, *to);
                route_edges.extend(before_index.incident_edges_for_node(*id));
            }
            GraphOp::SetNodeOrigin { id, .. }
            | GraphOp::SetNodeSize { id, .. }
            | GraphOp::SetNodeParent { id, .. } => {
                geometry_withdrawals.insert(*id);
                layout_dirty_nodes.insert(*id);
            }
            GraphOp::SetNodeKind { id, .. } | GraphOp::SetNodeKindVersion { id, .. } => {
                retired_nodes.insert(*id);
                layout_dirty_nodes.insert(*id);
            }
            GraphOp::SetNodeCollapsed { id, .. } | GraphOp::SetNodePorts { id, .. } => {
                geometry_withdrawals.insert(*id);
                layout_dirty_nodes.insert(*id);
            }
            GraphOp::SetNodeData { id, .. } => {
                layout_dirty_nodes.insert(*id);
            }
            GraphOp::SetNodeHidden { id, .. } => {
                geometry_withdrawals.insert(*id);
                layout_dirty_nodes.insert(*id);
            }
            GraphOp::AddNode { id, .. } => {
                layout_dirty_nodes.insert(*id);
            }
            GraphOp::AddPort { port, .. } | GraphOp::RemovePort { port, .. } => {
                geometry_withdrawals.insert(port.node);
                layout_dirty_nodes.insert(port.node);
            }
            GraphOp::SetPortConnectable { id, .. }
            | GraphOp::SetPortConnectableStart { id, .. }
            | GraphOp::SetPortConnectableEnd { id, .. }
            | GraphOp::SetPortType { id, .. }
            | GraphOp::SetPortData { id, .. } => {
                if let Some(owner) = before
                    .port(id)
                    .or_else(|| after.port(id))
                    .map(|port| port.node)
                {
                    geometry_withdrawals.insert(owner);
                    layout_dirty_nodes.insert(owner);
                }
            }
            GraphOp::RemoveGroup { detached, .. } => {
                for (node, _) in detached {
                    geometry_withdrawals.insert(*node);
                    layout_dirty_nodes.insert(*node);
                }
            }
            _ => {}
        }
    }

    for node in &layout_dirty_nodes {
        route_edges.extend(before_index.incident_edges_for_node(*node));
    }

    geometry.nodes.extend(affected_nodes.iter().copied());
    geometry.translation_only_nodes.extend(
        translated_nodes
            .keys()
            .filter(|node| {
                !retired_nodes.contains(node)
                    && !geometry_withdrawals.contains(node)
                    && !layout_dirty_nodes.contains(node)
                    && facts.iter().any(|fact| {
                        fact.key == RuntimeFactKey::Node(**node)
                            && fact.change == FactChange::Updated
                            && fact.change_mask.is_only(FactChangeMask::POSITION)
                    })
            })
            .copied(),
    );
    if geometry.global {
        geometry.make_global();
    }

    SessionImpact {
        affected_nodes: affected_nodes.into_iter().collect(),
        retired_nodes: retired_nodes.into_iter().collect(),
        geometry_withdrawals: geometry_withdrawals.into_iter().collect(),
        layout_dirty_nodes: layout_dirty_nodes.into_iter().collect(),
        translated_nodes: translated_nodes
            .into_iter()
            .map(|(node, delta)| NodeTranslation { node, delta })
            .collect(),
        route_edges: route_edges.into_iter().collect(),
        sanitize_view_state,
        geometry,
    }
}

fn staged_symbol_referrers(
    after: &Graph,
    transaction: &GraphTransaction,
) -> BTreeMap<SymbolId, BTreeSet<NodeId>> {
    let changed_nodes = transaction
        .ops()
        .iter()
        .filter_map(|operation| match operation {
            GraphOp::AddNode { id, .. }
            | GraphOp::RemoveNode { id, .. }
            | GraphOp::SetNodeKind { id, .. }
            | GraphOp::SetNodeData { id, .. } => Some(*id),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let mut referrers = BTreeMap::<SymbolId, BTreeSet<NodeId>>::new();
    for node in changed_nodes {
        let Some(symbol) = after
            .node(&node)
            .and_then(|record| symbol_ref_target_symbol_id(node, record).ok().flatten())
        else {
            continue;
        };
        referrers.entry(symbol).or_default().insert(node);
    }
    referrers
}

fn extend_binding_nodes(
    nodes: &mut BTreeSet<NodeId>,
    before: &Graph,
    after: &Graph,
    binding: &Binding,
) {
    for target in [
        binding.subject.graph_local_target(),
        binding.target.graph_local_target(),
    ]
    .into_iter()
    .flatten()
    {
        match target {
            GraphLocalBindingTarget::Node { id } => {
                nodes.insert(id);
            }
            GraphLocalBindingTarget::Port { id } => {
                nodes.extend(
                    [before.port(&id), after.port(&id)]
                        .into_iter()
                        .flatten()
                        .map(|port| port.node),
                );
            }
            GraphLocalBindingTarget::Edge { id } => {
                for edge in [before.edge(&id), after.edge(&id)].into_iter().flatten() {
                    nodes.extend(
                        [
                            before.port(&edge.from),
                            before.port(&edge.to),
                            after.port(&edge.from),
                            after.port(&edge.to),
                        ]
                        .into_iter()
                        .flatten()
                        .map(|port| port.node),
                    );
                }
            }
            GraphLocalBindingTarget::Graph
            | GraphLocalBindingTarget::Group { .. }
            | GraphLocalBindingTarget::StickyNote { .. } => {}
        }
    }
}

fn extend_binding_geometry(impact: &mut GeometryImpact, binding: &Binding) {
    for target in [
        binding.subject.graph_local_target(),
        binding.target.graph_local_target(),
    ]
    .into_iter()
    .flatten()
    {
        match target {
            GraphLocalBindingTarget::Graph => impact.make_global(),
            GraphLocalBindingTarget::Node { id } => {
                impact.nodes.insert(id);
            }
            GraphLocalBindingTarget::Port { id } => {
                impact.ports.insert(id);
            }
            GraphLocalBindingTarget::Edge { id } => {
                impact.incident_edges.insert(id);
            }
            GraphLocalBindingTarget::Group { .. } | GraphLocalBindingTarget::StickyNote { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_and_graph_footprints_require_global_geometry_invalidation() {
        let mut import_footprint = GraphMutationFootprint::new();
        import_footprint.touch_import(GraphId::from_u128(0x101));
        assert!(GeometryImpact::from_footprint(&import_footprint).is_global());

        let mut graph_footprint = GraphMutationFootprint::new();
        graph_footprint.touch_graph();
        assert!(GeometryImpact::from_footprint(&graph_footprint).is_global());
    }

    #[test]
    fn graph_binding_requires_global_geometry_invalidation() {
        let binding = Binding::graph_local_to_source(
            GraphLocalBindingTarget::Graph,
            "fixture://graph",
            serde_json::Value::Null,
        );
        let mut impact = GeometryImpact::default();

        extend_binding_geometry(&mut impact, &binding);

        assert!(impact.is_global());
        assert!(impact.nodes().is_empty());
        assert!(impact.ports().is_empty());
        assert!(impact.incident_edges().is_empty());
    }
}
