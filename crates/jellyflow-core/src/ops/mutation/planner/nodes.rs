use std::collections::BTreeSet;

use crate::core::{Node, NodeId, Port, PortId};
use crate::ops::{GraphOp, GraphTransaction};

use super::GraphMutationPlanner;
use crate::ops::mutation::GraphMutationError;
use crate::ops::mutation::collect::{incident_edges_for_ports, ports_for_node};

impl GraphMutationPlanner<'_> {
    pub fn add_node_with_ports_tx(
        &self,
        id: NodeId,
        node: Node,
        ports: impl IntoIterator<Item = (PortId, Port)>,
        label: impl Into<String>,
    ) -> Result<GraphTransaction, GraphMutationError> {
        Ok(GraphTransaction {
            label: Some(label.into()),
            ops: self.add_node_with_ports_ops(id, node, ports)?,
        })
    }

    pub fn add_node_with_ports_ops(
        &self,
        id: NodeId,
        mut node: Node,
        ports: impl IntoIterator<Item = (PortId, Port)>,
    ) -> Result<Vec<GraphOp>, GraphMutationError> {
        if self.graph.nodes.contains_key(&id) {
            return Err(GraphMutationError::NodeAlreadyExists(id));
        }
        if let Some(parent) = node.parent
            && !self.graph.groups.contains_key(&parent)
        {
            return Err(GraphMutationError::MissingGroup(parent));
        }

        let ports: Vec<(PortId, Port)> = ports.into_iter().collect();
        let mut seen = BTreeSet::new();
        let mut port_order = Vec::with_capacity(ports.len());
        for (port_id, port) in &ports {
            if self.graph.ports.contains_key(port_id) {
                return Err(GraphMutationError::PortAlreadyExists(*port_id));
            }
            if !seen.insert(*port_id) {
                return Err(GraphMutationError::DuplicateNodePort {
                    node: id,
                    port: *port_id,
                });
            }
            if port.node != id {
                return Err(GraphMutationError::PortOwnerMismatch {
                    port: *port_id,
                    expected: id,
                    got: port.node,
                });
            }
            port_order.push(*port_id);
        }

        node.ports = Vec::new();
        let mut ops = vec![GraphOp::AddNode { id, node }];
        for (port_id, port) in ports {
            ops.push(GraphOp::AddPort { id: port_id, port });
        }
        if !port_order.is_empty() {
            ops.push(GraphOp::SetNodePorts {
                id,
                from: Vec::new(),
                to: port_order,
            });
        }
        Ok(ops)
    }

    pub fn remove_node_op(&self, id: NodeId) -> Result<GraphOp, GraphMutationError> {
        let node = self
            .graph
            .nodes
            .get(&id)
            .cloned()
            .ok_or(GraphMutationError::MissingNode(id))?;

        let ports = ports_for_node(self.graph, id);
        let port_ids: BTreeSet<PortId> = ports.iter().map(|(port_id, _)| *port_id).collect();

        Ok(GraphOp::RemoveNode {
            id,
            node,
            ports,
            edges: incident_edges_for_ports(self.graph, &port_ids),
        })
    }

    pub fn remove_node_tx(
        &self,
        id: NodeId,
        label: impl Into<String>,
    ) -> Result<GraphTransaction, GraphMutationError> {
        Ok(GraphTransaction {
            label: Some(label.into()),
            ops: vec![self.remove_node_op(id)?],
        })
    }
}
