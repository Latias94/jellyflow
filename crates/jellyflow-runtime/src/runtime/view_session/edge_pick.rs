#[cfg(test)]
use std::collections::HashSet;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Bound::{Excluded, Unbounded};

use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize, EdgeId};

use crate::runtime::adapter_protocol::ViewPoint;
use crate::runtime::geometry::{
    DEFAULT_EDGE_INTERACTION_WIDTH, EdgePathFlattenError, FlattenedEdgePath,
};
use crate::runtime::graph_runtime::EdgeInstanceKey;
use crate::runtime::viewport::ViewportTransform;

use super::order::render_order_key;
use super::route_geometry::{
    RouteFlattenWork, RouteGeometryIndex, RouteQuerySource, RouteSegmentKey,
};
use super::{
    EdgePickQueryReceipt, InteractionQueryBinding, InteractionQueryProof, SelectedEdgeMembership,
    ViewQueryIdentity, ViewQueryIdentityStaleReason, ViewSession, ViewSessionError, ViewShared,
    WeakViewSession, ensure_open,
};
#[cfg(test)]
use super::{ViewGeneration, ViewId};

const HARD_EDGE_PICK_ENTRY_LIMIT: u64 = 16_384;
const MIN_PICK_RADIUS_IN_VIEW: f32 = 0.001;

pub(super) trait EdgeSelectionLookup {
    fn contains(&self, edge: EdgeInstanceKey) -> bool;
}

impl EdgeSelectionLookup for SelectedEdgeMembership {
    fn contains(&self, edge: EdgeInstanceKey) -> bool {
        self.contains(edge)
    }
}

#[cfg(test)]
impl EdgeSelectionLookup for HashSet<EdgeId> {
    fn contains(&self, edge: EdgeInstanceKey) -> bool {
        HashSet::contains(self, &edge.edge())
    }
}

/// A bounded edge-pick query expressed in framework view-space logical pixels.
#[derive(Debug)]
pub struct EdgePickQuery {
    request: EdgePickRequest,
    entry_budget: usize,
    interaction_binding: Option<InteractionQueryBinding>,
}

impl EdgePickQuery {
    pub fn new(pointer: ViewPoint, radius_in_view: f32, entry_budget: usize) -> Self {
        Self {
            request: EdgePickRequest::Start {
                pointer,
                radius_in_view,
            },
            entry_budget,
            interaction_binding: None,
        }
    }

    pub fn resume(cursor: Box<EdgePickCursor>, entry_budget: usize) -> Self {
        Self {
            request: EdgePickRequest::Resume(cursor),
            entry_budget,
            interaction_binding: None,
        }
    }

    pub(crate) fn bind_interaction(mut self, binding: InteractionQueryBinding) -> Self {
        self.interaction_binding = Some(binding);
        self
    }
}

#[derive(Debug)]
enum EdgePickRequest {
    Start {
        pointer: ViewPoint,
        radius_in_view: f32,
    },
    Resume(Box<EdgePickCursor>),
}

/// Opaque continuation for a bounded edge-pick query.
#[derive(Debug)]
pub struct EdgePickCursor {
    identity: ViewQueryIdentity,
    path_epoch: u128,
    pick_epoch: u128,
    order_revision: u128,
    canvas_pointer: CanvasPoint,
    zoom: f32,
    radius_in_view: f32,
    sources: Vec<RouteQuerySource>,
    source_index: usize,
    source_started: bool,
    last_entry: Option<RouteSegmentKey>,
    checked: BTreeSet<EdgeId>,
    best: Option<EdgePickCandidate>,
    geometry_failures: BTreeMap<EdgeId, EdgePathFlattenError>,
    total_entries_examined: u64,
}

/// One deterministic edge pick and its flattened-path distance from the pointer.
///
/// The analytic curve distance differs from `distance_in_view` by no more than
/// `distance_error_bound_in_view` logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgePick {
    instance: EdgeInstanceKey,
    distance_in_view: f32,
    distance_error_bound_in_view: f32,
}

impl EdgePick {
    pub fn edge(self) -> EdgeId {
        self.instance.edge()
    }

    /// Returns the unforgeable identity of the picked edge lifetime.
    pub fn instance(self) -> EdgeInstanceKey {
        self.instance
    }

    pub fn distance_in_view(self) -> f32 {
        self.distance_in_view
    }

    pub fn distance_error_bound_in_view(self) -> f32 {
        self.distance_error_bound_in_view
    }
}

/// One route whose bounded flatten operation could not prove the edge-pick result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EdgePickGeometryFailure {
    edge: EdgeId,
    error: EdgePathFlattenError,
}

impl EdgePickGeometryFailure {
    pub fn edge(self) -> EdgeId {
        self.edge
    }

    pub fn error(self) -> EdgePathFlattenError {
        self.error
    }
}

/// Work performed by one page of a bounded edge-pick query.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EdgePickWork {
    pub cells_examined: u64,
    pub entries_examined: u64,
    pub exact_tests: u64,
    pub candidates_matched: u64,
    pub flatten_cache_hits: u64,
    pub flatten_cache_misses: u64,
    pub flatten_cache_evictions: u64,
    pub flatten_segments_built: u64,
    pub flatten_overflows: u64,
}

impl EdgePickWork {
    fn record_flatten(&mut self, work: RouteFlattenWork) {
        self.flatten_cache_hits = self.flatten_cache_hits.saturating_add(work.cache_hits);
        self.flatten_cache_misses = self.flatten_cache_misses.saturating_add(work.cache_misses);
        self.flatten_cache_evictions = self
            .flatten_cache_evictions
            .saturating_add(work.cache_evictions);
        self.flatten_segments_built = self
            .flatten_segments_built
            .saturating_add(work.segments_built);
        self.flatten_overflows = self.flatten_overflows.saturating_add(work.overflows);
    }
}

