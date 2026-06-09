use crate::runtime::geometry::CanvasBounds;
use jellyflow_core::core::{CanvasPoint, CanvasSize, Graph, Node};
use jellyflow_core::ops::GraphOp;

pub(super) fn parent_expansion_op(
    graph: &Graph,
    node: &Node,
    to_pos: CanvasPoint,
    to_size: CanvasSize,
    node_origin: (f32, f32),
) -> Option<GraphOp> {
    if !node.expand_parent.unwrap_or(false) {
        return None;
    }

    let parent = node.parent?;
    let parent_rect = graph.groups.get(&parent)?.rect;
    let parent_bounds = CanvasBounds::from_rect(parent_rect)?;
    let child_bounds = CanvasBounds::from_node(to_pos, to_size, node_origin)?;
    let to = parent_bounds.union(child_bounds).to_rect();

    (parent_rect != to).then_some(GraphOp::SetGroupRect {
        id: parent,
        from: parent_rect,
        to,
    })
}
