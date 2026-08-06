use std::collections::{BTreeMap, BTreeSet};

use jellyflow_core::{CanvasPoint, CanvasRect, EdgeId, NodeId, PortId};
use jellyflow_runtime::geometry::{ViewPoint, ViewRect};
use jellyflow_runtime::interaction::CanvasInteractionPhase;
use jellyflow_runtime::viewport::ViewportTransform;

use super::SmokeScenario;

const REQUIRED_LOD_FRAMES: usize = 3;
const NODE_CHROME_ALIGNMENT_TOLERANCE_IN_VIEW: f32 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SmokeFrameStage {
    Interaction,
    NodeDragPending,
    NodeDragAuthoritative,
    ConnectionExploring,
    ConnectionPending,
    ConnectionAuthoritative,
    ReconnectExploring,
    ReconnectPending,
    ReconnectAuthoritative,
    Search,
    Lod,
    Final,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct SmokeNodeBounds {
    pub(super) node: NodeId,
    pub(super) bounds: ViewRect,
    pub(super) selected: bool,
    pub(super) chrome_expected: bool,
    pub(super) chrome_bounds: Option<ViewRect>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct SmokeRoute {
    pub(super) edge: EdgeId,
    pub(super) source: ViewPoint,
    pub(super) target: ViewPoint,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct SmokePortGeometry {
    pub(super) node: NodeId,
    pub(super) port: PortId,
    pub(super) visual_bounds: ViewRect,
    pub(super) hit_bounds: ViewRect,
    pub(super) interaction_bounds: Option<ViewRect>,
    pub(super) route_anchor: ViewPoint,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct SmokePreviewRoute {
    pub(super) source: ViewPoint,
    pub(super) target: ViewPoint,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct SmokePreviewTarget {
    pub(super) port: PortId,
    pub(super) anchor: ViewPoint,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct SmokePresentedFrame {
    pub(super) render_sequence: u64,
    pub(super) stage: SmokeFrameStage,
    pub(super) phase: CanvasInteractionPhase,
    pub(super) graph_revision: u64,
    pub(super) viewport: ViewportTransform,
    pub(super) node_bounds: Vec<SmokeNodeBounds>,
    pub(super) port_geometries: Vec<SmokePortGeometry>,
    pub(super) node_drag_nodes: Vec<NodeId>,
    pub(super) preview_route: Option<SmokePreviewRoute>,
    pub(super) preview_target: Option<SmokePreviewTarget>,
    pub(super) suppressed_edge: Option<EdgeId>,
    pub(super) authoritative_routes: Vec<SmokeRoute>,
}

pub(super) fn validate_presented_frames(
    frames: &[SmokePresentedFrame],
    scenario: SmokeScenario,
) -> Result<(), &'static str> {
    if frames.is_empty() {
        return Err("presented_frames_missing");
    }

    let mut previous_revision = 0;
    let mut previous_render_sequence: Option<u64> = None;
    let mut saw_connection_pending = false;
    let mut saw_connection_authoritative = false;
    let mut saw_reconnect_pending = false;
    let mut saw_reconnect_authoritative = false;
    let mut saw_node_drag_pending = false;
    let mut saw_node_drag_authoritative = false;
    let mut saw_search = false;
    let mut saw_final = false;
    let mut lod_frames = 0usize;
    let mut connection_pending_routes = None;
    let mut reconnect_pending_routes = None;
    let mut node_drag_pending = None;

    for (index, frame) in frames.iter().enumerate() {
        let invalid_render_sequence = previous_render_sequence.is_some_and(|previous| {
            previous
                .checked_add(1)
                .is_none_or(|next| frame.render_sequence != next)
        });
        if frame.graph_revision < previous_revision
            || invalid_render_sequence
            || !frame.viewport.is_valid()
        {
            return Err("presented_frame_order_invalid");
        }
        previous_revision = frame.graph_revision;
        previous_render_sequence = Some(frame.render_sequence);
        if !matches!(
            frame.stage,
            SmokeFrameStage::ConnectionPending | SmokeFrameStage::ConnectionAuthoritative
        ) && connection_pending_routes
            .take()
            .is_some_and(|pending: PendingRouteSwap| pending.committing)
        {
            return Err("connection_commit_span_interrupted");
        }
        if !matches!(
            frame.stage,
            SmokeFrameStage::ReconnectPending | SmokeFrameStage::ReconnectAuthoritative
        ) && reconnect_pending_routes
            .take()
            .is_some_and(|pending: PendingRouteSwap| pending.committing)
        {
            return Err("reconnect_commit_span_interrupted");
        }
        if node_drag_pending.is_some()
            && !matches!(
                frame.stage,
                SmokeFrameStage::NodeDragPending | SmokeFrameStage::NodeDragAuthoritative
            )
        {
            return Err("node_drag_settlement_span_interrupted");
        }

        validate_node_bounds(&frame.node_bounds)?;
        validate_node_drag_nodes(&frame.node_bounds, &frame.node_drag_nodes)?;
        validate_routes(&frame.authoritative_routes)?;
        if let Some(route) = frame.preview_route
            && (!route.source.is_finite() || !route.target.is_finite())
        {
            return Err("preview_route_invalid");
        }
        if let Some(target) = frame.preview_target
            && !target.anchor.is_finite()
        {
            return Err("preview_target_invalid");
        }

        match frame.stage {
            SmokeFrameStage::NodeDragPending => {
                validate_authoritative_only(frame)?;
                node_drag_pending = Some(PendingNodeDrag::from_frame(frame)?);
                saw_node_drag_pending = true;
            }
            SmokeFrameStage::NodeDragAuthoritative => {
                validate_authoritative_only(frame)?;
                let pending = node_drag_pending.take();
                if saw_node_drag_authoritative
                    || !pending
                        .as_ref()
                        .is_some_and(|pending| pending.aligns_with(frame))
                {
                    return Err("node_drag_release_jump");
                }
                saw_node_drag_authoritative = true;
            }
            SmokeFrameStage::ConnectionExploring => validate_exploring_frame(frame, false)?,
            SmokeFrameStage::ConnectionPending => {
                if saw_connection_authoritative
                    || frame.preview_route.is_none()
                    || frame.preview_target.is_none()
                    || frame.suppressed_edge.is_some()
                {
                    return Err("connection_pending_preview_missing");
                }
                validate_preview_target_alignment(frame)?;
                match connection_pending_routes.as_mut() {
                    Some(pending) => pending.observe(frame, None)?,
                    None => {
                        connection_pending_routes =
                            Some(PendingRouteSwap::from_frame(frame, None)?);
                    }
                }
                saw_connection_pending = true;
            }
            SmokeFrameStage::ConnectionAuthoritative => {
                let pending = connection_pending_routes.take();
                if frame.preview_route.is_some()
                    || frame.preview_target.is_some()
                    || frame.suppressed_edge.is_some()
                    || !atomic_route_addition(pending.as_ref(), frame)
                {
                    return Err("connection_route_swap_not_atomic");
                }
                saw_connection_authoritative = true;
            }
            SmokeFrameStage::ReconnectExploring => validate_exploring_frame(frame, true)?,
            SmokeFrameStage::ReconnectPending => {
                if saw_reconnect_authoritative
                    || frame.preview_route.is_none()
                    || frame.preview_target.is_none()
                    || frame.suppressed_edge.is_none()
                {
                    return Err("reconnect_pending_preview_missing");
                }
                validate_preview_target_alignment(frame)?;
                let expected_edge = frame.suppressed_edge;
                match reconnect_pending_routes.as_mut() {
                    Some(pending) => pending.observe(frame, expected_edge)?,
                    None => {
                        reconnect_pending_routes =
                            Some(PendingRouteSwap::from_frame(frame, expected_edge)?);
                    }
                }
                saw_reconnect_pending = true;
            }
            SmokeFrameStage::ReconnectAuthoritative => {
                let pending = reconnect_pending_routes.take();
                if frame.preview_route.is_some()
                    || frame.preview_target.is_some()
                    || frame.suppressed_edge.is_some()
                    || !atomic_route_addition(pending.as_ref(), frame)
                {
                    return Err("reconnect_route_swap_not_atomic");
                }
                saw_reconnect_authoritative = true;
            }
            SmokeFrameStage::Search => {
                if frame.suppressed_edge.is_some() || frame.preview_target.is_some() {
                    return Err("search_frame_has_connection_target");
                }
                saw_search = true;
            }
            SmokeFrameStage::Final => {
                validate_authoritative_only(frame)?;
                if index + 1 != frames.len() {
                    return Err("final_presented_frame_not_last");
                }
                saw_final = true;
            }
            SmokeFrameStage::Lod => {
                validate_authoritative_only(frame)?;
                lod_frames = lod_frames.saturating_add(1);
            }
            SmokeFrameStage::Interaction => validate_authoritative_only(frame)?,
        }
    }

    match scenario {
        SmokeScenario::Interaction
            if !saw_connection_pending
                || !saw_connection_authoritative
                || !saw_reconnect_pending
                || !saw_reconnect_authoritative
                || !saw_node_drag_pending
                || !saw_node_drag_authoritative
                || !saw_search
                || !saw_final =>
        {
            Err("interaction_presented_frames_incomplete")
        }
        SmokeScenario::LodThreshold if lod_frames < REQUIRED_LOD_FRAMES => {
            Err("lod_presented_frames_incomplete")
        }
        SmokeScenario::Interaction | SmokeScenario::LodThreshold => Ok(()),
    }
}

pub(super) fn validate_port_geometries(ports: &[SmokePortGeometry]) -> Result<(), &'static str> {
    let mut identities = BTreeSet::new();
    for port in ports {
        if !identities.insert((port.node, port.port)) {
            return Err("presented_port_geometry_duplicated");
        }
        if !port.visual_bounds.is_positive_finite()
            || !port.hit_bounds.is_positive_finite()
            || port
                .interaction_bounds
                .is_some_and(|bounds| !bounds.is_positive_finite())
            || !port.route_anchor.x.is_finite()
            || !port.route_anchor.y.is_finite()
        {
            return Err("presented_port_geometry_invalid");
        }
    }
    Ok(())
}

#[derive(Clone)]
struct PendingRouteSwap {
    routes: BTreeMap<EdgeId, CanvasRoute>,
    preview: CanvasPreviewRoute,
    target: CanvasPreviewTarget,
    expected_edge: Option<EdgeId>,
    graph_revision: u64,
    zoom: f32,
    committing: bool,
}

impl PendingRouteSwap {
    fn from_frame(
        frame: &SmokePresentedFrame,
        expected_edge: Option<EdgeId>,
    ) -> Result<Self, &'static str> {
        let routes = canvas_routes(frame)?;
        if expected_edge.is_some_and(|edge| routes.contains_key(&edge)) {
            return Err("reconnect_pending_edge_not_suppressed");
        }
        let preview = canvas_preview_route(
            frame
                .preview_route
                .expect("pending frame preview presence was validated"),
            frame.viewport,
        )
        .ok_or("pending_preview_projection_invalid")?;
        let target = frame
            .preview_target
            .expect("pending frame target presence was validated");
        let target = CanvasPreviewTarget {
            port: target.port,
            anchor: target
                .anchor
                .to_canvas_point(frame.viewport)
                .ok_or("pending_target_projection_invalid")?,
        };
        Ok(Self {
            routes,
            preview,
            target,
            expected_edge,
            graph_revision: frame.graph_revision,
            zoom: frame.viewport.zoom,
            committing: frame.phase == CanvasInteractionPhase::Committing,
        })
    }

    fn observe(
        &mut self,
        frame: &SmokePresentedFrame,
        expected_edge: Option<EdgeId>,
    ) -> Result<(), &'static str> {
        let current = Self::from_frame(frame, expected_edge)?;
        if self.committing && !current.committing {
            return Err("committing_preview_regressed_to_editable");
        }
        if self.expected_edge != current.expected_edge || self.target.port != current.target.port {
            if self.committing {
                return Err("committing_preview_target_changed");
            }
            *self = current;
            return Ok(());
        }
        let zoom = self.zoom.max(current.zoom);
        if !points_align_at_zoom(self.target.anchor, current.target.anchor, zoom)
            || !route_endpoints_align_at_zoom(
                self.preview.source,
                self.preview.target,
                current.preview.source,
                current.preview.target,
                zoom,
            )
            || !route_maps_overlap_align_at_zoom(&self.routes, &current.routes, zoom)
        {
            return Err("pending_route_changed_between_frames");
        }
        *self = current;
        Ok(())
    }
}

fn atomic_route_addition(pending: Option<&PendingRouteSwap>, frame: &SmokePresentedFrame) -> bool {
    let Some(pending) = pending else {
        return false;
    };
    if frame.graph_revision <= pending.graph_revision {
        return false;
    }
    let Ok(authoritative_routes) = canvas_routes(frame) else {
        return false;
    };
    let zoom = pending.zoom.max(frame.viewport.zoom);
    if !route_maps_overlap_align_at_zoom(&pending.routes, &authoritative_routes, zoom) {
        return false;
    }
    authoritative_routes
        .values()
        .filter(|route| !pending.routes.contains_key(&route.edge))
        .filter(|route| {
            pending
                .expected_edge
                .is_none_or(|expected| route.edge == expected)
        })
        .filter(|route| {
            route_endpoints_align_at_zoom(
                pending.preview.source,
                pending.preview.target,
                route.source,
                route.target,
                zoom,
            )
        })
        .filter(|route| {
            points_align_at_zoom(pending.target.anchor, route.source, zoom)
                || points_align_at_zoom(pending.target.anchor, route.target, zoom)
        })
        .count()
        == 1
}

#[derive(Clone)]
struct PendingNodeDrag {
    node: NodeId,
    node_bounds: CanvasRect,
    routes: BTreeMap<EdgeId, CanvasRoute>,
    zoom: f32,
}

impl PendingNodeDrag {
    fn from_frame(frame: &SmokePresentedFrame) -> Result<Self, &'static str> {
        let [dragged_node] = frame.node_drag_nodes.as_slice() else {
            return Err("node_drag_cohort_count_invalid");
        };
        let node = frame
            .node_bounds
            .iter()
            .find(|node| node.node == *dragged_node)
            .ok_or("node_drag_owner_missing")?;
        Ok(Self {
            node: node.node,
            node_bounds: node
                .bounds
                .to_canvas_rect(frame.viewport)
                .ok_or("node_drag_projection_invalid")?,
            routes: canvas_routes(frame)?,
            zoom: frame.viewport.zoom,
        })
    }

    fn aligns_with(&self, frame: &SmokePresentedFrame) -> bool {
        let Some(node) = frame.node_bounds.iter().find(|node| node.node == self.node) else {
            return false;
        };
        let Some(node_bounds) = node.bounds.to_canvas_rect(frame.viewport) else {
            return false;
        };
        let Ok(routes) = canvas_routes(frame) else {
            return false;
        };
        let zoom = self.zoom.max(frame.viewport.zoom);
        rects_align_at_zoom(self.node_bounds, node_bounds, zoom)
            && route_maps_align_at_zoom(&self.routes, &routes, zoom)
    }
}

#[derive(Clone, Copy, Debug)]
struct CanvasRoute {
    edge: EdgeId,
    source: CanvasPoint,
    target: CanvasPoint,
}

#[derive(Clone, Copy)]
struct CanvasPreviewRoute {
    source: CanvasPoint,
    target: CanvasPoint,
}

#[derive(Clone, Copy)]
struct CanvasPreviewTarget {
    port: PortId,
    anchor: CanvasPoint,
}

fn canvas_routes(
    frame: &SmokePresentedFrame,
) -> Result<BTreeMap<EdgeId, CanvasRoute>, &'static str> {
    frame
        .authoritative_routes
        .iter()
        .map(|route| {
            canvas_route(*route, frame.viewport)
                .map(|route| (route.edge, route))
                .ok_or("authoritative_route_projection_invalid")
        })
        .collect()
}

