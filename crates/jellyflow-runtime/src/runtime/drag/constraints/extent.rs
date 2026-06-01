use jellyflow_core::core::{CanvasRect, CanvasSize, Graph, Node, NodeExtent};

use super::geometry::normalized_rect;

pub(in crate::runtime::drag) fn resolved_extent_rect(
    graph: &Graph,
    node: &Node,
    extent: Option<NodeExtent>,
    expand_parent: bool,
) -> Option<CanvasRect> {
    match extent? {
        NodeExtent::Rect { rect } => normalized_rect(rect),
        NodeExtent::Parent if !expand_parent => node
            .parent
            .and_then(|parent| graph.groups.get(&parent))
            .and_then(|group| normalized_rect(group.rect)),
        NodeExtent::Parent => None,
    }
}

pub(in crate::runtime::drag) fn normalized_size(size: Option<CanvasSize>) -> CanvasSize {
    let Some(size) = size else {
        return CanvasSize::default();
    };
    if !size.is_finite() {
        return CanvasSize::default();
    }
    CanvasSize {
        width: size.width.max(0.0),
        height: size.height.max(0.0),
    }
}
