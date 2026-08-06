use std::collections::{BTreeMap, BTreeSet};

use crate::graph_kernel::{GraphOp, GraphTransaction};
use jellyflow_core::core::{
    Binding, BindingId, CanvasPoint, EdgeId, Graph, GraphLocalBindingTarget, NodeId, PortId,
    SymbolId, symbol_ref_target_symbol_id,
};

use super::index::GraphIndex;
use super::patch::{
    FactChange, FactChangeMask, GeometryImpact, NodeTranslation, RuntimeFactDelta, RuntimeFactKey,
    SessionImpact,
};

mod traversal {
    use jellyflow_core::core::{
        Binding, Edge, EdgeId, Graph, Node, NodeId, Port, PortId, SymbolId,
    };

    use super::{GraphIndex, RuntimeFactKey};

    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
    pub(super) struct ImpactTraversalStats {
        pub(super) operation_visits: usize,
        pub(super) embedded_relationship_visits: usize,
        pub(super) fact_materializations: usize,
        pub(super) graph_point_reads: usize,
        pub(super) affected_set_visits: usize,
        pub(super) index_bucket_reads: usize,
        pub(super) index_entries_read: usize,
    }

    pub(super) struct ImpactTraversal<'a> {
        before: &'a Graph,
        after: &'a Graph,
        before_index: &'a GraphIndex,
        stats: ImpactTraversalStats,
    }

    impl<'a> ImpactTraversal<'a> {
        pub(super) fn new(
            before: &'a Graph,
            after: &'a Graph,
            before_index: &'a GraphIndex,
        ) -> Self {
            Self {
                before,
                after,
                before_index,
                stats: ImpactTraversalStats::default(),
            }
        }

        pub(super) fn finish(self) -> ImpactTraversalStats {
            self.stats
        }

        pub(super) fn visit_operation(&mut self) {
            self.stats.operation_visits = self.stats.operation_visits.saturating_add(1);
        }

        pub(super) fn visit_embedded_relationships(&mut self, count: usize) {
            self.stats.embedded_relationship_visits = self
                .stats
                .embedded_relationship_visits
                .saturating_add(count);
        }

        pub(super) fn materialize_fact(&mut self) {
            self.stats.fact_materializations = self.stats.fact_materializations.saturating_add(1);
        }

        pub(super) fn visit_affected(&mut self, count: usize) {
            self.stats.affected_set_visits = self.stats.affected_set_visits.saturating_add(count);
        }

        pub(super) fn presence(&mut self, key: RuntimeFactKey) -> (bool, bool) {
            self.stats.graph_point_reads = self.stats.graph_point_reads.saturating_add(2);
            match key {
                RuntimeFactKey::Import(id) => (
                    self.before.import(&id).is_some(),
                    self.after.import(&id).is_some(),
                ),
                RuntimeFactKey::Symbol(id) => (
                    self.before.symbol(&id).is_some(),
                    self.after.symbol(&id).is_some(),
                ),
                RuntimeFactKey::Node(id) => (
                    self.before.node(&id).is_some(),
                    self.after.node(&id).is_some(),
                ),
                RuntimeFactKey::Port(id) => (
                    self.before.port(&id).is_some(),
                    self.after.port(&id).is_some(),
                ),
                RuntimeFactKey::Edge(id) => (
                    self.before.edge(&id).is_some(),
                    self.after.edge(&id).is_some(),
                ),
                RuntimeFactKey::Group(id) => (
                    self.before.group(&id).is_some(),
                    self.after.group(&id).is_some(),
                ),
                RuntimeFactKey::StickyNote(id) => (
                    self.before.sticky_note(&id).is_some(),
                    self.after.sticky_note(&id).is_some(),
                ),
                RuntimeFactKey::Binding(id) => (
                    self.before.binding(&id).is_some(),
                    self.after.binding(&id).is_some(),
                ),
            }
        }

        pub(super) fn before_node(&mut self, id: NodeId) -> Option<&'a Node> {
            self.stats.graph_point_reads = self.stats.graph_point_reads.saturating_add(1);
            self.before.node(&id)
        }

        pub(super) fn after_node(&mut self, id: NodeId) -> Option<&'a Node> {
            self.stats.graph_point_reads = self.stats.graph_point_reads.saturating_add(1);
            self.after.node(&id)
        }

        pub(super) fn before_port(&mut self, id: PortId) -> Option<&'a Port> {
            self.stats.graph_point_reads = self.stats.graph_point_reads.saturating_add(1);
            self.before.port(&id)
        }

        pub(super) fn after_port(&mut self, id: PortId) -> Option<&'a Port> {
            self.stats.graph_point_reads = self.stats.graph_point_reads.saturating_add(1);
            self.after.port(&id)
        }

        pub(super) fn before_edge(&mut self, id: EdgeId) -> Option<&'a Edge> {
            self.stats.graph_point_reads = self.stats.graph_point_reads.saturating_add(1);
            self.before.edge(&id)
        }

        pub(super) fn after_edge(&mut self, id: EdgeId) -> Option<&'a Edge> {
            self.stats.graph_point_reads = self.stats.graph_point_reads.saturating_add(1);
            self.after.edge(&id)
        }

        pub(super) fn before_binding(
            &mut self,
            id: jellyflow_core::core::BindingId,
        ) -> Option<&'a Binding> {
            self.stats.graph_point_reads = self.stats.graph_point_reads.saturating_add(1);
            self.before.binding(&id)
        }

        pub(super) fn after_binding(
            &mut self,
            id: jellyflow_core::core::BindingId,
        ) -> Option<&'a Binding> {
            self.stats.graph_point_reads = self.stats.graph_point_reads.saturating_add(1);
            self.after.binding(&id)
        }

        pub(super) fn symbol_referrers(&mut self, symbol: SymbolId) -> Vec<NodeId> {
            self.stats.index_bucket_reads = self.stats.index_bucket_reads.saturating_add(1);
            let nodes = self
                .before_index
                .symbol_referrers(symbol)
                .collect::<Vec<_>>();
            self.stats.index_entries_read =
                self.stats.index_entries_read.saturating_add(nodes.len());
            nodes
        }

        pub(super) fn incident_edges(&mut self, node: NodeId) -> Vec<EdgeId> {
            self.stats.index_bucket_reads = self.stats.index_bucket_reads.saturating_add(1);
            let edges = self
                .before_index
                .incident_edges_for_node(node)
                .collect::<Vec<_>>();
            self.stats.index_entries_read =
                self.stats.index_entries_read.saturating_add(edges.len());
            edges
        }
    }
}

use traversal::{ImpactTraversal, ImpactTraversalStats};

#[derive(Debug, Default)]
pub(super) struct IndexImpact {
    pub(super) ports: BTreeSet<PortId>,
    pub(super) edges: BTreeSet<EdgeId>,
    pub(super) bindings: BTreeSet<BindingId>,
    pub(super) symbol_ref_nodes: BTreeSet<NodeId>,
    pub(super) refresh_incident_nodes: BTreeSet<NodeId>,
    pub(super) reset_incident_nodes: BTreeSet<NodeId>,
}

#[derive(Debug, Default)]
pub(super) struct NodeIdentityImpact {
    pub(super) retired: BTreeSet<NodeId>,
    pub(super) added: BTreeSet<NodeId>,
}

#[derive(Debug, Default)]
pub(super) struct EdgeIdentityImpact {
    pub(super) retired: BTreeSet<EdgeId>,
    pub(super) added: BTreeSet<EdgeId>,
}

#[derive(Debug, Default)]
pub(super) struct PortIdentityImpact {
    pub(super) retired: BTreeSet<PortId>,
    pub(super) added: BTreeSet<PortId>,
}

#[derive(Debug)]
pub(super) struct InstalledTransactionImpact {
    pub(super) facts: Vec<RuntimeFactDelta>,
    pub(super) session: SessionImpact,
    pub(super) index: IndexImpact,
    pub(super) identities: NodeIdentityImpact,
    pub(super) edge_identities: EdgeIdentityImpact,
    pub(super) port_identities: PortIdentityImpact,
}

#[derive(Debug, Default)]
struct SessionDirectives {
    geometry_withdrawals: BTreeSet<NodeId>,
    layout_dirty_nodes: BTreeSet<NodeId>,
    translated_nodes: BTreeSet<NodeId>,
    complete_layout_retention_candidates: BTreeSet<NodeId>,
    complete_layout_retention_disqualifications: BTreeSet<NodeId>,
    sanitize_view_state: bool,
}

#[derive(Debug, Default)]
struct ImpactSeed {
    fact_masks: BTreeMap<RuntimeFactKey, FactChangeMask>,
    geometry: GeometryImpact,
    index: IndexImpact,
    identities: NodeIdentityImpact,
    edge_identities: EdgeIdentityImpact,
    port_identities: PortIdentityImpact,
    session: SessionDirectives,
}

impl InstalledTransactionImpact {
    pub(super) fn derive(
        before: &Graph,
        after: &Graph,
        before_index: &GraphIndex,
        transaction: &GraphTransaction,
    ) -> Self {
        Self::derive_with_stats(before, after, before_index, transaction).0
    }

    fn derive_with_stats(
        before: &Graph,
        after: &Graph,
        before_index: &GraphIndex,
        transaction: &GraphTransaction,
    ) -> (Self, ImpactTraversalStats) {
        let mut traversal = ImpactTraversal::new(before, after, before_index);
        let mut seed = ImpactSeed::default();
        for operation in transaction.ops() {
            traversal.visit_operation();
            declare_operation_impact(operation, &mut seed, &mut traversal);
        }

        seed.identities
            .retired
            .retain(|node| traversal.before_node(*node).is_some());
        seed.identities
            .added
            .retain(|node| traversal.after_node(*node).is_some());
        seed.port_identities
            .retired
            .retain(|port| traversal.before_port(*port).is_some());
        seed.port_identities
            .added
            .retain(|port| traversal.after_port(*port).is_some());

        let mut facts = Vec::with_capacity(seed.fact_masks.len());
        for (key, change_mask) in seed.fact_masks {
            traversal.materialize_fact();
            facts.push(RuntimeFactDelta::new(
                key,
                fact_change(&mut traversal, key),
                change_mask,
            ));
        }
        let session = build_session_impact(
            &mut traversal,
            &facts,
            seed.geometry,
            &seed.index,
            &seed.identities,
            seed.session,
        );

        (
            Self {
                facts,
                session,
                index: seed.index,
                identities: seed.identities,
                edge_identities: seed.edge_identities,
                port_identities: seed.port_identities,
            },
            traversal.finish(),
        )
    }
}

