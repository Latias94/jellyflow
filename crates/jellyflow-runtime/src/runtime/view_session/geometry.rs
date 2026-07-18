//! Per-view geometry publication, indexing, and route derivation.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use jellyflow_core::core::{
    CanvasPoint, CanvasRect, CanvasSize, EdgeId, Graph, Node, NodeId, NodeKindKey,
};

use crate::runtime::adapter_protocol::{
    FeedbackOperationReceipt, FrameBoundsSource, FrameRequest, GeometrySnapshot, LayoutNodeStatus,
    LayoutPublicationReceipt, PortLayoutOutcome, PublishedEdgeRoute, PublishedNodeGeometry,
    PublishedPortGeometry, PublishedPortOutcome, RendererStamp, SharedGeometryMap,
};
use crate::runtime::geometry::{
    CanvasBounds, EdgeEndpointPosition, EdgeInteractionFacts, EdgePosition, resolve_edge_route_path,
};
use crate::runtime::graph_runtime::{
    GraphSnapshot, IncidentEdgeSnapshot, IncidentSnapshotWork, NodeIncarnation,
};
use crate::runtime::policy::resolve_edge_policy;

#[cfg(test)]
use super::DerivedFactCounts;
use super::protocol::{ExpectedNodeLayout, PendingFrame, PendingLayout};
use super::{ViewSession, ViewSessionError, ViewShared, ViewState, WeakViewSession, ensure_open};

impl ViewSession {
    pub fn geometry_snapshot(&self) -> Result<GeometrySnapshot, ViewSessionError> {
        self.runtime()?;
        self.shared.geometry_snapshot()
    }

    pub fn nodes_intersecting(&self, bounds: CanvasRect) -> Result<Vec<NodeId>, ViewSessionError> {
        self.runtime()?;
        let state = self
            .shared
            .state
            .try_borrow()
            .map_err(|_| ViewSessionError::Busy)?;
        ensure_open(state.status)?;
        Ok(state
            .spatial
            .nodes_intersecting(bounds)
            .nodes
            .into_iter()
            .collect())
    }

    #[cfg(test)]
    pub(crate) fn seed_node_geometry_for_test(
        &self,
        node: NodeId,
        incarnation: NodeIncarnation,
        rect: CanvasRect,
    ) {
        let mut state = self.shared.state.borrow_mut();
        state.spatial.upsert(node, rect);
        state.upsert_node_geometry(
            node,
            PublishedNodeGeometry::new(
                node,
                incarnation,
                crate::runtime::adapter_protocol::RendererStamp::new("test.seed", 1),
                0,
                rect,
                BTreeMap::new(),
            ),
        );
    }

    #[cfg(test)]
    pub(crate) fn seed_all_node_facts_for_test(&self) {
        let runtime = self.runtime().expect("open test session");
        let (snapshot, incarnations) = runtime
            .snapshot_with_incarnations()
            .expect("runtime snapshot");
        let geometry: BTreeMap<NodeId, PublishedNodeGeometry> = snapshot
            .graph()
            .nodes()
            .iter()
            .filter_map(|(node_id, node)| {
                let incarnation = incarnations.get(node_id).copied()?;
                Some((
                    *node_id,
                    PublishedNodeGeometry::new(
                        *node_id,
                        incarnation,
                        crate::runtime::adapter_protocol::RendererStamp::new("test.seed", 1),
                        0,
                        CanvasRect {
                            origin: node.pos,
                            size: node.size.unwrap_or(CanvasSize {
                                width: 160.0,
                                height: 96.0,
                            }),
                        },
                        BTreeMap::new(),
                    ),
                ))
            })
            .collect();
        let mut state = self.shared.state.borrow_mut();
        state.spatial.clear();
        for (node, geometry) in &geometry {
            state.spatial.upsert(*node, geometry.bounds());
        }
        state.clear_node_geometry();
        for (node, geometry) in geometry {
            state.upsert_node_geometry(node, geometry);
        }
    }