fn canvas_route(route: SmokeRoute, viewport: ViewportTransform) -> Option<CanvasRoute> {
    Some(CanvasRoute {
        edge: route.edge,
        source: route.source.to_canvas_point(viewport)?,
        target: route.target.to_canvas_point(viewport)?,
    })
}

fn canvas_preview_route(
    route: SmokePreviewRoute,
    viewport: ViewportTransform,
) -> Option<CanvasPreviewRoute> {
    Some(CanvasPreviewRoute {
        source: route.source.to_canvas_point(viewport)?,
        target: route.target.to_canvas_point(viewport)?,
    })
}

fn route_maps_align_at_zoom(
    left: &BTreeMap<EdgeId, CanvasRoute>,
    right: &BTreeMap<EdgeId, CanvasRoute>,
    zoom: f32,
) -> bool {
    left.len() == right.len()
        && left.iter().all(|(edge, route)| {
            right.get(edge).is_some_and(|current| {
                route_endpoints_align_at_zoom(
                    route.source,
                    route.target,
                    current.source,
                    current.target,
                    zoom,
                )
            })
        })
}

fn route_maps_overlap_align_at_zoom(
    left: &BTreeMap<EdgeId, CanvasRoute>,
    right: &BTreeMap<EdgeId, CanvasRoute>,
    zoom: f32,
) -> bool {
    left.iter().all(|(edge, route)| {
        right.get(edge).is_none_or(|current| {
            route_endpoints_align_at_zoom(
                route.source,
                route.target,
                current.source,
                current.target,
                zoom,
            )
        })
    })
}

