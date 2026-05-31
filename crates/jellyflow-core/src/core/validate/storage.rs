use std::collections::BTreeSet;

use crate::core::{Graph, PortId};

use super::{GraphValidationError, GraphValidationReport};

/// Validates graph storage invariants required by the mutation layer.
///
/// This checks identity/reference integrity but intentionally leaves connection policy, duplicate
/// connection semantics, and port capacity to the fuller structural/profile validators.
pub fn validate_graph_storage(graph: &Graph) -> GraphValidationReport {
    let mut report = GraphValidationReport::default();

    if graph.graph_version != crate::core::model::GRAPH_VERSION {
        report
            .errors
            .push(GraphValidationError::UnsupportedGraphVersion {
                expected: crate::core::model::GRAPH_VERSION,
                found: graph.graph_version,
            });
        return report;
    }

    for (port_id, port) in &graph.ports {
        if !graph.nodes.contains_key(&port.node) {
            report.errors.push(GraphValidationError::PortMissingNode {
                port: *port_id,
                node: port.node,
            });
        }
    }

    for (port_id, port) in &graph.ports {
        let Some(node) = graph.nodes.get(&port.node) else {
            continue;
        };
        if !node.ports.contains(port_id) {
            report
                .errors
                .push(GraphValidationError::PortMissingFromOwner {
                    port: *port_id,
                    node: port.node,
                });
        }
    }

    for (node_id, node) in &graph.nodes {
        if let Some(group) = node.parent
            && !graph.groups.contains_key(&group)
        {
            report
                .errors
                .push(GraphValidationError::NodeParentMissingGroup {
                    node: *node_id,
                    group,
                });
        }

        if let Some(size) = node.size
            && (!size.width.is_finite()
                || !size.height.is_finite()
                || size.width <= 0.0
                || size.height <= 0.0)
        {
            report.errors.push(GraphValidationError::NodeInvalidSize {
                node: *node_id,
                width: size.width,
                height: size.height,
            });
        }

        let mut seen: BTreeSet<PortId> = BTreeSet::new();
        for port_id in &node.ports {
            if !seen.insert(*port_id) {
                report
                    .errors
                    .push(GraphValidationError::NodePortsDuplicate {
                        node: *node_id,
                        port: *port_id,
                    });
                continue;
            }
            let Some(port) = graph.ports.get(port_id) else {
                report
                    .errors
                    .push(GraphValidationError::NodePortsMissingPort {
                        node: *node_id,
                        port: *port_id,
                    });
                continue;
            };
            if port.node != *node_id {
                report
                    .errors
                    .push(GraphValidationError::NodePortsWrongOwner {
                        node: *node_id,
                        port: *port_id,
                        owner: port.node,
                    });
            }
        }
    }

    for (edge_id, edge) in &graph.edges {
        if !graph.ports.contains_key(&edge.from) {
            report.errors.push(GraphValidationError::EdgeMissingPort {
                edge: *edge_id,
                port: edge.from,
            });
        }
        if !graph.ports.contains_key(&edge.to) {
            report.errors.push(GraphValidationError::EdgeMissingPort {
                edge: *edge_id,
                port: edge.to,
            });
        }
    }

    report
}
