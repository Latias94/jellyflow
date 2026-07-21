use jellyflow_core::{CanvasPoint, EdgeId, NodeGraphConnectionMode, NodeId, PortId};
use jellyflow_runtime::runtime::geometry::EdgePathAnchor;
use jellyflow_runtime::{
    ConnectionEdit, EdgeEndpoint, PublishedPortOutcome, ViewPoint, ViewSize, WeakViewSession,
};

use super::SmokeExpectations;

#[derive(Clone, Copy, Debug)]
pub(super) struct NodePlan {
    pub(super) node: NodeId,
    pub(super) original_position: CanvasPoint,
    pub(super) original_geometry_origin: CanvasPoint,
    pub(super) press: ViewPoint,
    pub(super) release: ViewPoint,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct EdgePlan {
    pub(super) edge: EdgeId,
    pub(super) from: PortId,
    pub(super) to: PortId,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ReconnectPlan {
    pub(super) endpoint: EdgeEndpoint,
    pub(super) target: PortId,
}

pub(super) fn graph_contains_edge(
    view: &WeakViewSession,
    edge: EdgeId,
) -> Result<bool, &'static str> {
    Ok(view
        .graph_snapshot()
        .map_err(|_| "input_graph_snapshot_failed")?
        .graph()
        .edge(&edge)
        .is_some())
}

pub(super) fn geometry_fits_surface(
    view: &WeakViewSession,
    expectations: &SmokeExpectations,
    surface_size: ViewSize,
) -> Result<bool, &'static str> {
    let geometry = view
        .geometry_snapshot()
        .map_err(|_| "geometry_snapshot_read_failed")?;
    let transform = view
        .viewport_transform()
        .map_err(|_| "input_viewport_read_failed")?;
    let routes_ready = expectations
        .edges
        .iter()
        .all(|edge| geometry.route(*edge).is_some());
    let nodes_fit = expectations.node_ports.keys().all(|node| {
        let Some(node) = geometry.node(*node).filter(|node| node.is_fresh()) else {
            return false;
        };
        let Some(min) = transform.screen_point_for_canvas(node.bounds().origin) else {
            return false;
        };
        let Some(max) = transform.screen_point_for_canvas(CanvasPoint {
            x: node.bounds().origin.x + node.bounds().size.width,
            y: node.bounds().origin.y + node.bounds().size.height,
        }) else {
            return false;
        };
        min.x >= -1.0
            && min.y >= -1.0
            && max.x <= surface_size.width + 1.0
            && max.y <= surface_size.height + 1.0
    });
    Ok(routes_ready && nodes_fit)
}

pub(super) fn dragged_geometry_status(
    view: &WeakViewSession,
    expectations: &SmokeExpectations,
    plan: NodePlan,
) -> Result<(bool, bool), &'static str> {
    let graph = view
        .graph_snapshot()
        .map_err(|_| "input_graph_snapshot_failed")?;
    let geometry = view
        .geometry_snapshot()
        .map_err(|_| "geometry_snapshot_read_failed")?;
    let Some(model) = graph.graph().node(&plan.node) else {
        return Ok((false, false));
    };
    let Some(published) = geometry.node(plan.node).filter(|node| node.is_fresh()) else {
        return Ok((false, false));
    };
    let model_delta = point_delta(model.pos, plan.original_position);
    let geometry_delta = point_delta(published.bounds().origin, plan.original_geometry_origin);
    let node_converged = points_close(geometry_delta, model_delta);
    let routes_converged = expectations
        .edges
        .iter()
        .all(|edge| geometry.route(*edge).is_some());
    Ok((node_converged, routes_converged))
}

pub(super) fn choose_reconnect(
    view: &WeakViewSession,
    edge: EdgeId,
    freed: Option<EdgePlan>,
) -> Result<Option<ReconnectPlan>, &'static str> {
    let Some(freed) = freed else {
        return Ok(None);
    };
    for plan in [
        ReconnectPlan {
            endpoint: EdgeEndpoint::To,
            target: freed.to,
        },
        ReconnectPlan {
            endpoint: EdgeEndpoint::From,
            target: freed.from,
        },
    ] {
        let accepted = view
            .evaluate_connection(ConnectionEdit::reconnect(
                edge,
                plan.endpoint,
                plan.target,
                NodeGraphConnectionMode::Strict,
            ))
            .map_err(|_| "input_reconnect_evaluation_failed")?
            .is_accept();
        if accepted {
            return Ok(Some(plan));
        }
    }
    Ok(None)
}

