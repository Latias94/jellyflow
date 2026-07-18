use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::graph_kernel::{GraphOp, GraphTransaction};
use jellyflow_core::core::{
    Binding, BindingId, EdgeId, Graph, GraphLocalBindingTarget, Node, NodeId, PortId, SymbolId,
    symbol_ref_target_symbol_id,
};

#[derive(Debug, Default)]
pub(crate) struct IncidentNodeQuery {
    pub(crate) nodes: BTreeSet<NodeId>,
    pub(crate) index_bucket_reads: u64,
    pub(crate) index_entries_read: u64,
    pub(crate) truncated: bool,
}

#[derive(Debug, Default)]
pub(crate) struct GraphIndex {
    port_owners: BTreeMap<PortId, NodeId>,
    edge_endpoints: BTreeMap<EdgeId, (PortId, PortId)>,
    incident_edges: BTreeMap<PortId, BTreeSet<EdgeId>>,
    incident_edge_snapshots: BTreeMap<NodeId, Arc<BTreeSet<EdgeId>>>,
    binding_targets: BTreeMap<BindingId, BTreeSet<GraphLocalBindingTarget>>,
    bindings_by_target: BTreeMap<GraphLocalBindingTarget, BTreeSet<BindingId>>,
    symbol_ref_targets: BTreeMap<NodeId, SymbolId>,
    symbol_referrers: BTreeMap<SymbolId, BTreeSet<NodeId>>,
}

impl GraphIndex {
    pub(crate) fn from_graph(graph: &Graph) -> Self {
        let mut index = Self::default();
        let mut incident_edges_by_node = graph
            .nodes()
            .keys()
            .copied()
            .map(|node| (node, BTreeSet::new()))
            .collect::<BTreeMap<_, _>>();
        for (port_id, port) in graph.ports().iter() {
            index.port_owners.insert(*port_id, port.node);
        }
        for (edge_id, edge) in graph.edges().iter() {
            index.insert_edge(*edge_id, edge.from, edge.to);
            for endpoint in [edge.from, edge.to] {
                let Some(owner) = index.port_owners.get(&endpoint).copied() else {
                    continue;
                };
                incident_edges_by_node
                    .entry(owner)
                    .or_default()
                    .insert(*edge_id);
            }
        }
        index.incident_edge_snapshots = incident_edges_by_node
            .into_iter()
            .map(|(node, edges)| (node, Arc::new(edges)))
            .collect();
        for (binding_id, binding) in graph.bindings().iter() {
            index.insert_binding(*binding_id, binding);
        }
        for (node_id, node) in graph.nodes().iter() {
            index.insert_symbol_ref(*node_id, node);
        }
        index
    }

