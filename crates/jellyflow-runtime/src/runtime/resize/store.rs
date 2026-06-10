use crate::runtime::events::{
    NodeGraphGestureEvent, NodeResizeEnd, NodeResizeEndOutcome, NodeResizeStart, NodeResizeUpdate,
};
use crate::runtime::store::{DispatchError, DispatchOutcome, NodeGraphStore};

use super::planner::{plan_node_pointer_resize_with_policy_extent, plan_node_resize_with_context};
use super::session::{NodeResizeSession, NodeResizeSessionUpdateRequest};
use super::types::{
    NodePointerResizeRequest, NodeResizeContext, NodeResizePlan, NodeResizeRequest,
};

#[derive(Debug, Clone)]
pub struct NodeResizeSessionUpdateOutcome {
    pub update: NodeResizeUpdate,
    pub dispatch: DispatchOutcome,
}

#[derive(Debug, Clone)]
pub struct NodeResizeSessionOutcome {
    pub start: NodeResizeStart,
    pub update: Option<NodeResizeSessionUpdateOutcome>,
    pub end: NodeResizeEnd,
}

impl NodeResizeSessionOutcome {
    pub fn end_outcome(&self) -> NodeResizeEndOutcome {
        self.end.outcome
    }

    pub fn committed_update(&self) -> Option<&NodeResizeSessionUpdateOutcome> {
        self.update.as_ref()
    }
}

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

    /// Emits the start event for a headless pointer-driven node resize session.
    pub fn start_node_resize_session(&mut self, session: NodeResizeSession) {
        self.emit_gesture(NodeGraphGestureEvent::NodeResizeStart(session.start()));
    }

    /// Commits one pointer-driven session update and emits the derived resize update event.
    pub fn apply_node_resize_session_update(
        &mut self,
        session: NodeResizeSession,
        request: NodeResizeSessionUpdateRequest,
    ) -> Result<Option<NodeResizeSessionUpdateOutcome>, DispatchError> {
        let Some(plan) = self.plan_node_pointer_resize(session.pointer_resize_request(request))
        else {
            return Ok(None);
        };
        let update = session.update(&plan, request);
        let dispatch = self.dispatch_transaction(plan.transaction())?;
        self.emit_gesture(NodeGraphGestureEvent::NodeResizeUpdate(update.clone()));

        Ok(Some(NodeResizeSessionUpdateOutcome { update, dispatch }))
    }

    /// Emits the end event for a headless pointer-driven node resize session.
    pub fn finish_node_resize_session(
        &mut self,
        session: NodeResizeSession,
        pointer: jellyflow_core::core::CanvasPoint,
        outcome: NodeResizeEndOutcome,
    ) {
        self.emit_gesture(NodeGraphGestureEvent::NodeResizeEnd(
            session.end(pointer, outcome),
        ));
    }

    /// Runs a one-update pointer resize session through start, commit/update, and end events.
    pub fn apply_node_resize_session(
        &mut self,
        session: NodeResizeSession,
        request: NodeResizeSessionUpdateRequest,
    ) -> Result<NodeResizeSessionOutcome, DispatchError> {
        let start = session.start();
        self.emit_gesture(NodeGraphGestureEvent::NodeResizeStart(start.clone()));
        let update = self.apply_node_resize_session_update(session, request)?;
        let outcome = if update.is_some() {
            NodeResizeEndOutcome::Committed
        } else {
            NodeResizeEndOutcome::NoOp
        };
        let end = session.end(request.current, outcome);
        self.emit_gesture(NodeGraphGestureEvent::NodeResizeEnd(end.clone()));

        Ok(NodeResizeSessionOutcome { start, update, end })
    }

    fn resize_context(&self) -> NodeResizeContext {
        let interaction = self.resolved_interaction_state();
        let node_drag = interaction.node_drag_interaction();
        let node_origin = node_drag.node_origin.normalized();
        NodeResizeContext::new((node_origin.x, node_origin.y))
    }
}
