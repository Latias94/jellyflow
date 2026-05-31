use crate::core::{Port, PortId};
use crate::ops::{GraphOp, GraphTransaction};

use super::GraphMutationPlanner;
use crate::ops::mutation::collect::{incident_edges_for_port, remove_edge_ops_for_port};
use crate::ops::mutation::{GraphMutationError, PortInsert};

impl GraphMutationPlanner<'_> {
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
}
