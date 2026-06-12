use std::collections::{BTreeMap, BTreeSet};

use crate::core::{
    EdgeId, EdgeKind, Graph, Node, NodeId, PortCapacity, PortId, PortKind,
    subgraph_target_graph_id, symbol_ref_target_symbol_id,
};

use super::{GraphValidationError, GraphValidationReport, validate_graph_storage};

/// Validates a graph for structural consistency (contract-level invariants).
///
/// This intentionally does **not** enforce editor policies such as connection direction.
/// Direction, cycle policy, and domain-specific semantics belong in profiles/rules.
pub fn validate_graph_structural(graph: &Graph) -> GraphValidationReport {
    StructuralValidator::new(graph).finish()
}

struct StructuralValidator<'a> {
    graph: &'a Graph,
    report: GraphValidationReport,
}

impl<'a> StructuralValidator<'a> {
    fn new(graph: &'a Graph) -> Self {
        Self {
            graph,
            report: validate_graph_storage(graph),
        }
    }

    fn finish(mut self) -> GraphValidationReport {
        if self.report.has_unsupported_graph_version() {
            return self.report;
        }

        self.validate_node_bindings();
        self.validate_binding_relationships();
        let incident_counts = self.validate_edges();
        self.validate_port_capacities(incident_counts);
        self.report
    }

    fn validate_node_bindings(&mut self) {
        for (node_id, node) in &self.graph.nodes {
            self.validate_subgraph_binding(*node_id, node);
            self.validate_symbol_ref_binding(*node_id, node);
        }
    }

    fn validate_subgraph_binding(&mut self, node_id: NodeId, node: &Node) {
        match subgraph_target_graph_id(node_id, node) {
            Ok(Some(target)) => {
                if !self.graph.imports.contains_key(&target) {
                    self.report
                        .push(GraphValidationError::SubgraphTargetNotImported {
                            node: node_id,
                            graph_id: target,
                        });
                }
            }
            Ok(None) => {}
            Err(err) => self.report.push(err.into()),
        }
    }

    fn validate_symbol_ref_binding(&mut self, node_id: NodeId, node: &Node) {
        match symbol_ref_target_symbol_id(node_id, node) {
            Ok(Some(target)) => {
                if !self.graph.symbols.contains_key(&target) {
                    self.report
                        .push(GraphValidationError::SymbolRefTargetNotDeclared {
                            node: node_id,
                            symbol_id: target,
                        });
                }
            }
            Ok(None) => {}
            Err(err) => self.report.push(err.into()),
        }
    }

    fn validate_binding_relationships(&mut self) {
        for (binding_id, binding) in &self.graph.bindings {
            for target in [
                binding.subject.graph_local_target(),
                binding.target.graph_local_target(),
            ]
            .into_iter()
            .flatten()
            {
                if self.binding_target_exists(target) {
                    continue;
                }
                self.report
                    .push(GraphValidationError::BindingTargetMissing {
                        binding: *binding_id,
                        target,
                    });
            }
        }
    }

    fn binding_target_exists(&self, target: crate::core::GraphLocalBindingTarget) -> bool {
        match target {
            crate::core::GraphLocalBindingTarget::Graph => true,
            crate::core::GraphLocalBindingTarget::Node { id } => self.graph.nodes.contains_key(&id),
            crate::core::GraphLocalBindingTarget::Port { id } => self.graph.ports.contains_key(&id),
            crate::core::GraphLocalBindingTarget::Edge { id } => self.graph.edges.contains_key(&id),
            crate::core::GraphLocalBindingTarget::Group { id } => {
                self.graph.groups.contains_key(&id)
            }
            crate::core::GraphLocalBindingTarget::StickyNote { id } => {
                self.graph.sticky_notes.contains_key(&id)
            }
        }
    }

    fn validate_edges(&mut self) -> BTreeMap<PortId, usize> {
        let mut edges = EdgeValidationAccumulator::default();

        for (edge_id, edge) in &self.graph.edges {
            let Some(from) = self.graph.ports.get(&edge.from) else {
                continue;
            };
            let Some(to) = self.graph.ports.get(&edge.to) else {
                continue;
            };

            self.validate_edge_kind(*edge_id, from.kind, to.kind, edge.kind);

            if edges.record(from.kind, edge.from, edge.to) {
                self.report
                    .push(GraphValidationError::DuplicateEdge { edge: *edge_id });
            }
        }

        edges.into_incident_counts()
    }

    fn validate_edge_kind(
        &mut self,
        edge_id: EdgeId,
        from_kind: PortKind,
        to_kind: PortKind,
        edge_kind: EdgeKind,
    ) {
        if from_kind != to_kind {
            self.report.push(GraphValidationError::EdgeKindMismatch {
                edge: edge_id,
                from_kind,
                to_kind,
            });
            return;
        }

        let expected = from_kind.edge_kind();
        if edge_kind != expected {
            self.report
                .push(GraphValidationError::EdgeKindPortKindMismatch {
                    edge: edge_id,
                    edge_kind,
                    port_kind: from_kind,
                });
        }
    }

    fn validate_port_capacities(&mut self, incident_counts: BTreeMap<PortId, usize>) {
        for (port_id, count) in incident_counts {
            let Some(port) = self.graph.ports.get(&port_id) else {
                continue;
            };
            if port.capacity == PortCapacity::Single && count > 1 {
                self.report
                    .push(GraphValidationError::PortCapacityExceeded {
                        port: port_id,
                        capacity: port.capacity,
                        count,
                    });
            }
        }
    }
}

#[derive(Default)]
struct EdgeValidationAccumulator {
    edge_pairs: BTreeSet<(PortKind, PortId, PortId)>,
    incident_counts: BTreeMap<PortId, usize>,
}

impl EdgeValidationAccumulator {
    fn record(&mut self, port_kind: PortKind, from: PortId, to: PortId) -> bool {
        let is_duplicate = !self.edge_pairs.insert((port_kind, from, to));
        *self.incident_counts.entry(from).or_insert(0) += 1;
        *self.incident_counts.entry(to).or_insert(0) += 1;
        is_duplicate
    }

    fn into_incident_counts(self) -> BTreeMap<PortId, usize> {
        self.incident_counts
    }
}
