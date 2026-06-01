use std::collections::{BTreeMap, BTreeSet};

use crate::core::{CanvasSize, EdgeId, Graph, Node, NodeId, PortId};

use super::{GraphValidationError, GraphValidationReport};

/// Validates graph storage invariants required by the mutation layer.
///
/// This checks identity/reference integrity but intentionally leaves connection policy, duplicate
/// connection semantics, and port capacity to the fuller structural/profile validators.
pub fn validate_graph_storage(graph: &Graph) -> GraphValidationReport {
    StorageValidator::new(graph).finish()
}

struct StorageValidator<'a> {
    graph: &'a Graph,
    report: GraphValidationReport,
}

impl<'a> StorageValidator<'a> {
    fn new(graph: &'a Graph) -> Self {
        Self {
            graph,
            report: GraphValidationReport::default(),
        }
    }

    fn finish(mut self) -> GraphValidationReport {
        if !self.validate_graph_version() {
            return self.report;
        }

        self.validate_ports_reference_nodes();
        self.validate_ports_are_listed_by_owner();
        self.validate_nodes();
        self.validate_edges_reference_ports();
        self.report
    }

    fn validate_graph_version(&mut self) -> bool {
        if self.graph.graph_version == crate::core::model::GRAPH_VERSION {
            return true;
        }

        self.report
            .push(GraphValidationError::UnsupportedGraphVersion {
                expected: crate::core::model::GRAPH_VERSION,
                found: self.graph.graph_version,
            });
        false
    }

    fn validate_ports_reference_nodes(&mut self) {
        for (port_id, port) in &self.graph.ports {
            if !self.graph.nodes.contains_key(&port.node) {
                self.report.push(GraphValidationError::PortMissingNode {
                    port: *port_id,
                    node: port.node,
                });
            }
        }
    }

    fn validate_ports_are_listed_by_owner(&mut self) {
        let listed_ports_by_node = listed_ports_by_node(self.graph);
        for (port_id, port) in &self.graph.ports {
            if !self.graph.nodes.contains_key(&port.node) {
                continue;
            }
            if !node_lists_port(&listed_ports_by_node, port.node, *port_id) {
                self.report
                    .push(GraphValidationError::PortMissingFromOwner {
                        port: *port_id,
                        node: port.node,
                    });
            }
        }
    }

    fn validate_nodes(&mut self) {
        for (node_id, node) in &self.graph.nodes {
            self.validate_node(*node_id, node);
        }
    }

    fn validate_node(&mut self, node_id: NodeId, node: &Node) {
        self.validate_node_parent(node_id, node);
        self.validate_node_size(node_id, node.size);
        self.validate_node_port_list(node_id, node);
    }

    fn validate_node_parent(&mut self, node_id: NodeId, node: &Node) {
        if let Some(group) = node.parent
            && !self.graph.groups.contains_key(&group)
        {
            self.report
                .push(GraphValidationError::NodeParentMissingGroup {
                    node: node_id,
                    group,
                });
        }
    }

    fn validate_node_size(&mut self, node_id: NodeId, size: Option<CanvasSize>) {
        if let Some(size) = size
            && !size.is_positive_finite()
        {
            self.report.push(GraphValidationError::NodeInvalidSize {
                node: node_id,
                width: size.width,
                height: size.height,
            });
        }
    }

    fn validate_node_port_list(&mut self, node_id: NodeId, node: &Node) {
        let mut seen: BTreeSet<PortId> = BTreeSet::new();
        for port_id in &node.ports {
            if !seen.insert(*port_id) {
                self.report.push(GraphValidationError::NodePortsDuplicate {
                    node: node_id,
                    port: *port_id,
                });
                continue;
            }
            let Some(port) = self.graph.ports.get(port_id) else {
                self.report
                    .push(GraphValidationError::NodePortsMissingPort {
                        node: node_id,
                        port: *port_id,
                    });
                continue;
            };
            if port.node != node_id {
                self.report.push(GraphValidationError::NodePortsWrongOwner {
                    node: node_id,
                    port: *port_id,
                    owner: port.node,
                });
            }
        }
    }

    fn validate_edges_reference_ports(&mut self) {
        for (edge_id, edge) in &self.graph.edges {
            self.validate_edge_endpoint(*edge_id, edge.from);
            self.validate_edge_endpoint(*edge_id, edge.to);
        }
    }

    fn validate_edge_endpoint(&mut self, edge_id: EdgeId, port_id: PortId) {
        if !self.graph.ports.contains_key(&port_id) {
            self.report.push(GraphValidationError::EdgeMissingPort {
                edge: edge_id,
                port: port_id,
            });
        }
    }
}

fn node_lists_port(
    listed_ports_by_node: &BTreeMap<NodeId, BTreeSet<PortId>>,
    node_id: NodeId,
    port_id: PortId,
) -> bool {
    listed_ports_by_node
        .get(&node_id)
        .is_some_and(|ports| ports.contains(&port_id))
}

fn listed_ports_by_node(graph: &Graph) -> BTreeMap<NodeId, BTreeSet<PortId>> {
    graph
        .nodes
        .iter()
        .map(|(node_id, node)| (*node_id, node.ports.iter().copied().collect()))
        .collect()
}