    #[cfg(test)]
    pub(crate) fn has_node_geometry_for_test(&self, node: NodeId) -> bool {
        self.shared.state.borrow().geometry.contains_key(&node)
    }

    #[cfg(test)]
    pub(crate) fn node_geometry_for_test(&self, node: NodeId) -> Option<CanvasRect> {
        self.shared
            .state
            .borrow()
            .geometry
            .get(&node)
            .map(|geometry| geometry.bounds())
    }

    #[cfg(test)]
    pub(crate) fn derived_fact_counts_for_test(&self) -> DerivedFactCounts {
        let state = self.shared.state.borrow();
        debug_assert_eq!(state.geometry.size(), state.spatial.len());
        debug_assert_eq!(
            state
                .geometry
                .values()
                .map(|geometry| geometry.ports().len())
                .sum::<usize>(),
            state.geometry_ports.size()
        );
        debug_assert_eq!(
            state
                .geometry
                .values()
                .map(|geometry| geometry.port_outcomes().len())
                .sum::<usize>(),
            state.geometry_port_outcomes.size()
        );
        DerivedFactCounts {
            nodes: state.geometry.size(),
            routes: state.unresolved_routes.len(),
        }
    }
}

impl WeakViewSession {
    pub fn geometry_snapshot(&self) -> Result<GeometrySnapshot, ViewSessionError> {
        let (view, _) = self.open_capability()?;
        view.geometry_snapshot()
    }
}

impl ViewShared {
    fn geometry_snapshot(&self) -> Result<GeometrySnapshot, ViewSessionError> {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| ViewSessionError::Busy)?;
        ensure_open(state.status)?;
        if let Some(snapshot) = &state.geometry_snapshot_cache {
            return Ok(snapshot.clone());
        }
        let snapshot = GeometrySnapshot::new(
            state.layout_revision,
            state.geometry.clone(),
            state.geometry_ports.clone(),
            state.geometry_port_outcomes.clone(),
            state.routes.clone(),
        );
        state.geometry_snapshot_cache = Some(snapshot.clone());
        state.stats.geometry_snapshot_builds =
            state.stats.geometry_snapshot_builds.saturating_add(1);
        Ok(snapshot)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct SpatialCell {
    x: i32,
    y: i32,
}

#[derive(Debug, Default)]
pub(super) struct SpatialQuery {
    pub(super) nodes: BTreeSet<NodeId>,
    pub(super) cells_examined: u64,
    pub(super) entries_examined: u64,
    pub(super) truncated: bool,
}

#[derive(Debug, Default)]
pub(super) struct ViewSpatialIndex {
    cells: BTreeMap<SpatialCell, BTreeSet<NodeId>>,
    nodes: BTreeMap<NodeId, (CanvasRect, Vec<SpatialCell>)>,
    overflow_nodes: BTreeSet<NodeId>,
}

impl ViewSpatialIndex {
    const CELL_SIZE: f32 = 256.0;

    pub(super) fn upsert(&mut self, node: NodeId, bounds: CanvasRect) -> bool {
        if !bounds.is_positive_finite() {
            return self.remove(node);
        }
        if self
            .nodes
            .get(&node)
            .is_some_and(|(current, _)| *current == bounds)
        {
            return false;
        }
        self.remove(node);
        let covered = covered_spatial_cells(bounds).unwrap_or_default();
        if covered.is_empty() {
            self.overflow_nodes.insert(node);
        }
        for cell in &covered {
            self.cells.entry(*cell).or_default().insert(node);
        }
        self.nodes.insert(node, (bounds, covered));
        true
    }

    pub(super) fn remove(&mut self, node: NodeId) -> bool {
        let Some((_, covered)) = self.nodes.remove(&node) else {
            return false;
        };
        self.overflow_nodes.remove(&node);
        for cell in covered {
            let remove_cell = self.cells.get_mut(&cell).is_some_and(|nodes| {
                nodes.remove(&node);
                nodes.is_empty()
            });
            if remove_cell {
                self.cells.remove(&cell);
            }
        }
        true
    }

