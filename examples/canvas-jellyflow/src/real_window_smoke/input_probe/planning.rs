use super::SmokeExpectations;
use jellyflow_core::{CanvasPoint, CanvasRect, EdgeId, NodeGraphConnectionMode, NodeId, PortId};
use jellyflow_runtime::commands::ConnectionEdit;
use jellyflow_runtime::geometry::{
    EdgeHitTestOptions, EdgePath, EdgePathAnchor, EdgePathFlattenError, HandlePosition,
    edge_path_distance,
};
use jellyflow_runtime::geometry::{ViewPoint, ViewRect, ViewSize};
use jellyflow_runtime::graph::EdgeEndpoint;
use jellyflow_runtime::view::{GeometrySnapshot, PublishedPortOutcome, WeakViewSession};
use jellyflow_runtime::viewport::ViewportTransform;

// Keep synthetic pointer targets outside the default 36px auto-pan band so a
// queued native frame cannot move a target between candidate validation and release.
const INPUT_SAFE_AREA_IN_VIEW: f32 = 48.0;
const CONNECTION_AUTOPAN_SETTLE_MARGIN_IN_VIEW: f32 = 40.0;
const CONNECTION_AUTOPAN_MAX_TARGET_OFFSET_IN_VIEW: f32 = 8.0;
const AUTOPAN_EDGE_INSET_IN_VIEW: f32 = 12.0;
const RECONNECT_GRIP_OFFSET_IN_VIEW: f32 = 12.0;
const EDGE_PICK_SAMPLE_COUNT: u16 = 64;
const EDGE_PICK_ENDPOINT_INSET: f32 = 0.02;
const EDGE_PICK_CONTROL_CLEARANCE_IN_VIEW: f32 = 12.0;
const EDGE_PICK_UNIQUENESS_MARGIN_IN_VIEW: f32 = 2.0;
const BLANK_SURFACE_GRID_IN_VIEW: f32 = 40.0;
const BLANK_SURFACE_PORT_CLEARANCE_IN_VIEW: f32 = 40.0;

