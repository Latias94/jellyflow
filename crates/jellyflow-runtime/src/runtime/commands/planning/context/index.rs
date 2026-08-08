use std::collections::{BTreeMap, BTreeSet};

use crate::graph_kernel::GraphOp;
use jellyflow_core::core::{
    BindingId, EdgeId, Graph, GraphLocalBindingTarget, NodeId, PortId, SymbolId,
    symbol_ref_target_symbol_id,
};

use crate::runtime::graph_runtime::GraphIndex;

use super::super::{GraphCommandPlanningStats, stricter_incident_policy};
use crate::runtime::commands::IncidentEdgePolicy;

pub(super) struct CommandPlanningIndex<'a> {
    base: &'a GraphIndex,
    added_nodes: BTreeSet<NodeId>,
    removed_nodes: BTreeSet<NodeId>,
    added_ports: BTreeSet<PortId>,
    removed_ports: BTreeSet<PortId>,
    added_edges: BTreeSet<EdgeId>,
    removed_edges: BTreeSet<EdgeId>,
    type_revalidations: BTreeMap<PortId, IncidentEdgePolicy>,
    type_revalidation_policy: Option<IncidentEdgePolicy>,
    edge_overrides: BTreeMap<EdgeId, Option<(PortId, PortId)>>,
    staged_incident_edges: BTreeMap<PortId, BTreeSet<EdgeId>>,
    removed_bindings: BTreeSet<BindingId>,
    symbol_ref_overrides: BTreeMap<NodeId, Option<SymbolId>>,
    staged_symbol_referrers: BTreeMap<SymbolId, BTreeSet<NodeId>>,
}

impl<'a> CommandPlanningIndex<'a> {
    pub(super) fn new(base: &'a GraphIndex) -> Self {
        Self {
            base,
            added_nodes: BTreeSet::new(),
            removed_nodes: BTreeSet::new(),
            added_ports: BTreeSet::new(),
            removed_ports: BTreeSet::new(),
            added_edges: BTreeSet::new(),
            removed_edges: BTreeSet::new(),
            type_revalidations: BTreeMap::new(),
            type_revalidation_policy: None,
            edge_overrides: BTreeMap::new(),
            staged_incident_edges: BTreeMap::new(),
            removed_bindings: BTreeSet::new(),
            symbol_ref_overrides: BTreeMap::new(),
            staged_symbol_referrers: BTreeMap::new(),
        }
    }

    pub(super) fn was_node_added(&self, node: NodeId) -> bool {
        self.added_nodes.contains(&node)
    }

    pub(super) fn was_node_removed(&self, node: NodeId) -> bool {
        self.removed_nodes.contains(&node)
    }

    pub(super) fn was_port_added(&self, port: PortId) -> bool {
        self.added_ports.contains(&port)
    }

    pub(super) fn was_port_removed(&self, port: PortId) -> bool {
        self.removed_ports.contains(&port)
    }

    pub(super) fn was_edge_added(&self, edge: EdgeId) -> bool {
        self.added_edges.contains(&edge)
    }

    pub(super) fn was_edge_removed(&self, edge: EdgeId) -> bool {
        self.removed_edges.contains(&edge)
    }

    pub(super) fn schedule_type_revalidation(
        &mut self,
        ports: impl IntoIterator<Item = PortId>,
        policy: IncidentEdgePolicy,
    ) {
        self.type_revalidation_policy = Some(
            self.type_revalidation_policy
                .map(|current| stricter_incident_policy(current, policy))
                .unwrap_or(policy),
        );
        for port in ports {
            self.type_revalidations
                .entry(port)
                .and_modify(|current| *current = stricter_incident_policy(*current, policy))
                .or_insert(policy);
        }
    }

    pub(super) fn take_type_revalidations(&mut self) -> BTreeMap<PortId, IncidentEdgePolicy> {
        std::mem::take(&mut self.type_revalidations)
    }

    pub(super) fn type_revalidation_policy(&self) -> Option<IncidentEdgePolicy> {
        self.type_revalidation_policy
    }

