use std::collections::{BTreeMap, BTreeSet};

use crate::core::{Edge, EdgeId, Graph, Node, NodeId, Port, PortId};
use crate::ops::{EdgeEndpoints, GraphOp, GraphTransaction};

use super::error::GraphMutationError;
use super::planner::GraphMutationPlanner;

/// Plans a small transaction while tracking ids created by earlier staged ops.
pub struct GraphMutationBatchPlanner<'a> {
    graph: &'a Graph,
    ops: Vec<GraphOp>,
    staged_nodes: BTreeSet<NodeId>,
    staged_ports: BTreeSet<PortId>,
    staged_edges: BTreeSet<EdgeId>,
    staged_edge_endpoints: BTreeMap<EdgeId, EdgeEndpoints>,
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