#[derive(Clone, Copy, Debug)]
pub(super) struct NodePlan {
    pub(super) node: NodeId,
    pub(super) original_position: CanvasPoint,
    pub(super) original_geometry_origin: CanvasPoint,
    pub(super) press: ViewPoint,
    pub(super) activation: ViewPoint,
    pub(super) release: ViewPoint,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct EdgePlan {
    pub(super) edge: EdgeId,
    pub(super) from: PortId,
    pub(super) to: PortId,
    pub(super) from_anchor: Option<CanvasPoint>,
    pub(super) to_anchor: Option<CanvasPoint>,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum VisibleEdgePickPoint {
    Ready(ViewPoint),
    AwaitFrame(CanvasPoint),
    Unavailable,
}

#[derive(Clone, Copy, Debug)]
enum EdgeCandidateViewport {
    Ready(ViewPoint),
    AwaitFrame,
    Unavailable,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ReconnectPlan {
    pub(super) endpoint: EdgeEndpoint,
    pub(super) target: PortId,
    pub(super) target_anchor: CanvasPoint,
}

pub(super) fn refresh_edge_plan(
    view: &WeakViewSession,
    plan: EdgePlan,
) -> Result<EdgePlan, &'static str> {
    Ok(EdgePlan {
        from_anchor: Some(
            port_canvas_point(view, plan.from)?.ok_or("input_edge_from_port_culled")?,
        ),
        to_anchor: Some(port_canvas_point(view, plan.to)?.ok_or("input_edge_to_port_culled")?),
        ..plan
    })
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

pub(super) fn set_zoom_preserving_surface_center(
    view: &WeakViewSession,
    surface_size: ViewSize,
    zoom: f32,
) -> Result<(), &'static str> {
    let current = view
        .viewport_transform()
        .map_err(|_| "input_viewport_read_failed")?;
    let center = ViewPoint {
        x: surface_size.width * 0.5,
        y: surface_size.height * 0.5,
    };
    let canvas_center = current
        .canvas_point_at_screen(center)
        .ok_or("input_viewport_read_failed")?;
    let pan = CanvasPoint {
        x: center.x / zoom - canvas_center.x,
        y: center.y / zoom - canvas_center.y,
    };
    view.set_viewport(ViewportTransform::new(pan, zoom).ok_or("input_viewport_write_failed")?)
        .map(|_| ())
        .map_err(|_| "input_viewport_write_failed")
}

pub(super) fn set_zoom_centered_on_canvas_point(
    view: &WeakViewSession,
    surface_size: ViewSize,
    zoom: f32,
    anchor: CanvasPoint,
) -> Result<(), &'static str> {
    if !surface_size.is_positive_finite()
        || !zoom.is_finite()
        || zoom <= 0.0
        || !anchor.x.is_finite()
        || !anchor.y.is_finite()
    {
        return Err("input_zoom_anchor_invalid");
    }
    let center = CanvasPoint {
        x: surface_size.width * 0.5,
        y: surface_size.height * 0.5,
    };
    let pan = CanvasPoint {
        x: center.x / zoom - anchor.x,
        y: center.y / zoom - anchor.y,
    };
    view.set_viewport(ViewportTransform::new(pan, zoom).ok_or("input_viewport_write_failed")?)
        .map(|_| ())
        .map_err(|_| "input_viewport_write_failed")
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
    // A node move invalidates only its incident routes. Requiring unrelated
    // routes here turns legitimate virtualized work into a false smoke failure.
    let routes_converged = expectations
        .edges
        .iter()
        .filter(|edge_id| {
            let Some(edge) = graph.graph().edge(edge_id) else {
                return false;
            };
            [edge.from, edge.to].into_iter().any(|port_id| {
                graph
                    .graph()
                    .port(&port_id)
                    .is_some_and(|port| port.node == plan.node)
            })
        })
        .all(|edge_id| geometry.route(*edge_id).is_some());
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
    let snapshot = view
        .graph_snapshot()
        .map_err(|_| "input_graph_snapshot_failed")?;
    let Some(current) = snapshot.graph().edge(&edge) else {
        return Ok(None);
    };
    for (endpoint, target, target_anchor) in [
        (EdgeEndpoint::To, freed.to, freed.to_anchor),
        (EdgeEndpoint::From, freed.from, freed.from_anchor),
    ] {
        let Some(target_anchor) = target_anchor else {
            continue;
        };
        let plan = ReconnectPlan {
            endpoint,
            target,
            target_anchor,
        };
        let fixed = match plan.endpoint {
            EdgeEndpoint::From => current.to,
            EdgeEndpoint::To => current.from,
        };
        let Some(fixed_node) = snapshot.graph().port(&fixed).map(|port| port.node) else {
            continue;
        };
        let Some(target_node) = snapshot.graph().port(&plan.target).map(|port| port.node) else {
            continue;
        };
        if fixed_node == target_node {
            continue;
        }
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
            .into_outcome()
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

pub(super) fn port_owner_view_center(
    view: &WeakViewSession,
    port: PortId,
) -> Result<Option<ViewPoint>, &'static str> {
    let Some(anchor) = port_owner_canvas_center(view, port)? else {
        return Ok(None);
    };
    let transform = view
        .viewport_transform()
        .map_err(|_| "input_viewport_read_failed")?;
    Ok(transform.screen_point_for_canvas(anchor))
}

fn port_owner_canvas_center(
    view: &WeakViewSession,
    port: PortId,
) -> Result<Option<CanvasPoint>, &'static str> {
    let graph = view
        .graph_snapshot()
        .map_err(|_| "input_graph_snapshot_failed")?;
    let Some(node) = graph.graph().port(&port).map(|port| port.node) else {
        return Ok(None);
    };
    let Some(model) = graph.graph().node(&node) else {
        return Ok(None);
    };
    let geometry = view
        .geometry_snapshot()
        .map_err(|_| "geometry_snapshot_read_failed")?;
    let bounds = geometry
        .node(node)
        .filter(|geometry| geometry.is_fresh())
        .map(|geometry| geometry.bounds())
        .or_else(|| model.size.and_then(|size| model.bounds_for_size(size)));
    let Some(bounds) = bounds else {
        return Ok(None);
    };
    Ok(Some(CanvasPoint {
        x: bounds.origin.x + bounds.size.width * 0.5,
        y: bounds.origin.y + bounds.size.height * 0.5,
    }))
}

pub(super) fn visible_port_point(
    view: &WeakViewSession,
    port: PortId,
    surface_size: ViewSize,
) -> Result<Option<ViewPoint>, &'static str> {
    let anchor = match port_canvas_point(view, port)? {
        Some(anchor) => anchor,
        None => match port_owner_canvas_center(view, port)? {
            Some(anchor) => anchor,
            None => return Ok(None),
        },
    };
    if !reveal_canvas_points(view, surface_size, &[anchor])? {
        return Ok(None);
    }
    port_point(view, port)
}