    pub(crate) fn incident_edges(&self, port: PortId) -> impl Iterator<Item = EdgeId> + '_ {
        self.incident_edges
            .get(&port)
            .into_iter()
            .flat_map(|edges| edges.iter().copied())
    }

    pub(crate) fn incident_edge_count(&self, port: PortId) -> usize {
        self.incident_edges.get(&port).map_or(0, BTreeSet::len)
    }

    pub(crate) fn bindings_for_target(
        &self,
        target: GraphLocalBindingTarget,
    ) -> impl Iterator<Item = BindingId> + '_ {
        self.bindings_by_target
            .get(&target)
            .into_iter()
            .flat_map(|bindings| bindings.iter().copied())
    }

    pub(crate) fn incident_edges_for_node(
        &self,
        node: NodeId,
    ) -> impl Iterator<Item = EdgeId> + '_ {
        self.incident_edge_snapshots
            .get(&node)
            .into_iter()
            .flat_map(|edges| edges.iter().copied())
    }

    pub(crate) fn incident_edge_snapshot(&self, node: NodeId) -> Option<Arc<BTreeSet<EdgeId>>> {
        self.incident_edge_snapshots.get(&node).cloned()
    }

    pub(crate) fn incident_nodes_for_nodes(
        &self,
        graph: &Graph,
        nodes: &BTreeSet<NodeId>,
        excluded: &BTreeSet<NodeId>,
        node_limit: usize,
        scan_limit: usize,
    ) -> IncidentNodeQuery {
        let mut query = IncidentNodeQuery::default();
        if scan_limit == 0 {
            query.truncated = nodes.iter().any(|node_id| {
                graph
                    .node(node_id)
                    .is_some_and(|node| !node.hidden && !node.ports.is_empty())
            });
            return query;
        }
        for node_id in nodes {
            let Some(node) = graph.node(node_id) else {
                continue;
            };
            if node.hidden {
                continue;
            }
            for port in &node.ports {
                if query.index_bucket_reads >= scan_limit as u64 {
                    query.truncated = true;
                    return query;
                }
                query.index_bucket_reads = query.index_bucket_reads.saturating_add(1);
                let Some(edges) = self.incident_edges.get(port) else {
                    continue;
                };
                for edge_id in edges {
                    if query.index_entries_read >= scan_limit as u64 {
                        query.truncated = true;
                        return query;
                    }
                    query.index_entries_read = query.index_entries_read.saturating_add(1);
                    if graph.edge(edge_id).is_none_or(|edge| edge.hidden) {
                        continue;
                    }
                    let Some((from, to)) = self.edge_endpoints.get(edge_id).copied() else {
                        continue;
                    };
                    let opposite_port = if from == *port {
                        to
                    } else if to == *port {
                        from
                    } else {
                        continue;
                    };
                    let Some(opposite_node) = self.port_owners.get(&opposite_port).copied() else {
                        continue;
                    };
                    if opposite_node == *node_id
                        || excluded.contains(&opposite_node)
                        || query.nodes.contains(&opposite_node)
                    {
                        continue;
                    }
                    if query.nodes.len() >= node_limit {
                        query.truncated = true;
                        return query;
                    }
                    query.nodes.insert(opposite_node);
                }
            }
        }
        query
    }

    pub(crate) fn symbol_referrers(&self, symbol: SymbolId) -> impl Iterator<Item = NodeId> + '_ {
        self.symbol_referrers
            .get(&symbol)
            .into_iter()
            .flat_map(|nodes| nodes.iter().copied())
    }

    pub(super) fn apply_transaction(&mut self, graph: &Graph, tx: &GraphTransaction) {
        let mut edges = BTreeSet::new();
        let mut ports = BTreeSet::new();
        let mut bindings = BTreeSet::new();
        let mut symbol_ref_nodes = BTreeSet::new();
        let mut reset_incident_nodes = BTreeSet::new();
        let mut incident_nodes = BTreeSet::new();

        for op in tx.ops() {
            collect_index_changes(
                op,
                &mut ports,
                &mut edges,
                &mut bindings,
                &mut symbol_ref_nodes,
            );
            match op {
                GraphOp::AddNode { id, .. } | GraphOp::RemoveNode { id, .. } => {
                    reset_incident_nodes.insert(*id);
                    incident_nodes.insert(*id);
                }
                GraphOp::SetNodePorts { id, .. } => {
                    incident_nodes.insert(*id);
                }
                _ => {}
            }
        }

        for edge in &edges {
            if let Some((from, to)) = self.edge_endpoints.get(edge).copied() {
                incident_nodes.extend(
                    [from, to]
                        .into_iter()
                        .filter_map(|port| self.port_owners.get(&port).copied()),
                );
            }
            if let Some(current) = graph.edge(edge) {
                incident_nodes.extend(
                    [current.from, current.to]
                        .into_iter()
                        .filter_map(|port| graph.port(&port).map(|port| port.node)),
                );
            }
        }

        for port in &ports {
            incident_nodes.extend(self.port_owners.get(port).copied());
            incident_nodes.extend(graph.port(port).map(|port| port.node));
        }

        for edge in &edges {
            self.remove_edge(*edge);
        }

        for port in &ports {
            self.port_owners.remove(port);
            if let Some(current) = graph.port(port) {
                self.port_owners.insert(*port, current.node);
            }
        }

        for binding in &bindings {
            self.remove_binding(*binding);
            if let Some(current) = graph.binding(binding) {
                self.insert_binding(*binding, current);
            }
        }

        for node in symbol_ref_nodes {
            self.remove_symbol_ref(node);
            if let Some(current) = graph.node(&node) {
                self.insert_symbol_ref(node, current);
            }
        }

        for edge in edges {
            if let Some(current) = graph.edge(&edge) {
                self.insert_edge(edge, current.from, current.to);
            }
        }

        for node in incident_nodes {
            self.refresh_incident_edge_snapshot(graph, node, reset_incident_nodes.contains(&node));
        }
    }

    fn insert_edge(&mut self, edge: EdgeId, from: PortId, to: PortId) {
        self.edge_endpoints.insert(edge, (from, to));
        self.incident_edges.entry(from).or_default().insert(edge);
        self.incident_edges.entry(to).or_default().insert(edge);
    }

    fn remove_edge(&mut self, edge: EdgeId) {
        let Some((from, to)) = self.edge_endpoints.remove(&edge) else {
            return;
        };
        self.remove_incident_edge(from, edge);
        self.remove_incident_edge(to, edge);
    }

    fn remove_incident_edge(&mut self, port: PortId, edge: EdgeId) {
        let Some(edges) = self.incident_edges.get_mut(&port) else {
            return;
        };
        edges.remove(&edge);
        if edges.is_empty() {
            self.incident_edges.remove(&port);
        }
    }

    fn refresh_incident_edge_snapshot(&mut self, graph: &Graph, node: NodeId, force: bool) {
        let Some(current) = graph.node(&node) else {
            self.incident_edge_snapshots.remove(&node);
            return;
        };
        let incident_edges = current
            .ports
            .iter()
            .filter_map(|port| self.incident_edges.get(port))
            .flat_map(|edges| edges.iter().copied())
            .collect::<BTreeSet<_>>();
        if !force
            && self
                .incident_edge_snapshots
                .get(&node)
                .is_some_and(|snapshot| snapshot.as_ref() == &incident_edges)
        {
            return;
        }
        self.incident_edge_snapshots
            .insert(node, Arc::new(incident_edges));
    }

    fn insert_binding(&mut self, id: BindingId, binding: &Binding) {
        let targets = binding_targets(binding);
        for target in &targets {
            self.bindings_by_target
                .entry(*target)
                .or_default()
                .insert(id);
        }
        self.binding_targets.insert(id, targets);
    }

    fn remove_binding(&mut self, id: BindingId) {
        let Some(targets) = self.binding_targets.remove(&id) else {
            return;
        };
        for target in targets {
            let Some(bindings) = self.bindings_by_target.get_mut(&target) else {
                continue;
            };
            bindings.remove(&id);
            if bindings.is_empty() {
                self.bindings_by_target.remove(&target);
            }
        }
    }

    fn insert_symbol_ref(&mut self, node: NodeId, value: &Node) {
        let Ok(Some(symbol)) = symbol_ref_target_symbol_id(node, value) else {
            return;
        };
        self.symbol_ref_targets.insert(node, symbol);
        self.symbol_referrers
            .entry(symbol)
            .or_default()
            .insert(node);
    }

    fn remove_symbol_ref(&mut self, node: NodeId) {
        let Some(symbol) = self.symbol_ref_targets.remove(&node) else {
            return;
        };
        let Some(referrers) = self.symbol_referrers.get_mut(&symbol) else {
            return;
        };
        referrers.remove(&node);
        if referrers.is_empty() {
            self.symbol_referrers.remove(&symbol);
        }
    }
}

