use crate::node_origin::resolve_node_origin;
use jellyflow_core::core::{CanvasPoint, CanvasSize, Graph};
use jellyflow_core::ops::{GraphOp, GraphTransaction};

use super::types::{
    NODE_RESIZE_TRANSACTION_LABEL, NodeResizeContext, NodeResizeDirection, NodeResizePlan,
    NodeResizeRequest,
};

/// Plans a node resize update without mutating the graph.
pub fn plan_node_resize(graph: &Graph, request: NodeResizeRequest) -> Option<NodeResizePlan> {
    plan_node_resize_with_context(graph, NodeResizeContext::default(), request)
}

/// Plans a node resize update using a runtime geometry context.
pub fn plan_node_resize_with_context(
    graph: &Graph,
    context: NodeResizeContext,
    request: NodeResizeRequest,
) -> Option<NodeResizePlan> {
    let node = graph.nodes.get(&request.node)?;
    if node.hidden || !node.pos.is_finite() {
        return None;
    }

    let to = request
        .constraints
        .clamp(direction_target_size(node.size, request)?)?;
    let from_pos = node.pos;
    let node_origin = resolve_node_origin(node.origin, context.node_origin);
    let to_pos = resized_position(from_pos, node.size, to, request.direction, node_origin)?;
    if node.size == Some(to) && from_pos == to_pos {
        return None;
    }

    let mut ops = Vec::new();
    if from_pos != to_pos {
        ops.push(GraphOp::SetNodePos {
            id: request.node,
            from: from_pos,
            to: to_pos,
        });
    }
    if node.size != Some(to) {
        ops.push(GraphOp::SetNodeSize {
            id: request.node,
            from: node.size,
            to: Some(to),
        });
    }
    let transaction = GraphTransaction::from_ops(ops).with_label(NODE_RESIZE_TRANSACTION_LABEL);

    Some(NodeResizePlan::new(
        request.node,
        node.size,
        to,
        from_pos,
        to_pos,
        transaction,
    ))
}

fn direction_target_size(
    current: Option<CanvasSize>,
    request: NodeResizeRequest,
) -> Option<CanvasSize> {
    if !request.to.is_positive_finite() {
        return None;
    }

    let mut to = request.to;
    if !request.direction.is_horizontal() || !request.direction.is_vertical() {
        let current = positive_size(current)?;
        if !request.direction.is_horizontal() {
            to.width = current.width;
        }
        if !request.direction.is_vertical() {
            to.height = current.height;
        }
    }
    Some(to)
}

fn resized_position(
    from_pos: CanvasPoint,
    from_size: Option<CanvasSize>,
    to: CanvasSize,
    direction: NodeResizeDirection,
    origin: (f32, f32),
) -> Option<CanvasPoint> {
    let Some(from_size) = from_size else {
        return position_without_current_size(from_pos, direction, origin);
    };
    let from_size = positive_size(Some(from_size))?;
    let delta_width = to.width - from_size.width;
    let delta_height = to.height - from_size.height;

    Some(CanvasPoint {
        x: resized_axis(
            from_pos.x,
            delta_width,
            origin.0,
            direction.is_horizontal(),
            direction.affects_x(),
        ),
        y: resized_axis(
            from_pos.y,
            delta_height,
            origin.1,
            direction.is_vertical(),
            direction.affects_y(),
        ),
    })
}

fn position_without_current_size(
    from_pos: CanvasPoint,
    direction: NodeResizeDirection,
    origin: (f32, f32),
) -> Option<CanvasPoint> {
    (!position_may_change(direction, origin)).then_some(from_pos)
}

fn position_may_change(direction: NodeResizeDirection, origin: (f32, f32)) -> bool {
    axis_position_may_change(direction.is_horizontal(), direction.affects_x(), origin.0)
        || axis_position_may_change(direction.is_vertical(), direction.affects_y(), origin.1)
}

fn axis_position_may_change(is_axis: bool, affects_axis: bool, origin: f32) -> bool {
    is_axis
        && if affects_axis {
            origin < 1.0
        } else {
            origin > 0.0
        }
}

fn resized_axis(from: f32, delta: f32, origin: f32, is_axis: bool, affects_axis: bool) -> f32 {
    if !is_axis {
        return from;
    }
    if affects_axis {
        from - delta * (1.0 - origin)
    } else {
        from + delta * origin
    }
}

fn positive_size(size: Option<CanvasSize>) -> Option<CanvasSize> {
    size.filter(|size| size.is_positive_finite())
}
