use crate::graph_kernel::GraphOp;
use jellyflow_core::core::{EdgeId, Graph, PortId};

use crate::rules::ConnectPlan;

/// Graph facts and authoritative removal snapshots required by connection planning.
pub(crate) trait ConnectionPlanningContext {
    fn incident_edge_ids(&mut self, graph: &Graph, port: PortId) -> Vec<EdgeId>;

    fn remove_edge_op(&mut self, graph: &Graph, edge: EdgeId) -> Result<GraphOp, ConnectPlan>;
}