fn declare_operation_impact(
    operation: &GraphOp,
    seed: &mut ImpactSeed,
    traversal: &mut ImpactTraversal<'_>,
) {
    match operation {
        GraphOp::SetNodeSize { id, .. }
            if !seed
                .session
                .complete_layout_retention_disqualifications
                .contains(id) =>
        {
            seed.session
                .complete_layout_retention_candidates
                .insert(*id);
        }
        GraphOp::AddNode { id, .. }
        | GraphOp::RemoveNode { id, .. }
        | GraphOp::SetNodeOrigin { id, .. }
        | GraphOp::SetNodeKind { id, .. }
        | GraphOp::SetNodeKindVersion { id, .. }
        | GraphOp::SetNodeSelectable { id, .. }
        | GraphOp::SetNodeDraggable { id, .. }
        | GraphOp::SetNodeConnectable { id, .. }
        | GraphOp::SetNodeDeletable { id, .. }
        | GraphOp::SetNodeParent { id, .. }
        | GraphOp::SetNodeExtent { id, .. }
        | GraphOp::SetNodeExpandParent { id, .. }
        | GraphOp::SetNodePorts { id, .. }
        | GraphOp::SetNodeHidden { id, .. }
        | GraphOp::SetNodeCollapsed { id, .. }
        | GraphOp::SetNodeData { id, .. } => {
            seed.session.complete_layout_retention_candidates.remove(id);
            seed.session
                .complete_layout_retention_disqualifications
                .insert(*id);
        }
        _ => {}
    }
    match operation {
        GraphOp::AddNode { id, node } => {
            seed.fact(RuntimeFactKey::Node(*id), FactChangeMask::IDENTITY);
            traversal.visit_embedded_relationships(node.ports.len());
            for port in &node.ports {
                seed.geometry.include_port(*port);
            }
            seed.identities.added.insert(*id);
            seed.index.symbol_ref_nodes.insert(*id);
            seed.index.refresh_incident_nodes.insert(*id);
            seed.index.reset_incident_nodes.insert(*id);
            seed.session.layout_dirty_nodes.insert(*id);
        }
        GraphOp::RemoveNode {
            id,
            ports,
            edges,
            bindings,
            ..
        } => {
            traversal.visit_embedded_relationships(
                ports
                    .len()
                    .saturating_add(edges.len())
                    .saturating_add(bindings.len()),
            );
            seed.fact(RuntimeFactKey::Node(*id), FactChangeMask::IDENTITY);
            seed.identities.retired.insert(*id);
            seed.index.symbol_ref_nodes.insert(*id);
            seed.index.refresh_incident_nodes.insert(*id);
            seed.index.reset_incident_nodes.insert(*id);
            seed.session.sanitize_view_state = true;
            for (id, _) in ports {
                seed.fact(RuntimeFactKey::Port(*id), FactChangeMask::IDENTITY);
                seed.index.ports.insert(*id);
                seed.port_identities.retired.insert(*id);
            }
            for (id, edge) in edges {
                seed.fact(RuntimeFactKey::Edge(*id), FactChangeMask::IDENTITY);
                seed.edge_identities.retired.insert(*id);
                seed.include_edge_endpoints(edge);
                seed.index.edges.insert(*id);
            }
            for (id, _) in bindings {
                seed.fact(RuntimeFactKey::Binding(*id), FactChangeMask::IDENTITY);
                seed.index.bindings.insert(*id);
            }
        }
        GraphOp::SetNodePos { id, .. } => {
            seed.fact(RuntimeFactKey::Node(*id), FactChangeMask::POSITION);
            seed.session.translated_nodes.insert(*id);
        }
        GraphOp::SetNodeOrigin { id, .. } => {
            seed.fact(RuntimeFactKey::Node(*id), FactChangeMask::POSITION);
            seed.withdraw_and_dirty(*id);
        }
        GraphOp::SetNodeKind { id, .. } => {
            seed.fact(RuntimeFactKey::Node(*id), FactChangeMask::LAYOUT);
            seed.hard_replace(*id);
            seed.index.symbol_ref_nodes.insert(*id);
            seed.session.layout_dirty_nodes.insert(*id);
        }
        GraphOp::SetNodeKindVersion { id, .. } => {
            seed.fact(RuntimeFactKey::Node(*id), FactChangeMask::LAYOUT);
            seed.hard_replace(*id);
            seed.session.layout_dirty_nodes.insert(*id);
        }
        GraphOp::SetNodeSelectable { id, .. }
        | GraphOp::SetNodeDraggable { id, .. }
        | GraphOp::SetNodeConnectable { id, .. }
        | GraphOp::SetNodeDeletable { id, .. }
        | GraphOp::SetNodeExtent { id, .. }
        | GraphOp::SetNodeExpandParent { id, .. } => {
            seed.fact(RuntimeFactKey::Node(*id), FactChangeMask::PRESENTATION);
        }
        GraphOp::SetNodeParent { id, .. } => {
            seed.fact(RuntimeFactKey::Node(*id), FactChangeMask::STRUCTURE);
            seed.withdraw_and_dirty(*id);
        }
        GraphOp::SetNodePorts { id, from, to } => {
            seed.fact(RuntimeFactKey::Node(*id), FactChangeMask::STRUCTURE);
            traversal.visit_embedded_relationships(from.len().saturating_add(to.len()));
            for port in from.iter().chain(to) {
                seed.geometry.include_port(*port);
            }
            seed.index.refresh_incident_nodes.insert(*id);
            seed.withdraw_and_dirty(*id);
        }
        GraphOp::SetNodeSize { id, .. } => {
            seed.fact(RuntimeFactKey::Node(*id), FactChangeMask::LAYOUT);
            seed.session.layout_dirty_nodes.insert(*id);
        }
        GraphOp::SetNodeCollapsed { id, .. } => {
            seed.fact(RuntimeFactKey::Node(*id), FactChangeMask::LAYOUT);
            seed.withdraw_and_dirty(*id);
        }
        GraphOp::SetNodeHidden { id, .. } => {
            seed.fact(RuntimeFactKey::Node(*id), FactChangeMask::VISIBILITY);
            seed.session.sanitize_view_state = true;
            seed.withdraw_and_dirty(*id);
        }
        GraphOp::SetNodeData { id, .. } => {
            seed.fact(RuntimeFactKey::Node(*id), FactChangeMask::PAYLOAD);
            seed.index.symbol_ref_nodes.insert(*id);
            seed.session.layout_dirty_nodes.insert(*id);
        }

        GraphOp::AddPort { id, .. } => {
            seed.fact(RuntimeFactKey::Port(*id), FactChangeMask::IDENTITY);
            seed.index.ports.insert(*id);
            seed.port_identities.added.insert(*id);
        }
        GraphOp::RemovePort {
            id,
            edges,
            bindings,
            ..
        } => {
            traversal.visit_embedded_relationships(edges.len().saturating_add(bindings.len()));
            seed.fact(RuntimeFactKey::Port(*id), FactChangeMask::IDENTITY);
            seed.index.ports.insert(*id);
            seed.port_identities.retired.insert(*id);
            seed.session.sanitize_view_state = true;
            for (id, edge) in edges {
                seed.fact(RuntimeFactKey::Edge(*id), FactChangeMask::IDENTITY);
                seed.edge_identities.retired.insert(*id);
                seed.include_edge_endpoints(edge);
                seed.index.edges.insert(*id);
            }
            for (id, _) in bindings {
                seed.fact(RuntimeFactKey::Binding(*id), FactChangeMask::IDENTITY);
                seed.index.bindings.insert(*id);
            }
        }
        GraphOp::SetPortConnectable { id, .. }
        | GraphOp::SetPortConnectableStart { id, .. }
        | GraphOp::SetPortConnectableEnd { id, .. } => {
            seed.fact(RuntimeFactKey::Port(*id), FactChangeMask::PRESENTATION);
        }
        GraphOp::SetPortType { id, .. } | GraphOp::SetPortData { id, .. } => {
            seed.fact(RuntimeFactKey::Port(*id), FactChangeMask::PAYLOAD);
        }

        GraphOp::AddEdge { id, edge } => {
            seed.fact(RuntimeFactKey::Edge(*id), FactChangeMask::IDENTITY);
            seed.edge_identities.added.insert(*id);
            seed.include_edge_endpoints(edge);
            seed.index.edges.insert(*id);
        }
        GraphOp::RemoveEdge { id, edge, bindings } => {
            traversal.visit_embedded_relationships(bindings.len());
            seed.fact(RuntimeFactKey::Edge(*id), FactChangeMask::IDENTITY);
            seed.edge_identities.retired.insert(*id);
            seed.include_edge_endpoints(edge);
            seed.index.edges.insert(*id);
            seed.session.sanitize_view_state = true;
            for (id, _) in bindings {
                seed.fact(RuntimeFactKey::Binding(*id), FactChangeMask::IDENTITY);
                seed.index.bindings.insert(*id);
            }
        }
        GraphOp::SetEdgeEndpoints { id, from, to } => {
            seed.fact(RuntimeFactKey::Edge(*id), FactChangeMask::STRUCTURE);
            traversal.visit_embedded_relationships(4);
            for port in [from.from, from.to, to.from, to.to] {
                seed.geometry.include_port(port);
            }
            seed.index.edges.insert(*id);
            seed.session.sanitize_view_state = true;
        }
        GraphOp::SetEdgeHidden { id, .. } => {
            seed.fact(RuntimeFactKey::Edge(*id), FactChangeMask::VISIBILITY);
            seed.session.sanitize_view_state = true;
        }
        GraphOp::SetEdgeData { id, .. } => {
            seed.fact(RuntimeFactKey::Edge(*id), FactChangeMask::PAYLOAD);
        }
        GraphOp::SetEdgeKind { id, .. }
        | GraphOp::SetEdgeSelectable { id, .. }
        | GraphOp::SetEdgeInteractionWidth { id, .. }
        | GraphOp::SetEdgeDeletable { id, .. }
        | GraphOp::SetEdgeReconnectable { id, .. }
        | GraphOp::SetEdgeView { id, .. } => {
            seed.fact(RuntimeFactKey::Edge(*id), FactChangeMask::PRESENTATION);
        }

        GraphOp::AddImport { id, .. } | GraphOp::RemoveImport { id, .. } => {
            seed.fact(RuntimeFactKey::Import(*id), FactChangeMask::IDENTITY);
        }
        GraphOp::SetImportAlias { id, .. } => {
            seed.fact(RuntimeFactKey::Import(*id), FactChangeMask::PAYLOAD);
        }

        GraphOp::AddSymbol { id, .. } | GraphOp::RemoveSymbol { id, .. } => {
            seed.fact(RuntimeFactKey::Symbol(*id), FactChangeMask::IDENTITY);
        }
        GraphOp::SetSymbolName { id, .. }
        | GraphOp::SetSymbolType { id, .. }
        | GraphOp::SetSymbolDefaultValue { id, .. }
        | GraphOp::SetSymbolMeta { id, .. } => {
            seed.fact(RuntimeFactKey::Symbol(*id), FactChangeMask::PAYLOAD);
        }

        GraphOp::AddGroup { id, .. } => {
            seed.fact(RuntimeFactKey::Group(*id), FactChangeMask::IDENTITY);
        }
        GraphOp::RemoveGroup {
            id,
            detached,
            bindings,
            ..
        } => {
            traversal.visit_embedded_relationships(detached.len().saturating_add(bindings.len()));
            seed.fact(RuntimeFactKey::Group(*id), FactChangeMask::IDENTITY);
            seed.session.sanitize_view_state = true;
            for (node, _) in detached {
                seed.fact(RuntimeFactKey::Node(*node), FactChangeMask::STRUCTURE);
                seed.withdraw_and_dirty(*node);
            }
            for (binding, _) in bindings {
                seed.fact(RuntimeFactKey::Binding(*binding), FactChangeMask::IDENTITY);
                seed.index.bindings.insert(*binding);
            }
        }
        GraphOp::SetGroupRect { id, .. } => {
            seed.fact(RuntimeFactKey::Group(*id), FactChangeMask::POSITION);
        }
        GraphOp::SetGroupTitle { id, .. } | GraphOp::SetGroupColor { id, .. } => {
            seed.fact(RuntimeFactKey::Group(*id), FactChangeMask::PRESENTATION);
        }

        GraphOp::AddStickyNote { id, .. } => {
            seed.fact(RuntimeFactKey::StickyNote(*id), FactChangeMask::IDENTITY);
        }
        GraphOp::RemoveStickyNote { id, bindings, .. } => {
            traversal.visit_embedded_relationships(bindings.len());
            seed.fact(RuntimeFactKey::StickyNote(*id), FactChangeMask::IDENTITY);
            for (binding, _) in bindings {
                seed.fact(RuntimeFactKey::Binding(*binding), FactChangeMask::IDENTITY);
                seed.index.bindings.insert(*binding);
            }
        }
        GraphOp::SetStickyNoteRect { id, .. } => {
            seed.fact(RuntimeFactKey::StickyNote(*id), FactChangeMask::POSITION);
        }
        GraphOp::SetStickyNoteText { id, .. } | GraphOp::SetStickyNoteColor { id, .. } => {
            seed.fact(
                RuntimeFactKey::StickyNote(*id),
                FactChangeMask::PRESENTATION,
            );
        }

        GraphOp::AddBinding { id, .. } | GraphOp::RemoveBinding { id, .. } => {
            seed.fact(RuntimeFactKey::Binding(*id), FactChangeMask::IDENTITY);
            seed.index.bindings.insert(*id);
        }
        GraphOp::SetBindingSubject { id, .. } | GraphOp::SetBindingTarget { id, .. } => {
            seed.fact(RuntimeFactKey::Binding(*id), FactChangeMask::PAYLOAD);
            seed.index.bindings.insert(*id);
        }
        GraphOp::SetBindingKind { id, .. } | GraphOp::SetBindingMeta { id, .. } => {
            seed.fact(RuntimeFactKey::Binding(*id), FactChangeMask::PAYLOAD);
        }
    }
}

