use jellyflow_core::core::{CanvasPoint, CanvasRect};

/// Side of a node or handle where an edge endpoint attaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HandlePosition {
    Top,
    Right,
    Bottom,
    Left,
}

/// Renderer-neutral handle bounds relative to the owning node's top-left corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HandleBounds {
    pub rect: CanvasRect,
    pub position: HandlePosition,
}

/// Input for resolving one edge endpoint.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeEndpointInput {
    pub node_rect: CanvasRect,
    pub handle: Option<HandleBounds>,
    pub fallback_position: HandlePosition,
}

/// Resolved edge endpoint in canvas space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeEndpointPosition {
    pub point: CanvasPoint,
    pub position: HandlePosition,
}

/// Resolved source and target endpoint geometry in canvas space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgePosition {
    pub source: EdgeEndpointPosition,
    pub target: EdgeEndpointPosition,
}

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

#[cfg(test)]
mod tests {
    use super::{
        EdgeEndpointInput, HandleBounds, HandlePosition, edge_position, handle_anchor_position,
        handle_center_position,
    };
    use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize};

    #[test]
    fn edge_position_uses_handle_rect_side_anchors() {
        let source_rect = CanvasRect {
            origin: CanvasPoint { x: 100.0, y: 50.0 },
            size: CanvasSize {
                width: 200.0,
                height: 100.0,
            },
        };
        let target_rect = CanvasRect {
            origin: CanvasPoint { x: 400.0, y: 80.0 },
            size: CanvasSize {
                width: 120.0,
                height: 80.0,
            },
        };

        let position = edge_position(
            EdgeEndpointInput {
                node_rect: source_rect,
                handle: Some(HandleBounds {
                    rect: CanvasRect {
                        origin: CanvasPoint { x: 190.0, y: 40.0 },
                        size: CanvasSize {
                            width: 10.0,
                            height: 20.0,
                        },
                    },
                    position: HandlePosition::Right,
                }),
                fallback_position: HandlePosition::Bottom,
            },
            EdgeEndpointInput {
                node_rect: target_rect,
                handle: Some(HandleBounds {
                    rect: CanvasRect {
                        origin: CanvasPoint { x: 0.0, y: 30.0 },
                        size: CanvasSize {
                            width: 12.0,
                            height: 20.0,
                        },
                    },
                    position: HandlePosition::Left,
                }),
                fallback_position: HandlePosition::Top,
            },
        )
        .expect("edge position");

        assert_eq!(position.source.position, HandlePosition::Right);
        assert!((position.source.point.x - 300.0).abs() <= 1.0e-6);
        assert!((position.source.point.y - 100.0).abs() <= 1.0e-6);
        assert_eq!(position.target.position, HandlePosition::Left);
        assert!((position.target.point.x - 400.0).abs() <= 1.0e-6);
        assert!((position.target.point.y - 120.0).abs() <= 1.0e-6);
    }

    #[test]
    fn edge_position_falls_back_to_node_side_without_handle_bounds() {
        let node_rect = CanvasRect {
            origin: CanvasPoint { x: 10.0, y: 20.0 },
            size: CanvasSize {
                width: 80.0,
                height: 40.0,
            },
        };

        let source =
            handle_anchor_position(node_rect, None, HandlePosition::Bottom).expect("source anchor");
        assert_eq!(source.position, HandlePosition::Bottom);
        assert!((source.point.x - 50.0).abs() <= 1.0e-6);
        assert!((source.point.y - 60.0).abs() <= 1.0e-6);

        let target =
            handle_anchor_position(node_rect, None, HandlePosition::Top).expect("target anchor");
        assert_eq!(target.position, HandlePosition::Top);
        assert!((target.point.x - 50.0).abs() <= 1.0e-6);
        assert!((target.point.y - 20.0).abs() <= 1.0e-6);
    }

    #[test]
    fn edge_position_can_return_handle_center_for_hit_testing() {
        let node_rect = CanvasRect {
            origin: CanvasPoint { x: 10.0, y: 20.0 },
            size: CanvasSize {
                width: 80.0,
                height: 40.0,
            },
        };
        let center = handle_center_position(
            node_rect,
            Some(HandleBounds {
                rect: CanvasRect {
                    origin: CanvasPoint { x: 70.0, y: 10.0 },
                    size: CanvasSize {
                        width: 10.0,
                        height: 20.0,
                    },
                },
                position: HandlePosition::Right,
            }),
            HandlePosition::Left,
        )
        .expect("center");

        assert!((center.x - 85.0).abs() <= 1.0e-6);
        assert!((center.y - 40.0).abs() <= 1.0e-6);
    }

    #[test]
    fn edge_position_rejects_invalid_node_or_handle_rects() {
        let node_rect = CanvasRect {
            origin: CanvasPoint { x: 0.0, y: 0.0 },
            size: CanvasSize {
                width: 80.0,
                height: 40.0,
            },
        };

        assert!(
            handle_anchor_position(
                CanvasRect {
                    origin: CanvasPoint {
                        x: f32::INFINITY,
                        y: 0.0,
                    },
                    ..node_rect
                },
                None,
                HandlePosition::Left,
            )
            .is_none()
        );
        assert!(
            handle_anchor_position(
                node_rect,
                Some(HandleBounds {
                    rect: CanvasRect {
                        origin: CanvasPoint { x: 0.0, y: 0.0 },
                        size: CanvasSize {
                            width: 0.0,
                            height: 10.0,
                        },
                    },
                    position: HandlePosition::Left,
                }),
                HandlePosition::Left,
            )
            .is_none()
        );
    }
}