pub(super) fn free_connection_port(view: &WeakViewSession) -> Result<Option<PortId>, &'static str> {
    let snapshot = view
        .graph_snapshot()
        .map_err(|_| "input_graph_snapshot_failed")?;
    let geometry = view
        .geometry_snapshot()
        .map_err(|_| "geometry_snapshot_read_failed")?;
    for port in snapshot.graph().ports().keys().copied() {
        if geometry
            .port_outcome(port)
            .and_then(PublishedPortOutcome::rendered)
            .is_none()
        {
            continue;
        }
        let Some(summary) = view
            .port_connection_summary(port)
            .map_err(|_| "input_port_summary_failed")?
        else {
            continue;
        };
        if !summary.is_connected() && summary.can_begin_connection() {
            return Ok(Some(port));
        }
    }
    Ok(None)
}

pub(super) fn port_point(
    view: &WeakViewSession,
    port: PortId,
) -> Result<Option<ViewPoint>, &'static str> {
    let geometry = view
        .geometry_snapshot()
        .map_err(|_| "geometry_snapshot_read_failed")?;
    let Some(port) = geometry
        .port_outcome(port)
        .and_then(PublishedPortOutcome::rendered)
    else {
        return Ok(None);
    };
    let transform = view
        .viewport_transform()
        .map_err(|_| "input_viewport_read_failed")?;
    Ok(transform
        .screen_point_for_canvas(port.route_anchor())
        .map(|point| ViewPoint {
            x: point.x,
            y: point.y,
        }))
}

pub(super) fn edge_point(
    view: &WeakViewSession,
    edge: EdgeId,
    anchor: EdgePathAnchor,
) -> Result<Option<ViewPoint>, &'static str> {
    let geometry = view
        .geometry_snapshot()
        .map_err(|_| "geometry_snapshot_read_failed")?;
    let Some(route) = geometry.route(edge) else {
        return Ok(None);
    };
    let sample = route
        .facts()
        .path
        .sample(anchor)
        .map_err(|_| "input_edge_sample_failed")?;
    let transform = view
        .viewport_transform()
        .map_err(|_| "input_viewport_read_failed")?;
    Ok(transform
        .screen_point_for_canvas(sample.point)
        .map(|point| ViewPoint {
            x: point.x,
            y: point.y,
        }))
}

pub(super) fn blank_surface_point(
    view: &WeakViewSession,
    surface_size: ViewSize,
) -> Result<Option<ViewPoint>, &'static str> {
    let geometry = view
        .geometry_snapshot()
        .map_err(|_| "geometry_snapshot_read_failed")?;
    let transform = view
        .viewport_transform()
        .map_err(|_| "input_viewport_read_failed")?;
    let mut y = 40.0;
    while y <= surface_size.height - 40.0 {
        let mut x = 40.0;
        while x <= surface_size.width - 40.0 {
            let candidate = ViewPoint { x, y };
            let near_port = geometry.port_outcomes().values().any(|outcome| {
                let Some(port) = outcome.rendered() else {
                    return false;
                };
                transform
                    .screen_point_for_canvas(port.route_anchor())
                    .is_some_and(|port| {
                        let dx = f64::from(port.x - x);
                        let dy = f64::from(port.y - y);
                        dx.mul_add(dx, dy * dy) < 1_600.0
                    })
            });
            if !near_port {
                return Ok(Some(candidate));
            }
            x += 40.0;
        }
        y += 40.0;
    }
    Ok(None)
}

pub(super) fn point_inside_surface(point: ViewPoint, surface: ViewSize) -> bool {
    point.x.is_finite()
        && point.y.is_finite()
        && point.x >= 0.0
        && point.y >= 0.0
        && point.x <= surface.width
        && point.y <= surface.height
}

pub(super) fn point_changed(left: CanvasPoint, right: CanvasPoint) -> bool {
    (left.x - right.x).abs() > 0.5 || (left.y - right.y).abs() > 0.5
}

fn points_close(left: CanvasPoint, right: CanvasPoint) -> bool {
    (left.x - right.x).abs() <= 0.5 && (left.y - right.y).abs() <= 0.5
}

fn point_delta(current: CanvasPoint, original: CanvasPoint) -> CanvasPoint {
    CanvasPoint {
        x: current.x - original.x,
        y: current.y - original.y,
    }
}