impl ImpactSeed {
    fn fact(&mut self, key: RuntimeFactKey, mask: FactChangeMask) {
        self.fact_masks.entry(key).or_default().merge(mask);
        // Geometry follows the same declaration; snapshot-only endpoints are enriched in the op arm.
        match key {
            RuntimeFactKey::Import(_) => self.geometry.make_global(),
            RuntimeFactKey::Node(node) => self.geometry.include_nodes([node]),
            RuntimeFactKey::Port(port) => self.geometry.include_port(port),
            RuntimeFactKey::Edge(edge) => self.geometry.include_incident_edge(edge),
            RuntimeFactKey::Symbol(_)
            | RuntimeFactKey::Group(_)
            | RuntimeFactKey::StickyNote(_)
            | RuntimeFactKey::Binding(_) => {}
        }
    }

    fn include_edge_endpoints(&mut self, edge: &jellyflow_core::core::Edge) {
        self.geometry.include_port(edge.from);
        self.geometry.include_port(edge.to);
    }

    fn hard_replace(&mut self, node: NodeId) {
        self.identities.retired.insert(node);
        self.identities.added.insert(node);
    }

    fn withdraw_and_dirty(&mut self, node: NodeId) {
        self.session.geometry_withdrawals.insert(node);
        self.session.layout_dirty_nodes.insert(node);
    }
}

fn build_session_impact(
    traversal: &mut ImpactTraversal<'_>,
    facts: &[RuntimeFactDelta],
    mut geometry: GeometryImpact,
    index: &IndexImpact,
    identities: &NodeIdentityImpact,
    directives: SessionDirectives,
) -> SessionImpact {
    let mut affected_nodes = BTreeSet::new();
    let mut retired_nodes = identities.retired.clone();
    let mut geometry_withdrawals = directives.geometry_withdrawals;
    let mut layout_dirty_nodes = directives.layout_dirty_nodes;
    let complete_layout_retention_candidates = directives.complete_layout_retention_candidates;
    let mut complete_layout_retention_disqualifications =
        directives.complete_layout_retention_disqualifications;
    let mut route_edges = BTreeSet::new();
    let mut position_only_nodes = BTreeSet::new();
    let staged_symbol_referrers = staged_symbol_referrers(traversal, &index.symbol_ref_nodes);

    traversal.visit_affected(facts.len());
    for fact in facts {
        match fact.key() {
            RuntimeFactKey::Node(node) => {
                affected_nodes.insert(node);
                if fact.change() == FactChange::Removed {
                    retired_nodes.insert(node);
                }
                if fact.change() == FactChange::Updated
                    && fact.change_mask().is_only(FactChangeMask::POSITION)
                {
                    position_only_nodes.insert(node);
                }
            }
            RuntimeFactKey::Port(port) => {
                let owners = [traversal.before_port(port), traversal.after_port(port)]
                    .into_iter()
                    .flatten()
                    .map(|port| port.node)
                    .collect::<BTreeSet<_>>();
                traversal.visit_affected(owners.len());
                affected_nodes.extend(owners.iter().copied());
                let mut live_owners = BTreeSet::new();
                for node in owners {
                    if traversal.after_node(node).is_some() {
                        live_owners.insert(node);
                    }
                }
                geometry_withdrawals.extend(live_owners.iter().copied());
                layout_dirty_nodes.extend(live_owners.iter().copied());
                complete_layout_retention_disqualifications.extend(live_owners);
            }
            RuntimeFactKey::Edge(edge) => {
                let records = [traversal.before_edge(edge), traversal.after_edge(edge)];
                for current in records.into_iter().flatten() {
                    traversal.visit_affected(1);
                    for endpoint in [current.from, current.to] {
                        let endpoint_nodes = [
                            traversal.before_port(endpoint),
                            traversal.after_port(endpoint),
                        ]
                        .into_iter()
                        .flatten()
                        .map(|port| port.node)
                        .collect::<BTreeSet<_>>();
                        affected_nodes.extend(endpoint_nodes.iter().copied());
                        complete_layout_retention_disqualifications.extend(endpoint_nodes);
                    }
                }
                if fact.change() != FactChange::Updated
                    || !fact.change_mask().is_only(FactChangeMask::PAYLOAD)
                {
                    route_edges.insert(edge);
                }
            }
            RuntimeFactKey::Binding(binding) => {
                let records = [
                    traversal.before_binding(binding),
                    traversal.after_binding(binding),
                ];
                for current in records.into_iter().flatten() {
                    traversal.visit_affected(1);
                    let mut binding_nodes = BTreeSet::new();
                    extend_binding_nodes(&mut binding_nodes, traversal, current);
                    affected_nodes.extend(binding_nodes.iter().copied());
                    complete_layout_retention_disqualifications.extend(binding_nodes);
                    extend_binding_geometry(&mut geometry, current);
                }
            }
            RuntimeFactKey::Symbol(symbol) => {
                let mut referrers = traversal
                    .symbol_referrers(symbol)
                    .into_iter()
                    .collect::<BTreeSet<_>>();
                if let Some(staged) = staged_symbol_referrers.get(&symbol) {
                    traversal.visit_affected(staged.len());
                    referrers.extend(staged.iter().copied());
                }
                affected_nodes.extend(referrers.iter().copied());
                layout_dirty_nodes.extend(referrers.iter().copied());
                complete_layout_retention_disqualifications.extend(referrers);
            }
            RuntimeFactKey::Import(_)
            | RuntimeFactKey::Group(_)
            | RuntimeFactKey::StickyNote(_) => {}
        }
    }

    let mut translated_nodes = Vec::new();
    traversal.visit_affected(directives.translated_nodes.len());
    for node in directives.translated_nodes {
        let before = traversal.before_node(node);
        let after = traversal.after_node(node);
        if let (Some(before), Some(after)) = (before, after) {
            translated_nodes.push(NodeTranslation {
                node,
                delta: CanvasPoint {
                    x: after.pos.x - before.pos.x,
                    y: after.pos.y - before.pos.y,
                },
            });
        }
    }

    for node in translated_nodes.iter().map(|translation| translation.node) {
        extend_incident_edges(&mut route_edges, traversal, node);
    }
    for node in &layout_dirty_nodes {
        extend_incident_edges(&mut route_edges, traversal, *node);
    }

    geometry.include_nodes(affected_nodes.iter().copied());
    geometry.mark_translation_only(
        translated_nodes
            .iter()
            .map(|translation| translation.node)
            .filter(|node| {
                !retired_nodes.contains(node)
                    && !geometry_withdrawals.contains(node)
                    && !layout_dirty_nodes.contains(node)
                    && position_only_nodes.contains(node)
            }),
    );
    geometry.retain_complete_layout(
        complete_layout_retention_candidates
            .difference(&complete_layout_retention_disqualifications)
            .filter(|node| {
                !retired_nodes.contains(node)
                    && !identities.added.contains(node)
                    && !geometry_withdrawals.contains(node)
            })
            .copied(),
    );
    if geometry.is_global() {
        geometry.make_global();
    }

    SessionImpact {
        affected_nodes: affected_nodes.into_iter().collect(),
        retired_nodes: retired_nodes.into_iter().collect(),
        geometry_withdrawals: geometry_withdrawals.into_iter().collect(),
        layout_dirty_nodes: layout_dirty_nodes.into_iter().collect(),
        translated_nodes,
        route_edges: route_edges.into_iter().collect(),
        sanitize_view_state: directives.sanitize_view_state,
        geometry,
    }
}

fn fact_change(traversal: &mut ImpactTraversal<'_>, key: RuntimeFactKey) -> FactChange {
    let (before, after) = traversal.presence(key);
    match (before, after) {
        (false, true) => FactChange::Added,
        (true, false) => FactChange::Removed,
        _ => FactChange::Updated,
    }
}

fn staged_symbol_referrers(
    traversal: &mut ImpactTraversal<'_>,
    changed_nodes: &BTreeSet<NodeId>,
) -> BTreeMap<SymbolId, BTreeSet<NodeId>> {
    let mut referrers = BTreeMap::<SymbolId, BTreeSet<NodeId>>::new();
    traversal.visit_affected(changed_nodes.len());
    for node in changed_nodes {
        let Some(symbol) = traversal
            .after_node(*node)
            .and_then(|record| symbol_ref_target_symbol_id(*node, record).ok().flatten())
        else {
            continue;
        };
        referrers.entry(symbol).or_default().insert(*node);
    }
    referrers
}

fn extend_incident_edges(
    route_edges: &mut BTreeSet<EdgeId>,
    traversal: &mut ImpactTraversal<'_>,
    node: NodeId,
) {
    for edge in traversal.incident_edges(node) {
        route_edges.insert(edge);
    }
}