fn route_endpoints_align_at_zoom(
    left_source: CanvasPoint,
    left_target: CanvasPoint,
    right_source: CanvasPoint,
    right_target: CanvasPoint,
    zoom: f32,
) -> bool {
    (points_align_at_zoom(left_source, right_source, zoom)
        && points_align_at_zoom(left_target, right_target, zoom))
        || (points_align_at_zoom(left_source, right_target, zoom)
            && points_align_at_zoom(left_target, right_source, zoom))
}

fn points_align_at_zoom(left: CanvasPoint, right: CanvasPoint, zoom: f32) -> bool {
    (left.x - right.x).hypot(left.y - right.y) * zoom <= 0.5
}

fn rects_align_at_zoom(left: CanvasRect, right: CanvasRect, zoom: f32) -> bool {
    [
        (left.origin.x - right.origin.x).abs(),
        (left.origin.y - right.origin.y).abs(),
        (left.size.width - right.size.width).abs(),
        (left.size.height - right.size.height).abs(),
    ]
    .into_iter()
    .all(|difference| difference * zoom <= 0.5)
}

fn validate_authoritative_only(frame: &SmokePresentedFrame) -> Result<(), &'static str> {
    if frame.preview_route.is_some()
        || frame.preview_target.is_some()
        || frame.suppressed_edge.is_some()
    {
        return Err("authoritative_frame_contains_preview");
    }
    Ok(())
}

