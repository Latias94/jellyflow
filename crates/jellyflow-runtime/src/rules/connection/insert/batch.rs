use crate::rules::{ConnectPlan, InsertNodeSpec};
use jellyflow_core::core::{Edge, EdgeId, Graph};
use jellyflow_core::ops::{EdgeEndpoints, GraphMutationBatchPlanner, GraphOp};

use super::super::common::reject_mutation_error;

pub(super) struct InsertNodeMutationBuilder<'a> {
    batch: GraphMutationBatchPlanner<'a>,
}

impl<'a> InsertNodeMutationBuilder<'a> {
    pub(super) fn new(graph: &'a Graph, inserted: InsertNodeSpec) -> Result<Self, ConnectPlan> {
        let mut batch = GraphMutationBatchPlanner::new(graph);
        let node_id = inserted.node_id;
        batch
            .add_node_with_ports(node_id, inserted.node, inserted.ports)
            .map_err(reject_mutation_error)?;
        Ok(Self { batch })
    }

    pub(super) fn add_edge(&mut self, id: EdgeId, edge: Edge) -> Result<(), ConnectPlan> {
        self.batch.add_edge(id, edge).map_err(reject_mutation_error)
    }

    pub(super) fn set_edge_endpoints(
        &mut self,
        id: EdgeId,
        to: EdgeEndpoints,
    ) -> Result<(), ConnectPlan> {
        self.batch
            .set_edge_endpoints(id, to)
            .map_err(reject_mutation_error)
    }

    pub(super) fn into_ops(self) -> Vec<GraphOp> {
        self.batch.into_ops()
    }
}
