use crate::runtime::store::{DispatchError, DispatchOutcome, NodeGraphStore};

use super::planner::{plan_node_pointer_resize_with_policy_extent, plan_node_resize_with_context};
use super::types::{
    NodePointerResizeRequest, NodeResizeContext, NodeResizePlan, NodeResizeRequest,
};

impl NodeGraphStore {
    /// Plans a node resize update against the store's current graph.
    pub fn plan_node_resize(&self, request: NodeResizeRequest) -> Option<NodeResizePlan> {
        plan_node_resize_with_context(self.graph(), self.resize_context(), request)
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

    /// Plans a pointer-driven node resize update against the store's current graph.
    pub fn plan_node_pointer_resize(
        &self,
        request: NodePointerResizeRequest,
    ) -> Option<NodeResizePlan> {
        let interaction = self.resolved_interaction_state();
        let node_drag = interaction.node_drag_interaction();
        let node_origin = node_drag.node_origin.normalized();
        plan_node_pointer_resize_with_policy_extent(
            self.graph(),
            NodeResizeContext::new((node_origin.x, node_origin.y)),
            node_drag.node_extent,
            request,
        )
    }

    /// Commits a pointer-driven node resize update through the normal store dispatch path.
    pub fn apply_node_pointer_resize(
        &mut self,
        request: NodePointerResizeRequest,
    ) -> Result<Option<DispatchOutcome>, DispatchError> {
        let Some(plan) = self.plan_node_pointer_resize(request) else {
            return Ok(None);
        };
        self.dispatch_transaction(plan.transaction()).map(Some)
    }

    fn resize_context(&self) -> NodeResizeContext {
        let interaction = self.resolved_interaction_state();
        let node_drag = interaction.node_drag_interaction();
        let node_origin = node_drag.node_origin.normalized();
        NodeResizeContext::new((node_origin.x, node_origin.y))
    }
}
