//! Per-view geometry publication, indexing, and route derivation.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize, Graph, Node, NodeId, NodeKindKey};

use crate::runtime::adapter_protocol::{
    FeedbackOperationReceipt, FrameRequest, GeometrySnapshot, LayoutNodeStatus,
    LayoutPublicationReceipt, PortLayoutOutcome, PublishedNodeGeometry, PublishedPortGeometry,
    PublishedPortOutcome,
};
use crate::runtime::geometry::CanvasBounds;
use crate::runtime::graph_runtime::{
    GeometryImpact, GraphSnapshot, IncidentEdgeSnapshot, IncidentSnapshotWork, NodeIncarnation,
};
use crate::runtime::interaction::{
    AlignmentSnapQuery, AlignmentSnapQueryOutcome, AlignmentSnapWork, resolve_alignment_snap,
};
use crate::runtime::viewport::ViewportTransform;

use super::derived_geometry::LayoutWithdrawal;
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
            .derived
            .nodes_intersecting(bounds)
            .nodes
            .into_iter()
            .collect())
    }

    /// Resolves one bounded alignment query from this view's fresh spatial geometry.
    pub fn alignment_snap(
        &self,
        query: AlignmentSnapQuery,
    ) -> Result<AlignmentSnapQueryOutcome, ViewSessionError> {
        self.runtime()?;
        self.shared.alignment_snap(query)
    }

    #[cfg(test)]
    pub(crate) fn retired_geometry_is_empty_for_test(&self) -> bool {
        self.shared.state.borrow().derived.is_empty_for_test()
    }
}

impl WeakViewSession {
    pub fn geometry_snapshot(&self) -> Result<GeometrySnapshot, ViewSessionError> {
        let (view, _) = self.open_capability()?;
        view.geometry_snapshot()
    }

    /// Resolves one bounded alignment query without reviving or owning the view session.
    pub fn alignment_snap(
        &self,
        query: AlignmentSnapQuery,
    ) -> Result<AlignmentSnapQueryOutcome, ViewSessionError> {
        let (view, _) = self.open_capability()?;
        view.alignment_snap(query)
    }
}

impl ViewShared {
    pub(super) fn geometry_snapshot(&self) -> Result<GeometrySnapshot, ViewSessionError> {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| ViewSessionError::Busy)?;
        ensure_open(state.status)?;
        Ok(state.derived.snapshot())
    }

    fn alignment_snap(
        &self,
        query: AlignmentSnapQuery,
    ) -> Result<AlignmentSnapQueryOutcome, ViewSessionError> {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| ViewSessionError::Busy)?;
        ensure_open(state.status)?;
        let work = |entries_examined| AlignmentSnapWork::new(query.generation(), entries_examined);
        if query.view() != state.id || query.view_generation() != state.generation {
            return Ok(AlignmentSnapQueryOutcome::GeometryIndeterminate(work(0)));
        }
        let Some(transform) = ViewportTransform::new(state.facts.pan, state.facts.zoom) else {
            return Ok(AlignmentSnapQueryOutcome::GeometryIndeterminate(work(0)));
        };
        let acquire_tolerance = query.acquire_tolerance_in_view() / transform.zoom;
        let release_tolerance = query.release_tolerance_in_view() / transform.zoom;
        if !acquire_tolerance.is_finite()
            || acquire_tolerance <= 0.0
            || !release_tolerance.is_finite()
            || release_tolerance < acquire_tolerance
            || query.entry_budget() == 0
        {
            return Ok(AlignmentSnapQueryOutcome::GeometryIndeterminate(work(0)));
        }
        let geometry = state.derived.snapshot();
        if query.moving_nodes().len() > query.entry_budget() {
            return Ok(AlignmentSnapQueryOutcome::BudgetExhausted(work(0)));
        }
        let Some(moving_bounds) = query
            .moving_nodes()
            .iter()
            .copied()
            .map(|node| {
                let bounds = geometry.node(node).filter(|node| node.is_fresh())?.bounds();
                let moved = CanvasRect {
                    origin: CanvasPoint {
                        x: bounds.origin.x + query.delta().x,
                        y: bounds.origin.y + query.delta().y,
                    },
                    size: bounds.size,
                };
                moved.is_positive_finite().then_some(moved)
            })
            .collect::<Option<Vec<_>>>()
        else {
            return Ok(AlignmentSnapQueryOutcome::GeometryIndeterminate(work(0)));
        };
        let candidates = state.derived.nodes_near_alignment_with_limit(
            &moving_bounds,
            release_tolerance,
            query.entry_budget(),
        );
        if candidates.truncated {
            return Ok(AlignmentSnapQueryOutcome::BudgetExhausted(work(
                candidates.entries_examined,
            )));
        }
        Ok(AlignmentSnapQueryOutcome::Complete(resolve_alignment_snap(
            &geometry,
            &query,
            &candidates.nodes,
            acquire_tolerance,
            release_tolerance,
        )))
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
    alignment_x: BTreeMap<i32, BTreeSet<NodeId>>,
    alignment_y: BTreeMap<i32, BTreeSet<NodeId>>,
    nodes: BTreeMap<NodeId, IndexedNodeSpatial>,
    overflow_nodes: BTreeSet<NodeId>,
}