fn extend_binding_nodes(
    nodes: &mut BTreeSet<NodeId>,
    traversal: &mut ImpactTraversal<'_>,
    binding: &Binding,
) {
    for target in [
        binding.subject.graph_local_target(),
        binding.target.graph_local_target(),
    ]
    .into_iter()
    .flatten()
    {
        traversal.visit_affected(1);
        match target {
            GraphLocalBindingTarget::Node { id } => {
                nodes.insert(id);
            }
            GraphLocalBindingTarget::Port { id } => {
                nodes.extend(
                    [traversal.before_port(id), traversal.after_port(id)]
                        .into_iter()
                        .flatten()
                        .map(|port| port.node),
                );
            }
            GraphLocalBindingTarget::Edge { id } => {
                let records = [traversal.before_edge(id), traversal.after_edge(id)];
                for edge in records.into_iter().flatten() {
                    traversal.visit_affected(1);
                    for endpoint in [edge.from, edge.to] {
                        nodes.extend(
                            [
                                traversal.before_port(endpoint),
                                traversal.after_port(endpoint),
                            ]
                            .into_iter()
                            .flatten()
                            .map(|port| port.node),
                        );
                    }
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
            GraphLocalBindingTarget::Node { id } => impact.include_nodes([id]),
            GraphLocalBindingTarget::Port { id } => impact.include_port(id),
            GraphLocalBindingTarget::Edge { id } => impact.include_incident_edge(id),
            GraphLocalBindingTarget::Group { .. } | GraphLocalBindingTarget::StickyNote { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph_kernel::EdgeEndpoints;
    use crate::runtime::commands::{GraphCommand, IncidentEdgePolicy, TopologyEdit};
    use crate::runtime::graph_runtime::{GraphRuntime, RuntimePatch};
    use jellyflow_core::core::{
        BindingEndpoint, CanvasRect, CanvasSize, Edge, EdgeKind, EdgeReconnectable,
        EdgeViewDescriptor, GraphBuilder, GraphId, GraphImport, Group, GroupId, Node, NodeExtent,
        NodeKindKey, NodeOrigin, Port, PortCapacity, PortDirection, PortKey, PortKind,
        SYMBOL_REF_NODE_KIND, StickyNote, StickyNoteId, Symbol, symbol_ref_node_data,
    };
    use jellyflow_core::types::TypeDesc;
    use serde_json::{Value, json};

    fn node_at(pos: CanvasPoint, ports: Vec<PortId>) -> Node {
        Node {
            kind: NodeKindKey::new("test.node"),
            kind_version: 1,
            pos,
            origin: None,
            selectable: None,
            draggable: None,
            connectable: None,
            deletable: None,
            parent: None,
            extent: None,
            expand_parent: None,
            size: None,
            hidden: false,
            collapsed: false,
            ports,
            data: Value::Null,
        }
    }

    fn port(node: NodeId, key: &str, dir: PortDirection) -> Port {
        Port {
            node,
            key: PortKey::new(key),
            dir,
            kind: PortKind::Data,
            capacity: PortCapacity::Multi,
            connectable: None,
            connectable_start: None,
            connectable_end: None,
            ty: None,
            data: Value::Null,
        }
    }

    fn rect(x: f32) -> CanvasRect {
        CanvasRect {
            origin: CanvasPoint { x, y: x + 1.0 },
            size: CanvasSize {
                width: 100.0,
                height: 60.0,
            },
        }
    }

    fn all_operation_variants() -> Vec<GraphOp> {
        let node_id = NodeId::from_u128(1);
        let port_id = PortId::from_u128(2);
        let other_port_id = PortId::from_u128(3);
        let edge_id = EdgeId::from_u128(4);
        let graph_id = GraphId::from_u128(5);
        let symbol_id = SymbolId::from_u128(6);
        let group_id = GroupId::from_u128(7);
        let note_id = StickyNoteId::from_u128(9);
        let binding_id = BindingId::from_u128(10);
        let third_port_id = PortId::from_u128(12);
        let add_node_id = NodeId::from_u128(0x21);
        let add_port_id = PortId::from_u128(0x22);
        let add_edge_id = EdgeId::from_u128(0x23);
        let add_graph_id = GraphId::from_u128(0x24);
        let add_symbol_id = SymbolId::from_u128(0x25);
        let add_group_id = GroupId::from_u128(0x26);
        let add_note_id = StickyNoteId::from_u128(0x27);
        let add_binding_id = BindingId::from_u128(0x28);
        let node = node_at(CanvasPoint::default(), vec![port_id, other_port_id]);
        let port_record = port(node_id, "value", PortDirection::Out);
        let edge = Edge::new(EdgeKind::Data, port_id, other_port_id);
        let import = GraphImport::default();
        let symbol = Symbol {
            name: "value".into(),
            ty: Some(TypeDesc::Float),
            default_value: Some(json!(0.0)),
            meta: Value::Null,
        };
        let group = Group {
            title: "group".into(),
            rect: rect(0.0),
            color: None,
        };
        let note = StickyNote {
            text: "note".into(),
            rect: rect(0.0),
            color: None,
        };
        let subject = BindingEndpoint::node(node_id);
        let target = BindingEndpoint::source_payload("fixture://source", Value::Null);
        let binding = Binding::new(subject.clone(), target.clone());

        vec![
            GraphOp::AddNode {
                id: add_node_id,
                node: node_at(CanvasPoint { x: 20.0, y: 20.0 }, vec![]),
            },
            GraphOp::RemoveNode {
                id: node_id,
                node: node.clone(),
                ports: vec![
                    (port_id, port_record.clone()),
                    (other_port_id, port(node_id, "other", PortDirection::In)),
                ],
                edges: vec![(edge_id, edge.clone())],
                bindings: vec![(binding_id, binding.clone())],
            },
            GraphOp::SetNodePos {
                id: node_id,
                from: CanvasPoint::default(),
                to: CanvasPoint { x: 1.0, y: 2.0 },
            },
            GraphOp::SetNodeOrigin {
                id: node_id,
                from: None,
                to: Some(NodeOrigin { x: 0.5, y: 0.5 }),
            },
            GraphOp::SetNodeKind {
                id: node_id,
                from: NodeKindKey::new("test.node"),
                to: NodeKindKey::new("to"),
            },
            GraphOp::SetNodeKindVersion {
                id: node_id,
                from: 1,
                to: 2,
            },
            GraphOp::SetNodeSelectable {
                id: node_id,
                from: None,
                to: Some(true),
            },
            GraphOp::SetNodeDraggable {
                id: node_id,
                from: None,
                to: Some(true),
            },
            GraphOp::SetNodeConnectable {
                id: node_id,
                from: None,
                to: Some(true),
            },
            GraphOp::SetNodeDeletable {
                id: node_id,
                from: None,
                to: Some(true),
            },
            GraphOp::SetNodeParent {
                id: node_id,
                from: None,
                to: Some(group_id),
            },
            GraphOp::SetNodeExtent {
                id: node_id,
                from: None,
                to: Some(NodeExtent::Parent),
            },
            GraphOp::SetNodeExpandParent {
                id: node_id,
                from: None,
                to: Some(true),
            },
            GraphOp::SetNodeSize {
                id: node_id,
                from: None,
                to: Some(CanvasSize {
                    width: 10.0,
                    height: 20.0,
                }),
            },
            GraphOp::SetNodeHidden {
                id: node_id,
                from: false,
                to: true,
            },
            GraphOp::SetNodeCollapsed {
                id: node_id,
                from: false,
                to: true,
            },
            GraphOp::SetNodePorts {
                id: node_id,
                from: vec![port_id, other_port_id],
                to: vec![other_port_id, port_id],
            },
            GraphOp::SetNodeData {
                id: node_id,
                from: Value::Null,
                to: json!({"value": 1}),
            },
            GraphOp::AddPort {
                id: add_port_id,
                port: port(node_id, "added", PortDirection::In),
            },
            GraphOp::RemovePort {
                id: port_id,
                port: port_record.clone(),
                edges: vec![(edge_id, edge.clone())],
                bindings: vec![],
            },
            GraphOp::SetPortConnectable {
                id: port_id,
                from: None,
                to: Some(true),
            },
            GraphOp::SetPortConnectableStart {
                id: port_id,
                from: None,
                to: Some(true),
            },
            GraphOp::SetPortConnectableEnd {
                id: port_id,
                from: None,
                to: Some(true),
            },
            GraphOp::SetPortType {
                id: port_id,
                from: None,
                to: Some(TypeDesc::Float),
            },
            GraphOp::SetPortData {
                id: port_id,
                from: Value::Null,
                to: json!({"value": 1}),
            },
            GraphOp::AddEdge {
                id: add_edge_id,
                edge: edge.clone(),
            },
            GraphOp::RemoveEdge {
                id: edge_id,
                edge: edge.clone(),
                bindings: vec![],
            },
            GraphOp::SetEdgeKind {
                id: edge_id,
                from: EdgeKind::Data,
                to: EdgeKind::Exec,
            },
            GraphOp::SetEdgeSelectable {
                id: edge_id,
                from: None,
                to: Some(true),
            },
            GraphOp::SetEdgeHidden {
                id: edge_id,
                from: false,
                to: true,
            },
            GraphOp::SetEdgeInteractionWidth {
                id: edge_id,
                from: None,
                to: Some(12.0),
            },
            GraphOp::SetEdgeDeletable {
                id: edge_id,
                from: None,
                to: Some(true),
            },
            GraphOp::SetEdgeReconnectable {
                id: edge_id,
                from: None,
                to: Some(EdgeReconnectable::Bool(true)),
            },
            GraphOp::SetEdgeData {
                id: edge_id,
                from: Value::Null,
                to: json!({"value": 1}),
            },
            GraphOp::SetEdgeView {
                id: edge_id,
                from: EdgeViewDescriptor::default(),
                to: EdgeViewDescriptor::new().with_label("value"),
            },
            GraphOp::SetEdgeEndpoints {
                id: edge_id,
                from: EdgeEndpoints::new(port_id, other_port_id),
                to: EdgeEndpoints::new(port_id, third_port_id),
            },
            GraphOp::AddImport {
                id: add_graph_id,
                import: import.clone(),
            },
            GraphOp::RemoveImport {
                id: graph_id,
                import: import.clone(),
            },
            GraphOp::SetImportAlias {
                id: graph_id,
                from: None,
                to: Some("alias".into()),
            },
            GraphOp::AddSymbol {
                id: add_symbol_id,
                symbol: symbol.clone(),
            },
            GraphOp::RemoveSymbol {
                id: symbol_id,
                symbol: symbol.clone(),
            },
            GraphOp::SetSymbolName {
                id: symbol_id,
                from: "value".into(),
                to: "to".into(),
            },
            GraphOp::SetSymbolType {
                id: symbol_id,
                from: Some(TypeDesc::Float),
                to: None,
            },
            GraphOp::SetSymbolDefaultValue {
                id: symbol_id,
                from: Some(json!(0.0)),
                to: Some(json!(1.0)),
            },
            GraphOp::SetSymbolMeta {
                id: symbol_id,
                from: Value::Null,
                to: json!({"value": 1}),
            },
            GraphOp::AddGroup {
                id: add_group_id,
                group: group.clone(),
            },
            GraphOp::RemoveGroup {
                id: group_id,
                group: group.clone(),
                detached: vec![],
                bindings: vec![],
            },
            GraphOp::SetGroupRect {
                id: group_id,
                from: rect(0.0),
                to: rect(1.0),
            },
            GraphOp::SetGroupTitle {
                id: group_id,
                from: "group".into(),
                to: "to".into(),
            },
            GraphOp::SetGroupColor {
                id: group_id,
                from: None,
                to: Some("red".into()),
            },
            GraphOp::AddStickyNote {
                id: add_note_id,
                note: note.clone(),
            },
            GraphOp::RemoveStickyNote {
                id: note_id,
                note: note.clone(),
                bindings: vec![],
            },
            GraphOp::SetStickyNoteText {
                id: note_id,
                from: "note".into(),
                to: "to".into(),
            },
            GraphOp::SetStickyNoteRect {
                id: note_id,
                from: rect(0.0),
                to: rect(1.0),
            },
            GraphOp::SetStickyNoteColor {
                id: note_id,
                from: None,
                to: Some("yellow".into()),
            },
            GraphOp::AddBinding {
                id: add_binding_id,
                binding: binding.clone(),
            },
            GraphOp::RemoveBinding {
                id: binding_id,
                binding: binding.clone(),
            },
            GraphOp::SetBindingSubject {
                id: binding_id,
                from: subject.clone(),
                to: BindingEndpoint::port(port_id),
            },
            GraphOp::SetBindingTarget {
                id: binding_id,
                from: target.clone(),
                to: BindingEndpoint::edge(edge_id),
            },
            GraphOp::SetBindingKind {
                id: binding_id,
                from: None,
                to: Some("depends_on".into()),
            },
            GraphOp::SetBindingMeta {
                id: binding_id,
                from: Value::Null,
                to: json!({"value": 1}),
            },
        ]
    }

    fn operation_fixture_graph() -> Graph {
        let node_id = NodeId::from_u128(1);
        let port_id = PortId::from_u128(2);
        let other_port_id = PortId::from_u128(3);
        let edge_id = EdgeId::from_u128(4);
        let graph_id = GraphId::from_u128(5);
        let symbol_id = SymbolId::from_u128(6);
        let group_id = GroupId::from_u128(7);
        let note_id = StickyNoteId::from_u128(9);
        let binding_id = BindingId::from_u128(10);
        let other_node_id = NodeId::from_u128(11);
        let third_port_id = PortId::from_u128(12);
        let binding = Binding::new(
            BindingEndpoint::node(node_id),
            BindingEndpoint::source_payload("fixture://source", Value::Null),
        );

        GraphBuilder::new(GraphId::from_u128(0xff))
            .with_import(graph_id, GraphImport::default())
            .with_symbol(
                symbol_id,
                Symbol {
                    name: "value".into(),
                    ty: Some(TypeDesc::Float),
                    default_value: Some(json!(0.0)),
                    meta: Value::Null,
                },
            )
            .with_group(
                group_id,
                Group {
                    title: "group".into(),
                    rect: rect(0.0),
                    color: None,
                },
            )
            .with_sticky_note(
                note_id,
                StickyNote {
                    text: "note".into(),
                    rect: rect(0.0),
                    color: None,
                },
            )
            .with_node(
                node_id,
                node_at(CanvasPoint::default(), vec![port_id, other_port_id]),
            )
            .with_node(
                other_node_id,
                node_at(CanvasPoint { x: 100.0, y: 0.0 }, vec![third_port_id]),
            )
            .with_port(port_id, port(node_id, "value", PortDirection::Out))
            .with_port(other_port_id, port(node_id, "other", PortDirection::In))
            .with_port(
                third_port_id,
                port(other_node_id, "third", PortDirection::In),
            )
            .with_edge(edge_id, Edge::new(EdgeKind::Data, port_id, other_port_id))
            .with_binding(binding_id, binding)
            .build()
            .expect("valid operation fixture graph")
    }

    fn valid_transaction_for_operation(operation: GraphOp) -> GraphTransaction {
        match &operation {
            GraphOp::AddPort { id, port } => {
                let (id, owner) = (*id, port.node);
                GraphTransaction::from_ops([
                    operation,
                    GraphOp::SetNodePorts {
                        id: owner,
                        from: vec![PortId::from_u128(2), PortId::from_u128(3)],
                        to: vec![PortId::from_u128(2), PortId::from_u128(3), id],
                    },
                ])
            }
            GraphOp::RemovePort { id, port, .. } => {
                let (id, owner) = (*id, port.node);
                let from = vec![PortId::from_u128(2), PortId::from_u128(3)];
                let to = from.iter().copied().filter(|port| *port != id).collect();
                GraphTransaction::from_ops([
                    GraphOp::SetNodePorts {
                        id: owner,
                        from,
                        to,
                    },
                    operation,
                ])
            }
            _ => GraphTransaction::from_ops([operation]),
        }
    }

    fn assert_graph_equivalent(actual: &Graph, expected: &Graph, context: &str) {
        let project = |graph: &Graph| {
            json!({
                "graph_id": graph.graph_id(),
                "graph_version": graph.graph_version(),
                "imports": graph.imports().iter().collect::<Vec<_>>(),
                "symbols": graph.symbols().iter().collect::<Vec<_>>(),
                "nodes": graph.nodes().iter().collect::<Vec<_>>(),
                "ports": graph.ports().iter().collect::<Vec<_>>(),
                "edges": graph.edges().iter().collect::<Vec<_>>(),
                "groups": graph.groups().iter().collect::<Vec<_>>(),
                "sticky_notes": graph.sticky_notes().iter().collect::<Vec<_>>(),
                "bindings": graph.bindings().iter().collect::<Vec<_>>(),
            })
        };
        assert_eq!(project(actual), project(expected), "{context}");
    }

    #[derive(Debug)]
    struct OracleImpact {
        facts: Vec<RuntimeFactDelta>,
        session: SessionImpact,
        identities: NodeIdentityImpact,
    }

    fn graph_fact_values(graph: &Graph) -> BTreeMap<RuntimeFactKey, Value> {
        let mut facts = BTreeMap::new();
        for (id, record) in graph.imports().iter() {
            facts.insert(
                RuntimeFactKey::Import(*id),
                serde_json::to_value(record).expect("serialize import"),
            );
        }
        for (id, record) in graph.symbols().iter() {
            facts.insert(
                RuntimeFactKey::Symbol(*id),
                serde_json::to_value(record).expect("serialize symbol"),
            );
        }
        for (id, record) in graph.nodes().iter() {
            facts.insert(
                RuntimeFactKey::Node(*id),
                serde_json::to_value(record).expect("serialize node"),
            );
        }
        for (id, record) in graph.ports().iter() {
            facts.insert(
                RuntimeFactKey::Port(*id),
                serde_json::to_value(record).expect("serialize port"),
            );
        }
        for (id, record) in graph.edges().iter() {
            facts.insert(
                RuntimeFactKey::Edge(*id),
                serde_json::to_value(record).expect("serialize edge"),
            );
        }
        for (id, record) in graph.groups().iter() {
            facts.insert(
                RuntimeFactKey::Group(*id),
                serde_json::to_value(record).expect("serialize group"),
            );
        }
        for (id, record) in graph.sticky_notes().iter() {
            facts.insert(
                RuntimeFactKey::StickyNote(*id),
                serde_json::to_value(record).expect("serialize sticky note"),
            );
        }
        for (id, record) in graph.bindings().iter() {
            facts.insert(
                RuntimeFactKey::Binding(*id),
                serde_json::to_value(record).expect("serialize binding"),
            );
        }
        facts
    }

    fn changed_fields(before: &Value, after: &Value) -> BTreeSet<String> {
        let before = before.as_object().expect("before fact must be an object");
        let after = after.as_object().expect("after fact must be an object");
        before
            .keys()
            .chain(after.keys())
            .filter(|field| before.get(*field) != after.get(*field))
            .cloned()
            .collect()
    }

    fn field_mask(key: RuntimeFactKey, field: &str) -> FactChangeMask {
        match key {
            RuntimeFactKey::Import(_) => match field {
                "alias" => FactChangeMask::PAYLOAD,
                _ => panic!("unclassified import field: {field}"),
            },
            RuntimeFactKey::Symbol(_) => match field {
                "name" | "ty" | "default_value" | "meta" => FactChangeMask::PAYLOAD,
                _ => panic!("unclassified symbol field: {field}"),
            },
            RuntimeFactKey::Node(_) => match field {
                "pos" | "origin" => FactChangeMask::POSITION,
                "kind" | "kind_version" | "size" | "collapsed" => FactChangeMask::LAYOUT,
                "parent" | "ports" => FactChangeMask::STRUCTURE,
                "hidden" => FactChangeMask::VISIBILITY,
                "data" => FactChangeMask::PAYLOAD,
                "selectable" | "draggable" | "connectable" | "deletable" | "extent"
                | "expand_parent" => FactChangeMask::PRESENTATION,
                _ => panic!("unclassified node field: {field}"),
            },
            RuntimeFactKey::Port(_) => match field {
                "connectable" | "connectable_start" | "connectable_end" => {
                    FactChangeMask::PRESENTATION
                }
                "ty" | "data" => FactChangeMask::PAYLOAD,
                _ => panic!("unclassified mutable port field: {field}"),
            },
            RuntimeFactKey::Edge(_) => match field {
                "from" | "to" => FactChangeMask::STRUCTURE,
                "hidden" => FactChangeMask::VISIBILITY,
                "data" => FactChangeMask::PAYLOAD,
                "kind" | "selectable" | "interaction_width" | "deletable" | "reconnectable"
                | "view" => FactChangeMask::PRESENTATION,
                _ => panic!("unclassified edge field: {field}"),
            },
            RuntimeFactKey::Group(_) => match field {
                "rect" => FactChangeMask::POSITION,
                "title" | "color" => FactChangeMask::PRESENTATION,
                _ => panic!("unclassified group field: {field}"),
            },
            RuntimeFactKey::StickyNote(_) => match field {
                "rect" => FactChangeMask::POSITION,
                "text" | "color" => FactChangeMask::PRESENTATION,
                _ => panic!("unclassified sticky-note field: {field}"),
            },
            RuntimeFactKey::Binding(_) => match field {
                "subject" | "target" | "kind" | "meta" => FactChangeMask::PAYLOAD,
                _ => panic!("unclassified binding field: {field}"),
            },
        }
    }

    fn updated_fact(
        key: RuntimeFactKey,
        before: &Value,
        after: &Value,
    ) -> (RuntimeFactDelta, BTreeSet<String>) {
        let fields = changed_fields(before, after);
        assert!(
            !fields.is_empty(),
            "updated fact must change at least one field"
        );
        let mut mask = FactChangeMask::default();
        for field in &fields {
            mask.merge(field_mask(key, field));
        }
        (
            RuntimeFactDelta::new(key, FactChange::Updated, mask),
            fields,
        )
    }

    fn oracle_fact_diff(
        before: &Graph,
        after: &Graph,
    ) -> (
        Vec<RuntimeFactDelta>,
        BTreeMap<RuntimeFactKey, BTreeSet<String>>,
    ) {
        let before_values = graph_fact_values(before);
        let after_values = graph_fact_values(after);
        let keys = before_values
            .keys()
            .chain(after_values.keys())
            .copied()
            .collect::<BTreeSet<_>>();
        let mut facts = Vec::new();
        let mut fields = BTreeMap::new();
        for key in keys {
            match (before_values.get(&key), after_values.get(&key)) {
                (None, Some(_)) => facts.push(RuntimeFactDelta::new(
                    key,
                    FactChange::Added,
                    FactChangeMask::IDENTITY,
                )),
                (Some(_), None) => facts.push(RuntimeFactDelta::new(
                    key,
                    FactChange::Removed,
                    FactChangeMask::IDENTITY,
                )),
                (Some(before), Some(after)) if before != after => {
                    let (fact, changed) = updated_fact(key, before, after);
                    facts.push(fact);
                    fields.insert(key, changed);
                }
                (Some(_), Some(_)) => {}
                (None, None) => unreachable!("union key must exist in one graph"),
            }
        }
        (facts, fields)
    }

    fn extend_oracle_edge_nodes(
        nodes: &mut BTreeSet<NodeId>,
        before: &Graph,
        after: &Graph,
        edge: &Edge,
    ) {
        for endpoint in [edge.from, edge.to] {
            nodes.extend(
                [before.port(&endpoint), after.port(&endpoint)]
                    .into_iter()
                    .flatten()
                    .map(|port| port.node),
            );
        }
    }

    fn extend_oracle_binding(
        nodes: &mut BTreeSet<NodeId>,
        geometry: &mut GeometryImpact,
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
                GraphLocalBindingTarget::Graph => geometry.make_global(),
                GraphLocalBindingTarget::Node { id } => {
                    nodes.insert(id);
                    geometry.include_nodes([id]);
                }
                GraphLocalBindingTarget::Port { id } => {
                    geometry.include_port(id);
                    nodes.extend(
                        [before.port(&id), after.port(&id)]
                            .into_iter()
                            .flatten()
                            .map(|port| port.node),
                    );
                }
                GraphLocalBindingTarget::Edge { id } => {
                    geometry.include_incident_edge(id);
                    for edge in [before.edge(&id), after.edge(&id)].into_iter().flatten() {
                        extend_oracle_edge_nodes(nodes, before, after, edge);
                    }
                }
                GraphLocalBindingTarget::Group { .. }
                | GraphLocalBindingTarget::StickyNote { .. } => {}
            }
        }
    }

    fn oracle_incident_edges(graph: &Graph, node: NodeId) -> BTreeSet<EdgeId> {
        graph
            .edges()
            .iter()
            .filter_map(|(edge_id, edge)| {
                let touches_node = [edge.from, edge.to]
                    .into_iter()
                    .filter_map(|port| graph.port(&port))
                    .any(|port| port.node == node);
                touches_node.then_some(*edge_id)
            })
            .collect()
    }

    fn derive_oracle_impact(before: &Graph, after: &Graph) -> OracleImpact {
        let (facts, changed_fields) = oracle_fact_diff(before, after);
        let mut affected_nodes = BTreeSet::new();
        let mut retired_nodes = BTreeSet::new();
        let mut added_nodes = BTreeSet::new();
        let mut geometry_withdrawals = BTreeSet::new();
        let mut layout_dirty_nodes = BTreeSet::new();
        let mut translated_nodes = Vec::new();
        let mut route_edges = BTreeSet::new();
        let mut position_only_nodes = BTreeSet::new();
        let mut complete_layout_retention_nodes = BTreeSet::new();
        let mut complete_layout_retention_disqualifications = BTreeSet::new();
        let mut sanitize_view_state = false;
        let mut geometry = GeometryImpact::default();

        for fact in &facts {
            let key = fact.key();
            let fields = changed_fields.get(&key).cloned().unwrap_or_default();
            match key {
                RuntimeFactKey::Import(_) => geometry.make_global(),
                RuntimeFactKey::Node(node) => {
                    geometry.include_nodes([node]);
                    affected_nodes.insert(node);
                    match fact.change() {
                        FactChange::Added => {
                            added_nodes.insert(node);
                            layout_dirty_nodes.insert(node);
                            if let Some(record) = after.node(&node) {
                                for port in &record.ports {
                                    geometry.include_port(*port);
                                }
                            }
                        }
                        FactChange::Removed => {
                            retired_nodes.insert(node);
                            sanitize_view_state = true;
                        }
                        FactChange::Updated => {
                            if fields.contains("kind") || fields.contains("kind_version") {
                                retired_nodes.insert(node);
                                added_nodes.insert(node);
                                layout_dirty_nodes.insert(node);
                            }
                            if fields.contains("data") {
                                layout_dirty_nodes.insert(node);
                            }
                            if fields.contains("size") {
                                layout_dirty_nodes.insert(node);
                                if fields
                                    .iter()
                                    .all(|field| matches!(field.as_str(), "size" | "pos"))
                                {
                                    complete_layout_retention_nodes.insert(node);
                                }
                            }
                            if fields.iter().any(|field| {
                                matches!(
                                    field.as_str(),
                                    "origin" | "parent" | "ports" | "collapsed" | "hidden"
                                )
                            }) {
                                geometry_withdrawals.insert(node);
                                layout_dirty_nodes.insert(node);
                            }
                            if fields.contains("hidden") {
                                sanitize_view_state = true;
                            }
                            if fields.contains("ports") {
                                for record in [before.node(&node), after.node(&node)]
                                    .into_iter()
                                    .flatten()
                                {
                                    for port in &record.ports {
                                        geometry.include_port(*port);
                                    }
                                }
                            }
                            if fields.contains("pos") {
                                let before = before.node(&node).expect("updated node before");
                                let after = after.node(&node).expect("updated node after");
                                translated_nodes.push(NodeTranslation {
                                    node,
                                    delta: CanvasPoint {
                                        x: after.pos.x - before.pos.x,
                                        y: after.pos.y - before.pos.y,
                                    },
                                });
                            }
                            if fact.change_mask().is_only(FactChangeMask::POSITION) {
                                position_only_nodes.insert(node);
                            }
                        }
                    }
                }
                RuntimeFactKey::Port(port) => {
                    geometry.include_port(port);
                    let owners = [before.port(&port), after.port(&port)]
                        .into_iter()
                        .flatten()
                        .map(|port| port.node)
                        .collect::<BTreeSet<_>>();
                    affected_nodes.extend(owners.iter().copied());
                    for owner in owners {
                        if after.node(&owner).is_some() {
                            geometry_withdrawals.insert(owner);
                            layout_dirty_nodes.insert(owner);
                            complete_layout_retention_disqualifications.insert(owner);
                        }
                    }
                    if fact.change() == FactChange::Removed {
                        sanitize_view_state = true;
                    }
                }
                RuntimeFactKey::Edge(edge_id) => {
                    geometry.include_incident_edge(edge_id);
                    for edge in [before.edge(&edge_id), after.edge(&edge_id)]
                        .into_iter()
                        .flatten()
                    {
                        let mut endpoint_nodes = BTreeSet::new();
                        extend_oracle_edge_nodes(&mut endpoint_nodes, before, after, edge);
                        affected_nodes.extend(endpoint_nodes.iter().copied());
                        complete_layout_retention_disqualifications.extend(endpoint_nodes);
                        if fact.change() != FactChange::Updated
                            || fields.contains("from")
                            || fields.contains("to")
                        {
                            geometry.include_port(edge.from);
                            geometry.include_port(edge.to);
                        }
                    }
                    if fact.change() != FactChange::Updated
                        || !fact.change_mask().is_only(FactChangeMask::PAYLOAD)
                    {
                        route_edges.insert(edge_id);
                    }
                    if fact.change() == FactChange::Removed
                        || fields.contains("from")
                        || fields.contains("to")
                        || fields.contains("hidden")
                    {
                        sanitize_view_state = true;
                    }
                }
                RuntimeFactKey::Symbol(symbol) => {
                    let referrers = before
                        .nodes()
                        .iter()
                        .chain(after.nodes().iter())
                        .filter_map(|(node, record)| {
                            (symbol_ref_target_symbol_id(*node, record).ok().flatten()
                                == Some(symbol))
                            .then_some(*node)
                        })
                        .collect::<BTreeSet<_>>();
                    affected_nodes.extend(referrers.iter().copied());
                    layout_dirty_nodes.extend(referrers.iter().copied());
                    complete_layout_retention_disqualifications.extend(referrers);
                }
                RuntimeFactKey::Group(_) => {
                    if fact.change() == FactChange::Removed {
                        sanitize_view_state = true;
                    }
                }
                RuntimeFactKey::StickyNote(_) => {}
                RuntimeFactKey::Binding(binding) => {
                    for binding in [before.binding(&binding), after.binding(&binding)]
                        .into_iter()
                        .flatten()
                    {
                        let mut binding_nodes = BTreeSet::new();
                        extend_oracle_binding(
                            &mut binding_nodes,
                            &mut geometry,
                            before,
                            after,
                            binding,
                        );
                        affected_nodes.extend(binding_nodes.iter().copied());
                        complete_layout_retention_disqualifications.extend(binding_nodes);
                    }
                }
            }
        }

        for node in translated_nodes
            .iter()
            .map(|translation| translation.node)
            .chain(layout_dirty_nodes.iter().copied())
        {
            route_edges.extend(oracle_incident_edges(before, node));
        }
        geometry.include_nodes(affected_nodes.iter().copied());
        geometry.mark_translation_only(
            translated_nodes
                .iter()
                .map(|translation| translation.node)
                .filter(|node| {
                    !retired_nodes.contains(node)
                        && !geometry_withdrawals.contains(node)
                        && !layout_dirty_nodes.contains(node)
                        && position_only_nodes.contains(node)
                }),
        );
        geometry.retain_complete_layout(
            complete_layout_retention_nodes
                .difference(&complete_layout_retention_disqualifications)
                .filter(|node| {
                    !retired_nodes.contains(node)
                        && !added_nodes.contains(node)
                        && !geometry_withdrawals.contains(node)
                })
                .copied(),
        );
        if geometry.is_global() {
            geometry.make_global();
        }

        OracleImpact {
            facts,
            session: SessionImpact {
                affected_nodes: affected_nodes.into_iter().collect(),
                retired_nodes: retired_nodes.iter().copied().collect(),
                geometry_withdrawals: geometry_withdrawals.into_iter().collect(),
                layout_dirty_nodes: layout_dirty_nodes.into_iter().collect(),
                translated_nodes,
                route_edges: route_edges.into_iter().collect(),
                sanitize_view_state,
                geometry,
            },
            identities: NodeIdentityImpact {
                retired: retired_nodes,
                added: added_nodes,
            },
        }
    }

    fn assert_patch_matches_oracle(patch: &RuntimePatch, expected: &OracleImpact, context: &str) {
        assert_eq!(patch.facts(), expected.facts, "{context}");
        let actual = patch.impact();
        assert_eq!(
            actual.affected_nodes, expected.session.affected_nodes,
            "{context}"
        );
        assert_eq!(
            actual.retired_nodes, expected.session.retired_nodes,
            "{context}"
        );
        assert_eq!(
            actual.geometry_withdrawals, expected.session.geometry_withdrawals,
            "{context}"
        );
        assert_eq!(
            actual.layout_dirty_nodes, expected.session.layout_dirty_nodes,
            "{context}"
        );
        assert_eq!(
            actual.translated_nodes, expected.session.translated_nodes,
            "{context}"
        );
        assert_eq!(
            actual.route_edges, expected.session.route_edges,
            "{context}"
        );
        assert_eq!(
            actual.sanitize_view_state, expected.session.sanitize_view_state,
            "{context}"
        );
        assert_eq!(actual.geometry, expected.session.geometry, "{context}");

        let expected_nodes = expected
            .identities
            .retired
            .union(&expected.identities.added)
            .copied()
            .collect::<BTreeSet<_>>();
        let actual_nodes = patch
            .node_lifetime_changes()
            .map(|(node, _)| node)
            .collect::<BTreeSet<_>>();
        assert_eq!(actual_nodes, expected_nodes, "{context}");
        for node in expected_nodes {
            let change = patch
                .node_lifetime_change(node)
                .expect("expected lifetime change");
            if expected.identities.added.contains(&node) {
                assert!(change.is_some(), "{context}: {node:?} must be reincarnated");
            } else {
                assert!(change.is_none(), "{context}: {node:?} must be retired");
            }
        }
    }

    fn install_transaction_for_impact_test(
        runtime: &GraphRuntime,
        transaction: GraphTransaction,
    ) -> RuntimePatch {
        let mut state = runtime.shared.state.borrow_mut();
        let mut graph = (*state.graph).clone();
        transaction
            .apply_to(&mut graph)
            .expect("valid installed impact transaction");
        let (patch, sessions) = super::super::execution::install_incremental(
            &mut state,
            graph,
            transaction,
            true,
            &runtime.shared.views,
        )
        .expect("install transaction");
        assert!(sessions.is_empty());
        patch
    }

    fn assert_installed_index_matches_graph(runtime: &GraphRuntime, context: &str) {
        let snapshot = runtime.snapshot().expect("runtime snapshot");
        let (index, incarnations, _) = runtime.installed_impact_state_for_test();
        assert_eq!(index, GraphIndex::from_graph(snapshot.graph()), "{context}");
        assert_eq!(
            incarnations.keys().copied().collect::<BTreeSet<_>>(),
            snapshot.graph().nodes().keys().copied().collect(),
            "{context}"
        );
    }

    #[test]
    fn every_operation_installs_and_replays_against_rebuilt_authority() {
        let operations = all_operation_variants();
        let mut operation_names = BTreeSet::new();
        assert_eq!(operations.len(), 61);

        for operation in operations {
            let operation_name =
                serde_json::to_value(&operation).expect("serialize operation")["op"]
                    .as_str()
                    .expect("operation tag")
                    .to_owned();
            assert!(
                operation_names.insert(operation_name.clone()),
                "duplicate operation fixture: {operation_name}"
            );
            let transaction = valid_transaction_for_operation(operation);
            let runtime = GraphRuntime::builder(Default::default())
                .build(operation_fixture_graph())
                .expect("operation fixture runtime");
            let before = runtime.snapshot().expect("before snapshot").graph().clone();
            let mut after = before.clone();
            transaction
                .apply_to(&mut after)
                .unwrap_or_else(|error| panic!("{operation_name}: invalid fixture: {error}"));
            let forward = derive_oracle_impact(&before, &after);

            let applied = install_transaction_for_impact_test(&runtime, transaction.clone());
            assert_patch_matches_oracle(&applied, &forward, &format!("{operation_name}: execute"));
            assert_graph_equivalent(
                runtime.snapshot().expect("applied snapshot").graph(),
                &after,
                &format!("{operation_name}: execute graph"),
            );
            assert_installed_index_matches_graph(&runtime, &format!("{operation_name}: execute"));

            let undo_impact = derive_oracle_impact(&after, &before);
            let undone = execute_applied(
                &runtime,
                GraphCommand::undo(runtime.snapshot().expect("applied snapshot").version()),
            );
            assert_patch_matches_oracle(&undone, &undo_impact, &format!("{operation_name}: undo"));
            assert_graph_equivalent(
                runtime.snapshot().expect("undone snapshot").graph(),
                &before,
                &format!("{operation_name}: undo graph"),
            );
            assert_installed_index_matches_graph(&runtime, &format!("{operation_name}: undo"));

            let redo_impact = derive_oracle_impact(&before, &after);
            let redone = execute_applied(
                &runtime,
                GraphCommand::redo(runtime.snapshot().expect("undone snapshot").version()),
            );
            assert_patch_matches_oracle(&redone, &redo_impact, &format!("{operation_name}: redo"));
            assert_graph_equivalent(
                runtime.snapshot().expect("redone snapshot").graph(),
                &after,
                &format!("{operation_name}: redo graph"),
            );
            assert_installed_index_matches_graph(&runtime, &format!("{operation_name}: redo"));
        }
        assert_eq!(operation_names.len(), 61);
    }

    #[test]
    fn interleaved_position_updates_report_the_authoritative_net_delta() {
        let node_id = NodeId::from_u128(0x101);
        let before = GraphBuilder::new(GraphId::from_u128(0x102))
            .with_node(node_id, node_at(CanvasPoint::default(), vec![]))
            .build()
            .expect("valid fixture graph");
        let transaction = GraphTransaction::from_ops([
            GraphOp::SetNodePos {
                id: node_id,
                from: CanvasPoint::default(),
                to: CanvasPoint { x: 10.0, y: 2.0 },
            },
            GraphOp::SetNodeData {
                id: node_id,
                from: Value::Null,
                to: json!({"interleaved": true}),
            },
            GraphOp::SetNodePos {
                id: node_id,
                from: CanvasPoint { x: 10.0, y: 2.0 },
                to: CanvasPoint { x: 25.0, y: 5.0 },
            },
        ]);
        let mut after = before.clone();
        transaction
            .apply_to(&mut after)
            .expect("apply fixture transaction");
        let index = GraphIndex::from_graph(&before);

        let impact = InstalledTransactionImpact::derive(&before, &after, &index, &transaction);

        assert_eq!(impact.facts.len(), 1);
        assert!(
            impact.facts[0]
                .change_mask()
                .contains(FactChangeMask::POSITION)
        );
        assert!(
            impact.facts[0]
                .change_mask()
                .contains(FactChangeMask::PAYLOAD)
        );
        assert_eq!(impact.session.translated_nodes.len(), 1);
        assert_eq!(impact.session.translated_nodes[0].node, node_id);
        assert_eq!(
            impact.session.translated_nodes[0].delta,
            CanvasPoint { x: 25.0, y: 5.0 }
        );
        assert!(!impact.session.geometry.preserves_translated_layout(
            node_id,
            std::iter::empty(),
            std::iter::empty()
        ));
    }

    #[test]
    fn unrelated_interleaved_edit_preserves_net_translation_layout() {
        let moved = NodeId::from_u128(0x111);
        let other = NodeId::from_u128(0x112);
        let before = GraphBuilder::new(GraphId::from_u128(0x113))
            .with_node(moved, node_at(CanvasPoint::default(), vec![]))
            .with_node(other, node_at(CanvasPoint { x: 100.0, y: 0.0 }, vec![]))
            .build()
            .expect("valid fixture graph");
        let transaction = GraphTransaction::from_ops([
            GraphOp::SetNodePos {
                id: moved,
                from: CanvasPoint::default(),
                to: CanvasPoint { x: 10.0, y: 2.0 },
            },
            GraphOp::SetNodeSelectable {
                id: other,
                from: None,
                to: Some(true),
            },
            GraphOp::SetNodePos {
                id: moved,
                from: CanvasPoint { x: 10.0, y: 2.0 },
                to: CanvasPoint { x: 25.0, y: 5.0 },
            },
        ]);
        let mut after = before.clone();
        transaction
            .apply_to(&mut after)
            .expect("apply fixture transaction");
        let index = GraphIndex::from_graph(&before);

        let impact = InstalledTransactionImpact::derive(&before, &after, &index, &transaction);

        assert_eq!(impact.session.translated_nodes.len(), 1);
        assert_eq!(impact.session.translated_nodes[0].node, moved);
        assert_eq!(
            impact.session.translated_nodes[0].delta,
            CanvasPoint { x: 25.0, y: 5.0 }
        );
        assert!(impact.session.layout_dirty_nodes.is_empty());
        assert!(impact.session.geometry.preserves_translated_layout(
            moved,
            std::iter::empty(),
            std::iter::empty()
        ));
    }

    #[test]
    fn size_and_translation_can_project_the_last_complete_layout_together() {
        let node_id = NodeId::from_u128(0x114);
        let before = GraphBuilder::new(GraphId::from_u128(0x115))
            .with_node(node_id, node_at(CanvasPoint::default(), vec![]))
            .build()
            .expect("valid fixture graph");
        let size = CanvasSize {
            width: 220.0,
            height: 140.0,
        };
        let transaction = GraphTransaction::from_ops([
            GraphOp::SetNodePos {
                id: node_id,
                from: CanvasPoint::default(),
                to: CanvasPoint { x: 25.0, y: 5.0 },
            },
            GraphOp::SetNodeSize {
                id: node_id,
                from: None,
                to: Some(size),
            },
        ]);
        let mut after = before.clone();
        transaction
            .apply_to(&mut after)
            .expect("apply fixture transaction");
        let index = GraphIndex::from_graph(&before);

        let impact = InstalledTransactionImpact::derive(&before, &after, &index, &transaction);

        assert_eq!(impact.session.translated_nodes.len(), 1);
        assert_eq!(impact.session.layout_dirty_nodes, vec![node_id]);
        assert!(
            impact
                .session
                .geometry
                .retains_last_complete_layout(node_id)
        );
        assert!(impact.session.geometry_withdrawals.is_empty());
    }

    #[test]
    fn geometry_impact_merge_uses_a_per_node_retention_meet() {
        let first = NodeId::from_u128(0x118);
        let second = NodeId::from_u128(0x119);
        let size_impact = |node| {
            let mut impact = GeometryImpact::default();
            impact.include_nodes([node]);
            impact.retain_complete_layout([node]);
            impact
        };
        let ordinary_impact = |node| {
            let mut impact = GeometryImpact::default();
            impact.include_nodes([node]);
            impact
        };

        let mut repeated_size = size_impact(first);
        repeated_size.merge(&size_impact(first));
        assert!(repeated_size.retains_last_complete_layout(first));

        let mut size_then_data = size_impact(first);
        size_then_data.merge(&ordinary_impact(first));
        assert!(!size_then_data.retains_last_complete_layout(first));

        let mut data_then_size = ordinary_impact(first);
        data_then_size.merge(&size_impact(first));
        assert!(!data_then_size.retains_last_complete_layout(first));

        let mut independent_nodes = size_impact(first);
        independent_nodes.merge(&ordinary_impact(second));
        assert!(independent_nodes.retains_last_complete_layout(first));
        assert!(!independent_nodes.retains_last_complete_layout(second));

        let mut global = size_impact(first);
        let mut invalidates_all = GeometryImpact::default();
        invalidates_all.make_global();
        global.merge(&invalidates_all);
        assert!(!global.retains_last_complete_layout(first));
    }

    #[test]
    fn cascaded_port_removal_does_not_redirty_a_retired_node() {
        let node_id = NodeId::from_u128(0x121);
        let port_id = PortId::from_u128(0x122);
        let node = node_at(CanvasPoint::default(), vec![port_id]);
        let port = port(node_id, "value", PortDirection::Out);
        let before = GraphBuilder::new(GraphId::from_u128(0x123))
            .with_node(node_id, node.clone())
            .with_port(port_id, port.clone())
            .build()
            .expect("valid fixture graph");
        let transaction = GraphTransaction::from_ops([GraphOp::RemoveNode {
            id: node_id,
            node,
            ports: vec![(port_id, port)],
            edges: vec![],
            bindings: vec![],
        }]);
        let mut after = before.clone();
        transaction
            .apply_to(&mut after)
            .expect("remove fixture node");
        let index = GraphIndex::from_graph(&before);

        let impact = InstalledTransactionImpact::derive(&before, &after, &index, &transaction);

        assert_eq!(impact.session.affected_nodes, vec![node_id]);
        assert_eq!(impact.session.retired_nodes, vec![node_id]);
        assert!(impact.session.geometry_withdrawals.is_empty());
        assert!(impact.session.layout_dirty_nodes.is_empty());
        assert_eq!(impact.index.ports, BTreeSet::from([port_id]));
    }

    fn indexed_fixture() -> (Graph, GraphTransaction) {
        let symbol_a = SymbolId::from_u128(0x201);
        let symbol_b = SymbolId::from_u128(0x202);
        let source_node = NodeId::from_u128(0x203);
        let first_target = NodeId::from_u128(0x204);
        let second_target = NodeId::from_u128(0x205);
        let source_port = PortId::from_u128(0x206);
        let first_port = PortId::from_u128(0x207);
        let second_port = PortId::from_u128(0x208);
        let edge_id = EdgeId::from_u128(0x209);
        let binding_id = BindingId::from_u128(0x20a);
        let mut symbol_node = node_at(CanvasPoint::default(), vec![source_port]);
        symbol_node.kind = NodeKindKey::new(SYMBOL_REF_NODE_KIND);
        symbol_node.data = symbol_ref_node_data(symbol_a);
        let symbol = |name: &str| Symbol {
            name: name.into(),
            ty: Some(TypeDesc::Float),
            default_value: None,
            meta: Value::Null,
        };
        let binding = Binding::new(
            BindingEndpoint::port(first_port),
            BindingEndpoint::source_payload("fixture://source", Value::Null),
        );
        let graph = GraphBuilder::new(GraphId::from_u128(0x20b))
            .with_symbol(symbol_a, symbol("a"))
            .with_symbol(symbol_b, symbol("b"))
            .with_node(source_node, symbol_node)
            .with_node(
                first_target,
                node_at(CanvasPoint { x: 50.0, y: 0.0 }, vec![first_port]),
            )
            .with_node(
                second_target,
                node_at(CanvasPoint { x: 50.0, y: 50.0 }, vec![second_port]),
            )
            .with_port(source_port, port(source_node, "out", PortDirection::Out))
            .with_port(first_port, port(first_target, "in", PortDirection::In))
            .with_port(second_port, port(second_target, "in", PortDirection::In))
            .with_edge(edge_id, Edge::new(EdgeKind::Data, source_port, first_port))
            .with_binding(binding_id, binding.clone())
            .build()
            .expect("valid indexed fixture");
        let transaction = GraphTransaction::from_ops([
            GraphOp::SetEdgeEndpoints {
                id: edge_id,
                from: EdgeEndpoints::new(source_port, first_port),
                to: EdgeEndpoints::new(source_port, second_port),
            },
            GraphOp::SetBindingSubject {
                id: binding_id,
                from: binding.subject,
                to: BindingEndpoint::port(second_port),
            },
            GraphOp::SetNodeData {
                id: source_node,
                from: symbol_ref_node_data(symbol_a),
                to: symbol_ref_node_data(symbol_b),
            },
        ]);
        (graph, transaction)
    }

    fn apply_and_verify_index(
        before: &Graph,
        before_index: &GraphIndex,
        transaction: &GraphTransaction,
    ) -> (Graph, GraphIndex) {
        let mut after = before.clone();
        transaction
            .apply_to(&mut after)
            .expect("apply indexed fixture transaction");
        let impact = InstalledTransactionImpact::derive(before, &after, before_index, transaction);
        let mut updated = before_index.clone();
        updated.apply_impact(&after, &impact.index);
        assert_eq!(updated, GraphIndex::from_graph(&after));
        (after, updated)
    }

    #[test]
    fn incremental_index_matches_rebuild_for_transaction_and_inverse_replay() {
        let (before, transaction) = indexed_fixture();
        let before_index = GraphIndex::from_graph(&before);

        let (after, after_index) = apply_and_verify_index(&before, &before_index, &transaction);
        let inverse = transaction.inverse();
        let (undone, undone_index) = apply_and_verify_index(&after, &after_index, &inverse);
        assert_eq!(undone_index, before_index);
        let (redone, redone_index) = apply_and_verify_index(&undone, &undone_index, &transaction);
        assert_eq!(redone_index, after_index);
        assert_eq!(redone.nodes().len(), after.nodes().len());
    }

    fn execute_applied(runtime: &GraphRuntime, command: GraphCommand) -> RuntimePatch {
        runtime
            .execute(command)
            .into_result()
            .expect("runtime command")
            .applied()
            .expect("command applies")
            .patch()
            .clone()
    }

    fn assert_installed_index_matches_snapshot(
        runtime: &GraphRuntime,
    ) -> (
        GraphIndex,
        BTreeMap<NodeId, super::super::NodeIncarnation>,
        u64,
    ) {
        let state = runtime.installed_impact_state_for_test();
        let snapshot = runtime.snapshot().expect("runtime snapshot");
        assert_eq!(state.0, GraphIndex::from_graph(snapshot.graph()));
        assert_eq!(
            state.1.keys().copied().collect::<BTreeSet<_>>(),
            snapshot.graph().nodes().keys().copied().collect()
        );
        state
    }

    fn assert_identity_facts(
        patch: &RuntimePatch,
        expected_change: FactChange,
        expected_keys: impl IntoIterator<Item = RuntimeFactKey>,
    ) {
        let expected = expected_keys.into_iter().collect::<BTreeSet<_>>();
        let actual = patch
            .facts()
            .iter()
            .map(|fact| {
                assert_eq!(fact.change(), expected_change);
                assert_eq!(fact.change_mask(), FactChangeMask::IDENTITY);
                fact.key()
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(actual, expected);
    }

    #[test]
    fn runtime_execute_undo_redo_installs_consistent_impact_state() {
        let source = NodeId::from_u128(0x281);
        let target = NodeId::from_u128(0x282);
        let source_port = PortId::from_u128(0x283);
        let target_port = PortId::from_u128(0x284);
        let edge = EdgeId::from_u128(0x285);
        let binding = BindingId::from_u128(0x286);
        let source_record = node_at(CanvasPoint::default(), vec![source_port]);
        let target_record = node_at(CanvasPoint { x: 100.0, y: 0.0 }, vec![target_port]);
        let graph = GraphBuilder::new(GraphId::from_u128(0x287))
            .with_node(source, source_record)
            .with_node(target, target_record)
            .with_port(source_port, port(source, "out", PortDirection::Out))
            .with_port(target_port, port(target, "in", PortDirection::In))
            .with_edge(edge, Edge::new(EdgeKind::Data, source_port, target_port))
            .with_binding(
                binding,
                Binding::new(
                    BindingEndpoint::node(source),
                    BindingEndpoint::source_payload("fixture://source", Value::Null),
                ),
            )
            .build()
            .expect("valid runtime history fixture");
        let runtime = GraphRuntime::builder(Default::default())
            .build(graph)
            .expect("runtime");
        let (_, initial_incarnations, initial_allocator) =
            assert_installed_index_matches_snapshot(&runtime);
        let initial_source_incarnation = initial_incarnations[&source];
        let initial_target_incarnation = initial_incarnations[&target];
        let initial_edge_incarnation = runtime
            .snapshot()
            .expect("initial graph snapshot")
            .edge_incarnation(edge)
            .expect("initial edge incarnation");
        let expected_keys = [
            RuntimeFactKey::Node(source),
            RuntimeFactKey::Port(source_port),
            RuntimeFactKey::Edge(edge),
            RuntimeFactKey::Binding(binding),
        ];

        let removed = execute_applied(
            &runtime,
            GraphCommand::topology(
                runtime
                    .snapshot()
                    .expect("initial graph snapshot")
                    .version(),
                TopologyEdit::new().remove_node(source, IncidentEdgePolicy::Cascade),
            ),
        );
        assert_identity_facts(&removed, FactChange::Removed, expected_keys);
        let (_, removed_incarnations, removed_allocator) =
            assert_installed_index_matches_snapshot(&runtime);
        assert!(!removed_incarnations.contains_key(&source));
        assert_eq!(removed_incarnations[&target], initial_target_incarnation);
        assert_eq!(removed_allocator, initial_allocator);
        assert_eq!(
            runtime
                .snapshot()
                .expect("removed graph snapshot")
                .edge_incarnation(edge),
            None
        );
        assert_eq!(removed.node_lifetime_change(source), Some(None));
        assert_eq!(removed.impact().retired_nodes, vec![source]);
        assert!(removed.impact().layout_dirty_nodes.is_empty());
        assert_eq!(removed.impact().route_edges, vec![edge]);
        assert!(removed.impact().sanitize_view_state);
        assert!(removed.geometry_impact().nodes().contains(&source));
        assert!(removed.geometry_impact().nodes().contains(&target));
        assert!(removed.geometry_impact().ports().contains(&source_port));
        assert!(removed.geometry_impact().ports().contains(&target_port));
        assert!(removed.geometry_impact().incident_edges().contains(&edge));

        let restored = execute_applied(
            &runtime,
            GraphCommand::undo(
                runtime
                    .snapshot()
                    .expect("removed graph snapshot")
                    .version(),
            ),
        );
        assert_identity_facts(&restored, FactChange::Added, expected_keys);
        let (_, restored_incarnations, restored_allocator) =
            assert_installed_index_matches_snapshot(&runtime);
        let restored_source_incarnation = restored_incarnations[&source];
        assert!(restored_source_incarnation > initial_source_incarnation);
        assert_eq!(restored_incarnations[&target], initial_target_incarnation);
        assert_eq!(restored_allocator, initial_allocator + 3);
        assert!(
            runtime
                .snapshot()
                .expect("restored graph snapshot")
                .edge_incarnation(edge)
                .expect("restored edge incarnation")
                > initial_edge_incarnation
        );
        assert_eq!(
            restored.node_lifetime_change(source),
            Some(Some(
                restored
                    .snapshot()
                    .node_lifetime_key(source)
                    .expect("restored source lifetime")
            ))
        );
        assert!(restored.impact().retired_nodes.is_empty());
        assert_eq!(restored.impact().geometry_withdrawals, vec![source]);
        assert_eq!(restored.impact().layout_dirty_nodes, vec![source]);
        assert_eq!(restored.impact().route_edges, vec![edge]);
        assert!(!restored.impact().sanitize_view_state);

        let removed_again = execute_applied(
            &runtime,
            GraphCommand::redo(
                runtime
                    .snapshot()
                    .expect("restored graph snapshot")
                    .version(),
            ),
        );
        assert_identity_facts(&removed_again, FactChange::Removed, expected_keys);
        let (_, redone_incarnations, redone_allocator) =
            assert_installed_index_matches_snapshot(&runtime);
        assert!(!redone_incarnations.contains_key(&source));
        assert_eq!(redone_incarnations[&target], initial_target_incarnation);
        assert_eq!(redone_allocator, restored_allocator);
        assert_eq!(removed_again.node_lifetime_change(source), Some(None));
        assert_eq!(removed_again.impact().retired_nodes, vec![source]);
        assert!(removed_again.impact().layout_dirty_nodes.is_empty());
        assert_eq!(removed_again.impact().route_edges, vec![edge]);
        assert!(removed_again.impact().sanitize_view_state);
    }

    fn derive_local_node_edit(
        node_count: u128,
    ) -> (InstalledTransactionImpact, ImpactTraversalStats) {
        let target = NodeId::from_u128(0x301);
        let mut builder = GraphBuilder::new(GraphId::from_u128(0x302));
        builder.insert_node(target, node_at(CanvasPoint::default(), vec![]));
        for index in 0..node_count - 1 {
            builder.insert_node(
                NodeId::from_u128(0x1_000 + index),
                node_at(
                    CanvasPoint {
                        x: index as f32,
                        y: 0.0,
                    },
                    vec![],
                ),
            );
        }
        let before = builder.build().expect("valid large graph");
        let transaction = GraphTransaction::from_ops([GraphOp::SetNodeData {
            id: target,
            from: Value::Null,
            to: json!({"changed": true}),
        }]);
        let mut after = before.clone();
        transaction.apply_to(&mut after).expect("apply local edit");
        let index = GraphIndex::from_graph(&before);

        InstalledTransactionImpact::derive_with_stats(&before, &after, &index, &transaction)
    }

    #[test]
    fn ten_thousand_node_edit_stays_within_the_declared_traversal_budget() {
        let target = NodeId::from_u128(0x301);
        let (_, small_stats) = derive_local_node_edit(10);
        let (impact, large_stats) = derive_local_node_edit(10_000);

        assert_eq!(impact.facts.len(), 1);
        assert_eq!(impact.session.affected_nodes, vec![target]);
        assert!(impact.session.geometry_withdrawals.is_empty());
        assert_eq!(impact.session.layout_dirty_nodes, vec![target]);
        assert!(impact.index.ports.is_empty());
        assert!(impact.index.edges.is_empty());
        assert!(impact.index.bindings.is_empty());
        assert_eq!(impact.index.symbol_ref_nodes, BTreeSet::from([target]));
        assert_eq!(large_stats, small_stats);
        assert_eq!(large_stats.operation_visits, 1);
        assert_eq!(large_stats.embedded_relationship_visits, 0);
        assert_eq!(large_stats.fact_materializations, 1);
        assert_eq!(large_stats.graph_point_reads, 3);
        assert_eq!(large_stats.affected_set_visits, 2);
        assert_eq!(large_stats.index_bucket_reads, 1);
        assert_eq!(large_stats.index_entries_read, 0);
    }

    #[test]
    fn graph_binding_requires_global_geometry_invalidation() {
        let binding = Binding::graph_local_to_source(
            GraphLocalBindingTarget::Graph,
            "fixture://graph",
            Value::Null,
        );
        let mut impact = GeometryImpact::default();

        extend_binding_geometry(&mut impact, &binding);

        assert!(impact.is_global());
        assert!(impact.nodes().is_empty());
        assert!(impact.ports().is_empty());
        assert!(impact.incident_edges().is_empty());
    }
}
