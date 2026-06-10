use crate::runtime::events::{
    NodeGraphGestureEvent, NodeResizeEnd, NodeResizeEndOutcome, NodeResizeStart, NodeResizeUpdate,
};
use jellyflow_core::core::{CanvasPoint, NodeId};

use super::types::{
    NodePointerResizeRequest, NodeResizeAxis, NodeResizeConstraints, NodeResizeDirection,
    NodeResizePlan,
};

/// Headless state for one pointer-driven node resize gesture.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeResizeSession {
    pub node: NodeId,
    pub direction: NodeResizeDirection,
    pub start: CanvasPoint,
}

impl NodeResizeSession {
    pub fn new(node: NodeId, start: CanvasPoint, direction: NodeResizeDirection) -> Self {
        Self {
            node,
            direction,
            start,
        }
    }

    pub fn start(self) -> NodeResizeStart {
        NodeResizeStart {
            node: self.node,
            direction: self.direction,
            pointer: self.start,
        }
    }

    pub fn start_event(self) -> NodeGraphGestureEvent {
        NodeGraphGestureEvent::NodeResizeStart(self.start())
    }

    pub fn update(
        self,
        plan: &NodeResizePlan,
        request: NodeResizeSessionUpdateRequest,
    ) -> NodeResizeUpdate {
        NodeResizeUpdate {
            node: self.node,
            direction: self.direction,
            pointer: request.current,
            position: plan.to_pos,
            size: plan.to,
        }
    }

    pub fn update_event(
        self,
        plan: &NodeResizePlan,
        request: NodeResizeSessionUpdateRequest,
    ) -> NodeGraphGestureEvent {
        NodeGraphGestureEvent::NodeResizeUpdate(self.update(plan, request))
    }

    pub fn end(self, pointer: CanvasPoint, outcome: NodeResizeEndOutcome) -> NodeResizeEnd {
        NodeResizeEnd {
            node: self.node,
            direction: self.direction,
            pointer,
            outcome,
        }
    }

    pub fn end_event(
        self,
        pointer: CanvasPoint,
        outcome: NodeResizeEndOutcome,
    ) -> NodeGraphGestureEvent {
        NodeGraphGestureEvent::NodeResizeEnd(self.end(pointer, outcome))
    }

    pub fn pointer_resize_request(
        self,
        request: NodeResizeSessionUpdateRequest,
    ) -> NodePointerResizeRequest {
        NodePointerResizeRequest::new(self.node, self.start, request.current, self.direction)
            .with_constraints(request.constraints)
            .with_keep_aspect_ratio(request.keep_aspect_ratio)
            .with_axis(request.axis)
    }
}

/// Per-update inputs for a node resize session.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeResizeSessionUpdateRequest {
    pub current: CanvasPoint,
    pub constraints: NodeResizeConstraints,
    pub keep_aspect_ratio: bool,
    pub axis: NodeResizeAxis,
}

impl NodeResizeSessionUpdateRequest {
    pub fn new(current: CanvasPoint) -> Self {
        Self {
            current,
            constraints: NodeResizeConstraints::default(),
            keep_aspect_ratio: false,
            axis: NodeResizeAxis::default(),
        }
    }

    pub fn with_constraints(mut self, constraints: NodeResizeConstraints) -> Self {
        self.constraints = constraints;
        self
    }

    pub fn with_keep_aspect_ratio(mut self, keep_aspect_ratio: bool) -> Self {
        self.keep_aspect_ratio = keep_aspect_ratio;
        self
    }

    pub fn with_axis(mut self, axis: NodeResizeAxis) -> Self {
        self.axis = axis;
        self
    }
}