fn edge_owner_canvas_centers(
    view: &WeakViewSession,
    edge: EdgeId,
) -> Result<[Option<CanvasPoint>; 2], &'static str> {
    let graph = view
        .graph_snapshot()
        .map_err(|_| "input_graph_snapshot_failed")?;
    let Some(edge) = graph.graph().edge(&edge) else {
        return Ok([None, None]);
    };
    Ok([
        port_owner_canvas_center(view, edge.from)?,
        port_owner_canvas_center(view, edge.to)?,
    ])
}

pub(super) fn edge_owner_reveal_anchor(
    view: &WeakViewSession,
    edge: EdgeId,
    surface_size: ViewSize,
) -> Result<Option<CanvasPoint>, &'static str> {
    let [from, to] = edge_owner_canvas_centers(view, edge)?;
    Ok(match (from, to) {
        (Some(from), Some(to)) => {
            let transform = view
                .viewport_transform()
                .map_err(|_| "input_viewport_read_failed")?;
            let viewport_center = transform
                .canvas_point_at_screen(ViewPoint {
                    x: surface_size.width * 0.5,
                    y: surface_size.height * 0.5,
                })
                .ok_or("input_viewport_read_failed")?;
            let distance_squared = |point: CanvasPoint| {
                (point.x - viewport_center.x).powi(2) + (point.y - viewport_center.y).powi(2)
            };
            Some(if distance_squared(from) <= distance_squared(to) {
                from
            } else {
                to
            })
        }
        (Some(point), None) | (None, Some(point)) => Some(point),
        (None, None) => None,
    })
}

fn edge_candidate_axis_position(current: f32, delta: f32, extent: f32) -> Option<f32> {
    let lower = INPUT_SAFE_AREA_IN_VIEW.max(-delta);
    let upper = (extent - INPUT_SAFE_AREA_IN_VIEW).min(extent - delta);
    (lower <= upper).then(|| current.clamp(lower, upper))
}

fn reveal_edge_candidate_with_owner(
    view: &WeakViewSession,
    surface_size: ViewSize,
    canvas_point: CanvasPoint,
    owner_centers: [Option<CanvasPoint>; 2],
) -> Result<EdgeCandidateViewport, &'static str> {
    let transform = view
        .viewport_transform()
        .map_err(|_| "input_viewport_read_failed")?;
    let current = transform
        .screen_point_for_canvas(canvas_point)
        .ok_or("input_edge_projection_failed")?;
    if input_point_is_safe(current, surface_size) {
        return Ok(EdgeCandidateViewport::Ready(current));
    }

    let mut plans = owner_centers
        .into_iter()
        .flatten()
        .filter_map(|owner| {
            let delta_x = (owner.x - canvas_point.x) * transform.zoom;
            let delta_y = (owner.y - canvas_point.y) * transform.zoom;
            let x = edge_candidate_axis_position(current.x, delta_x, surface_size.width)?;
            let y = edge_candidate_axis_position(current.y, delta_y, surface_size.height)?;
            let pan = CanvasPoint {
                x: x / transform.zoom - canvas_point.x,
                y: y / transform.zoom - canvas_point.y,
            };
            let dx = pan.x - transform.pan.x;
            let dy = pan.y - transform.pan.y;
            Some((pan, dx.mul_add(dx, dy * dy)))
        })
        .collect::<Vec<_>>();
    plans.sort_by(|left, right| left.1.total_cmp(&right.1));
    let Some((pan, _)) = plans.first().copied() else {
        return Ok(EdgeCandidateViewport::Unavailable);
    };
    view.set_viewport(
        ViewportTransform::new(pan, transform.zoom).ok_or("input_reveal_viewport_failed")?,
    )
    .map_err(|_| "input_reveal_viewport_failed")?;
    Ok(EdgeCandidateViewport::AwaitFrame)
}