    pub(super) fn clear(&mut self) {
        self.cells.clear();
        self.nodes.clear();
        self.overflow_nodes.clear();
    }

    pub(super) fn nodes_intersecting(&self, bounds: CanvasRect) -> SpatialQuery {
        self.nodes_intersecting_with_limit(bounds, usize::MAX)
    }

    pub(super) fn nodes_intersecting_with_limit(
        &self,
        bounds: CanvasRect,
        entry_limit: usize,
    ) -> SpatialQuery {
        if !bounds.is_positive_finite() {
            return SpatialQuery::default();
        }
        let mut candidates = BTreeSet::new();
        let mut remaining = entry_limit;
        let mut entries_examined = 0_u64;
        let (cells_examined, truncated) = if let Some(cells) = covered_spatial_cells(bounds) {
            let mut cells_examined = 0_u64;
            let mut truncated = extend_spatial_candidates(
                &mut candidates,
                self.overflow_nodes.iter(),
                &mut remaining,
                &mut entries_examined,
            );
            for cell in cells {
                if truncated {
                    break;
                }
                cells_examined = cells_examined.saturating_add(1);
                if let Some(nodes) = self.cells.get(&cell) {
                    truncated = extend_spatial_candidates(
                        &mut candidates,
                        nodes.iter(),
                        &mut remaining,
                        &mut entries_examined,
                    );
                }
            }
            (cells_examined, truncated)
        } else {
            let truncated = extend_spatial_candidates(
                &mut candidates,
                self.nodes.keys(),
                &mut remaining,
                &mut entries_examined,
            );
            (0, truncated)
        };
        let Some(query) = CanvasBounds::from_rect(bounds) else {
            return SpatialQuery {
                cells_examined,
                entries_examined,
                truncated,
                ..SpatialQuery::default()
            };
        };
        let nodes = candidates
            .into_iter()
            .filter(|node| {
                self.nodes
                    .get(node)
                    .and_then(|(bounds, _)| CanvasBounds::from_rect(*bounds))
                    .is_some_and(|bounds| bounds.intersects(query))
            })
            .collect();
        SpatialQuery {
            nodes,
            cells_examined,
            entries_examined,
            truncated,
        }
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.nodes.len()
    }
}

#[derive(Debug, Clone, PartialEq)]
struct AdmissionPolicy {
    kind_default_sizes: Arc<BTreeMap<NodeKindKey, CanvasSize>>,
    fallback_size: CanvasSize,
}

impl AdmissionPolicy {
    fn from_request(request: &FrameRequest) -> Self {
        Self {
            kind_default_sizes: request.kind_default_sizes.clone(),
            fallback_size: request.fallback_size,
        }
    }

    fn matches(&self, request: &FrameRequest) -> bool {
        self.fallback_size == request.fallback_size
            && (Arc::ptr_eq(&self.kind_default_sizes, &request.kind_default_sizes)
                || self.kind_default_sizes == request.kind_default_sizes)
    }

    fn bounds_for(&self, node: &Node) -> Option<CanvasRect> {
        if node.hidden {
            return None;
        }
        let size = node
            .size
            .filter(|size| size.is_positive_finite())
            .or_else(|| self.kind_default_sizes.get(&node.kind).copied())
            .unwrap_or(self.fallback_size);
        node.bounds_for_size(size)
    }
}

#[derive(Debug, Default)]
pub(super) struct AdmissionIndex {
    policy: Option<AdmissionPolicy>,
    spatial: ViewSpatialIndex,
}

impl AdmissionIndex {
    pub(super) fn prepare(&mut self, graph: &Graph, request: &FrameRequest) -> (u64, bool) {
        if self
            .policy
            .as_ref()
            .is_some_and(|policy| policy.matches(request))
        {
            return (0, false);
        }
        let policy = AdmissionPolicy::from_request(request);
        self.spatial.clear();
        for (node_id, node) in graph.nodes() {
            if let Some(bounds) = policy.bounds_for(node) {
                self.spatial.upsert(*node_id, bounds);
            }
        }
        self.policy = Some(policy);
        (graph.nodes().len() as u64, true)
    }