#[derive(Debug)]
struct IndexedNodeSpatial {
    bounds: CanvasRect,
    covered: Vec<SpatialCell>,
    alignment_x: Vec<i32>,
    alignment_y: Vec<i32>,
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
            .is_some_and(|current| current.bounds == bounds)
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
        let alignment_x = alignment_feature_cells(bounds.origin.x, bounds.size.width);
        let alignment_y = alignment_feature_cells(bounds.origin.y, bounds.size.height);
        for cell in &alignment_x {
            self.alignment_x.entry(*cell).or_default().insert(node);
        }
        for cell in &alignment_y {
            self.alignment_y.entry(*cell).or_default().insert(node);
        }
        self.nodes.insert(
            node,
            IndexedNodeSpatial {
                bounds,
                covered,
                alignment_x,
                alignment_y,
            },
        );
        true
    }

    pub(super) fn remove(&mut self, node: NodeId) -> bool {
        let Some(indexed) = self.nodes.remove(&node) else {
            return false;
        };
        self.overflow_nodes.remove(&node);
        for cell in indexed.covered {
            let remove_cell = self.cells.get_mut(&cell).is_some_and(|nodes| {
                nodes.remove(&node);
                nodes.is_empty()
            });
            if remove_cell {
                self.cells.remove(&cell);
            }
        }
        remove_axis_membership(&mut self.alignment_x, indexed.alignment_x, node);
        remove_axis_membership(&mut self.alignment_y, indexed.alignment_y, node);
        true
    }

    pub(super) fn clear(&mut self) {
        self.cells.clear();
        self.alignment_x.clear();
        self.alignment_y.clear();
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
                    .and_then(|indexed| CanvasBounds::from_rect(indexed.bounds))
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

    pub(super) fn nodes_near_alignment_with_limit(
        &self,
        moving_bounds: &[CanvasRect],
        tolerance: f32,
        entry_limit: usize,
    ) -> SpatialQuery {
        if moving_bounds.is_empty()
            || moving_bounds
                .iter()
                .any(|bounds| !bounds.is_positive_finite())
            || !tolerance.is_finite()
            || tolerance <= 0.0
            || entry_limit == 0
        {
            return SpatialQuery::default();
        }
        let mut x_cells = BTreeSet::new();
        let mut y_cells = BTreeSet::new();
        for bounds in moving_bounds {
            for feature in alignment_axis_features(bounds.origin.x, bounds.size.width) {
                if !insert_axis_cell_range(&mut x_cells, feature, tolerance, entry_limit) {
                    return SpatialQuery {
                        truncated: true,
                        ..SpatialQuery::default()
                    };
                }
            }
            for feature in alignment_axis_features(bounds.origin.y, bounds.size.height) {
                if !insert_axis_cell_range(&mut y_cells, feature, tolerance, entry_limit) {
                    return SpatialQuery {
                        truncated: true,
                        ..SpatialQuery::default()
                    };
                }
            }
        }

        let mut nodes = BTreeSet::new();
        let mut remaining = entry_limit;
        let mut entries_examined = 0_u64;
        let mut cells_examined = 0_u64;
        let mut truncated = extend_unique_spatial_candidates(
            &mut nodes,
            self.overflow_nodes.iter(),
            &mut remaining,
            &mut entries_examined,
        );
        for cell in x_cells {
            if truncated {
                break;
            }
            cells_examined = cells_examined.saturating_add(1);
            if let Some(candidates) = self.alignment_x.get(&cell) {
                truncated = extend_unique_spatial_candidates(
                    &mut nodes,
                    candidates.iter(),
                    &mut remaining,
                    &mut entries_examined,
                );
            }
        }
        for cell in y_cells {
            if truncated {
                break;
            }
            cells_examined = cells_examined.saturating_add(1);
            if let Some(candidates) = self.alignment_y.get(&cell) {
                truncated = extend_unique_spatial_candidates(
                    &mut nodes,
                    candidates.iter(),
                    &mut remaining,
                    &mut entries_examined,
                );
            }
        }
        SpatialQuery {
            nodes,
            cells_examined,
            entries_examined,
            truncated,
        }
    }

    #[cfg(test)]
    pub(super) fn indexed_bounds_for_test(&self) -> BTreeMap<NodeId, CanvasRect> {
        self.nodes
            .iter()
            .map(|(node, indexed)| (*node, indexed.bounds))
            .collect()
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

fn extend_unique_spatial_candidates<'a>(
    candidates: &mut BTreeSet<NodeId>,
    nodes: impl IntoIterator<Item = &'a NodeId>,
    remaining: &mut usize,
    entries_examined: &mut u64,
) -> bool {
    for node in nodes {
        if candidates.contains(node) {
            continue;
        }
        if *remaining == 0 {
            return true;
        }
        *remaining -= 1;
        *entries_examined = entries_examined.saturating_add(1);
        candidates.insert(*node);
    }
    false
}

fn alignment_axis_features(origin: f32, extent: f32) -> [f32; 3] {
    [origin, origin + extent * 0.5, origin + extent]
}

fn alignment_feature_cells(origin: f32, extent: f32) -> Vec<i32> {
    alignment_axis_features(origin, extent)
        .into_iter()
        .map(spatial_cell_coordinate)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn insert_axis_cell_range(
    cells: &mut BTreeSet<i32>,
    feature: f32,
    tolerance: f32,
    cell_limit: usize,
) -> bool {
    let start = spatial_cell_coordinate(feature - tolerance);
    let end = spatial_cell_coordinate(feature + tolerance);
    let width = i64::from(end) - i64::from(start) + 1;
    if width <= 0 || width as u64 > cell_limit as u64 {
        return false;
    }
    for cell in start..=end {
        cells.insert(cell);
        if cells.len() > cell_limit {
            return false;
        }
    }
    true
}

fn remove_axis_membership(
    index: &mut BTreeMap<i32, BTreeSet<NodeId>>,
    cells: Vec<i32>,
    node: NodeId,
) {
    for cell in cells {
        let remove_cell = index.get_mut(&cell).is_some_and(|nodes| {
            nodes.remove(&node);
            nodes.is_empty()
        });
        if remove_cell {
            index.remove(&cell);
        }
    }
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
pub(super) fn publish_layouts(
    state: &mut ViewState,
    snapshot: &GraphSnapshot,
    pending: &PendingFrame,
    current_node_facts: &BTreeMap<NodeId, (NodeIncarnation, IncidentEdgeSnapshot)>,
    geometry_impact: &GeometryImpact,
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
    let mut impact_candidates_checked = 0_u64;
    let mut impact_candidates_stale = 0_u64;

    for pending_layout in layouts {
        let node = pending_layout.layout.node();
        let expected = pending.nodes.get(&node);
        let impact_stale = expected.is_some_and(|expected| {
            impact_candidates_checked = impact_candidates_checked.saturating_add(1);
            geometry_impact.affects_layout(
                node,
                expected.ports.iter().copied(),
                expected.incident_edges.iter().copied(),
            )
        });
        let preserves_translated_geometry = !global_stale
            && impact_stale
            && expected.is_some_and(|expected| {
                geometry_impact.preserves_translated_layout(
                    node,
                    expected.ports.iter().copied(),
                    expected.incident_edges.iter().copied(),
                )
            });
        let (status, candidate) = if node_layout_counts.get(&node).copied().unwrap_or_default() > 1
        {
            (LayoutNodeStatus::DuplicateNode, None)
        } else if global_stale || impact_stale {
            (LayoutNodeStatus::Stale, None)
        } else if let Some(expected) = expected {
            evaluate_layout_candidate(
                state,
                snapshot,
                current_node_facts,
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
        if impact_stale && status == LayoutNodeStatus::Stale {
            impact_candidates_stale = impact_candidates_stale.saturating_add(1);
        }

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
        ) && !preserves_translated_geometry
            && let Some(expected) = expected
        {
            withdrawals.insert(node, expected.incident_edges.clone());
        }
        receipts[pending_layout.operation_index] =
            Some(FeedbackOperationReceipt::Layout { node, status });
    }
    let withdrawals = withdrawals
        .into_iter()
        .map(|(node, incident_edges)| {
            LayoutWithdrawal::new(
                node,
                pending
                    .nodes
                    .get(&node)
                    .map(|expected| expected.ports.clone())
                    .unwrap_or_default(),
                incident_edges,
                current_node_facts
                    .get(&node)
                    .map(|(_, incident_edges)| incident_edges.clone()),
            )
        })
        .collect();
    let policy = state.policy;
    let selected_edges = state.selected_edge_membership.clone();
    state
        .derived
        .record_layout_candidate_impact(impact_candidates_checked, impact_candidates_stale);
    state.derived.publish_complete_layout(
        snapshot,
        staged,
        withdrawals,
        geometry_impact.incident_edges(),
        request_refresh,
        policy,
        &selected_edges,
    )
}

fn evaluate_layout_candidate(
    state: &ViewState,
    snapshot: &GraphSnapshot,
    current_node_facts: &BTreeMap<NodeId, (NodeIncarnation, IncidentEdgeSnapshot)>,
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
    let Some((incarnation, _)) = current_node_facts.get(&node) else {
        return (LayoutNodeStatus::MissingNode, None);
    };
    if *incarnation != expected.incarnation
        || state.derived.layout_generation(node) != expected.layout_generation
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
    if state.derived.node_approximately_equals(&candidate, epsilon) {
        (LayoutNodeStatus::Unchanged, None)
    } else {
        (LayoutNodeStatus::Published, Some(candidate))
    }
}