fn edge_candidate_intersects_control(
    geometry: &GeometrySnapshot,
    point: CanvasPoint,
    clearance: f32,
) -> bool {
    geometry.nodes().into_iter().any(|(_, node)| {
        let bounds = node.bounds();
        point.x >= bounds.origin.x - clearance
            && point.x <= bounds.origin.x + bounds.size.width + clearance
            && point.y >= bounds.origin.y - clearance
            && point.y <= bounds.origin.y + bounds.size.height + clearance
    }) || geometry.ports().into_iter().any(|(_, port)| {
        port.hit_bounds().is_some_and(|bounds| {
            point.x >= bounds.origin.x - clearance
                && point.x <= bounds.origin.x + bounds.size.width + clearance
                && point.y >= bounds.origin.y - clearance
                && point.y <= bounds.origin.y + bounds.size.height + clearance
        })
    })
}

fn unique_edge_candidate_margin<'a>(
    target: &EdgePath,
    others: impl IntoIterator<Item = &'a EdgePath>,
    point: CanvasPoint,
    options: EdgeHitTestOptions,
    minimum_margin: f32,
) -> Result<Option<f32>, EdgePathFlattenError> {
    let Some(target_distance) = edge_path_distance(target, point, options)? else {
        return Ok(None);
    };
    if target_distance > options.flatten.max_error {
        return Ok(None);
    }
    let nearest_other = others.into_iter().try_fold(
        f32::INFINITY,
        |nearest, path| -> Result<_, EdgePathFlattenError> {
            Ok(nearest.min(edge_path_distance(path, point, options)?.unwrap_or(f32::INFINITY)))
        },
    )?;
    let margin = nearest_other - target_distance;
    Ok((margin >= minimum_margin).then_some(margin))
}

pub(super) fn visible_unique_edge_point(
    view: &WeakViewSession,
    edge: EdgeId,
    surface_size: ViewSize,
    preferred: Option<CanvasPoint>,
) -> Result<VisibleEdgePickPoint, &'static str> {
    let geometry = view
        .geometry_snapshot()
        .map_err(|_| "geometry_snapshot_read_failed")?;
    let transform = view
        .viewport_transform()
        .map_err(|_| "input_viewport_read_failed")?;
    let options = EdgeHitTestOptions::default()
        .for_zoom(transform.zoom)
        .ok_or("input_edge_pick_options_invalid")?;
    let Some(target) = geometry.route(edge) else {
        let Some(canvas_point) = edge_owner_reveal_anchor(view, edge, surface_size)? else {
            return Ok(VisibleEdgePickPoint::Unavailable);
        };
        if !reveal_canvas_points(view, surface_size, &[canvas_point])? {
            return Ok(VisibleEdgePickPoint::AwaitFrame(canvas_point));
        }
        return Ok(VisibleEdgePickPoint::Unavailable);
    };
    let owner_centers = edge_owner_canvas_centers(view, edge)?;
    let control_clearance = EDGE_PICK_CONTROL_CLEARANCE_IN_VIEW / transform.zoom;
    let minimum_margin =
        (2.0 * options.flatten.max_error).max(EDGE_PICK_UNIQUENESS_MARGIN_IN_VIEW / transform.zoom);
    if let Some(canvas_point) = preferred {
        let is_current =
            !edge_candidate_intersects_control(&geometry, canvas_point, control_clearance)
                && unique_edge_candidate_margin(
                    &target.facts().path,
                    geometry
                        .routes()
                        .into_iter()
                        .filter(|(candidate, _)| **candidate != edge)
                        .map(|(_, route)| &route.facts().path),
                    canvas_point,
                    options,
                    minimum_margin,
                )
                .map_err(|_| "input_edge_distance_failed")?
                .is_some();
        if is_current {
            match reveal_edge_candidate_with_owner(view, surface_size, canvas_point, owner_centers)?
            {
                EdgeCandidateViewport::Ready(point) => {
                    return Ok(VisibleEdgePickPoint::Ready(point));
                }
                EdgeCandidateViewport::AwaitFrame => {
                    return Ok(VisibleEdgePickPoint::AwaitFrame(canvas_point));
                }
                EdgeCandidateViewport::Unavailable => {}
            }
        }
    }
    let mut candidates = Vec::new();
    for index in 1..EDGE_PICK_SAMPLE_COUNT {
        let progress = f32::from(index) / f32::from(EDGE_PICK_SAMPLE_COUNT);
        if !(EDGE_PICK_ENDPOINT_INSET..=1.0 - EDGE_PICK_ENDPOINT_INSET).contains(&progress) {
            continue;
        }
        let sample = target
            .facts()
            .path
            .sample_with_options(EdgePathAnchor::Progress(progress), options.flatten)
            .map_err(|_| "input_edge_sample_failed")?;
        if edge_candidate_intersects_control(&geometry, sample.point, control_clearance) {
            continue;
        }
        let margin = unique_edge_candidate_margin(
            &target.facts().path,
            geometry
                .routes()
                .into_iter()
                .filter(|(candidate, _)| **candidate != edge)
                .map(|(_, route)| &route.facts().path),
            sample.point,
            options,
            minimum_margin,
        )
        .map_err(|_| "input_edge_distance_failed")?;
        if let Some(margin) = margin {
            candidates.push((sample.point, margin));
        }
    }
    candidates.sort_by(|left, right| right.1.total_cmp(&left.1));
    for (canvas_point, _) in &candidates {
        let Some(point) = transform.screen_point_for_canvas(*canvas_point) else {
            continue;
        };
        if input_point_is_safe(point, surface_size) {
            return Ok(VisibleEdgePickPoint::Ready(point));
        }
    }
    for (canvas_point, _) in candidates {
        match reveal_edge_candidate_with_owner(view, surface_size, canvas_point, owner_centers)? {
            EdgeCandidateViewport::Ready(point) => {
                return Ok(VisibleEdgePickPoint::Ready(point));
            }
            EdgeCandidateViewport::AwaitFrame => {
                return Ok(VisibleEdgePickPoint::AwaitFrame(canvas_point));
            }
            EdgeCandidateViewport::Unavailable => {}
        }
    }
    Ok(VisibleEdgePickPoint::Unavailable)
}

