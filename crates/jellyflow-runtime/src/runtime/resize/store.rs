use crate::runtime::store::{DispatchError, DispatchOutcome, NodeGraphStore};

use super::planner::plan_node_resize_with_context;
use super::types::{NodeResizeContext, NodeResizePlan, NodeResizeRequest};

impl NodeGraphStore {
    /// Plans a node resize update against the store's current graph.
    pub fn plan_node_resize(&self, request: NodeResizeRequest) -> Option<NodeResizePlan> {
        let interaction = self.resolved_interaction_state();
        let node_origin = interaction.node_drag_interaction().node_origin.normalized();
        plan_node_resize_with_context(
            self.graph(),
            NodeResizeContext::new((node_origin.x, node_origin.y)),
            request,
        )
    }

    /// Commits a node resize update through the normal store dispatch path.
    pub fn apply_node_resize(
        &mut self,
        request: NodeResizeRequest,
    ) -> Result<Option<DispatchOutcome>, DispatchError> {
        let Some(plan) = self.plan_node_resize(request) else {
            return Ok(None);
        };
        self.dispatch_transaction(plan.transaction()).map(Some)
    }
}
