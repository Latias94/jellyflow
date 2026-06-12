use crate::core::{NodeId, Port, PortId};
use crate::ops::{GraphOp, GraphTransaction};

use super::GraphMutationPlanner;
use crate::ops::mutation::collect::{
    bindings_for_port_removal, incident_edges_for_port, remove_edge_ops_for_port,
};
use crate::ops::mutation::{GraphMutationError, PortInsert};

impl GraphMutationPlanner<'_> {
    pub fn add_port_tx(
        &self,
        id: PortId,
        port: Port,
        insert: PortInsert,
        label: impl Into<String>,
    ) -> Result<GraphTransaction, GraphMutationError> {
        Ok(GraphTransaction::new()
            .with_label(label)
            .with_ops(self.add_port_ops(id, port, insert)?))
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

        let order = NodePortOrderEdit::insert(node_id, &node.ports, id, insert)?;

        Ok(vec![
            GraphOp::AddPort { id, port },
            GraphOp::SetNodePorts {
                id: node_id,
                from: order.from,
                to: order.to,
            },
        ])
    }

    pub fn remove_port_op(&self, id: PortId) -> Result<GraphOp, GraphMutationError> {
        let port = self
            .graph
            .ports
            .get(&id)
            .cloned()
            .ok_or(GraphMutationError::MissingPort(id))?;

        let edges = incident_edges_for_port(self.graph, id);
        Ok(GraphOp::RemovePort {
            id,
            port,
            bindings: bindings_for_port_removal(self.graph, id, &edges),
            edges,
        })
    }

    pub fn remove_port_ops(&self, id: PortId) -> Result<Vec<GraphOp>, GraphMutationError> {
        let remove_op = self.remove_port_op(id)?;
        let mut ops = Vec::new();

        if let GraphOp::RemovePort { port, .. } = &remove_op
            && let Some(node) = self.graph.nodes.get(&port.node)
        {
            let order = NodePortOrderEdit::remove(&node.ports, id);
            if order.from != order.to {
                ops.push(GraphOp::SetNodePorts {
                    id: port.node,
                    from: order.from,
                    to: order.to,
                });
            }
        }

        ops.push(remove_op);
        Ok(ops)
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
        Ok(GraphTransaction::new()
            .with_label(label)
            .with_ops(self.remove_port_ops(id)?))
    }
}

struct NodePortOrderEdit {
    from: Vec<PortId>,
    to: Vec<PortId>,
}

impl NodePortOrderEdit {
    fn insert(
        node: NodeId,
        existing: &[PortId],
        inserted: PortId,
        insert: PortInsert,
    ) -> Result<Self, GraphMutationError> {
        let from = existing.to_vec();
        let mut to = from.clone();
        match insert {
            PortInsert::Append => to.push(inserted),
            PortInsert::At(index) => {
                if index > to.len() {
                    return Err(GraphMutationError::PortInsertOutOfBounds {
                        node,
                        index,
                        len: to.len(),
                    });
                }
                to.insert(index, inserted);
            }
        }

        Ok(Self { from, to })
    }

    fn remove(existing: &[PortId], removed: PortId) -> Self {
        let from = existing.to_vec();
        let mut to = from.clone();
        to.retain(|id| *id != removed);

        Self { from, to }
    }
}
