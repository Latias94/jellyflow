use std::collections::BTreeSet;

use crate::core::{Edge, EdgeId, Graph, GroupId, Node, NodeId, Port, PortId};
use crate::ops::{GraphOp, GraphTransaction};

use super::collect::{
    detached_nodes_for_group, incident_edges_for_port, incident_edges_for_ports, ports_for_node,
    remove_edge_ops_for_port,
};
use super::error::GraphMutationError;
use super::types::PortInsert;

/// Plans graph mutations while preserving v1 `Graph` storage invariants.
pub struct GraphMutationPlanner<'a> {
    graph: &'a Graph,
}

impl<'a> GraphMutationPlanner<'a> {
    pub fn new(graph: &'a Graph) -> Self {
        Self { graph }
    }

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

    pub fn add_port_tx(
        &self,
        id: PortId,
        port: Port,
        insert: PortInsert,
        label: impl Into<String>,
    ) -> Result<GraphTransaction, GraphMutationError> {
        Ok(GraphTransaction {
            label: Some(label.into()),
            ops: self.add_port_ops(id, port, insert)?,
        })
    }

    pub fn add_port_ops(
        &self,
        id: PortId,
        port: Port,
        insert: PortInsert,
    ) -> Result<Vec<GraphOp>, GraphMutationError> {
        let node_id = port.node;
        if self.graph.ports.contains_key(&id) {
            return Err(GraphMutationError::PortAlreadyExists(id));
        }
        let node = self
            .graph
            .nodes
            .get(&node_id)
            .ok_or(GraphMutationError::MissingNode(node_id))?;
        if node.ports.contains(&id) {
            return Err(GraphMutationError::DuplicateNodePort {
                node: node_id,
                port: id,
            });
        }

        let from = node.ports.clone();
        let mut to = from.clone();
        match insert {
            PortInsert::Append => to.push(id),
            PortInsert::At(index) => {
                if index > to.len() {
                    return Err(GraphMutationError::PortInsertOutOfBounds {
                        node: node_id,
                        index,
                        len: to.len(),
                    });
                }
                to.insert(index, id);
            }
        }

        Ok(vec![
            GraphOp::AddPort { id, port },
            GraphOp::SetNodePorts {
                id: node_id,
                from,
                to,
            },
        ])
    }

    pub fn add_edge_tx(
        &self,
        id: EdgeId,
        edge: Edge,
        label: impl Into<String>,
    ) -> Result<GraphTransaction, GraphMutationError> {
        Ok(GraphTransaction {
            label: Some(label.into()),
            ops: vec![self.add_edge_op(id, edge)?],
        })
    }

    pub fn add_edge_op(&self, id: EdgeId, edge: Edge) -> Result<GraphOp, GraphMutationError> {
        if self.graph.edges.contains_key(&id) {
            return Err(GraphMutationError::EdgeAlreadyExists(id));
        }
        if !self.graph.ports.contains_key(&edge.from) {
            return Err(GraphMutationError::MissingPort(edge.from));
        }
        if !self.graph.ports.contains_key(&edge.to) {
            return Err(GraphMutationError::MissingPort(edge.to));
        }
        Ok(GraphOp::AddEdge { id, edge })
    }

    pub fn remove_edge_op(&self, id: EdgeId) -> Result<GraphOp, GraphMutationError> {
        let edge = self
            .graph
            .edges
            .get(&id)
            .cloned()
            .ok_or(GraphMutationError::MissingEdge(id))?;
        Ok(GraphOp::RemoveEdge { id, edge })
    }

    pub fn remove_edge_tx(
        &self,
        id: EdgeId,
        label: impl Into<String>,
    ) -> Result<GraphTransaction, GraphMutationError> {
        Ok(GraphTransaction {
            label: Some(label.into()),
            ops: vec![self.remove_edge_op(id)?],
        })
    }

    pub fn remove_port_op(&self, id: PortId) -> Result<GraphOp, GraphMutationError> {
        let port = self
            .graph
            .ports
            .get(&id)
            .cloned()
            .ok_or(GraphMutationError::MissingPort(id))?;

        Ok(GraphOp::RemovePort {
            id,
            port,
            edges: incident_edges_for_port(self.graph, id),
        })
    }

    pub fn disconnect_port_ops(&self, id: PortId) -> Result<Vec<GraphOp>, GraphMutationError> {
        self.graph
            .ports
            .get(&id)
            .ok_or(GraphMutationError::MissingPort(id))?;

        Ok(remove_edge_ops_for_port(self.graph, id))
    }

    pub fn remove_port_tx(
        &self,
        id: PortId,
        label: impl Into<String>,
    ) -> Result<GraphTransaction, GraphMutationError> {
        Ok(GraphTransaction {
            label: Some(label.into()),
            ops: vec![self.remove_port_op(id)?],
        })
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

    pub fn remove_group_op(&self, id: GroupId) -> Result<GraphOp, GraphMutationError> {
        let group = self
            .graph
            .groups
            .get(&id)
            .cloned()
            .ok_or(GraphMutationError::MissingGroup(id))?;

        Ok(GraphOp::RemoveGroup {
            id,
            group,
            detached: detached_nodes_for_group(self.graph, id),
        })
    }

    pub fn remove_group_tx(
        &self,
        id: GroupId,
        label: impl Into<String>,
    ) -> Result<GraphTransaction, GraphMutationError> {
        Ok(GraphTransaction {
            label: Some(label.into()),
            ops: vec![self.remove_group_op(id)?],
        })
    }
}