pub(super) fn visible_reconnect_grip(
    view: &WeakViewSession,
    edge: EdgeId,
    endpoint: EdgeEndpoint,
    surface_size: ViewSize,
) -> Result<Option<ViewPoint>, &'static str> {
    let Some(edge_canvas) = reconnect_grip_canvas_point(view, edge, endpoint)? else {
        return Ok(None);
    };
    if !reveal_canvas_points(view, surface_size, &[edge_canvas])? {
        return Ok(None);
    }
    let transform = view
        .viewport_transform()
        .map_err(|_| "input_viewport_read_failed")?;
    Ok(transform.screen_point_for_canvas(edge_canvas))
}

fn reconnect_grip_canvas_point(
    view: &WeakViewSession,
    edge: EdgeId,
    endpoint: EdgeEndpoint,
) -> Result<Option<CanvasPoint>, &'static str> {
    let geometry = view
        .geometry_snapshot()
        .map_err(|_| "geometry_snapshot_read_failed")?;
    let Some(route) = geometry.route(edge) else {
        return Ok(None);
    };
    let position = match endpoint {
        EdgeEndpoint::From => route.facts().endpoints.source,
        EdgeEndpoint::To => route.facts().endpoints.target,
    };
    let zoom = view
        .viewport_transform()
        .map_err(|_| "input_viewport_read_failed")?
        .zoom;
    let offset = RECONNECT_GRIP_OFFSET_IN_VIEW / zoom;
    let direction = match position.position {
        HandlePosition::Top => CanvasPoint { x: 0.0, y: -1.0 },
        HandlePosition::Right => CanvasPoint { x: 1.0, y: 0.0 },
        HandlePosition::Bottom => CanvasPoint { x: 0.0, y: 1.0 },
        HandlePosition::Left => CanvasPoint { x: -1.0, y: 0.0 },
    };
    Ok(Some(CanvasPoint {
        x: position.point.x + direction.x * offset,
        y: position.point.y + direction.y * offset,
    }))
}

fn port_canvas_point(
    view: &WeakViewSession,
    port: PortId,
) -> Result<Option<CanvasPoint>, &'static str> {
    let geometry = view
        .geometry_snapshot()
        .map_err(|_| "geometry_snapshot_read_failed")?;
    Ok(geometry
        .port_outcome(port)
        .and_then(PublishedPortOutcome::rendered)
        .map(|port| port.route_anchor()))
}

