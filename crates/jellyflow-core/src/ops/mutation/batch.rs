use std::collections::{BTreeMap, BTreeSet};

use crate::core::{Edge, EdgeId, Graph, Node, NodeId, Port, PortId};
use crate::ops::{EdgeEndpoints, GraphOp, GraphTransaction};

use super::error::GraphMutationError;
use super::planner::GraphMutationPlanner;

/// Plans a small transaction while tracking ids created by earlier staged ops.
pub struct GraphMutationBatchPlanner<'a> {
    graph: &'a Graph,
    ops: Vec<GraphOp>,
    staged: StagedMutationIds,
}

#[derive(Default)]
struct StagedMutationIds {
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
            staged: StagedMutationIds::default(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    pub fn into_ops(self) -> Vec<GraphOp> {
        self.ops
    }

    pub fn into_transaction(self, label: impl Into<String>) -> GraphTransaction {
        GraphTransaction::from_ops(self.ops).with_label(label)
    }

    pub fn add_node_with_ports(
        &mut self,
        id: NodeId,
        node: Node,
        ports: impl IntoIterator<Item = (PortId, Port)>,
    ) -> Result<(), GraphMutationError> {
        if self.staged.contains_node(id) {
            return Err(GraphMutationError::NodeAlreadyExists(id));
        }

        let ports: Vec<(PortId, Port)> = ports.into_iter().collect();
        for (port_id, _) in &ports {
            if self.staged.contains_port(*port_id) {
                return Err(GraphMutationError::PortAlreadyExists(*port_id));
            }
        }

        let ops = GraphMutationPlanner::new(self.graph).add_node_with_ports_ops(
            id,
            node,
            ports.clone(),
        )?;

        self.staged.insert_node(id);
        for (port_id, _) in ports {
            self.staged.insert_port(port_id);
        }
        self.extend_ops(ops);
        Ok(())
    }

    pub fn add_edge(&mut self, id: EdgeId, edge: Edge) -> Result<(), GraphMutationError> {
        if self.graph.edges.contains_key(&id) || self.staged.contains_edge(id) {
            return Err(GraphMutationError::EdgeAlreadyExists(id));
        }
        self.require_known_port(edge.from)?;
        self.require_known_port(edge.to)?;

        self.staged.insert_edge(id, &edge);
        self.push_op(GraphOp::AddEdge { id, edge });
        Ok(())
    }

    pub fn set_edge_endpoints(
        &mut self,
        id: EdgeId,
        to: EdgeEndpoints,
    ) -> Result<(), GraphMutationError> {
        self.require_known_port(to.from)?;
        self.require_known_port(to.to)?;

        let from = if let Some(endpoints) = self.staged.edge_endpoints(id) {
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

        self.staged.set_edge_endpoints(id, to);
        self.push_op(GraphOp::SetEdgeEndpoints { id, from, to });
        Ok(())
    }

    fn push_op(&mut self, op: GraphOp) {
        self.ops.push(op);
    }

    fn extend_ops(&mut self, ops: impl IntoIterator<Item = GraphOp>) {
        self.ops.extend(ops);
    }

    fn require_known_port(&self, id: PortId) -> Result<(), GraphMutationError> {
        if self.graph.ports.contains_key(&id) || self.staged.contains_port(id) {
            Ok(())
        } else {
            Err(GraphMutationError::MissingPort(id))
        }
    }
}

impl StagedMutationIds {
    fn contains_node(&self, id: NodeId) -> bool {
        self.staged_nodes.contains(&id)
    }

    fn contains_port(&self, id: PortId) -> bool {
        self.staged_ports.contains(&id)
    }

    fn contains_edge(&self, id: EdgeId) -> bool {
        self.staged_edges.contains(&id)
    }

    fn insert_node(&mut self, id: NodeId) {
        self.staged_nodes.insert(id);
    }

    fn insert_port(&mut self, id: PortId) {
        self.staged_ports.insert(id);
    }

    fn insert_edge(&mut self, id: EdgeId, edge: &Edge) {
        self.staged_edges.insert(id);
        self.staged_edge_endpoints.insert(
            id,
            EdgeEndpoints {
                from: edge.from,
                to: edge.to,
            },
        );
    }

    fn edge_endpoints(&self, id: EdgeId) -> Option<EdgeEndpoints> {
        self.staged_edge_endpoints.get(&id).copied()
    }

    fn set_edge_endpoints(&mut self, id: EdgeId, to: EdgeEndpoints) {
        self.staged_edge_endpoints.insert(id, to);
    }
}
