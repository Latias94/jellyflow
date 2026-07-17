use std::collections::{BTreeMap, BTreeSet};

use crate::graph_kernel::{GraphOp, GraphTransaction};
use jellyflow_core::core::{
    BindingId, EdgeId, Graph, GraphLocalBindingTarget, NodeId, PortId, SymbolId,
};

use crate::profile::PortTypeImpactQuery;
use crate::runtime::commands::IncidentEdgePolicy;
use crate::runtime::graph_runtime::GraphIndex;
use crate::runtime::transaction_gate::GraphTransactionGate;
use crate::schema::NodeCatalog;

use super::{GraphCommandPlanningError, GraphCommandPlanningStats};

mod index;

use self::index::CommandPlanningIndex;

/// Owns every mutable artifact produced while one command is planned.
pub(super) struct PlanningContext<'index> {
    scratch: Graph,
    catalog: &'index NodeCatalog,
    index: CommandPlanningIndex<'index>,
    operations: Vec<GraphOp>,
    stats: GraphCommandPlanningStats,
}

impl<'index> PlanningContext<'index> {
    pub(super) fn new(
        graph: &Graph,
        index: &'index GraphIndex,
        catalog: &'index NodeCatalog,
    ) -> Self {
        Self {
            scratch: graph.clone(),
            catalog,
            index: CommandPlanningIndex::new(index),
            operations: Vec::new(),
            stats: GraphCommandPlanningStats::default(),
        }
    }

    pub(super) fn graph(&self) -> &Graph {
        &self.scratch
    }

    pub(super) fn catalog(&self) -> &NodeCatalog {
        self.catalog
    }

    pub(super) fn operations(&self) -> &[GraphOp] {
        &self.operations
    }

    pub(super) fn read_facts(&mut self, count: usize) {
        self.stats.read_facts(count);
    }

    pub(super) fn record_full_type_revalidation(&mut self) {
        self.stats.record_full_type_revalidation();
    }

    pub(super) fn was_node_added(&self, node: NodeId) -> bool {
        self.index.was_node_added(node)
    }

    pub(super) fn was_node_removed(&self, node: NodeId) -> bool {
        self.index.was_node_removed(node)
    }

    pub(super) fn was_port_added(&self, port: PortId) -> bool {
        self.index.was_port_added(port)
    }

    pub(super) fn was_port_removed(&self, port: PortId) -> bool {
        self.index.was_port_removed(port)
    }

    pub(super) fn was_edge_added(&self, edge: EdgeId) -> bool {
        self.index.was_edge_added(edge)
    }

    pub(super) fn was_edge_removed(&self, edge: EdgeId) -> bool {
        self.index.was_edge_removed(edge)
    }

    pub(super) fn schedule_type_revalidation(
        &mut self,
        ports: impl IntoIterator<Item = PortId>,
        policy: IncidentEdgePolicy,
    ) {
        self.index.schedule_type_revalidation(ports, policy);
    }

    pub(super) fn take_type_revalidations(&mut self) -> BTreeMap<PortId, IncidentEdgePolicy> {
        self.index.take_type_revalidations()
    }

    pub(super) fn type_revalidation_policy(&self) -> Option<IncidentEdgePolicy> {
        self.index.type_revalidation_policy()
    }

    pub(super) fn incident_edges(&mut self, port: PortId) -> BTreeSet<EdgeId> {
        self.index.incident_edges(port, &mut self.stats)
    }

    pub(super) fn symbol_referrers(&mut self, symbol: SymbolId) -> Vec<NodeId> {
        self.index.symbol_referrers_after(symbol, &mut self.stats)
    }

    pub(super) fn query_parts(&mut self) -> (&Graph, PlanningQuery<'_, 'index>) {
        (
            &self.scratch,
            PlanningQuery {
                index: &self.index,
                stats: &mut self.stats,
            },
        )
    }

    /// Advances the scratch graph, incremental index, journal, and counters as one step.
    pub(super) fn stage(
        &mut self,
        operations: impl IntoIterator<Item = GraphOp>,
    ) -> Result<(), GraphCommandPlanningError> {
        let operations = operations.into_iter().collect::<Vec<_>>();
        if operations.is_empty() {
            return Ok(());
        }
        let transaction = GraphTransaction::from_ops(operations);
        GraphTransactionGate::validate(&transaction)?;
        transaction.apply_prevalidated_batch(&mut self.scratch)?;
        let operations = transaction.into_ops();
        self.index.apply_operations(&self.scratch, &operations);
        self.stats.emit_operations(operations.len());
        self.operations.extend(operations);
        Ok(())
    }

    pub(super) fn into_parts(self) -> (Graph, Vec<GraphOp>, GraphCommandPlanningStats) {
        (self.scratch, self.operations, self.stats)
    }
}

pub(super) struct PlanningQuery<'query, 'index> {
    index: &'query CommandPlanningIndex<'index>,
    stats: &'query mut GraphCommandPlanningStats,
}

impl PlanningQuery<'_, '_> {
    pub(super) fn read_facts(&mut self, count: usize) {
        self.stats.read_facts(count);
    }

    pub(super) fn was_port_added(&self, port: PortId) -> bool {
        self.index.was_port_added(port)
    }

    pub(super) fn was_edge_added(&self, edge: EdgeId) -> bool {
        self.index.was_edge_added(edge)
    }

    pub(super) fn incident_edges(&mut self, port: PortId) -> BTreeSet<EdgeId> {
        self.index.incident_edges(port, self.stats)
    }

    pub(super) fn bindings_for_targets(
        &mut self,
        targets: impl IntoIterator<Item = GraphLocalBindingTarget>,
    ) -> BTreeSet<BindingId> {
        self.index.bindings_for_targets(targets, self.stats)
    }
}

impl PortTypeImpactQuery for PlanningQuery<'_, '_> {
    fn record_fact_reads(&mut self, count: usize) {
        self.read_facts(count);
    }

    fn incident_edge_ids_before(&mut self, port: PortId) -> Vec<EdgeId> {
        self.index
            .base_incident_edges(port, self.stats)
            .into_iter()
            .collect()
    }

    fn incident_edge_ids_after(&mut self, port: PortId) -> Vec<EdgeId> {
        self.incident_edges(port).into_iter().collect()
    }

    fn symbol_referrer_ids_before(&mut self, symbol: SymbolId) -> Vec<NodeId> {
        self.index.symbol_referrers_before(symbol, self.stats)
    }

    fn symbol_referrer_ids_after(&mut self, symbol: SymbolId) -> Vec<NodeId> {
        self.index.symbol_referrers_after(symbol, self.stats)
    }
}
