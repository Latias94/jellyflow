use crate::node_origin::resolve_node_origin;
use crate::runtime::geometry::CanvasBounds;
use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize, Graph, Node, NodeExtent, NodeId};
use jellyflow_core::ops::{GraphOp, GraphTransaction};

use super::parent_expansion::parent_expansion_op;
use super::types::{
    NODE_RESIZE_TRANSACTION_LABEL, NodePointerResizeRequest, NodeResizeContext,
    NodeResizeDirection, NodeResizePlan, NodeResizeRequest,
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

    let mut to = request
        .constraints
        .clamp(direction_target_size(node.size, request)?)?;
    let from_pos = node.pos;
    let node_origin = resolve_node_origin(node.origin, context.node_origin);
    let mut to_pos = resized_position(from_pos, node.size, to, request.direction, node_origin)?;
    if let Some(extent) = resolved_resize_extent(graph, node, None) {
        let clamped = clamp_geometry_to_extent(to_pos, to, request.direction, node_origin, extent)?;
        to_pos = clamped.0;
        to = clamped.1;
    }

    resize_plan_for_geometry(graph, request.node, node, to, to_pos, node_origin)
}

/// Plans a node resize update from canvas-space pointer movement without mutating the graph.
pub fn plan_node_pointer_resize(
    graph: &Graph,
    request: NodePointerResizeRequest,
) -> Option<NodeResizePlan> {
    plan_node_pointer_resize_with_context(graph, NodeResizeContext::default(), request)
}

/// Plans a pointer-driven node resize update using a runtime geometry context.
pub fn plan_node_pointer_resize_with_context(
    graph: &Graph,
    context: NodeResizeContext,
    request: NodePointerResizeRequest,
) -> Option<NodeResizePlan> {
    plan_node_pointer_resize_with_policy_extent(graph, context, None, request)
}

pub(super) fn plan_node_pointer_resize_with_policy_extent(
    graph: &Graph,
    context: NodeResizeContext,
    node_extent: Option<CanvasRect>,
    request: NodePointerResizeRequest,
) -> Option<NodeResizePlan> {
    let node = graph.nodes.get(&request.node)?;
    if node.hidden
        || !node.pos.is_finite()
        || !request.start.is_finite()
        || !request.current.is_finite()
    {
        return None;
    }

    let from_size = positive_size(node.size)?;
    let node_origin = resolve_node_origin(node.origin, context.node_origin);
    let (to_pos, to) =
        pointer_resize_geometry(graph, node, node_extent, request, from_size, node_origin)?;

    resize_plan_for_geometry(graph, request.node, node, to, to_pos, node_origin)
}

fn resize_plan_for_geometry(
    graph: &Graph,
    node_id: NodeId,
    node: &Node,
    to: CanvasSize,
    to_pos: CanvasPoint,
    node_origin: (f32, f32),
) -> Option<NodeResizePlan> {
    if !to.is_positive_finite() || !to_pos.is_finite() {
        return None;
    }
    if node.size == Some(to) && node.pos == to_pos {
        return None;
    }

    let mut ops = Vec::new();
    if node.pos != to_pos {
        ops.push(GraphOp::SetNodePos {
            id: node_id,
            from: node.pos,
            to: to_pos,
        });
    }
    if node.size != Some(to) {
        ops.push(GraphOp::SetNodeSize {
            id: node_id,
            from: node.size,
            to: Some(to),
        });
    }
    if let Some(op) = parent_expansion_op(graph, node, to_pos, to, node_origin) {
        ops.push(op);
    }
    let transaction = GraphTransaction::from_ops(ops).with_label(NODE_RESIZE_TRANSACTION_LABEL);

    Some(NodeResizePlan::new(
        node_id,
        node.size,
        to,
        node.pos,
        to_pos,
        transaction,
    ))
}