pub(super) fn reveal_canvas_points(
    view: &WeakViewSession,
    surface_size: ViewSize,
    points: &[CanvasPoint],
) -> Result<bool, &'static str> {
    if points.is_empty() || !surface_size.is_positive_finite() {
        return Err("input_reveal_plan_invalid");
    }
    let transform = view
        .viewport_transform()
        .map_err(|_| "input_viewport_read_failed")?;
    let projected = points
        .iter()
        .map(|point| transform.screen_point_for_canvas(*point))
        .collect::<Option<Vec<_>>>()
        .ok_or("input_reveal_projection_failed")?;
    let visible = projected
        .iter()
        .all(|point| input_point_is_safe(ViewPoint::new(point.x, point.y), surface_size));
    if visible {
        return Ok(true);
    }

    let (min_x, max_x, min_y, max_y) = points.iter().fold(
        (
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ),
        |(min_x, max_x, min_y, max_y), point| {
            (
                min_x.min(point.x),
                max_x.max(point.x),
                min_y.min(point.y),
                max_y.max(point.y),
            )
        },
    );
    let available_width = surface_size.width - INPUT_SAFE_AREA_IN_VIEW * 2.0;
    let available_height = surface_size.height - INPUT_SAFE_AREA_IN_VIEW * 2.0;
    if (max_x - min_x) * transform.zoom > available_width
        || (max_y - min_y) * transform.zoom > available_height
    {
        return Err("input_reveal_span_exceeds_surface");
    }
    let canvas_center = CanvasPoint {
        x: (min_x + max_x) * 0.5,
        y: (min_y + max_y) * 0.5,
    };
    let view_center = CanvasPoint {
        x: surface_size.width * 0.5,
        y: surface_size.height * 0.5,
    };
    let pan = CanvasPoint {
        x: view_center.x / transform.zoom - canvas_center.x,
        y: view_center.y / transform.zoom - canvas_center.y,
    };
    view.set_viewport(
        ViewportTransform::new(pan, transform.zoom).ok_or("input_reveal_viewport_failed")?,
    )
    .map_err(|_| "input_reveal_viewport_failed")?;
    Ok(false)
}

pub(super) fn input_point_is_safe(point: ViewPoint, surface_size: ViewSize) -> bool {
    surface_size.is_positive_finite()
        && point.x.is_finite()
        && point.y.is_finite()
        && point.x >= INPUT_SAFE_AREA_IN_VIEW
        && point.y >= INPUT_SAFE_AREA_IN_VIEW
        && point.x <= surface_size.width - INPUT_SAFE_AREA_IN_VIEW
        && point.y <= surface_size.height - INPUT_SAFE_AREA_IN_VIEW
}

/// Places the synthetic pointer in each auto-pan band needed to reveal the target.
/// A target can be outside both axes at once, so preserving a safe coordinate on the
/// secondary axis would make a diagonal target permanently unreachable.
pub(super) fn auto_pan_pointer_toward(
    target: ViewPoint,
    surface_size: ViewSize,
) -> Option<ViewPoint> {
    if !surface_size.is_positive_finite() || !target.x.is_finite() || !target.y.is_finite() {
        return None;
    }
    let x = auto_pan_axis_pointer(target.x, surface_size.width);
    let y = auto_pan_axis_pointer(target.y, surface_size.height);
    if x != target.x || y != target.y {
        return Some(ViewPoint::new(x, y));
    }

    // The interaction scenario is also the native auto-pan witness. When the
    // target already fits, deliberately enter the edge band on its dominant
    // axis so layout changes cannot silently remove that coverage.
    let normalized_x = (target.x - surface_size.width * 0.5).abs() / surface_size.width;
    let normalized_y = (target.y - surface_size.height * 0.5).abs() / surface_size.height;
    if normalized_x >= normalized_y {
        Some(ViewPoint::new(
            auto_pan_edge_for_direction(target.x, surface_size.width),
            target.y,
        ))
    } else {
        Some(ViewPoint::new(
            target.x,
            auto_pan_edge_for_direction(target.y, surface_size.height),
        ))
    }
}

fn auto_pan_axis_pointer(target: f32, extent: f32) -> f32 {
    if target < INPUT_SAFE_AREA_IN_VIEW {
        AUTOPAN_EDGE_INSET_IN_VIEW
    } else if target > extent - INPUT_SAFE_AREA_IN_VIEW {
        extent - AUTOPAN_EDGE_INSET_IN_VIEW
    } else {
        target
    }
}

fn auto_pan_edge_for_direction(target: f32, extent: f32) -> f32 {
    if target < extent * 0.5 {
        AUTOPAN_EDGE_INSET_IN_VIEW
    } else {
        extent - AUTOPAN_EDGE_INSET_IN_VIEW
    }
}

