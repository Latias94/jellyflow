use std::collections::{BTreeMap, BTreeSet};

use crate::core::{Edge, EdgeId, Graph, GroupId, Node, NodeId, Port, PortId};
use crate::ops::{EdgeEndpoints, GraphOp, GraphTransaction};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortInsert {
    Append,
    At(usize),
}

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum GraphMutationError {
    #[error("node already exists: {0:?}")]
    NodeAlreadyExists(NodeId),
    #[error("missing node: {0:?}")]
    MissingNode(NodeId),
    #[error("port already exists: {0:?}")]
    PortAlreadyExists(PortId),
    #[error("missing port: {0:?}")]
    MissingPort(PortId),
    #[error("edge already exists: {0:?}")]
    EdgeAlreadyExists(EdgeId),
    #[error("missing edge: {0:?}")]
    MissingEdge(EdgeId),
    #[error("missing group: {0:?}")]
    MissingGroup(GroupId),
    #[error("port owner mismatch: port={port:?} expected={expected:?} got={got:?}")]
    PortOwnerMismatch {
        port: PortId,
        expected: NodeId,
        got: NodeId,
    },
    #[error("duplicate port in node planning: node={node:?} port={port:?}")]
    DuplicateNodePort { node: NodeId, port: PortId },
    #[error("port insert index out of bounds: node={node:?} index={index} len={len}")]
    PortInsertOutOfBounds {
        node: NodeId,
        index: usize,
        len: usize,
    },
}

/// Plans graph mutations while preserving v1 `Graph` storage invariants.
pub struct GraphMutationPlanner<'a> {
    graph: &'a Graph,
}