    pub(super) fn base_incident_edges(
        &self,
        port: PortId,
        stats: &mut GraphCommandPlanningStats,
    ) -> BTreeSet<EdgeId> {
        stats.read_index_bucket();
        let edges = self.base.incident_edges(port).collect::<BTreeSet<_>>();
        stats.read_index_entries(edges.len());
        edges
    }

    pub(super) fn incident_edges(
        &self,
        port: PortId,
        stats: &mut GraphCommandPlanningStats,
    ) -> BTreeSet<EdgeId> {
        let mut edges = self
            .base_incident_edges(port, stats)
            .into_iter()
            .filter(|edge| {
                self.edge_overrides
                    .get(edge)
                    .map(|endpoints| endpoints.is_some_and(|(from, to)| from == port || to == port))
                    .unwrap_or(true)
            })
            .collect::<BTreeSet<_>>();
        let staged = self
            .staged_incident_edges
            .get(&port)
            .into_iter()
            .flatten()
            .copied()
            .collect::<Vec<_>>();
        stats.read_index_entries(staged.len());
        edges.extend(staged);
        edges
    }

    pub(super) fn bindings_for_targets(
        &self,
        targets: impl IntoIterator<Item = GraphLocalBindingTarget>,
        stats: &mut GraphCommandPlanningStats,
    ) -> BTreeSet<BindingId> {
        targets
            .into_iter()
            .flat_map(|target| {
                stats.read_index_bucket();
                let bindings = self.base.bindings_for_target(target).collect::<Vec<_>>();
                stats.read_index_entries(bindings.len());
                bindings
            })
            .filter(|binding| !self.removed_bindings.contains(binding))
            .collect()
    }

    pub(super) fn symbol_referrers_before(
        &self,
        symbol: SymbolId,
        stats: &mut GraphCommandPlanningStats,
    ) -> Vec<NodeId> {
        stats.read_index_bucket();
        let referrers = self.base.symbol_referrers(symbol).collect::<Vec<_>>();
        stats.read_index_entries(referrers.len());
        referrers
    }

    pub(super) fn symbol_referrers_after(
        &self,
        symbol: SymbolId,
        stats: &mut GraphCommandPlanningStats,
    ) -> Vec<NodeId> {
        stats.read_index_bucket();
        let base = self.base.symbol_referrers(symbol).collect::<Vec<_>>();
        stats.read_index_entries(base.len());
        stats.read_index_bucket();
        let staged = self
            .staged_symbol_referrers
            .get(&symbol)
            .into_iter()
            .flatten()
            .copied()
            .collect::<Vec<_>>();
        stats.read_index_entries(staged.len());
        base.into_iter()
            .filter(|node| !self.symbol_ref_overrides.contains_key(node))
            .chain(staged)
            .collect()
    }

    pub(super) fn apply_operations(&mut self, graph: &Graph, operations: &[GraphOp]) {
        let mut symbol_ref_nodes = BTreeSet::new();
        for operation in operations {
            match operation {
                GraphOp::AddNode { id, .. } => {
                    self.added_nodes.insert(*id);
                    symbol_ref_nodes.insert(*id);
                }
                GraphOp::RemoveNode {
                    id,
                    ports,
                    edges,
                    bindings,
                    ..
                } => {
                    self.removed_nodes.insert(*id);
                    symbol_ref_nodes.insert(*id);
                    self.removed_ports
                        .extend(ports.iter().map(|(port, _)| *port));
                    self.removed_edges
                        .extend(edges.iter().map(|(edge, _)| *edge));
                    for (edge, _) in edges {
                        self.set_edge_override(*edge, None);
                    }
                    self.removed_bindings
                        .extend(bindings.iter().map(|(binding, _)| *binding));
                }
                GraphOp::AddPort { id, .. } => {
                    self.added_ports.insert(*id);
                }
                GraphOp::RemovePort {
                    id,
                    edges,
                    bindings,
                    ..
                } => {
                    self.removed_ports.insert(*id);
                    self.removed_edges
                        .extend(edges.iter().map(|(edge, _)| *edge));
                    for (edge, _) in edges {
                        self.set_edge_override(*edge, None);
                    }
                    self.removed_bindings
                        .extend(bindings.iter().map(|(binding, _)| *binding));
                }
                GraphOp::AddEdge { id, edge } => {
                    self.added_edges.insert(*id);
                    self.set_edge_override(*id, Some((edge.from, edge.to)));
                }
                GraphOp::SetEdgeEndpoints { id, to, .. } => {
                    self.set_edge_override(*id, Some((to.from, to.to)));
                }
                GraphOp::RemoveEdge { id, bindings, .. } => {
                    self.removed_edges.insert(*id);
                    self.set_edge_override(*id, None);
                    self.removed_bindings
                        .extend(bindings.iter().map(|(id, _)| *id));
                }
                GraphOp::RemoveBinding { id, .. } => {
                    self.removed_bindings.insert(*id);
                }
                GraphOp::SetNodeData { id, .. } | GraphOp::SetNodeKind { id, .. } => {
                    symbol_ref_nodes.insert(*id);
                }
                _ => {}
            }
        }
        for node in symbol_ref_nodes {
            let target = graph
                .node(&node)
                .and_then(|record| symbol_ref_target_symbol_id(node, record).ok().flatten());
            self.set_symbol_ref_override(node, target);
        }
    }