/// Moves a candidate target out of the default auto-pan band without losing the default
/// connection probe. The visual port can be rendered for only one edge frame while the drag
/// continues, so waiting for it to become fully interior is not reliable.
pub(super) fn settled_connection_target_point(
    target: ViewPoint,
    surface_size: ViewSize,
) -> Option<ViewPoint> {
    if !surface_size.is_positive_finite() || !target.x.is_finite() || !target.y.is_finite() {
        return None;
    }
    let settled = ViewPoint::new(
        target.x.clamp(
            CONNECTION_AUTOPAN_SETTLE_MARGIN_IN_VIEW,
            surface_size.width - CONNECTION_AUTOPAN_SETTLE_MARGIN_IN_VIEW,
        ),
        target.y.clamp(
            CONNECTION_AUTOPAN_SETTLE_MARGIN_IN_VIEW,
            surface_size.height - CONNECTION_AUTOPAN_SETTLE_MARGIN_IN_VIEW,
        ),
    );
    let dx = target.x - settled.x;
    let dy = target.y - settled.y;
    (dx.mul_add(dx, dy * dy)
        <= CONNECTION_AUTOPAN_MAX_TARGET_OFFSET_IN_VIEW
            * CONNECTION_AUTOPAN_MAX_TARGET_OFFSET_IN_VIEW)
        .then_some(settled)
}

pub(super) fn position_node_for_selection(
    view: &WeakViewSession,
    node: NodeId,
    surface_size: ViewSize,
) -> Result<(), &'static str> {
    let geometry = view
        .geometry_snapshot()
        .map_err(|_| "geometry_snapshot_read_failed")?;
    let bounds = geometry
        .node(node)
        .filter(|node| node.is_fresh())
        .ok_or("input_selection_node_geometry_missing")?
        .bounds();
    let transform = view
        .viewport_transform()
        .map_err(|_| "input_viewport_read_failed")?;
    let canvas_anchor = CanvasPoint {
        x: bounds.origin.x + bounds.size.width * 0.5,
        y: bounds.origin.y + bounds.size.height,
    };
    let desired = CanvasPoint {
        x: surface_size.width * 0.5,
        y: surface_size.height * 0.62,
    };
    let pan = CanvasPoint {
        x: desired.x / transform.zoom - canvas_anchor.x,
        y: desired.y / transform.zoom - canvas_anchor.y,
    };
    view.set_viewport(
        ViewportTransform::new(pan, transform.zoom).ok_or("input_selection_reframe_failed")?,
    )
    .map(|_| ())
    .map_err(|_| "input_selection_reframe_failed")
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
    let node_bounds = geometry
        .nodes()
        .values()
        .map(|node| projected_node_bounds(node.bounds(), transform))
        .collect::<Option<Vec<_>>>()
        .ok_or("input_blank_surface_node_projection_failed")?;
    let port_points = geometry
        .port_outcomes()
        .values()
        .filter_map(|outcome| outcome.rendered())
        .map(|port| transform.screen_point_for_canvas(port.route_anchor()))
        .collect::<Option<Vec<_>>>()
        .ok_or("input_blank_surface_port_projection_failed")?;
    Ok(blank_surface_point_from_obstacles(
        surface_size,
        &node_bounds,
        &port_points,
    ))
}

fn projected_node_bounds(bounds: CanvasRect, transform: ViewportTransform) -> Option<ViewRect> {
    if !bounds.is_positive_finite() {
        return None;
    }
    let origin = transform.screen_point_for_canvas(bounds.origin)?;
    let bottom_right = transform.screen_point_for_canvas(CanvasPoint {
        x: bounds.origin.x + bounds.size.width,
        y: bounds.origin.y + bounds.size.height,
    })?;
    let projected = ViewRect::new(
        ViewPoint::new(origin.x, origin.y),
        ViewSize::new(bottom_right.x - origin.x, bottom_right.y - origin.y),
    );
    projected.is_positive_finite().then_some(projected)
}