    pub(super) fn update_nodes(
        &mut self,
        graph: &Graph,
        nodes: impl IntoIterator<Item = NodeId>,
    ) -> u64 {
        let Some(policy) = self.policy.as_ref() else {
            return 0;
        };
        let mut visited = 0_u64;
        for node_id in nodes {
            visited = visited.saturating_add(1);
            if let Some(bounds) = graph
                .node(&node_id)
                .and_then(|node| policy.bounds_for(node))
            {
                self.spatial.upsert(node_id, bounds);
            } else {
                self.spatial.remove(node_id);
            }
        }
        visited
    }

    pub(super) fn nodes_intersecting_with_limit(
        &self,
        bounds: CanvasRect,
        entry_limit: usize,
    ) -> SpatialQuery {
        self.spatial
            .nodes_intersecting_with_limit(bounds, entry_limit)
    }

    pub(super) fn invalidate(&mut self) {
        self.policy = None;
        self.spatial.clear();
    }
}

fn extend_spatial_candidates<'a>(
    candidates: &mut BTreeSet<NodeId>,
    nodes: impl IntoIterator<Item = &'a NodeId>,
    remaining: &mut usize,
    entries_examined: &mut u64,
) -> bool {
    for node in nodes {
        if *remaining == 0 {
            return true;
        }
        *remaining -= 1;
        *entries_examined = entries_examined.saturating_add(1);
        candidates.insert(*node);
    }
    false
}

fn covered_spatial_cells(bounds: CanvasRect) -> Option<Vec<SpatialCell>> {
    let min_x = spatial_cell_coordinate(bounds.origin.x);
    let min_y = spatial_cell_coordinate(bounds.origin.y);
    let max_x = spatial_cell_coordinate(bounds.origin.x + bounds.size.width);
    let max_y = spatial_cell_coordinate(bounds.origin.y + bounds.size.height);
    let width = i64::from(max_x) - i64::from(min_x) + 1;
    let height = i64::from(max_y) - i64::from(min_y) + 1;
    if width <= 0 || height <= 0 || width.saturating_mul(height) > 4_096 {
        return None;
    }
    let mut cells = Vec::with_capacity((width * height) as usize);
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            cells.push(SpatialCell { x, y });
        }
    }
    Some(cells)
}

fn spatial_cell_coordinate(value: f32) -> i32 {
    let coordinate = (value / ViewSpatialIndex::CELL_SIZE).floor();
    coordinate.clamp(i32::MIN as f32, i32::MAX as f32) as i32
}

pub(super) fn recompute_routes(
    state: &mut ViewState,
    snapshot: &GraphSnapshot,
    edges: &BTreeSet<EdgeId>,
) -> RouteRecomputation {
    let mut changed = false;
    for edge_id in edges {
        let Some(edge) = snapshot.graph().edge(edge_id) else {
            changed |= state.remove_route(*edge_id);
            changed |= state.unresolved_routes.remove(edge_id);
            continue;
        };
        if edge.hidden {
            changed |= state.remove_route(*edge_id);
            changed |= state.unresolved_routes.remove(edge_id);
            continue;
        }
        let route = (|| {
            let source = published_port(state, snapshot, edge.from)?;
            let target = published_port(state, snapshot, edge.to)?;
            let endpoints = EdgePosition {
                source: absolute_port_endpoint(source)?,
                target: absolute_port_endpoint(target)?,
            };
            let policy = resolve_edge_policy(edge, &state.policy);
            resolve_edge_route_path(edge, endpoints)
                .map(|facts| {
                    facts.with_interaction(EdgeInteractionFacts {
                        selectable: policy.can_select(),
                        selected: state.selected_edge_membership.contains(edge_id),
                        deletable: policy.can_delete(),
                        reconnect_source: policy.can_reconnect_source(),
                        reconnect_target: policy.can_reconnect_target(),
                    })
                })
                .map(|facts| PublishedEdgeRoute::new(*edge_id, facts))
        })();
        if let Some(route) = route {
            changed |= state.upsert_route(*edge_id, route);
            changed |= state.unresolved_routes.remove(edge_id);
        } else {
            changed |= state.remove_route(*edge_id);
            changed |= state.unresolved_routes.insert(*edge_id);
        }
    }
    RouteRecomputation {
        attempts: edges.len() as u64,
        changed,
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct RouteRecomputation {
    pub(super) attempts: u64,
    pub(super) changed: bool,
}

fn published_port<'a>(
    state: &'a ViewState,
    snapshot: &GraphSnapshot,
    port: jellyflow_core::core::PortId,
) -> Option<&'a PublishedPortGeometry> {
    let owner = snapshot.graph().port(&port)?.node;
    let node = state.geometry.get(&owner)?;
    node.is_fresh().then(|| node.ports().get(&port)).flatten()
}

