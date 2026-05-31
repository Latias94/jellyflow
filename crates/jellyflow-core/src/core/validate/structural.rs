use std::collections::{BTreeMap, BTreeSet};

use crate::core::{
    EdgeKind, Graph, PortCapacity, PortId, PortKind, SubgraphNodeError, SymbolRefNodeError,
    subgraph_target_graph_id, symbol_ref_target_symbol_id,
};

use super::{GraphValidationError, GraphValidationReport, validate_graph_storage};

/// Validates a graph for structural consistency (contract-level invariants).
///
/// This intentionally does **not** enforce editor policies such as connection direction.
/// Direction, cycle policy, and domain-specific semantics belong in profiles/rules.
pub fn validate_graph_structural(graph: &Graph) -> GraphValidationReport {
    let mut report = validate_graph_storage(graph);
    if report
        .errors
        .iter()
        .any(|error| matches!(error, GraphValidationError::UnsupportedGraphVersion { .. }))
    {
        return report;
    }

    for (node_id, node) in &graph.nodes {
        match subgraph_target_graph_id(*node_id, node) {
            Ok(Some(target)) => {
                if !graph.imports.contains_key(&target) {
                    report
                        .errors
                        .push(GraphValidationError::SubgraphTargetNotImported {
                            node: *node_id,
                            graph_id: target,
                        });
                }
            }
            Ok(None) => {}
            Err(err) => match err {
                SubgraphNodeError::MissingGraphId { node } => {
                    report
                        .errors
                        .push(GraphValidationError::SubgraphNodeMissingGraphId { node });
                }
                SubgraphNodeError::GraphIdNotString { node } => {
                    report
                        .errors
                        .push(GraphValidationError::SubgraphNodeGraphIdNotString { node });
                }
                SubgraphNodeError::InvalidGraphId { node, value } => {
                    report
                        .errors
                        .push(GraphValidationError::SubgraphNodeInvalidGraphId { node, value });
                }
            },
        }

        match symbol_ref_target_symbol_id(*node_id, node) {
            Ok(Some(target)) => {
                if !graph.symbols.contains_key(&target) {
                    report
                        .errors
                        .push(GraphValidationError::SymbolRefTargetNotDeclared {
                            node: *node_id,
                            symbol_id: target,
                        });
                }
            }
            Ok(None) => {}
            Err(err) => match err {
                SymbolRefNodeError::MissingSymbolId { node } => {
                    report
                        .errors
                        .push(GraphValidationError::SymbolRefNodeMissingSymbolId { node });
                }
                SymbolRefNodeError::SymbolIdNotString { node } => {
                    report
                        .errors
                        .push(GraphValidationError::SymbolRefNodeSymbolIdNotString { node });
                }
                SymbolRefNodeError::InvalidSymbolId { node, value } => {
                    report
                        .errors
                        .push(GraphValidationError::SymbolRefNodeInvalidSymbolId { node, value });
                }
            },
        }
    }

    let mut edge_pairs: BTreeSet<(PortKind, PortId, PortId)> = BTreeSet::new();
    let mut incident_counts: BTreeMap<PortId, usize> = BTreeMap::new();

    for (edge_id, edge) in &graph.edges {
        let Some(from) = graph.ports.get(&edge.from) else {
            continue;
        };
        let Some(to) = graph.ports.get(&edge.to) else {
            continue;
        };

        if from.kind != to.kind {
            report.errors.push(GraphValidationError::EdgeKindMismatch {
                edge: *edge_id,
                from_kind: from.kind,
                to_kind: to.kind,
            });
        } else {
            let expected = match from.kind {
                PortKind::Data => EdgeKind::Data,
                PortKind::Exec => EdgeKind::Exec,
            };
            if edge.kind != expected {
                report
                    .errors
                    .push(GraphValidationError::EdgeKindPortKindMismatch {
                        edge: *edge_id,
                        edge_kind: edge.kind,
                        port_kind: from.kind,
                    });
            }
        }

        if !edge_pairs.insert((from.kind, edge.from, edge.to)) {
            report
                .errors
                .push(GraphValidationError::DuplicateEdge { edge: *edge_id });
        }

        *incident_counts.entry(edge.from).or_insert(0) += 1;
        *incident_counts.entry(edge.to).or_insert(0) += 1;
    }

    for (port_id, count) in incident_counts {
        let Some(port) = graph.ports.get(&port_id) else {
            continue;
        };
        if port.capacity == PortCapacity::Single && count > 1 {
            report
                .errors
                .push(GraphValidationError::PortCapacityExceeded {
                    port: port_id,
                    capacity: port.capacity,
                    count,
                });
        }
    }

    report
}