/// Plans a small transaction while tracking ids created by earlier staged ops.
pub struct GraphMutationBatchPlanner<'a> {
    graph: &'a Graph,
    ops: Vec<GraphOp>,
    staged_nodes: BTreeSet<NodeId>,
    staged_ports: BTreeSet<PortId>,
    staged_edges: BTreeSet<EdgeId>,
    staged_edge_endpoints: BTreeMap<EdgeId, EdgeEndpoints>,
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

        let mut edges: Vec<(EdgeId, Edge)> = self
            .graph
            .edges
            .iter()
            .filter_map(|(edge_id, edge)| {
                if edge.from == id || edge.to == id {
                    Some((*edge_id, edge.clone()))
                } else {
                    None
                }
            })
            .collect();
        edges.sort_by_key(|(edge_id, _)| *edge_id);

        Ok(GraphOp::RemovePort { id, port, edges })
    }

    pub fn disconnect_port_ops(&self, id: PortId) -> Result<Vec<GraphOp>, GraphMutationError> {
        self.graph
            .ports
            .get(&id)
            .ok_or(GraphMutationError::MissingPort(id))?;

        let mut ops: Vec<GraphOp> = self
            .graph
            .edges
            .iter()
            .filter_map(|(edge_id, edge)| {
                if edge.from == id || edge.to == id {
                    Some(GraphOp::RemoveEdge {
                        id: *edge_id,
                        edge: edge.clone(),
                    })
                } else {
                    None
                }
            })
            .collect();

        ops.sort_by_key(|op| match op {
            GraphOp::RemoveEdge { id, .. } => *id,
            _ => unreachable!(),
        });

        Ok(ops)
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

        let mut ports: Vec<(PortId, Port)> = self
            .graph
            .ports
            .iter()
            .filter_map(|(port_id, port)| {
                if port.node == id {
                    Some((*port_id, port.clone()))
                } else {
                    None
                }
            })
            .collect();
        ports.sort_by_key(|(port_id, _)| *port_id);

        let port_ids: BTreeSet<PortId> = ports.iter().map(|(port_id, _)| *port_id).collect();

        let mut edges: Vec<(EdgeId, Edge)> = self
            .graph
            .edges
            .iter()
            .filter_map(|(edge_id, edge)| {
                if port_ids.contains(&edge.from) || port_ids.contains(&edge.to) {
                    Some((*edge_id, edge.clone()))
                } else {
                    None
                }
            })
            .collect();
        edges.sort_by_key(|(edge_id, _)| *edge_id);

        Ok(GraphOp::RemoveNode {
            id,
            node,
            ports,
            edges,
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

        let mut detached: Vec<(NodeId, Option<GroupId>)> = self
            .graph
            .nodes
            .iter()
            .filter_map(|(node_id, node)| {
                (node.parent == Some(id)).then_some((*node_id, node.parent))
            })
            .collect();
        detached.sort_by_key(|(node_id, _)| *node_id);

        Ok(GraphOp::RemoveGroup {
            id,
            group,
            detached,
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

impl<'a> GraphMutationBatchPlanner<'a> {
    pub fn new(graph: &'a Graph) -> Self {
        Self {
            graph,
            ops: Vec::new(),
            staged_nodes: BTreeSet::new(),
            staged_ports: BTreeSet::new(),
            staged_edges: BTreeSet::new(),
            staged_edge_endpoints: BTreeMap::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    pub fn into_ops(self) -> Vec<GraphOp> {
        self.ops
    }

    pub fn into_transaction(self, label: impl Into<String>) -> GraphTransaction {
        GraphTransaction {
            label: Some(label.into()),
            ops: self.ops,
        }
    }

    pub fn add_node_with_ports(
        &mut self,
        id: NodeId,
        node: Node,
        ports: impl IntoIterator<Item = (PortId, Port)>,
    ) -> Result<(), GraphMutationError> {
        if self.staged_nodes.contains(&id) {
            return Err(GraphMutationError::NodeAlreadyExists(id));
        }

        let ports: Vec<(PortId, Port)> = ports.into_iter().collect();
        for (port_id, _) in &ports {
            if self.staged_ports.contains(port_id) {
                return Err(GraphMutationError::PortAlreadyExists(*port_id));
            }
        }

        let ops = GraphMutationPlanner::new(self.graph).add_node_with_ports_ops(
            id,
            node,
            ports.clone(),
        )?;

        self.staged_nodes.insert(id);
        for (port_id, _) in ports {
            self.staged_ports.insert(port_id);
        }
        self.ops.extend(ops);
        Ok(())
    }

    pub fn add_edge(&mut self, id: EdgeId, edge: Edge) -> Result<(), GraphMutationError> {
        if self.graph.edges.contains_key(&id) || self.staged_edges.contains(&id) {
            return Err(GraphMutationError::EdgeAlreadyExists(id));
        }
        self.require_known_port(edge.from)?;
        self.require_known_port(edge.to)?;

        self.staged_edges.insert(id);
        self.staged_edge_endpoints.insert(
            id,
            EdgeEndpoints {
                from: edge.from,
                to: edge.to,
            },
        );
        self.ops.push(GraphOp::AddEdge { id, edge });
        Ok(())
    }

    pub fn set_edge_endpoints(
        &mut self,
        id: EdgeId,
        to: EdgeEndpoints,
    ) -> Result<(), GraphMutationError> {
        self.require_known_port(to.from)?;
        self.require_known_port(to.to)?;

        let from = if let Some(endpoints) = self.staged_edge_endpoints.get(&id).copied() {
            endpoints
        } else {
            let edge = self
                .graph
                .edges
                .get(&id)
                .ok_or(GraphMutationError::MissingEdge(id))?;
            EdgeEndpoints {
                from: edge.from,
                to: edge.to,
            }
        };

        self.staged_edge_endpoints.insert(id, to);
        self.ops.push(GraphOp::SetEdgeEndpoints { id, from, to });
        Ok(())
    }

    fn require_known_port(&self, id: PortId) -> Result<(), GraphMutationError> {
        if self.graph.ports.contains_key(&id) || self.staged_ports.contains(&id) {
            Ok(())
        } else {
            Err(GraphMutationError::MissingPort(id))
        }
    }
}