fn pointer_resize_geometry(
    graph: &Graph,
    node: &Node,
    node_extent: Option<CanvasRect>,
    request: NodePointerResizeRequest,
    from_size: CanvasSize,
    origin: (f32, f32),
) -> Option<(CanvasPoint, CanvasSize)> {
    let dist_x = (request.current.x - request.start.x).floor();
    let dist_y = (request.current.y - request.start.y).floor();
    let dir_x = resize_direction_x(request.direction);
    let dir_y = resize_direction_y(request.direction);

    let mut to = CanvasSize {
        width: pointer_axis_size(from_size.width, dist_x, dir_x, origin.0),
        height: pointer_axis_size(from_size.height, dist_y, dir_y, origin.1),
    };
    if request.keep_aspect_ratio {
        to = keep_aspect_ratio_size(to, from_size, request.direction)?;
    }
    to = request.constraints.clamp(to)?;
    if !request.axis.includes_width() {
        to.width = from_size.width;
    }
    if !request.axis.includes_height() {
        to.height = from_size.height;
    }
    if !to.is_positive_finite() {
        return None;
    }

    let mut to_pos = resized_position(node.pos, Some(from_size), to, request.direction, origin)?;
    if let Some(extent) = resolved_resize_extent(graph, node, node_extent) {
        let clamped = clamp_geometry_to_extent(to_pos, to, request.direction, origin, extent)?;
        to_pos = clamped.0;
        to = clamped.1;
    }

    Some((to_pos, to))
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

fn resize_direction_x(direction: NodeResizeDirection) -> f32 {
    if direction.affects_x() {
        -1.0
    } else if direction.is_horizontal() {
        1.0
    } else {
        0.0
    }
}

fn resize_direction_y(direction: NodeResizeDirection) -> f32 {
    if direction.affects_y() {
        -1.0
    } else if direction.is_vertical() {
        1.0
    } else {
        0.0
    }
}

fn pointer_axis_size(from: f32, delta: f32, direction: f32, origin: f32) -> f32 {
    if direction == 0.0 {
        return from;
    }
    from + direction * delta / (origin * 2.0 + 1.0)
}

fn keep_aspect_ratio_size(
    mut to: CanvasSize,
    from: CanvasSize,
    direction: NodeResizeDirection,
) -> Option<CanvasSize> {
    let aspect_ratio = from.width / from.height;
    if !aspect_ratio.is_finite() || aspect_ratio <= 0.0 {
        return None;
    }

    if direction.is_horizontal() && !direction.is_vertical() {
        to.height = to.width / aspect_ratio;
    } else if direction.is_vertical() && !direction.is_horizontal() {
        to.width = to.height * aspect_ratio;
    } else if to.width / to.height > aspect_ratio {
        to.height = to.width / aspect_ratio;
    } else {
        to.width = to.height * aspect_ratio;
    }

    to.is_positive_finite().then_some(to)
}

fn resolved_resize_extent(
    graph: &Graph,
    node: &Node,
    node_extent: Option<CanvasRect>,
) -> Option<CanvasRect> {
    let extent = node
        .extent
        .or_else(|| node_extent.map(|rect| NodeExtent::Rect { rect }))?;
    match extent {
        NodeExtent::Rect { rect } => normalized_rect(rect),
        NodeExtent::Parent if !node.expand_parent.unwrap_or(false) => node
            .parent
            .and_then(|parent| graph.groups.get(&parent))
            .and_then(|group| normalized_rect(group.rect)),
        NodeExtent::Parent => None,
    }
}

fn normalized_rect(rect: CanvasRect) -> Option<CanvasRect> {
    CanvasBounds::from_rect(rect).map(CanvasBounds::to_rect)
}

fn clamp_geometry_to_extent(
    position: CanvasPoint,
    size: CanvasSize,
    direction: NodeResizeDirection,
    origin: (f32, f32),
    extent: CanvasRect,
) -> Option<(CanvasPoint, CanvasSize)> {
    if !extent.is_positive_finite() {
        return None;
    }

    let mut top_left = CanvasPoint {
        x: position.x - origin.0 * size.width,
        y: position.y - origin.1 * size.height,
    };
    let mut size = size;
    let extent_max_x = extent.origin.x + extent.size.width;
    let extent_max_y = extent.origin.y + extent.size.height;

    if direction.is_horizontal() {
        if direction.affects_x() {
            if top_left.x < extent.origin.x {
                let overflow = extent.origin.x - top_left.x;
                top_left.x = extent.origin.x;
                size.width -= overflow;
            }
            let right = top_left.x + size.width;
            if right > extent_max_x {
                size.width -= right - extent_max_x;
            }
        } else {
            let right = top_left.x + size.width;
            if right > extent_max_x {
                size.width -= right - extent_max_x;
            }
        }
    }

    if direction.is_vertical() {
        if direction.affects_y() {
            if top_left.y < extent.origin.y {
                let overflow = extent.origin.y - top_left.y;
                top_left.y = extent.origin.y;
                size.height -= overflow;
            }
            let bottom = top_left.y + size.height;
            if bottom > extent_max_y {
                size.height -= bottom - extent_max_y;
            }
        } else {
            let bottom = top_left.y + size.height;
            if bottom > extent_max_y {
                size.height -= bottom - extent_max_y;
            }
        }
    }

    if !size.is_positive_finite() {
        return None;
    }

    Some((
        CanvasPoint {
            x: top_left.x + origin.0 * size.width,
            y: top_left.y + origin.1 * size.height,
        },
        size,
    ))
}

fn positive_size(size: Option<CanvasSize>) -> Option<CanvasSize> {
    size.filter(|size| size.is_positive_finite())
}