fn blank_surface_point_from_obstacles(
    surface_size: ViewSize,
    node_bounds: &[ViewRect],
    port_points: &[ViewPoint],
) -> Option<ViewPoint> {
    if !surface_size.is_positive_finite() {
        return None;
    }
    let mut y = BLANK_SURFACE_GRID_IN_VIEW;
    while y <= surface_size.height - BLANK_SURFACE_GRID_IN_VIEW {
        let mut x = BLANK_SURFACE_GRID_IN_VIEW;
        while x <= surface_size.width - BLANK_SURFACE_GRID_IN_VIEW {
            let candidate = ViewPoint::new(x, y);
            let inside_node = node_bounds.iter().any(|bounds| {
                candidate.x >= bounds.origin.x
                    && candidate.x <= bounds.origin.x + bounds.size.width
                    && candidate.y >= bounds.origin.y
                    && candidate.y <= bounds.origin.y + bounds.size.height
            });
            let near_port = port_points.iter().any(|port| {
                let dx = port.x - candidate.x;
                let dy = port.y - candidate.y;
                dx.mul_add(dx, dy * dy)
                    < BLANK_SURFACE_PORT_CLEARANCE_IN_VIEW * BLANK_SURFACE_PORT_CLEARANCE_IN_VIEW
            });
            if !inside_node && !near_port {
                return Some(candidate);
            }
            x += BLANK_SURFACE_GRID_IN_VIEW;
        }
        y += BLANK_SURFACE_GRID_IN_VIEW;
    }
    None
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

#[cfg(test)]
mod tests {
    use super::*;
    use jellyflow_core::{CanvasRect, CanvasSize};
    use jellyflow_runtime::geometry::{EdgePathLabel, PathCommand};

    fn horizontal_path(y: f32) -> EdgePath {
        EdgePath {
            commands: vec![
                PathCommand::MoveTo(CanvasPoint { x: 0.0, y }),
                PathCommand::LineTo(CanvasPoint { x: 100.0, y }),
            ],
            label: EdgePathLabel {
                point: CanvasPoint { x: 50.0, y },
                offset_x: 0.0,
                offset_y: 0.0,
            },
        }
    }

    #[test]
    fn edge_pick_planning_rejects_nearly_overlapping_routes() {
        let target = horizontal_path(0.0);
        let overlapping = horizontal_path(0.5);

        assert_eq!(
            unique_edge_candidate_margin(
                &target,
                [&overlapping],
                CanvasPoint { x: 50.0, y: 0.0 },
                EdgeHitTestOptions::default(),
                EDGE_PICK_UNIQUENESS_MARGIN_IN_VIEW,
            )
            .expect("valid routes"),
            None,
        );
    }

    #[test]
    fn edge_pick_planning_accepts_a_route_with_stable_separation() {
        let target = horizontal_path(0.0);
        let separated = horizontal_path(4.0);

        let margin = unique_edge_candidate_margin(
            &target,
            [&separated],
            CanvasPoint { x: 50.0, y: 0.0 },
            EdgeHitTestOptions::default(),
            EDGE_PICK_UNIQUENESS_MARGIN_IN_VIEW,
        )
        .expect("valid routes")
        .expect("stable unique candidate");
        assert!((margin - 4.0).abs() <= f32::EPSILON);
    }

    #[test]
    fn edge_pick_planning_invalidates_a_cached_point_after_the_route_moves() {
        let moved_target = horizontal_path(20.0);
        let route_at_cached_point = horizontal_path(0.0);

        assert_eq!(
            unique_edge_candidate_margin(
                &moved_target,
                [&route_at_cached_point],
                CanvasPoint { x: 50.0, y: 0.0 },
                EdgeHitTestOptions::default(),
                EDGE_PICK_UNIQUENESS_MARGIN_IN_VIEW,
            )
            .expect("valid routes"),
            None,
        );
    }

    #[test]
    fn node_bounds_project_into_view_space_for_blank_surface_planning() {
        let transform = ViewportTransform::new(CanvasPoint { x: -20.0, y: 10.0 }, 2.0)
            .expect("valid transform");
        let bounds = CanvasRect {
            origin: CanvasPoint { x: 100.0, y: 50.0 },
            size: CanvasSize {
                width: 200.0,
                height: 100.0,
            },
        };

        assert_eq!(
            projected_node_bounds(bounds, transform),
            Some(ViewRect::new(
                ViewPoint::new(160.0, 120.0),
                ViewSize::new(400.0, 200.0),
            ))
        );
    }

    #[test]
    fn blank_surface_planning_avoids_node_bounds_and_port_clearance() {
        let node_bounds = [ViewRect::new(
            ViewPoint::new(0.0, 0.0),
            ViewSize::new(80.0, 80.0),
        )];
        let port_points = [ViewPoint::new(140.0, 40.0)];

        assert_eq!(
            blank_surface_point_from_obstacles(
                ViewSize::new(240.0, 160.0),
                &node_bounds,
                &port_points,
            ),
            Some(ViewPoint::new(200.0, 40.0))
        );
    }
}