/// Terminal or resumable result of an edge-pick query.
#[derive(Debug)]
pub enum EdgePickQueryOutcome {
    Complete {
        pick: Option<EdgePick>,
        receipt: EdgePickQueryReceipt,
    },
    Incomplete {
        cursor: Box<EdgePickCursor>,
        receipt: EdgePickQueryReceipt,
    },
    BudgetExhausted {
        receipt: EdgePickQueryReceipt,
    },
    GeometryIndeterminate {
        failures: Vec<EdgePickGeometryFailure>,
        receipt: EdgePickQueryReceipt,
    },
}

impl EdgePickQueryOutcome {
    pub fn receipt(&self) -> EdgePickQueryReceipt {
        match self {
            Self::Complete { receipt, .. }
            | Self::Incomplete { receipt, .. }
            | Self::BudgetExhausted { receipt }
            | Self::GeometryIndeterminate { receipt, .. } => *receipt,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum EdgePickCursorStaleReason {
    #[error("the cursor belongs to another view")]
    ForeignView,
    #[error("the view generation changed")]
    ViewGeneration,
    #[error("the viewport transform changed")]
    ViewportTransform,
    #[error("indexed edge paths changed")]
    RouteGeometry,
    #[error("edge interaction or presentation facts changed")]
    EdgeInteraction,
    #[error("edge stacking or selection changed")]
    PaintOrder,
}

impl EdgePickCursorStaleReason {
    /// Returns whether normal mutable view state can invalidate this cursor between pages.
    ///
    /// Foreign-view and generation mismatches are capability integrity failures and therefore are
    /// not eligible for a first-page restart.
    pub const fn is_restartable(self) -> bool {
        matches!(
            self,
            Self::ViewportTransform
                | Self::RouteGeometry
                | Self::EdgeInteraction
                | Self::PaintOrder
        )
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EdgePickQueryError {
    #[error(transparent)]
    Session(#[from] ViewSessionError),
    #[error("edge-pick point or radius is invalid")]
    InvalidQuery,
    #[error(transparent)]
    StaleCursor(EdgePickCursorStaleReason),
}

#[derive(Debug, Clone, Copy)]
struct EdgePickCandidate {
    pick: EdgePick,
    paint_order: (bool, bool, bool, usize, EdgeId),
}

pub(super) enum InternalEdgePickOutcome {
    Complete {
        pick: Option<EdgePick>,
        work: EdgePickWork,
    },
    Incomplete {
        cursor: Box<EdgePickCursor>,
        work: EdgePickWork,
    },
    BudgetExhausted {
        work: EdgePickWork,
    },
    GeometryIndeterminate {
        failures: BTreeMap<EdgeId, EdgePathFlattenError>,
        work: EdgePickWork,
    },
}

impl RouteGeometryIndex {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new_pick_cursor(
        &self,
        identity: ViewQueryIdentity,
        order_revision: u128,
        pointer: ViewPoint,
        radius_in_view: f32,
        transform: ViewportTransform,
    ) -> Option<EdgePickCursor> {
        if !pointer.is_finite() || !radius_in_view.is_finite() || radius_in_view < 0.0 {
            return None;
        }
        let canvas_pointer = pointer.to_canvas_point(transform)?;
        let broad_radius_in_view = radius_in_view
            .max(self.max_interaction_half_width())
            .max(MIN_PICK_RADIUS_IN_VIEW);
        let broad_radius_in_canvas = broad_radius_in_view / transform.zoom;
        if !broad_radius_in_canvas.is_finite() || broad_radius_in_canvas <= 0.0 {
            return None;
        }
        let query_bounds = centered_rect(canvas_pointer, broad_radius_in_canvas)?;
        Some(EdgePickCursor {
            identity,
            path_epoch: self.index_epoch(),
            pick_epoch: self.pick_epoch(),
            order_revision,
            canvas_pointer,
            zoom: transform.zoom,
            radius_in_view,
            sources: self.pick_query_sources(query_bounds),
            source_index: 0,
            source_started: false,
            last_entry: None,
            checked: BTreeSet::new(),
            best: None,
            geometry_failures: BTreeMap::new(),
            total_entries_examined: 0,
        })
    }

    pub(super) fn query_pick_page(
        &mut self,
        edge_draw_order_ranks: &BTreeMap<EdgeId, usize>,
        selected_edges: &impl EdgeSelectionLookup,
        mut cursor: EdgePickCursor,
        entry_budget: usize,
    ) -> InternalEdgePickOutcome {
        let mut work = EdgePickWork::default();
        let mut remaining = entry_budget;
        loop {
            if cursor.source_index >= cursor.sources.len() {
                if cursor.geometry_failures.is_empty() {
                    return InternalEdgePickOutcome::Complete {
                        pick: cursor.best.map(|candidate| candidate.pick),
                        work,
                    };
                }
                return InternalEdgePickOutcome::GeometryIndeterminate {
                    failures: cursor.geometry_failures,
                    work,
                };
            }
            if !cursor.source_started {
                if matches!(
                    cursor.sources[cursor.source_index],
                    RouteQuerySource::Cell(_)
                ) {
                    work.cells_examined = work.cells_examined.saturating_add(1);
                }
                cursor.source_started = true;
            }
            let source = cursor.sources[cursor.source_index];
            let next = self.pick_source(source).and_then(|entries| {
                cursor.last_entry.map_or_else(
                    || entries.iter().next().copied(),
                    |last| entries.range((Excluded(last), Unbounded)).next().copied(),
                )
            });
            let Some(entry) = next else {
                cursor.source_index += 1;
                cursor.source_started = false;
                cursor.last_entry = None;
                continue;
            };
            if remaining == 0 {
                return InternalEdgePickOutcome::Incomplete {
                    cursor: Box::new(cursor),
                    work,
                };
            }
            if cursor.total_entries_examined >= HARD_EDGE_PICK_ENTRY_LIMIT {
                return InternalEdgePickOutcome::BudgetExhausted { work };
            }
            remaining -= 1;
            cursor.last_entry = Some(entry);
            cursor.total_entries_examined = cursor.total_entries_examined.saturating_add(1);
            work.entries_examined = work.entries_examined.saturating_add(1);

            let edge = entry.edge();
            if !cursor.checked.insert(edge) {
                continue;
            }
            let Some(facts) = self.pick_facts(edge) else {
                continue;
            };
            if !facts.selectable {
                continue;
            }
            let threshold_in_view = cursor
                .radius_in_view
                .max(valid_interaction_width(facts.interaction_width) * 0.5);
            let threshold_in_canvas = threshold_in_view / cursor.zoom;
            if point_rect_distance(cursor.canvas_pointer, facts.bounds) > threshold_in_canvas {
                continue;
            }

            work.exact_tests = work.exact_tests.saturating_add(1);
            let mut flatten_work = RouteFlattenWork::default();
            let flattened = self.flattened_for_pick(edge, cursor.zoom, &mut flatten_work);
            work.record_flatten(flatten_work);
            let (distance_in_canvas, distance_error_bound_in_canvas) =
                match flattened.and_then(|approximation| {
                    flattened_path_distance(&approximation.path, cursor.canvas_pointer)
                        .map(|distance| (distance, approximation.max_error_in_canvas))
                }) {
                    Ok((Some(distance), error_bound)) => (distance, error_bound),
                    Ok((None, _)) => continue,
                    Err(error) => {
                        cursor.geometry_failures.insert(edge, error);
                        continue;
                    }
                };
            let distance_in_view = distance_in_canvas * cursor.zoom;
            let distance_error_bound_in_view = distance_error_bound_in_canvas * cursor.zoom;
            if !distance_in_view.is_finite()
                || !distance_error_bound_in_view.is_finite()
                || distance_in_view > threshold_in_view + distance_error_bound_in_view
            {
                continue;
            }
            work.candidates_matched = work.candidates_matched.saturating_add(1);
            let instance = self
                .edge_instance(edge)
                .expect("pick facts must retain their indexed edge identity");
            let candidate = EdgePickCandidate {
                pick: EdgePick {
                    instance,
                    distance_in_view,
                    distance_error_bound_in_view,
                },
                paint_order: render_order_key(
                    edge,
                    selected_edges.contains(instance),
                    false,
                    edge_draw_order_ranks,
                ),
            };
            if cursor
                .best
                .is_none_or(|current| edge_candidate_is_better(candidate, current))
            {
                cursor.best = Some(candidate);
            }
        }
    }
}

impl ViewSession {
    pub fn edge_pick(
        &self,
        query: EdgePickQuery,
    ) -> Result<EdgePickQueryOutcome, EdgePickQueryError> {
        self.runtime()?;
        self.shared.edge_pick(query)
    }
}

impl WeakViewSession {
    pub(crate) fn edge_pick(
        &self,
        query: EdgePickQuery,
    ) -> Result<EdgePickQueryOutcome, EdgePickQueryError> {
        let (view, _) = self.open_capability()?;
        view.edge_pick(query)
    }
}

impl ViewShared {
    fn edge_pick(&self, query: EdgePickQuery) -> Result<EdgePickQueryOutcome, EdgePickQueryError> {
        let (outcome, proof) = {
            let mut state = self
                .state
                .try_borrow_mut()
                .map_err(|_| ViewSessionError::Busy)?;
            ensure_open(state.status)?;
            let transform = ViewportTransform::new(state.facts.pan, state.facts.zoom)
                .ok_or(ViewSessionError::InvalidViewport)?;
            let cursor = match query.request {
                EdgePickRequest::Start {
                    pointer,
                    radius_in_view,
                } => state
                    .derived
                    .new_edge_pick_cursor(
                        state.query_identity(),
                        state.interaction_order_revision,
                        pointer,
                        radius_in_view,
                        transform,
                    )
                    .ok_or(EdgePickQueryError::InvalidQuery)?,
                EdgePickRequest::Resume(cursor) => {
                    validate_edge_pick_cursor(&state, &cursor)?;
                    *cursor
                }
            };
            let proof = InteractionQueryProof::issue(
                query.interaction_binding,
                cursor.identity,
                [
                    cursor.path_epoch,
                    cursor.pick_epoch,
                    cursor.order_revision,
                    0,
                ],
            )?;
            let outcome = {
                let state = &mut *state;
                state.derived.query_edge_pick_page(
                    &state.edge_draw_order_ranks,
                    &state.selected_edge_membership,
                    cursor,
                    query.entry_budget,
                )
            };
            (outcome, proof)
        };

        Ok(match outcome {
            InternalEdgePickOutcome::Complete { pick, work } => EdgePickQueryOutcome::Complete {
                pick,
                receipt: EdgePickQueryReceipt::new(work, proof),
            },
            InternalEdgePickOutcome::Incomplete { cursor, work } => {
                EdgePickQueryOutcome::Incomplete {
                    cursor,
                    receipt: EdgePickQueryReceipt::new(work, proof),
                }
            }
            InternalEdgePickOutcome::BudgetExhausted { work } => {
                EdgePickQueryOutcome::BudgetExhausted {
                    receipt: EdgePickQueryReceipt::new(work, proof),
                }
            }
            InternalEdgePickOutcome::GeometryIndeterminate { failures, work } => {
                EdgePickQueryOutcome::GeometryIndeterminate {
                    failures: failures
                        .into_iter()
                        .map(|(edge, error)| EdgePickGeometryFailure { edge, error })
                        .collect(),
                    receipt: EdgePickQueryReceipt::new(work, proof),
                }
            }
        })
    }
}

fn validate_edge_pick_cursor(
    state: &super::ViewState,
    cursor: &EdgePickCursor,
) -> Result<(), EdgePickQueryError> {
    let (path_epoch, pick_epoch) = state.derived.edge_pick_epochs();
    edge_pick_cursor_stale_reason(
        state.query_identity(),
        path_epoch,
        pick_epoch,
        state.interaction_order_revision,
        cursor,
    )
    .map(EdgePickQueryError::StaleCursor)
    .map_or(Ok(()), Err)
}

fn edge_pick_cursor_stale_reason(
    identity: ViewQueryIdentity,
    path_epoch: u128,
    pick_epoch: u128,
    order_revision: u128,
    cursor: &EdgePickCursor,
) -> Option<EdgePickCursorStaleReason> {
    match cursor.identity.stale_reason(identity) {
        Some(ViewQueryIdentityStaleReason::ForeignView) => {
            Some(EdgePickCursorStaleReason::ForeignView)
        }
        Some(ViewQueryIdentityStaleReason::ViewGeneration) => {
            Some(EdgePickCursorStaleReason::ViewGeneration)
        }
        Some(ViewQueryIdentityStaleReason::ViewportTransform) => {
            Some(EdgePickCursorStaleReason::ViewportTransform)
        }
        None if cursor.path_epoch != path_epoch => Some(EdgePickCursorStaleReason::RouteGeometry),
        None if cursor.pick_epoch != pick_epoch => Some(EdgePickCursorStaleReason::EdgeInteraction),
        None if cursor.order_revision != order_revision => {
            Some(EdgePickCursorStaleReason::PaintOrder)
        }
        None => None,
    }
}

pub(super) fn record_edge_pick_flatten_stats(
    stats: &mut super::DerivedGeometryStats,
    outcome: &InternalEdgePickOutcome,
) {
    let work = match outcome {
        InternalEdgePickOutcome::Complete { work, .. }
        | InternalEdgePickOutcome::Incomplete { work, .. }
        | InternalEdgePickOutcome::BudgetExhausted { work }
        | InternalEdgePickOutcome::GeometryIndeterminate { work, .. } => *work,
    };
    stats.route_flatten_cache_hits = stats
        .route_flatten_cache_hits
        .saturating_add(work.flatten_cache_hits);
    stats.route_flatten_cache_misses = stats
        .route_flatten_cache_misses
        .saturating_add(work.flatten_cache_misses);
    stats.route_flatten_cache_evictions = stats
        .route_flatten_cache_evictions
        .saturating_add(work.flatten_cache_evictions);
    stats.route_flatten_segments_built = stats
        .route_flatten_segments_built
        .saturating_add(work.flatten_segments_built);
    stats.route_flatten_overflows = stats
        .route_flatten_overflows
        .saturating_add(work.flatten_overflows);
}

fn edge_candidate_is_better(candidate: EdgePickCandidate, current: EdgePickCandidate) -> bool {
    candidate
        .pick
        .distance_in_view
        .total_cmp(&current.pick.distance_in_view)
        .reverse()
        .then_with(|| candidate.paint_order.cmp(&current.paint_order))
        .is_gt()
}

fn valid_interaction_width(width: f32) -> f32 {
    if width.is_finite() && width > 0.0 {
        width
    } else {
        DEFAULT_EDGE_INTERACTION_WIDTH
    }
}

fn centered_rect(center: CanvasPoint, radius: f32) -> Option<CanvasRect> {
    let rect = CanvasRect {
        origin: CanvasPoint {
            x: center.x - radius,
            y: center.y - radius,
        },
        size: CanvasSize {
            width: radius * 2.0,
            height: radius * 2.0,
        },
    };
    rect.is_positive_finite().then_some(rect)
}

fn point_rect_distance(point: CanvasPoint, rect: CanvasRect) -> f32 {
    let max_x = rect.origin.x + rect.size.width;
    let max_y = rect.origin.y + rect.size.height;
    let dx = if point.x < rect.origin.x {
        rect.origin.x - point.x
    } else if point.x > max_x {
        point.x - max_x
    } else {
        0.0
    };
    let dy = if point.y < rect.origin.y {
        rect.origin.y - point.y
    } else if point.y > max_y {
        point.y - max_y
    } else {
        0.0
    };
    (dx * dx + dy * dy).sqrt()
}

fn flattened_path_distance(
    path: &FlattenedEdgePath,
    point: CanvasPoint,
) -> Result<Option<f32>, EdgePathFlattenError> {
    if !point.is_finite() {
        return Ok(None);
    }
    let mut minimum = f32::INFINITY;
    for segment in path.segments() {
        minimum = minimum.min(point_segment_distance(point, segment.from(), segment.to())?);
    }
    Ok(minimum.is_finite().then_some(minimum))
}

fn point_segment_distance(
    point: CanvasPoint,
    from: CanvasPoint,
    to: CanvasPoint,
) -> Result<f32, EdgePathFlattenError> {
    let dx = to.x - from.x;
    let dy = to.y - from.y;
    let length_squared = dx * dx + dy * dy;
    if !length_squared.is_finite() {
        return Err(EdgePathFlattenError::InvalidGeometry);
    }
    let projected = if length_squared <= f32::EPSILON {
        from
    } else {
        let t =
            (((point.x - from.x) * dx + (point.y - from.y) * dy) / length_squared).clamp(0.0, 1.0);
        CanvasPoint {
            x: from.x + t * dx,
            y: from.y + t * dy,
        }
    };
    let distance_x = point.x - projected.x;
    let distance_y = point.y - projected.y;
    let distance = (distance_x * distance_x + distance_y * distance_y).sqrt();
    distance
        .is_finite()
        .then_some(distance)
        .ok_or(EdgePathFlattenError::InvalidGeometry)
}

#[cfg(test)]
mod tests {
    use jellyflow_core::core::{Edge, EdgeKind, EdgeRouteSpec, EdgeViewDescriptor, PortId};

    use super::*;
    use crate::runtime::adapter_protocol::PublishedEdgeRoute;
    use crate::runtime::geometry::{
        EdgeEndpointPosition, EdgePath, EdgePathLabel, EdgePosition, HandlePosition, PathCommand,
        resolve_edge_route_path,
    };
    use crate::runtime::viewport::ViewportRevision;

    #[test]
    fn edge_pick_cursor_identity_and_epoch_staleness_matrix() {
        let index = RouteGeometryIndex::default();
        let cursor = cursor(&index, ViewPoint::new(0.0, 0.0), 4.0, 1.0);
        let identity = cursor.identity;
        let foreign_identity = ViewQueryIdentity::new(
            ViewId {
                runtime: identity.view.runtime,
                local: identity.view.local.saturating_add(1),
            },
            identity.generation,
            identity.viewport_revision,
        );
        let next_generation = ViewQueryIdentity::new(
            identity.view,
            identity.generation.terminal(),
            identity.viewport_revision,
        );
        let next_transform = ViewQueryIdentity::new(
            identity.view,
            identity.generation,
            identity.viewport_revision.checked_next().unwrap(),
        );
        for (current_identity, path_epoch, pick_epoch, order_revision, expected) in [
            (
                foreign_identity,
                cursor.path_epoch,
                cursor.pick_epoch,
                cursor.order_revision,
                Some(EdgePickCursorStaleReason::ForeignView),
            ),
            (
                next_generation,
                cursor.path_epoch,
                cursor.pick_epoch,
                cursor.order_revision,
                Some(EdgePickCursorStaleReason::ViewGeneration),
            ),
            (
                next_transform,
                cursor.path_epoch,
                cursor.pick_epoch,
                cursor.order_revision,
                Some(EdgePickCursorStaleReason::ViewportTransform),
            ),
            (
                identity,
                cursor.path_epoch.saturating_add(1),
                cursor.pick_epoch,
                cursor.order_revision,
                Some(EdgePickCursorStaleReason::RouteGeometry),
            ),
            (
                identity,
                cursor.path_epoch,
                cursor.pick_epoch.saturating_add(1),
                cursor.order_revision,
                Some(EdgePickCursorStaleReason::EdgeInteraction),
            ),
            (
                identity,
                cursor.path_epoch,
                cursor.pick_epoch,
                cursor.order_revision.saturating_add(1),
                Some(EdgePickCursorStaleReason::PaintOrder),
            ),
            (
                identity,
                cursor.path_epoch,
                cursor.pick_epoch,
                cursor.order_revision,
                None,
            ),
        ] {
            assert_eq!(
                edge_pick_cursor_stale_reason(
                    current_identity,
                    path_epoch,
                    pick_epoch,
                    order_revision,
                    &cursor,
                ),
                expected,
            );
        }
    }

    #[test]
    fn crossing_edges_pick_nearest_then_effective_paint_order() {
        let back = EdgeId::from_u128(1);
        let front = EdgeId::from_u128(2);
        let mut index = RouteGeometryIndex::default();
        assert!(
            index
                .upsert(back, &straight_route(back, -50.0, 0.0, 50.0, 0.0))
                .index_changed
        );
        assert!(
            index
                .upsert(front, &straight_route(front, 0.0, -50.0, 0.0, 50.0))
                .index_changed
        );
        let ranks = BTreeMap::from([(back, 0), (front, 1)]);

        let (pick, _) = complete_pick(
            cursor(&index, ViewPoint::new(0.0, 0.0), 4.0, 1.0),
            &mut index,
            &ranks,
            &HashSet::new(),
            64,
        );
        assert_eq!(pick.map(EdgePick::edge), Some(front));

        let selected = HashSet::from([back]);
        let (pick, _) = complete_pick(
            cursor(&index, ViewPoint::new(0.0, 0.0), 4.0, 1.0),
            &mut index,
            &ranks,
            &selected,
            64,
        );
        assert_eq!(pick.map(EdgePick::edge), Some(back));

        let closer = EdgeId::from_u128(3);
        index.upsert(closer, &straight_route(closer, -50.0, 1.0, 50.0, 1.0));
        let selected = HashSet::from([back]);
        let (pick, _) = complete_pick(
            cursor(&index, ViewPoint::new(20.0, 1.0), 4.0, 1.0),
            &mut index,
            &ranks,
            &selected,
            64,
        );
        assert_eq!(pick.map(EdgePick::edge), Some(closer));
    }

    #[test]
    fn non_selectable_edge_cannot_steal_input() {
        let selectable = EdgeId::from_u128(1);
        let blocked = EdgeId::from_u128(2);
        let mut index = RouteGeometryIndex::default();
        index.upsert(
            selectable,
            &route_with_options(selectable, -50.0, 0.0, 50.0, 0.0, 18.0, true),
        );
        index.upsert(
            blocked,
            &route_with_options(blocked, -50.0, 0.0, 50.0, 0.0, 80.0, false),
        );
        let ranks = BTreeMap::from([(selectable, 0), (blocked, 1)]);

        let (pick, work) = complete_pick(
            cursor(&index, ViewPoint::new(0.0, 0.0), 0.0, 1.0),
            &mut index,
            &ranks,
            &HashSet::new(),
            64,
        );

        assert_eq!(pick.map(EdgePick::edge), Some(selectable));
        assert_eq!(work.exact_tests, 1);
    }

    #[test]
    fn screen_space_radius_and_interaction_width_are_zoom_invariant() {
        for zoom in [0.5, 1.0, 2.0] {
            let edge = EdgeId::from_u128(1);
            let mut index = RouteGeometryIndex::default();
            index.upsert(
                edge,
                &route_with_options(edge, -100.0, 0.0, 100.0, 0.0, 18.0, true),
            );

            let (inside, _) = complete_pick(
                cursor(&index, ViewPoint::new(0.0, 9.0), 0.0, zoom),
                &mut index,
                &BTreeMap::new(),
                &HashSet::new(),
                64,
            );
            assert_eq!(inside.map(EdgePick::edge), Some(edge), "zoom {zoom}");
            assert_eq!(
                inside.map(EdgePick::distance_in_view),
                Some(9.0),
                "zoom {zoom}"
            );

            let (outside, _) = complete_pick(
                cursor(&index, ViewPoint::new(0.0, 9.25), 0.0, zoom),
                &mut index,
                &BTreeMap::new(),
                &HashSet::new(),
                64,
            );
            assert_eq!(outside, None, "zoom {zoom}");

            let (query_radius, _) = complete_pick(
                cursor(&index, ViewPoint::new(0.0, 12.0), 12.0, zoom),
                &mut index,
                &BTreeMap::new(),
                &HashSet::new(),
                64,
            );
            assert_eq!(query_radius.map(EdgePick::edge), Some(edge), "zoom {zoom}");
        }
    }

    #[test]
    fn curved_edge_pick_compensates_for_flattening_error() {
        let edge = EdgeId::from_u128(1);
        let mut index = RouteGeometryIndex::default();
        index.upsert(edge, &high_curvature_route(edge, 0.02));

        let (pick, work) = complete_pick(
            cursor(&index, ViewPoint::new(21.6, 63.0), 0.0, 1.0),
            &mut index,
            &BTreeMap::new(),
            &HashSet::new(),
            64,
        );

        assert_eq!(pick.map(EdgePick::edge), Some(edge));
        assert_eq!(work.exact_tests, 1);
    }

    #[test]
    fn collinear_cubic_overshoot_remains_pickable_beyond_its_endpoint_chord() {
        let edge = EdgeId::from_u128(1);
        let mut index = RouteGeometryIndex::default();
        index.upsert(edge, &overshooting_collinear_route(edge));

        let (pick, work) = complete_pick(
            cursor(&index, ViewPoint::new(29.6875, 0.0), 0.0, 1.0),
            &mut index,
            &BTreeMap::new(),
            &HashSet::new(),
            64,
        );

        assert_eq!(pick.map(EdgePick::edge), Some(edge));
        assert_eq!(work.exact_tests, 1);
    }

    #[test]
    fn extreme_finite_cubic_pick_is_geometry_indeterminate_instead_of_false() {
        let edge = EdgeId::from_u128(1);
        let mut index = RouteGeometryIndex::default();
        assert!(
            index
                .upsert(edge, &extreme_finite_route(edge))
                .index_changed
        );
        let cursor = cursor(&index, ViewPoint::new(0.0, 0.0), 1.0, 1.0);

        let outcome = index.query_pick_page(&BTreeMap::new(), &HashSet::new(), cursor, 64);

        let InternalEdgePickOutcome::GeometryIndeterminate { failures, work } = outcome else {
            panic!("unrepresentable flatten distance must be explicit");
        };
        assert_eq!(
            failures.get(&edge),
            Some(&EdgePathFlattenError::InvalidGeometry)
        );
        assert_eq!(work.flatten_overflows, 1);
    }

    #[test]
    fn maximum_interaction_width_keeps_wide_edges_in_the_local_scan() {
        let edge = EdgeId::from_u128(1);
        let mut index = RouteGeometryIndex::default();
        index.upsert(
            edge,
            &route_with_options(edge, -100.0, 30.0, 100.0, 30.0, 80.0, true),
        );

        let (pick, work) = complete_pick(
            cursor(&index, ViewPoint::new(0.0, 0.0), 0.0, 1.0),
            &mut index,
            &BTreeMap::new(),
            &HashSet::new(),
            64,
        );

        assert_eq!(pick.map(EdgePick::edge), Some(edge));
        assert_eq!(pick.map(EdgePick::distance_in_view), Some(30.0));
        assert_eq!(work.exact_tests, 1);

        let path_epoch = index.index_epoch();
        index.upsert(
            edge,
            &route_with_options(edge, -100.0, 30.0, 100.0, 30.0, 18.0, true),
        );
        assert_eq!(index.index_epoch(), path_epoch);
        assert_eq!(index.max_interaction_half_width(), 9.0);
        let (pick, work) = complete_pick(
            cursor(&index, ViewPoint::new(0.0, 0.0), 0.0, 1.0),
            &mut index,
            &BTreeMap::new(),
            &HashSet::new(),
            64,
        );
        assert_eq!(pick, None);
        assert_eq!(work.exact_tests, 0);
    }

    #[test]
    fn edge_pick_resumes_deterministically_across_small_pages() {
        let mut index = RouteGeometryIndex::default();
        let mut ranks = BTreeMap::new();
        for ordinal in 0..8_u128 {
            let edge = EdgeId::from_u128(ordinal + 1);
            index.upsert(edge, &straight_route(edge, -50.0, 0.0, 50.0, 0.0));
            ranks.insert(edge, ordinal as usize);
        }
        let expected = EdgeId::from_u128(8);

        let (pick, work) = complete_pick(
            cursor(&index, ViewPoint::new(0.0, 0.0), 4.0, 1.0),
            &mut index,
            &ranks,
            &HashSet::new(),
            1,
        );

        assert_eq!(pick.map(EdgePick::edge), Some(expected));
        assert_eq!(work.exact_tests, 8);
        assert!(work.entries_examined >= 8);
    }

    #[test]
    fn flatten_overflow_is_geometry_indeterminate() {
        let edge = EdgeId::from_u128(1);
        let mut route = straight_route(edge, -10.0, 0.0, 10.0, 0.0);
        let mut commands = vec![PathCommand::MoveTo(CanvasPoint { x: -10.0, y: 0.0 })];
        commands.extend((0..=256).map(|ordinal| {
            let x = if ordinal % 2 == 0 { 10.0 } else { -10.0 };
            PathCommand::LineTo(CanvasPoint { x, y: 0.0 })
        }));
        let mut facts = route.facts().clone();
        facts.path = EdgePath {
            commands,
            label: EdgePathLabel {
                point: CanvasPoint { x: 0.0, y: 0.0 },
                offset_x: 0.0,
                offset_y: 0.0,
            },
        };
        route = PublishedEdgeRoute::new_for_test(edge, facts);
        let mut index = RouteGeometryIndex::default();
        index.upsert(edge, &route);
        let cursor = cursor(&index, ViewPoint::new(0.0, 0.0), 4.0, 1.0);

        let outcome = index.query_pick_page(&BTreeMap::new(), &HashSet::new(), cursor, 64);

        let InternalEdgePickOutcome::GeometryIndeterminate { failures, work } = outcome else {
            panic!("overflowing route must not produce a silently wrong pick");
        };
        assert_eq!(
            failures.get(&edge),
            Some(&EdgePathFlattenError::SegmentLimitExceeded { max_segments: 256 })
        );
        assert_eq!(work.exact_tests, 1);
        assert_eq!(work.flatten_overflows, 1);
    }

    #[test]
    fn paint_only_changes_preserve_pick_epoch_and_path_cache() {
        let edge = EdgeId::from_u128(1);
        let mut index = RouteGeometryIndex::default();
        let route = straight_route(edge, -50.0, 0.0, 50.0, 0.0);
        index.upsert(edge, &route);
        let old_cursor = cursor(&index, ViewPoint::new(0.0, 0.0), 4.0, 1.0);
        let (_, first_work) = complete_pick(
            cursor(&index, ViewPoint::new(0.0, 0.0), 4.0, 1.0),
            &mut index,
            &BTreeMap::new(),
            &HashSet::new(),
            64,
        );
        assert_eq!(first_work.flatten_cache_misses, 1);

        let mut changed_facts = route.facts().clone();
        changed_facts.presentation.label = Some("changed".into());
        let changed = PublishedEdgeRoute::new_for_test(edge, changed_facts);
        assert!(!index.upsert(edge, &changed).index_changed);
        assert_eq!(old_cursor.path_epoch, index.index_epoch());
        assert_eq!(old_cursor.pick_epoch, index.pick_epoch());

        let (_, second_work) = complete_pick(
            cursor(&index, ViewPoint::new(0.0, 0.0), 4.0, 1.0),
            &mut index,
            &BTreeMap::new(),
            &HashSet::new(),
            64,
        );
        assert_eq!(second_work.flatten_cache_hits, 1);
        assert_eq!(second_work.flatten_cache_misses, 0);
    }

    #[test]
    fn selectable_change_stales_pick_epoch_without_rebuilding_path_cache() {
        let edge = EdgeId::from_u128(1);
        let mut index = RouteGeometryIndex::default();
        let route = straight_route(edge, -50.0, 0.0, 50.0, 0.0);
        index.upsert(edge, &route);
        let old_cursor = cursor(&index, ViewPoint::new(0.0, 0.0), 4.0, 1.0);
        let (_, first_work) = complete_pick(
            cursor(&index, ViewPoint::new(0.0, 0.0), 4.0, 1.0),
            &mut index,
            &BTreeMap::new(),
            &HashSet::new(),
            64,
        );
        assert_eq!(first_work.flatten_cache_misses, 1);

        let mut changed_facts = route.facts().clone();
        changed_facts.interaction.selectable = false;
        let changed = PublishedEdgeRoute::new_for_test(edge, changed_facts);
        assert!(!index.upsert(edge, &changed).index_changed);
        assert_eq!(old_cursor.path_epoch, index.index_epoch());
        assert_ne!(old_cursor.pick_epoch, index.pick_epoch());

        let (_, second_work) = complete_pick(
            cursor(&index, ViewPoint::new(0.0, 0.0), 4.0, 1.0),
            &mut index,
            &BTreeMap::new(),
            &HashSet::new(),
            64,
        );
        assert_eq!(second_work.flatten_cache_hits, 0);
        assert_eq!(second_work.flatten_cache_misses, 0);
    }

    #[test]
    fn ten_thousand_routes_only_exact_test_the_local_candidate() {
        const GRID_SIDE: u128 = 100;
        const SPACING: f32 = 512.0;

        let mut index = RouteGeometryIndex::default();
        for row in 0..GRID_SIDE {
            for column in 0..GRID_SIDE {
                let ordinal = row * GRID_SIDE + column;
                let edge = EdgeId::from_u128(ordinal + 1);
                let x = column as f32 * SPACING;
                let y = row as f32 * SPACING;
                index.upsert(edge, &straight_route(edge, x, y, x + 100.0, y));
            }
        }

        let (pick, work) = complete_pick(
            cursor(&index, ViewPoint::new(50.0, 0.0), 4.0, 1.0),
            &mut index,
            &BTreeMap::new(),
            &HashSet::new(),
            256,
        );

        assert_eq!(pick.map(EdgePick::edge), Some(EdgeId::from_u128(1)));
        assert!(work.entries_examined <= 8, "{work:?}");
        assert_eq!(work.exact_tests, 1);
        assert_eq!(work.flatten_cache_misses, 1);
    }

    fn complete_pick(
        mut cursor: EdgePickCursor,
        index: &mut RouteGeometryIndex,
        ranks: &BTreeMap<EdgeId, usize>,
        selected: &HashSet<EdgeId>,
        page_budget: usize,
    ) -> (Option<EdgePick>, EdgePickWork) {
        let mut total = EdgePickWork::default();
        for _ in 0..128 {
            match index.query_pick_page(ranks, selected, cursor, page_budget) {
                InternalEdgePickOutcome::Complete { pick, work } => {
                    accumulate_work(&mut total, work);
                    return (pick, total);
                }
                InternalEdgePickOutcome::Incomplete { cursor: next, work } => {
                    accumulate_work(&mut total, work);
                    cursor = *next;
                }
                InternalEdgePickOutcome::BudgetExhausted { .. } => {
                    panic!("small fixture must not exhaust the hard budget")
                }
                InternalEdgePickOutcome::GeometryIndeterminate { failures, .. } => {
                    panic!("unexpected geometry failure: {failures:?}")
                }
            }
        }
        panic!("edge-pick query did not finish")
    }

    fn accumulate_work(total: &mut EdgePickWork, page: EdgePickWork) {
        total.cells_examined = total.cells_examined.saturating_add(page.cells_examined);
        total.entries_examined = total.entries_examined.saturating_add(page.entries_examined);
        total.exact_tests = total.exact_tests.saturating_add(page.exact_tests);
        total.candidates_matched = total
            .candidates_matched
            .saturating_add(page.candidates_matched);
        total.flatten_cache_hits = total
            .flatten_cache_hits
            .saturating_add(page.flatten_cache_hits);
        total.flatten_cache_misses = total
            .flatten_cache_misses
            .saturating_add(page.flatten_cache_misses);
        total.flatten_cache_evictions = total
            .flatten_cache_evictions
            .saturating_add(page.flatten_cache_evictions);
        total.flatten_segments_built = total
            .flatten_segments_built
            .saturating_add(page.flatten_segments_built);
        total.flatten_overflows = total
            .flatten_overflows
            .saturating_add(page.flatten_overflows);
    }

    fn cursor(
        index: &RouteGeometryIndex,
        pointer: ViewPoint,
        radius_in_view: f32,
        zoom: f32,
    ) -> EdgePickCursor {
        index
            .new_pick_cursor(
                ViewQueryIdentity::new(
                    ViewId::unavailable(),
                    ViewGeneration::initial(),
                    ViewportRevision::initial(),
                ),
                0,
                pointer,
                radius_in_view,
                ViewportTransform::new(CanvasPoint::default(), zoom).expect("transform"),
            )
            .expect("cursor")
    }

    fn straight_route(
        edge: EdgeId,
        from_x: f32,
        from_y: f32,
        to_x: f32,
        to_y: f32,
    ) -> PublishedEdgeRoute {
        route_with_options(edge, from_x, from_y, to_x, to_y, 18.0, true)
    }

    #[allow(clippy::too_many_arguments)]
    fn route_with_options(
        edge: EdgeId,
        from_x: f32,
        from_y: f32,
        to_x: f32,
        to_y: f32,
        interaction_width: f32,
        selectable: bool,
    ) -> PublishedEdgeRoute {
        let mut model = Edge::new(EdgeKind::Data, PortId::from_u128(1), PortId::from_u128(2));
        model.view = EdgeViewDescriptor::new().with_route(EdgeRouteSpec::straight());
        model.interaction_width = Some(interaction_width);
        model.selectable = Some(selectable);
        let facts = resolve_edge_route_path(
            &model,
            EdgePosition {
                source: EdgeEndpointPosition {
                    point: CanvasPoint {
                        x: from_x,
                        y: from_y,
                    },
                    position: HandlePosition::Right,
                },
                target: EdgeEndpointPosition {
                    point: CanvasPoint { x: to_x, y: to_y },
                    position: HandlePosition::Left,
                },
            },
        )
        .expect("route");
        PublishedEdgeRoute::new_for_test(edge, facts)
    }

    fn high_curvature_route(edge: EdgeId, interaction_width: f32) -> PublishedEdgeRoute {
        let route = route_with_options(edge, 0.0, 0.0, 100.0, 0.0, interaction_width, true);
        let mut facts = route.facts().clone();
        facts.path = EdgePath {
            commands: vec![
                PathCommand::MoveTo(CanvasPoint { x: 0.0, y: 0.0 }),
                PathCommand::CubicTo {
                    control1: CanvasPoint { x: 0.0, y: 100.0 },
                    control2: CanvasPoint { x: 100.0, y: 100.0 },
                    to: CanvasPoint { x: 100.0, y: 0.0 },
                },
            ],
            label: EdgePathLabel {
                point: CanvasPoint { x: 50.0, y: 75.0 },
                offset_x: 0.0,
                offset_y: 0.0,
            },
        };
        PublishedEdgeRoute::new_for_test(edge, facts)
    }

    fn overshooting_collinear_route(edge: EdgeId) -> PublishedEdgeRoute {
        let route = route_with_options(edge, 0.0, 0.0, 10.0, 0.0, 0.02, true);
        let mut facts = route.facts().clone();
        facts.path = EdgePath {
            commands: vec![
                PathCommand::MoveTo(CanvasPoint { x: 0.0, y: 0.0 }),
                PathCommand::CubicTo {
                    control1: CanvasPoint { x: 100.0, y: 0.0 },
                    control2: CanvasPoint { x: -90.0, y: 0.0 },
                    to: CanvasPoint { x: 10.0, y: 0.0 },
                },
            ],
            label: EdgePathLabel {
                point: CanvasPoint { x: 5.0, y: 0.0 },
                offset_x: 0.0,
                offset_y: 0.0,
            },
        };
        PublishedEdgeRoute::new_for_test(edge, facts)
    }

    fn extreme_finite_route(edge: EdgeId) -> PublishedEdgeRoute {
        let extent = 1.0e20;
        let route = route_with_options(edge, -extent, 0.0, extent, 0.0, 2.0, true);
        let mut facts = route.facts().clone();
        facts.path = EdgePath {
            commands: vec![
                PathCommand::MoveTo(CanvasPoint { x: -extent, y: 0.0 }),
                PathCommand::CubicTo {
                    control1: CanvasPoint {
                        x: -extent,
                        y: extent,
                    },
                    control2: CanvasPoint {
                        x: extent,
                        y: -extent,
                    },
                    to: CanvasPoint { x: extent, y: 0.0 },
                },
            ],
            label: EdgePathLabel {
                point: CanvasPoint::default(),
                offset_x: 0.0,
                offset_y: 0.0,
            },
        };
        PublishedEdgeRoute::new_for_test(edge, facts)
    }
}
