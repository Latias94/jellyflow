use jellyflow_core::core::Graph;
use jellyflow_core::ops::{GraphOp, GraphTransaction};

use super::types::{NODE_RESIZE_TRANSACTION_LABEL, NodeResizePlan, NodeResizeRequest};

/// Plans a node resize update without mutating the graph.
pub fn plan_node_resize(graph: &Graph, request: NodeResizeRequest) -> Option<NodeResizePlan> {
    let node = graph.nodes.get(&request.node)?;
    if node.hidden {
        return None;
    }

    let to = request.constraints.clamp(request.to)?;
    if node.size == Some(to) {
        return None;
    }

    let transaction = GraphTransaction::from_ops([GraphOp::SetNodeSize {
        id: request.node,
        from: node.size,
        to: Some(to),
    }])
    .with_label(NODE_RESIZE_TRANSACTION_LABEL);

    Some(NodeResizePlan::new(
        request.node,
        node.size,
        to,
        transaction,
    ))
}
