use std::collections::BTreeSet;

use crate::core::{Binding, BindingId, Edge, EdgeId, Graph, Node, NodeId, Port, PortId};
use crate::ops::{GraphOp, GraphTransaction};

use super::GraphMutationPlanner;
use crate::ops::mutation::GraphMutationError;
use crate::ops::mutation::collect::{
    bindings_for_node_removal, incident_edges_for_ports, ports_for_node,
};

impl GraphMutationPlanner<'_> {
    pub fn add_node_with_ports_tx(
        &self,
        id: NodeId,
        node: Node,
        ports: impl IntoIterator<Item = (PortId, Port)>,
        label: impl Into<String>,
    ) -> Result<GraphTransaction, GraphMutationError> {
        Ok(GraphTransaction::new()
            .with_label(label)
            .with_ops(self.add_node_with_ports_ops(id, node, ports)?))
    }

    pub fn add_node_with_ports_ops(
        &self,
        id: NodeId,
        mut node: Node,
        ports: impl IntoIterator<Item = (PortId, Port)>,
    ) -> Result<Vec<GraphOp>, GraphMutationError> {
        if self.graph.nodes().contains_key(&id) {
            return Err(GraphMutationError::NodeAlreadyExists(id));
        }
        if let Some(parent) = node.parent
            && !self.graph.groups().contains_key(&parent)
        {
            return Err(GraphMutationError::MissingGroup(parent));
        }

        let NodePortsForInsert { ports, order } =
            NodePortsForInsert::collect(self.graph, id, ports)?;

        node.ports = Vec::new();
        let mut ops = vec![GraphOp::AddNode { id, node }];
        for (port_id, port) in ports {
            ops.push(GraphOp::AddPort { id: port_id, port });
        }
        if !order.is_empty() {
            ops.push(GraphOp::SetNodePorts {
                id,
                from: Vec::new(),
                to: order,
            });
        }
        Ok(ops)
    }

    pub fn remove_node_op(&self, id: NodeId) -> Result<GraphOp, GraphMutationError> {
        let snapshot = NodeRemovalSnapshot::capture(self.graph, id)?;

        Ok(GraphOp::RemoveNode {
            id,
            node: snapshot.node,
            ports: snapshot.ports,
            edges: snapshot.edges,
            bindings: snapshot.bindings,
        })
    }

    pub fn remove_node_tx(
        &self,
        id: NodeId,
        label: impl Into<String>,
    ) -> Result<GraphTransaction, GraphMutationError> {
        Ok(GraphTransaction::new()
            .with_label(label)
            .with_ops([self.remove_node_op(id)?]))
    }
}

struct NodePortsForInsert {
    ports: Vec<(PortId, Port)>,
    order: Vec<PortId>,
}

struct NodeRemovalSnapshot {
    node: Node,
    ports: Vec<(PortId, Port)>,
    edges: Vec<(EdgeId, Edge)>,
    bindings: Vec<(BindingId, Binding)>,
}

impl NodePortsForInsert {
    fn collect(
        graph: &Graph,
        node: NodeId,
        ports: impl IntoIterator<Item = (PortId, Port)>,
    ) -> Result<Self, GraphMutationError> {
        let mut seen = BTreeSet::new();
        let mut collected = Vec::new();
        let mut order = Vec::new();

        for (port_id, port) in ports {
            if graph.ports().contains_key(&port_id) {
                return Err(GraphMutationError::PortAlreadyExists(port_id));
            }
            if !seen.insert(port_id) {
                return Err(GraphMutationError::DuplicateNodePort {
                    node,
                    port: port_id,
                });
            }
            if port.node != node {
                return Err(GraphMutationError::PortOwnerMismatch {
                    port: port_id,
                    expected: node,
                    got: port.node,
                });
            }
            order.push(port_id);
            collected.push((port_id, port));
        }

        Ok(Self {
            ports: collected,
            order,
        })
    }
}

impl NodeRemovalSnapshot {
    fn capture(graph: &Graph, node_id: NodeId) -> Result<Self, GraphMutationError> {
        let node = graph
            .nodes
            .get(&node_id)
            .cloned()
            .ok_or(GraphMutationError::MissingNode(node_id))?;

        let ports = ports_for_node(graph, node_id);
        let port_ids = Self::port_ids(&ports);
        let edges = incident_edges_for_ports(graph, &port_ids);

        let bindings = bindings_for_node_removal(graph, node_id, &ports, &edges);

        Ok(Self {
            node,
            ports,
            edges,
            bindings,
        })
    }

    fn port_ids(ports: &[(PortId, Port)]) -> BTreeSet<PortId> {
        ports.iter().map(|(port_id, _)| *port_id).collect()
    }
}