fn validate_exploring_frame(
    frame: &SmokePresentedFrame,
    reconnect: bool,
) -> Result<(), &'static str> {
    if frame.preview_route.is_none() || frame.preview_target.is_some() {
        return Err("exploring_preview_state_invalid");
    }
    match (reconnect, frame.suppressed_edge) {
        (false, None) => Ok(()),
        (true, Some(edge)) => {
            if frame
                .authoritative_routes
                .iter()
                .any(|route| route.edge == edge)
            {
                return Err("reconnect_exploring_edge_not_suppressed");
            }
            Ok(())
        }
        _ => Err("exploring_suppressed_edge_invalid"),
    }
}

fn validate_preview_target_alignment(frame: &SmokePresentedFrame) -> Result<(), &'static str> {
    let route = frame
        .preview_route
        .expect("pending frames checked preview route presence");
    let target = frame
        .preview_target
        .expect("pending frames checked preview target presence");
    let distance = |point: ViewPoint| (point.x - target.anchor.x).hypot(point.y - target.anchor.y);
    if distance(route.source).min(distance(route.target)) > 0.5 {
        return Err("preview_target_route_misaligned");
    }
    Ok(())
}

fn validate_node_bounds(bounds: &[SmokeNodeBounds]) -> Result<(), &'static str> {
    let mut nodes = BTreeSet::new();
    for entry in bounds {
        if !entry.bounds.is_positive_finite() || !nodes.insert(entry.node) {
            return Err("presented_node_bounds_invalid");
        }
        if entry.selected && !entry.chrome_expected {
            return Err("presented_selected_node_chrome_missing");
        }
        match (entry.chrome_expected, entry.chrome_bounds) {
            (true, Some(chrome_bounds))
                if chrome_bounds.is_positive_finite()
                    && view_rects_align(
                        entry.bounds,
                        chrome_bounds,
                        NODE_CHROME_ALIGNMENT_TOLERANCE_IN_VIEW,
                    ) => {}
            (true, Some(_)) => return Err("presented_node_chrome_misaligned"),
            (true, None) => return Err("presented_node_chrome_missing"),
            (false, Some(_)) => return Err("presented_node_chrome_unexpected"),
            (false, None) => {}
        }
    }
    Ok(())
}

fn view_rects_align(left: ViewRect, right: ViewRect, tolerance: f32) -> bool {
    [
        (left.origin.x - right.origin.x).abs(),
        (left.origin.y - right.origin.y).abs(),
        (left.size.width - right.size.width).abs(),
        (left.size.height - right.size.height).abs(),
    ]
    .into_iter()
    .all(|difference| difference <= tolerance)
}

fn validate_node_drag_nodes(
    nodes: &[SmokeNodeBounds],
    dragged: &[NodeId],
) -> Result<(), &'static str> {
    let mut unique = BTreeSet::new();
    if dragged
        .iter()
        .any(|node| !unique.insert(*node) || !nodes.iter().any(|bounds| bounds.node == *node))
    {
        return Err("node_drag_cohort_invalid");
    }
    Ok(())
}

fn validate_routes(routes: &[SmokeRoute]) -> Result<(), &'static str> {
    let mut edges = BTreeSet::new();
    if routes.iter().any(|route| {
        !edges.insert(route.edge) || !route.source.is_finite() || !route.target.is_finite()
    }) {
        return Err("authoritative_routes_invalid");
    }
    Ok(())
}
