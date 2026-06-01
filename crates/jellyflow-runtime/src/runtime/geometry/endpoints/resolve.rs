use jellyflow_core::core::{CanvasPoint, CanvasRect};

use super::types::{
    EdgeEndpointInput, EdgeEndpointPosition, EdgePosition, HandleBounds, HandlePosition,
};

pub fn edge_position(source: EdgeEndpointInput, target: EdgeEndpointInput) -> Option<EdgePosition> {
    Some(EdgePosition {
        source: handle_anchor_position(source.node_rect, source.handle, source.fallback_position)?,
        target: handle_anchor_position(target.node_rect, target.handle, target.fallback_position)?,
    })
}

pub fn handle_anchor_position(
    node_rect: CanvasRect,
    handle: Option<HandleBounds>,
    fallback_position: HandlePosition,
) -> Option<EdgeEndpointPosition> {
    let (rect, position) = absolute_handle_rect(node_rect, handle, fallback_position)?;
    let point = match position {
        HandlePosition::Top => CanvasPoint {
            x: rect.origin.x + rect.size.width * 0.5,
            y: rect.origin.y,
        },
        HandlePosition::Right => CanvasPoint {
            x: rect.origin.x + rect.size.width,
            y: rect.origin.y + rect.size.height * 0.5,
        },
        HandlePosition::Bottom => CanvasPoint {
            x: rect.origin.x + rect.size.width * 0.5,
            y: rect.origin.y + rect.size.height,
        },
        HandlePosition::Left => CanvasPoint {
            x: rect.origin.x,
            y: rect.origin.y + rect.size.height * 0.5,
        },
    };

    point
        .is_finite()
        .then_some(EdgeEndpointPosition { point, position })
}

pub fn handle_center_position(
    node_rect: CanvasRect,
    handle: Option<HandleBounds>,
    fallback_position: HandlePosition,
) -> Option<CanvasPoint> {
    let (rect, _position) = absolute_handle_rect(node_rect, handle, fallback_position)?;
    Some(CanvasPoint {
        x: rect.origin.x + rect.size.width * 0.5,
        y: rect.origin.y + rect.size.height * 0.5,
    })
}

fn absolute_handle_rect(
    node_rect: CanvasRect,
    handle: Option<HandleBounds>,
    fallback_position: HandlePosition,
) -> Option<(CanvasRect, HandlePosition)> {
    if !node_rect.is_positive_finite() {
        return None;
    }

    let Some(handle) = handle else {
        return Some((node_rect, fallback_position));
    };

    if !handle.rect.is_positive_finite() {
        return None;
    }

    let rect = CanvasRect {
        origin: CanvasPoint {
            x: node_rect.origin.x + handle.rect.origin.x,
            y: node_rect.origin.y + handle.rect.origin.y,
        },
        size: handle.rect.size,
    };

    rect.is_positive_finite().then_some((rect, handle.position))
}