fn collect_index_changes(
    op: &GraphOp,
    ports: &mut BTreeSet<PortId>,
    edges: &mut BTreeSet<EdgeId>,
    bindings: &mut BTreeSet<BindingId>,
    symbol_ref_nodes: &mut BTreeSet<NodeId>,
) {
    match op {
        GraphOp::AddNode { id, .. } => {
            symbol_ref_nodes.insert(*id);
        }
        GraphOp::SetNodeData { id, .. } | GraphOp::SetNodeKind { id, .. } => {
            symbol_ref_nodes.insert(*id);
        }
        GraphOp::RemoveNode {
            id,
            ports: removed_ports,
            edges: removed_edges,
            bindings: removed_bindings,
            ..
        } => {
            symbol_ref_nodes.insert(*id);
            ports.extend(removed_ports.iter().map(|(id, _)| *id));
            edges.extend(removed_edges.iter().map(|(id, _)| *id));
            bindings.extend(removed_bindings.iter().map(|(id, _)| *id));
        }
        GraphOp::AddPort { id, .. } => {
            ports.insert(*id);
        }
        GraphOp::RemovePort {
            id,
            edges: removed_edges,
            bindings: removed_bindings,
            ..
        } => {
            ports.insert(*id);
            edges.extend(removed_edges.iter().map(|(id, _)| *id));
            bindings.extend(removed_bindings.iter().map(|(id, _)| *id));
        }
        GraphOp::AddEdge { id, .. } | GraphOp::SetEdgeEndpoints { id, .. } => {
            edges.insert(*id);
        }
        GraphOp::RemoveEdge {
            id,
            bindings: removed_bindings,
            ..
        } => {
            edges.insert(*id);
            bindings.extend(removed_bindings.iter().map(|(id, _)| *id));
        }
        GraphOp::RemoveGroup {
            bindings: removed_bindings,
            ..
        } => {
            bindings.extend(removed_bindings.iter().map(|(id, _)| *id));
        }
        GraphOp::RemoveStickyNote {
            bindings: removed_bindings,
            ..
        } => {
            bindings.extend(removed_bindings.iter().map(|(id, _)| *id));
        }
        GraphOp::AddBinding { id, .. }
        | GraphOp::RemoveBinding { id, .. }
        | GraphOp::SetBindingSubject { id, .. }
        | GraphOp::SetBindingTarget { id, .. } => {
            bindings.insert(*id);
        }
        _ => {}
    }
}

fn binding_targets(binding: &Binding) -> BTreeSet<GraphLocalBindingTarget> {
    [
        binding.subject.graph_local_target(),
        binding.target.graph_local_target(),
    ]
    .into_iter()
    .flatten()
    .collect()
}