fn absolute_port_endpoint(port: &PublishedPortGeometry) -> Option<EdgeEndpointPosition> {
    let point = port.route_anchor();
    if !point.is_finite() {
        return None;
    }
    Some(EdgeEndpointPosition {
        point,
        position: port.outward(),
    })
}

pub(super) fn expanded_bounds(viewport: CanvasRect, overscan: f32) -> Option<CanvasBounds> {
    let expanded = CanvasRect {
        origin: CanvasPoint {
            x: viewport.origin.x - overscan,
            y: viewport.origin.y - overscan,
        },
        size: CanvasSize {
            width: viewport.size.width + overscan * 2.0,
            height: viewport.size.height + overscan * 2.0,
        },
    };
    CanvasBounds::from_rect(expanded).filter(|bounds| bounds.is_valid())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn admission_bounds(
    node_id: NodeId,
    node: &Node,
    renderer: &RendererStamp,
    layout_generation: u64,
    geometry: &SharedGeometryMap<NodeId, PublishedNodeGeometry>,
    request: &FrameRequest,
) -> Option<(CanvasRect, FrameBoundsSource)> {
    if let Some(published) = geometry.get(&node_id)
        && published.is_fresh()
        && published.renderer() == renderer
        && published.layout_generation() == layout_generation
        && published.bounds().is_positive_finite()
    {
        return Some((published.bounds(), FrameBoundsSource::Published));
    }
    let (size, source) = if let Some(size) = node.size.filter(|size| size.is_positive_finite()) {
        (size, FrameBoundsSource::Persisted)
    } else if let Some(size) = request.kind_default_sizes.get(&node.kind).copied() {
        (size, FrameBoundsSource::KindDefault)
    } else {
        (request.fallback_size, FrameBoundsSource::ProtocolFallback)
    };
    node.bounds_for_size(size).map(|bounds| (bounds, source))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn publish_layouts(
    state: &mut ViewState,
    snapshot: &GraphSnapshot,
    pending: &PendingFrame,
    current_incarnations: &BTreeMap<NodeId, NodeIncarnation>,
    layouts: Vec<PendingLayout>,
    receipts: &mut [Option<FeedbackOperationReceipt>],
    global_stale: bool,
) -> (Option<LayoutPublicationReceipt>, IncidentSnapshotWork) {
    const GEOMETRY_EPSILON: f32 = 1.0 / 64.0;

    let mut node_layout_counts = BTreeMap::<NodeId, usize>::new();
    for pending_layout in &layouts {
        let count = node_layout_counts
            .entry(pending_layout.layout.node())
            .or_default();
        *count = count.saturating_add(1);
    }
    let mut staged = Vec::new();
    let mut withdrawals = BTreeMap::<NodeId, IncidentEdgeSnapshot>::new();
    let mut request_refresh = false;

    for pending_layout in layouts {
        let node = pending_layout.layout.node();
        let expected = pending.nodes.get(&node);
        let (status, candidate) = if node_layout_counts.get(&node).copied().unwrap_or_default() > 1
        {
            (LayoutNodeStatus::DuplicateNode, None)
        } else if global_stale {
            (LayoutNodeStatus::Stale, None)
        } else if let Some(expected) = expected {
            evaluate_layout_candidate(
                state,
                snapshot,
                current_incarnations,
                expected,
                pending_layout.layout,
                pending.sequence,
                pending.transform,
                GEOMETRY_EPSILON,
            )
        } else {
            let status = match snapshot.graph().node(&node) {
                None => LayoutNodeStatus::MissingNode,
                Some(current) if current.hidden => LayoutNodeStatus::Hidden,
                Some(_) => LayoutNodeStatus::Culled,
            };
            (status, None)
        };

        request_refresh |= status == LayoutNodeStatus::Stale;
        if let Some(candidate) = candidate {
            let incident_edges = expected
                .map(|expected| expected.incident_edges.clone())
                .unwrap_or_default();
            staged.push((node, candidate, incident_edges));
        } else if matches!(
            status,
            LayoutNodeStatus::InvalidBounds
                | LayoutNodeStatus::IncompletePorts
                | LayoutNodeStatus::DuplicatePorts
                | LayoutNodeStatus::ForeignPort
                | LayoutNodeStatus::DuplicateNode
                | LayoutNodeStatus::Stale
        ) && let Some(expected) = expected
        {
            withdrawals.insert(node, expected.incident_edges.clone());
        }
        receipts[pending_layout.operation_index] =
            Some(FeedbackOperationReceipt::Layout { node, status });
    }

    let mut published_nodes = Vec::new();
    let mut withdrawn_nodes = Vec::new();
    let mut route_edges = BTreeSet::new();
    let mut spatial_updates = 0_u64;
    let mut incident_work = IncidentSnapshotWork::default();
    for (node, geometry, incident_edges) in staged {
        spatial_updates = spatial_updates
            .saturating_add(u64::from(state.spatial.upsert(node, geometry.bounds())));
        state.upsert_node_geometry(node, geometry);
        incident_work.materialize(&incident_edges);
        route_edges.extend(incident_edges.iter().copied());
        published_nodes.push(node);
    }
    for (node, incident_edges) in withdrawals {
        let expected_ports = pending
            .nodes
            .get(&node)
            .map(|expected| expected.ports.iter().copied())
            .into_iter()
            .flatten();
        if state.mark_node_geometry_stale(node, expected_ports) {
            incident_work.materialize(&incident_edges);
            route_edges.extend(incident_edges.iter().copied());
            withdrawn_nodes.push(node);
        }
    }

    if request_refresh {
        state.stats.refresh_requests = state.stats.refresh_requests.saturating_add(1);
    }

    if published_nodes.is_empty() && withdrawn_nodes.is_empty() {
        return (None, incident_work);
    }
    let route_invalidations = route_edges.len() as u64;
    let route_recomputations = recompute_routes(state, snapshot, &route_edges).attempts;
    let before_revision = state.layout_revision;
    state.advance_layout_revision();
    state.stats.layout_publications = state.stats.layout_publications.saturating_add(1);
    state.stats.spatial_updates = state.stats.spatial_updates.saturating_add(spatial_updates);
    state.stats.route_invalidations = state
        .stats
        .route_invalidations
        .saturating_add(route_invalidations);
    state.stats.route_recomputations = state
        .stats
        .route_recomputations
        .saturating_add(route_recomputations);
    (
        Some(LayoutPublicationReceipt::new(
            before_revision,
            state.layout_revision,
            published_nodes,
            withdrawn_nodes,
            spatial_updates,
            route_recomputations,
        )),
        incident_work,
    )
}

fn evaluate_layout_candidate(
    state: &ViewState,
    snapshot: &GraphSnapshot,
    current_incarnations: &BTreeMap<NodeId, NodeIncarnation>,
    expected: &ExpectedNodeLayout,
    layout: crate::runtime::adapter_protocol::NodeLayout,
    frame_sequence: u64,
    transform: crate::runtime::viewport::ViewportTransform,
    epsilon: f32,
) -> (LayoutNodeStatus, Option<PublishedNodeGeometry>) {
    let node = layout.node();
    let target = layout.target().clone();
    if target.instance().view() != state.id
        || target.instance().node() != node
        || target.instance().incarnation() != expected.incarnation
        || target.instance().renderer() != &expected.renderer
        || target.layout_generation() != expected.layout_generation
        || target.frame_sequence() != frame_sequence
    {
        return (LayoutNodeStatus::Stale, None);
    }
    let Some(incarnation) = current_incarnations.get(&node) else {
        return (LayoutNodeStatus::MissingNode, None);
    };
    if *incarnation != expected.incarnation
        || state
            .layout_generations
            .get(&node)
            .copied()
            .unwrap_or_default()
            != expected.layout_generation
    {
        return (LayoutNodeStatus::Stale, None);
    }
    let Some(node_bounds) = layout.bounds.to_canvas_rect(transform) else {
        return (LayoutNodeStatus::InvalidBounds, None);
    };
    let port_ids = layout
        .port_outcomes
        .iter()
        .map(PortLayoutOutcome::port)
        .collect::<BTreeSet<_>>();
    if port_ids.len() != layout.port_outcomes.len() {
        return (LayoutNodeStatus::DuplicatePorts, None);
    }
    if !port_ids.is_subset(&expected.ports) {
        return (LayoutNodeStatus::ForeignPort, None);
    }
    if port_ids != expected.ports {
        return (LayoutNodeStatus::IncompletePorts, None);
    }
    let mut port_outcomes = BTreeMap::new();
    for outcome in layout.port_outcomes {
        let port_id = outcome.port();
        if outcome.target().instance() != target.instance()
            || outcome.target().layout_generation() != target.layout_generation()
            || outcome.target().frame_sequence() != frame_sequence
        {
            return (LayoutNodeStatus::ForeignPort, None);
        }
        let Some(current) = snapshot.graph().port(&port_id) else {
            return (LayoutNodeStatus::ForeignPort, None);
        };
        if current.node != node {
            return (LayoutNodeStatus::ForeignPort, None);
        }
        let published = match outcome {
            PortLayoutOutcome::Rendered(port) => {
                let Some(visual_bounds) = port.visual_bounds.to_canvas_rect(transform) else {
                    return (LayoutNodeStatus::InvalidBounds, None);
                };
                let Some(hit_bounds) = port.hit_bounds.to_canvas_rect(transform) else {
                    return (LayoutNodeStatus::InvalidBounds, None);
                };
                let Some(route_anchor) = port.route_anchor.to_canvas_point(transform) else {
                    return (LayoutNodeStatus::InvalidBounds, None);
                };
                PublishedPortOutcome::Rendered(PublishedPortGeometry::new(
                    port_id,
                    node,
                    visual_bounds,
                    hit_bounds,
                    route_anchor,
                    port.outward,
                ))
            }
            PortLayoutOutcome::Hidden { .. } => PublishedPortOutcome::Hidden,
            PortLayoutOutcome::Culled { .. } => PublishedPortOutcome::Culled,
            PortLayoutOutcome::Unavailable { .. } => PublishedPortOutcome::Unavailable,
        };
        port_outcomes.insert(port_id, published);
    }
    let candidate = PublishedNodeGeometry::new(
        node,
        expected.incarnation,
        expected.renderer.clone(),
        expected.layout_generation,
        node_bounds,
        port_outcomes,
    );
    if state
        .geometry
        .get(&node)
        .is_some_and(|current| current.approximately_eq(&candidate, epsilon))
    {
        (LayoutNodeStatus::Unchanged, None)
    } else {
        (LayoutNodeStatus::Published, Some(candidate))
    }
}