    fn set_edge_override(&mut self, edge: EdgeId, endpoints: Option<(PortId, PortId)>) {
        if let Some(Some((from, to))) = self.edge_overrides.get(&edge).copied() {
            self.remove_staged_incident_edge(from, edge);
            self.remove_staged_incident_edge(to, edge);
        }
        self.edge_overrides.insert(edge, endpoints);
        if let Some((from, to)) = endpoints {
            self.staged_incident_edges
                .entry(from)
                .or_default()
                .insert(edge);
            self.staged_incident_edges
                .entry(to)
                .or_default()
                .insert(edge);
        }
    }

    fn remove_staged_incident_edge(&mut self, port: PortId, edge: EdgeId) {
        let remove_bucket = self
            .staged_incident_edges
            .get_mut(&port)
            .is_some_and(|edges| {
                edges.remove(&edge);
                edges.is_empty()
            });
        if remove_bucket {
            self.staged_incident_edges.remove(&port);
        }
    }

    fn set_symbol_ref_override(&mut self, node: NodeId, target: Option<SymbolId>) {
        if let Some(Some(previous)) = self.symbol_ref_overrides.insert(node, target) {
            let remove_bucket =
                self.staged_symbol_referrers
                    .get_mut(&previous)
                    .is_some_and(|nodes| {
                        nodes.remove(&node);
                        nodes.is_empty()
                    });
            if remove_bucket {
                self.staged_symbol_referrers.remove(&previous);
            }
        }
        if let Some(target) = target {
            self.staged_symbol_referrers
                .entry(target)
                .or_default()
                .insert(node);
        }
    }
}

#[cfg(test)]
mod tests {
    use jellyflow_core::core::{Graph, GraphId, PortId};

    use crate::runtime::graph_runtime::GraphIndex;

    use super::{CommandPlanningIndex, IncidentEdgePolicy};

    #[test]
    fn type_revalidation_policy_survives_empty_drains_and_never_weakens() {
        let graph = Graph::new(GraphId::from_u128(1));
        let base = GraphIndex::from_graph(&graph);
        let mut index = CommandPlanningIndex::new(&base);

        index.schedule_type_revalidation([], IncidentEdgePolicy::Cascade);
        assert!(index.take_type_revalidations().is_empty());
        assert_eq!(
            index.type_revalidation_policy(),
            Some(IncidentEdgePolicy::Cascade)
        );

        index.schedule_type_revalidation([PortId::from_u128(1)], IncidentEdgePolicy::Reject);
        index.take_type_revalidations();
        index.schedule_type_revalidation([PortId::from_u128(2)], IncidentEdgePolicy::Cascade);
        assert_eq!(
            index.type_revalidation_policy(),
            Some(IncidentEdgePolicy::Reject)
        );
    }
}
