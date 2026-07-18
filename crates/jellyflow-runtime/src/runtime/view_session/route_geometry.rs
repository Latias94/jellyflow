use std::collections::{BTreeMap, BTreeSet};
use std::ops::Bound::{Excluded, Unbounded};
use std::sync::Arc;

use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize, EdgeId};

use crate::runtime::adapter_protocol::{GeometrySnapshot, PublishedEdgeRoute, ViewRect};
use crate::runtime::geometry::{
    EdgeFlattenOptions, EdgePath, EdgePathFlattenError, EdgePathSegment, FlattenedEdgePath,
    edge_path_conservative_bounds, flatten_edge_path,
};
use crate::runtime::viewport::ViewportTransform;

use super::order::sort_render_order;
use super::spatial_grid::{
    HierarchicalSpatialIndex, SpatialCell, SpatialQuerySource, bounded_spatial_cells_for_rect,
    max_spatial_cell_level, spatial_cell_coords, spatial_cell_size,
};
use super::{ViewGeneration, ViewId, ViewSession, ViewSessionError, ensure_open};

const ROUTE_INDEX_EPSILON: f32 = 0.001;
const MAX_CELLS_PER_SEGMENT: usize = 128;
const MAX_QUERY_CELLS_PER_LEVEL: i64 = 4_096;
const INDEX_FLATTEN_ERROR: f32 = 8.0;
const HARD_ROUTE_QUERY_ENTRY_LIMIT: u64 = 16_384;

#[derive(Debug)]
pub struct VisibleRouteQuery {
    request: VisibleRouteRequest,
    entry_budget: usize,
}

impl VisibleRouteQuery {
    pub fn new(bounds: ViewRect, entry_budget: usize) -> Self {
        Self {
            request: VisibleRouteRequest::Start {
                bounds,
                margin: 0.0,
            },
            entry_budget,
        }
    }

    pub fn with_margin(mut self, margin: f32) -> Self {
        if let VisibleRouteRequest::Start {
            margin: current, ..
        } = &mut self.request
        {
            *current = margin;
        }
        self
    }

    pub fn resume(cursor: Box<VisibleRouteCursor>, entry_budget: usize) -> Self {
        Self {
            request: VisibleRouteRequest::Resume(cursor),
            entry_budget,
        }
    }
}

#[derive(Debug)]
enum VisibleRouteRequest {
    Start { bounds: ViewRect, margin: f32 },
    Resume(Box<VisibleRouteCursor>),
}

#[derive(Debug)]
pub struct VisibleRouteCursor {
    view: ViewId,
    generation: ViewGeneration,
    transform_revision: u64,
    index_epoch: u128,
    canvas_bounds: CanvasRect,
    scan: RouteScan,
    source_index: usize,
    source_started: bool,
    last_entry: Option<RouteSegmentKey>,
    checked: BTreeSet<EdgeId>,
    matched: BTreeSet<EdgeId>,
    flatten_overflows: BTreeMap<EdgeId, EdgePathFlattenError>,
    total_entries_examined: u64,
}

#[derive(Debug)]
struct RouteScan {
    sources: Vec<RouteQuerySource>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum RouteQuerySource {
    Cell(SpatialCell),
    Level(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct RouteSegmentKey {
    edge: EdgeId,
    ordinal: usize,
}

impl RouteSegmentKey {
    pub(super) fn edge(self) -> EdgeId {
        self.edge
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VisibleRouteQueryWork {
    pub cells_examined: u64,
    pub entries_examined: u64,
    pub candidates_matched: u64,
    pub flatten_cache_hits: u64,
    pub flatten_cache_misses: u64,
    pub flatten_segments_built: u64,
    pub flatten_overflows: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisibleRouteGeometryOverflow {
    edge: EdgeId,
    error: EdgePathFlattenError,
}

impl VisibleRouteGeometryOverflow {
    pub fn edge(self) -> EdgeId {
        self.edge
    }

    pub fn error(self) -> EdgePathFlattenError {
        self.error
    }
}

#[derive(Debug, Clone)]
pub struct VisibleRoutes {
    snapshot: GeometrySnapshot,
    edges: Vec<EdgeId>,
    geometry_revisions: BTreeMap<EdgeId, RouteGeometryRevision>,
    geometry_overflows: Vec<VisibleRouteGeometryOverflow>,
}

/// Stable identity for one edge's resolved path geometry inside a view session.
///
/// Selection, hover, and adapter theme changes do not change this revision. Adapters can use it
/// to reuse native path resources until the authoritative route itself changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RouteGeometryRevision(u128);

impl RouteGeometryRevision {
    pub fn get(self) -> u128 {
        self.0
    }
}

/// Stable invalidation stamp for one view's visible-route query result.
///
/// Adapters can retain a complete query result while this value and their query bounds remain
/// unchanged. The stamp advances for viewport, route-fact, selection, or draw-order changes even
/// when the resolved path geometry itself remains identical.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VisibleRouteQueryRevision {
    view: ViewId,
    generation: ViewGeneration,
    transform_revision: u64,
    route_facts_revision: u128,
    interaction_order_revision: u128,
}

/// One visible edge route paired with its geometry-only revision.
#[derive(Debug, Clone, Copy)]
pub struct VisibleRoute<'a> {
    edge: EdgeId,
    route: &'a PublishedEdgeRoute,
    geometry_revision: RouteGeometryRevision,
}

impl<'a> VisibleRoute<'a> {
    pub fn edge(self) -> EdgeId {
        self.edge
    }

    pub fn route(self) -> &'a PublishedEdgeRoute {
        self.route
    }

    pub fn facts(self) -> &'a crate::runtime::geometry::EdgeRouteFacts {
        self.route.facts()
    }

    pub fn geometry_revision(self) -> RouteGeometryRevision {
        self.geometry_revision
    }
}

impl VisibleRoutes {
    pub fn edge_ids(&self) -> &[EdgeId] {
        &self.edges
    }

    pub fn iter(&self) -> impl Iterator<Item = VisibleRoute<'_>> {
        self.edges.iter().filter_map(|edge| {
            Some(VisibleRoute {
                edge: *edge,
                route: self.snapshot.route(*edge)?,
                geometry_revision: *self.geometry_revisions.get(edge)?,
            })
        })
    }

    pub fn snapshot(&self) -> &GeometrySnapshot {
        &self.snapshot
    }

    pub fn geometry_overflows(&self) -> &[VisibleRouteGeometryOverflow] {
        &self.geometry_overflows
    }
}

#[derive(Debug)]
pub enum VisibleRouteQueryOutcome {
    Complete {
        routes: VisibleRoutes,
        work: VisibleRouteQueryWork,
    },
    Incomplete {
        cursor: Box<VisibleRouteCursor>,
        work: VisibleRouteQueryWork,
    },
    BudgetExhausted {
        work: VisibleRouteQueryWork,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum VisibleRouteCursorStaleReason {
    #[error("the cursor belongs to another view")]
    ForeignView,
    #[error("the view generation changed")]
    ViewGeneration,
    #[error("the viewport transform changed")]
    ViewportTransform,
    #[error("indexed route geometry changed")]
    RouteGeometry,
}

impl VisibleRouteCursorStaleReason {
    /// Returns whether ordinary mutable view state can invalidate this cursor between pages.
    pub const fn is_restartable(self) -> bool {
        matches!(self, Self::ViewportTransform | Self::RouteGeometry)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum VisibleRouteQueryError {
    #[error(transparent)]
    Session(#[from] ViewSessionError),
    #[error("visible-route query bounds or margin are invalid")]
    InvalidQuery,
    #[error(transparent)]
    StaleCursor(VisibleRouteCursorStaleReason),
}

#[derive(Debug, Default)]
pub(super) struct RouteGeometryIndex {
    spatial: RouteSpatialIndex,
    indexed: BTreeMap<EdgeId, IndexedRoute>,
    flatten_cache: BTreeMap<EdgeId, CachedFlatten>,
    interaction_half_widths: BTreeMap<u32, usize>,
    index_epoch: u128,
    pick_epoch: u128,
    next_path_revision: u128,
}

#[derive(Debug, Clone)]
struct IndexedRoute {
    path_revision: u128,
    path: EdgePath,
    bounds: CanvasRect,
    pick: IndexedRoutePickFacts,
}

#[derive(Debug, Clone, PartialEq)]
struct IndexedRoutePickFacts {
    interaction_width: f32,
    selectable: bool,
}

impl From<&PublishedEdgeRoute> for IndexedRoutePickFacts {
    fn from(route: &PublishedEdgeRoute) -> Self {
        Self {
            interaction_width: route.facts().hit_test.interaction_width,
            selectable: route.facts().interaction.selectable,
        }
    }
}

#[derive(Debug)]
struct CachedFlatten {
    path_revision: u128,
    tolerance_bucket: u32,
    result: Result<Arc<FlattenedEdgePath>, EdgePathFlattenError>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct RouteFlattenWork {
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub segments_built: u64,
    pub overflows: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct RoutePickFacts {
    pub bounds: CanvasRect,
    pub interaction_width: f32,
    pub selectable: bool,
}

fn interaction_half_width(interaction_width: f32) -> f32 {
    if interaction_width.is_finite() && interaction_width > 0.0 {
        interaction_width * 0.5
    } else {
        crate::runtime::geometry::DEFAULT_EDGE_INTERACTION_WIDTH * 0.5
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct RouteIndexMutation {
    pub index_changed: bool,
    pub cache_invalidations: u64,
}

#[derive(Debug, Default)]
struct RouteSpatialIndex {
    spatial: HierarchicalSpatialIndex<RouteSegmentKey>,
    routes: BTreeMap<EdgeId, Vec<RouteSegmentKey>>,
}

impl RouteSpatialIndex {
    fn upsert_segments(&mut self, edge: EdgeId, segments: &[EdgePathSegment]) {
        self.remove(edge);
        let entries = segments
            .iter()
            .enumerate()
            .map(|(ordinal, segment)| {
                let key = RouteSegmentKey { edge, ordinal };
                let cells = route_cells_for_segment(segment.from(), segment.to());
                self.spatial.upsert(key, cells);
                key
            })
            .collect();
        self.routes.insert(edge, entries);
    }

    fn upsert_bounds(&mut self, edge: EdgeId, bounds: CanvasRect) {
        self.remove(edge);
        let key = RouteSegmentKey { edge, ordinal: 0 };
        let cells = route_cells_for_bounds(bounds);
        self.spatial.upsert(key, cells);
        self.routes.insert(edge, vec![key]);
    }

    fn remove(&mut self, edge: EdgeId) -> bool {
        let Some(entries) = self.routes.remove(&edge) else {
            return false;
        };
        for entry in entries {
            self.spatial.remove(entry);
        }
        true
    }

    fn clear(&mut self) {
        self.spatial.clear();
        self.routes.clear();
    }

    fn query_sources(&self, bounds: CanvasRect) -> Vec<RouteQuerySource> {
        let query_bounds = inflate_canvas_rect(bounds, INDEX_FLATTEN_ERROR).unwrap_or(bounds);
        self.spatial
            .query_sources(query_bounds, MAX_QUERY_CELLS_PER_LEVEL)
            .into_iter()
            .map(|source| match source {
                SpatialQuerySource::Cell(cell) => RouteQuerySource::Cell(cell),
                SpatialQuerySource::Level(level) => RouteQuerySource::Level(level),
            })
            .collect()
    }

    fn source(&self, source: RouteQuerySource) -> Option<&BTreeSet<RouteSegmentKey>> {
        match source {
            RouteQuerySource::Cell(cell) => self.spatial.source(SpatialQuerySource::Cell(cell)),
            RouteQuerySource::Level(level) => self.spatial.source(SpatialQuerySource::Level(level)),
        }
    }
}

impl RouteGeometryIndex {
    fn geometry_revision(&self, edge: EdgeId) -> Option<RouteGeometryRevision> {
        self.indexed
            .get(&edge)
            .map(|route| RouteGeometryRevision(route.path_revision))
    }

    pub(super) fn upsert(
        &mut self,
        edge: EdgeId,
        route: &PublishedEdgeRoute,
    ) -> RouteIndexMutation {
        let pick = IndexedRoutePickFacts::from(route);
        if let Some(indexed) = self.indexed.get(&edge)
            && indexed.path == route.facts().path
        {
            if indexed.pick == pick {
                return RouteIndexMutation::default();
            }
            let old_width = indexed.pick.interaction_width;
            self.replace_interaction_half_width(old_width, pick.interaction_width);
            self.indexed
                .get_mut(&edge)
                .expect("the route was read immediately before mutation")
                .pick = pick;
            self.pick_epoch = self.pick_epoch.saturating_add(1);
            return RouteIndexMutation::default();
        }
        let Some(bounds) = edge_path_conservative_bounds(&route.facts().path)
            .ok()
            .flatten()
            .and_then(expand_route_bounds)
        else {
            return self.remove(edge);
        };
        let cache_invalidations = u64::from(self.flatten_cache.remove(&edge).is_some());
        self.next_path_revision = self.next_path_revision.saturating_add(1);
        let path_revision = self.next_path_revision;
        if let Some(indexed) = self.indexed.get(&edge) {
            self.remove_interaction_half_width(indexed.pick.interaction_width);
        }
        self.insert_interaction_half_width(pick.interaction_width);
        match flatten_edge_path(
            &route.facts().path,
            EdgeFlattenOptions::new(INDEX_FLATTEN_ERROR),
        ) {
            Ok(path) => {
                self.spatial.upsert_segments(edge, path.segments());
            }
            Err(_) => self.spatial.upsert_bounds(edge, bounds),
        }
        self.indexed.insert(
            edge,
            IndexedRoute {
                path_revision,
                path: route.facts().path.clone(),
                bounds,
                pick,
            },
        );
        self.index_epoch = self.index_epoch.saturating_add(1);
        self.pick_epoch = self.pick_epoch.saturating_add(1);
        RouteIndexMutation {
            index_changed: true,
            cache_invalidations,
        }
    }

    pub(super) fn remove(&mut self, edge: EdgeId) -> RouteIndexMutation {
        let indexed = self.indexed.remove(&edge);
        if let Some(indexed) = &indexed {
            self.remove_interaction_half_width(indexed.pick.interaction_width);
        }
        let spatial = self.spatial.remove(edge);
        let cache_invalidations = u64::from(self.flatten_cache.remove(&edge).is_some());
        if indexed.is_some() || spatial {
            self.index_epoch = self.index_epoch.saturating_add(1);
            self.pick_epoch = self.pick_epoch.saturating_add(1);
        }
        RouteIndexMutation {
            index_changed: indexed.is_some() || spatial,
            cache_invalidations,
        }
    }

    pub(super) fn clear(&mut self) -> RouteIndexMutation {
        let index_changed = !self.indexed.is_empty();
        let cache_invalidations = self.flatten_cache.len() as u64;
        self.spatial.clear();
        self.indexed.clear();
        self.flatten_cache.clear();
        self.interaction_half_widths.clear();
        if index_changed {
            self.index_epoch = self.index_epoch.saturating_add(1);
            self.pick_epoch = self.pick_epoch.saturating_add(1);
        }
        RouteIndexMutation {
            index_changed,
            cache_invalidations,
        }
    }

    pub(super) fn index_epoch(&self) -> u128 {
        self.index_epoch
    }

    pub(super) fn pick_epoch(&self) -> u128 {
        self.pick_epoch
    }

    pub(super) fn max_interaction_half_width(&self) -> f32 {
        self.interaction_half_widths
            .last_key_value()
            .map_or(0.0, |(bits, _)| f32::from_bits(*bits))
    }

    pub(super) fn pick_query_sources(&self, bounds: CanvasRect) -> Vec<RouteQuerySource> {
        self.spatial.query_sources(bounds)
    }

    pub(super) fn pick_source(
        &self,
        source: RouteQuerySource,
    ) -> Option<&BTreeSet<RouteSegmentKey>> {
        self.spatial.source(source)
    }

    pub(super) fn pick_facts(&self, edge: EdgeId) -> Option<RoutePickFacts> {
        let indexed = self.indexed.get(&edge)?;
        Some(RoutePickFacts {
            bounds: indexed.bounds,
            interaction_width: indexed.pick.interaction_width,
            selectable: indexed.pick.selectable,
        })
    }

    pub(super) fn flattened_for_pick(
        &mut self,
        edge: EdgeId,
        zoom: f32,
        work: &mut RouteFlattenWork,
    ) -> Result<Arc<FlattenedEdgePath>, EdgePathFlattenError> {
        self.flattened(edge, zoom, work)
    }

    fn insert_interaction_half_width(&mut self, interaction_width: f32) {
        let bits = interaction_half_width(interaction_width).to_bits();
        *self.interaction_half_widths.entry(bits).or_default() += 1;
    }

    fn remove_interaction_half_width(&mut self, interaction_width: f32) {
        let bits = interaction_half_width(interaction_width).to_bits();
        let remove = self
            .interaction_half_widths
            .get_mut(&bits)
            .is_some_and(|count| {
                *count = count.saturating_sub(1);
                *count == 0
            });
        if remove {
            self.interaction_half_widths.remove(&bits);
        }
    }

    fn replace_interaction_half_width(&mut self, old: f32, new: f32) {
        if interaction_half_width(old).to_bits() == interaction_half_width(new).to_bits() {
            return;
        }
        self.remove_interaction_half_width(old);
        self.insert_interaction_half_width(new);
    }

    fn new_cursor(
        &self,
        view: ViewId,
        generation: ViewGeneration,
        transform_revision: u64,
        canvas_bounds: CanvasRect,
    ) -> VisibleRouteCursor {
        let scan = RouteScan {
            sources: self.spatial.query_sources(canvas_bounds),
        };
        VisibleRouteCursor {
            view,
            generation,
            transform_revision,
            index_epoch: self.index_epoch,
            canvas_bounds,
            scan,
            source_index: 0,
            source_started: false,
            last_entry: None,
            checked: BTreeSet::new(),
            matched: BTreeSet::new(),
            flatten_overflows: BTreeMap::new(),
            total_entries_examined: 0,
        }
    }

    fn query_page(
        &mut self,
        mut cursor: VisibleRouteCursor,
        entry_budget: usize,
        zoom: f32,
    ) -> InternalRouteQueryOutcome {
        let mut work = VisibleRouteQueryWork::default();
        let mut remaining = entry_budget;
        loop {
            if cursor.source_index >= cursor.scan.sources.len() {
                return InternalRouteQueryOutcome::Complete {
                    edges: cursor.matched,
                    overflows: cursor.flatten_overflows,
                    work,
                };
            }
            if !cursor.source_started {
                if matches!(
                    cursor.scan.sources[cursor.source_index],
                    RouteQuerySource::Cell(_)
                ) {
                    work.cells_examined = work.cells_examined.saturating_add(1);
                }
                cursor.source_started = true;
            }
            let source = cursor.scan.sources[cursor.source_index];
            let next = self
                .spatial
                .source(source)
                .and_then(|source| match cursor.last_entry {
                    Some(last) => source.range((Excluded(last), Unbounded)).next().copied(),
                    None => source.iter().next().copied(),
                });
            let Some(entry) = next else {
                cursor.source_index += 1;
                cursor.source_started = false;
                cursor.last_entry = None;
                continue;
            };
            if remaining == 0 {
                return InternalRouteQueryOutcome::Incomplete { cursor, work };
            }
            if cursor.total_entries_examined >= HARD_ROUTE_QUERY_ENTRY_LIMIT {
                return InternalRouteQueryOutcome::BudgetExhausted { work };
            }
            remaining -= 1;
            cursor.last_entry = Some(entry);
            cursor.total_entries_examined = cursor.total_entries_examined.saturating_add(1);
            work.entries_examined = work.entries_examined.saturating_add(1);

            let edge = entry.edge;
            if !cursor.checked.insert(edge) {
                continue;
            }
            match self.route_intersects(edge, cursor.canvas_bounds, zoom, &mut work) {
                RouteIntersection::Visible => {
                    cursor.matched.insert(edge);
                    work.candidates_matched = work.candidates_matched.saturating_add(1);
                }
                RouteIntersection::Hidden => {}
                RouteIntersection::Overflow(error) => {
                    cursor.matched.insert(edge);
                    cursor.flatten_overflows.insert(edge, error);
                    work.candidates_matched = work.candidates_matched.saturating_add(1);
                }
            }
        }
    }

    fn route_intersects(
        &mut self,
        edge: EdgeId,
        query: CanvasRect,
        zoom: f32,
        work: &mut VisibleRouteQueryWork,
    ) -> RouteIntersection {
        let Some(indexed) = self.indexed.get(&edge) else {
            return RouteIntersection::Hidden;
        };
        if !rects_intersect(indexed.bounds, query) {
            return RouteIntersection::Hidden;
        }
        let mut flatten_work = RouteFlattenWork::default();
        let result = self.flattened(edge, zoom, &mut flatten_work);
        work.flatten_cache_hits = work
            .flatten_cache_hits
            .saturating_add(flatten_work.cache_hits);
        work.flatten_cache_misses = work
            .flatten_cache_misses
            .saturating_add(flatten_work.cache_misses);
        work.flatten_segments_built = work
            .flatten_segments_built
            .saturating_add(flatten_work.segments_built);
        work.flatten_overflows = work
            .flatten_overflows
            .saturating_add(flatten_work.overflows);
        match result {
            Ok(path) => {
                if flattened_path_intersects_rect(&path, query) {
                    RouteIntersection::Visible
                } else {
                    RouteIntersection::Hidden
                }
            }
            Err(error) => RouteIntersection::Overflow(error),
        }
    }

    fn flattened(
        &mut self,
        edge: EdgeId,
        zoom: f32,
        work: &mut RouteFlattenWork,
    ) -> Result<Arc<FlattenedEdgePath>, EdgePathFlattenError> {
        let mut options = EdgeFlattenOptions::default()
            .for_zoom(zoom)
            .ok_or(EdgePathFlattenError::InvalidGeometry)?;
        let (tolerance_bucket, tolerance) = tolerance_bucket(options.max_error);
        let Some(indexed) = self.indexed.get(&edge) else {
            return Err(EdgePathFlattenError::InvalidGeometry);
        };
        if let Some(cached) = self.flatten_cache.get(&edge)
            && cached.path_revision == indexed.path_revision
            && cached.tolerance_bucket == tolerance_bucket
        {
            work.cache_hits = work.cache_hits.saturating_add(1);
            return cached.result.clone();
        }
        let path_revision = indexed.path_revision;
        work.cache_misses = work.cache_misses.saturating_add(1);
        options.max_error = tolerance;
        let result = flatten_edge_path(&indexed.path, options).map(Arc::new);
        if let Ok(path) = &result {
            work.segments_built = work
                .segments_built
                .saturating_add(path.segments().len() as u64);
        } else {
            work.overflows = work.overflows.saturating_add(1);
        }
        self.flatten_cache.insert(
            edge,
            CachedFlatten {
                path_revision,
                tolerance_bucket,
                result: result.clone(),
            },
        );
        result
    }
}

enum RouteIntersection {
    Visible,
    Hidden,
    Overflow(EdgePathFlattenError),
}

enum InternalRouteQueryOutcome {
    Complete {
        edges: BTreeSet<EdgeId>,
        overflows: BTreeMap<EdgeId, EdgePathFlattenError>,
        work: VisibleRouteQueryWork,
    },
    Incomplete {
        cursor: VisibleRouteCursor,
        work: VisibleRouteQueryWork,
    },
    BudgetExhausted {
        work: VisibleRouteQueryWork,
    },
}

fn record_route_query_stats(
    stats: &mut super::ViewSessionStats,
    outcome: &InternalRouteQueryOutcome,
) {
    let work = match outcome {
        InternalRouteQueryOutcome::Complete { work, .. }
        | InternalRouteQueryOutcome::Incomplete { work, .. }
        | InternalRouteQueryOutcome::BudgetExhausted { work } => *work,
    };
    stats.route_query_entries_examined = stats
        .route_query_entries_examined
        .saturating_add(work.entries_examined);
    stats.route_query_cells_examined = stats
        .route_query_cells_examined
        .saturating_add(work.cells_examined);
    stats.route_query_candidates_matched = stats
        .route_query_candidates_matched
        .saturating_add(work.candidates_matched);
    stats.route_flatten_cache_hits = stats
        .route_flatten_cache_hits
        .saturating_add(work.flatten_cache_hits);
    stats.route_flatten_cache_misses = stats
        .route_flatten_cache_misses
        .saturating_add(work.flatten_cache_misses);
    stats.route_flatten_segments_built = stats
        .route_flatten_segments_built
        .saturating_add(work.flatten_segments_built);
    stats.route_flatten_overflows = stats
        .route_flatten_overflows
        .saturating_add(work.flatten_overflows);
    match outcome {
        InternalRouteQueryOutcome::Incomplete { .. } => {
            stats.route_query_incomplete = stats.route_query_incomplete.saturating_add(1);
        }
        InternalRouteQueryOutcome::BudgetExhausted { .. } => {
            stats.route_query_budget_exhausted =
                stats.route_query_budget_exhausted.saturating_add(1);
        }
        InternalRouteQueryOutcome::Complete { .. } => {}
    }
}

impl ViewSession {
    /// Returns the current invalidation stamp for complete visible-route query results.
    pub fn visible_route_query_revision(
        &self,
    ) -> Result<VisibleRouteQueryRevision, ViewSessionError> {
        self.runtime()?;
        let state = self
            .shared
            .state
            .try_borrow()
            .map_err(|_| ViewSessionError::Busy)?;
        ensure_open(state.status)?;
        Ok(VisibleRouteQueryRevision {
            view: state.id,
            generation: state.generation,
            transform_revision: state.transform_revision,
            route_facts_revision: state.route_facts_revision,
            interaction_order_revision: state.interaction_order_revision,
        })
    }

    pub fn visible_routes(
        &self,
        query: VisibleRouteQuery,
    ) -> Result<VisibleRouteQueryOutcome, VisibleRouteQueryError> {
        self.runtime()?;
        let result = {
            let mut state = self
                .shared
                .state
                .try_borrow_mut()
                .map_err(|_| ViewSessionError::Busy)?;
            ensure_open(state.status)?;
            let transform = ViewportTransform::new(state.facts.pan, state.facts.zoom)
                .ok_or(ViewSessionError::InvalidViewport)?;
            let cursor = match query.request {
                VisibleRouteRequest::Start { bounds, margin } => {
                    let bounds = expanded_view_rect(bounds, margin)
                        .and_then(|bounds| bounds.to_canvas_rect(transform))
                        .ok_or(VisibleRouteQueryError::InvalidQuery)?;
                    state.route_geometry.new_cursor(
                        state.id,
                        state.generation,
                        state.transform_revision,
                        bounds,
                    )
                }
                VisibleRouteRequest::Resume(cursor) => {
                    validate_cursor(&state, &cursor)?;
                    *cursor
                }
            };
            let result =
                state
                    .route_geometry
                    .query_page(cursor, query.entry_budget, transform.zoom);
            record_route_query_stats(&mut state.stats, &result);
            result
        };

        match result {
            InternalRouteQueryOutcome::Complete {
                edges,
                overflows,
                work,
            } => {
                let mut edges = edges.into_iter().collect::<Vec<_>>();
                let geometry_revisions = {
                    let state = self
                        .shared
                        .state
                        .try_borrow()
                        .map_err(|_| ViewSessionError::Busy)?;
                    ensure_open(state.status)?;
                    sort_render_order(
                        &mut edges,
                        |edge| *edge,
                        |edge| state.selected_edge_membership.contains(edge),
                        &state.edge_draw_order_ranks,
                    );
                    edges
                        .iter()
                        .filter_map(|edge| {
                            state
                                .route_geometry
                                .geometry_revision(*edge)
                                .map(|revision| (*edge, revision))
                        })
                        .collect()
                };
                let snapshot = self.shared.geometry_snapshot()?;
                let geometry_overflows = overflows
                    .into_iter()
                    .map(|(edge, error)| VisibleRouteGeometryOverflow { edge, error })
                    .collect();
                Ok(VisibleRouteQueryOutcome::Complete {
                    routes: VisibleRoutes {
                        snapshot,
                        edges,
                        geometry_revisions,
                        geometry_overflows,
                    },
                    work,
                })
            }
            InternalRouteQueryOutcome::Incomplete { cursor, work } => {
                Ok(VisibleRouteQueryOutcome::Incomplete {
                    cursor: Box::new(cursor),
                    work,
                })
            }
            InternalRouteQueryOutcome::BudgetExhausted { work } => {
                Ok(VisibleRouteQueryOutcome::BudgetExhausted { work })
            }
        }
    }
}

fn validate_cursor(
    state: &super::ViewState,
    cursor: &VisibleRouteCursor,
) -> Result<(), VisibleRouteQueryError> {
    let reason = if cursor.view != state.id {
        Some(VisibleRouteCursorStaleReason::ForeignView)
    } else if cursor.generation != state.generation {
        Some(VisibleRouteCursorStaleReason::ViewGeneration)
    } else if cursor.transform_revision != state.transform_revision {
        Some(VisibleRouteCursorStaleReason::ViewportTransform)
    } else if cursor.index_epoch != state.route_geometry.index_epoch {
        Some(VisibleRouteCursorStaleReason::RouteGeometry)
    } else {
        None
    };
    reason
        .map(VisibleRouteQueryError::StaleCursor)
        .map_or(Ok(()), Err)
}

fn expanded_view_rect(bounds: ViewRect, margin: f32) -> Option<ViewRect> {
    if !bounds.is_positive_finite() || !margin.is_finite() || margin < 0.0 {
        return None;
    }
    let expanded = ViewRect::new(
        crate::runtime::adapter_protocol::ViewPoint::new(
            bounds.origin.x - margin,
            bounds.origin.y - margin,
        ),
        crate::runtime::adapter_protocol::ViewSize::new(
            bounds.size.width + margin * 2.0,
            bounds.size.height + margin * 2.0,
        ),
    );
    expanded.is_positive_finite().then_some(expanded)
}

fn expand_route_bounds(bounds: CanvasRect) -> Option<CanvasRect> {
    inflate_canvas_rect(bounds, ROUTE_INDEX_EPSILON)
}

fn inflate_canvas_rect(bounds: CanvasRect, amount: f32) -> Option<CanvasRect> {
    if !amount.is_finite() || amount < 0.0 {
        return None;
    }
    let expanded = CanvasRect {
        origin: CanvasPoint {
            x: bounds.origin.x - amount,
            y: bounds.origin.y - amount,
        },
        size: CanvasSize {
            width: bounds.size.width + amount * 2.0,
            height: bounds.size.height + amount * 2.0,
        },
    };
    expanded.is_positive_finite().then_some(expanded)
}

fn route_cells_for_segment(from: CanvasPoint, to: CanvasPoint) -> Vec<SpatialCell> {
    for level in 0..=max_spatial_cell_level() {
        if let Some(cells) = route_cells_for_segment_at_level(from, to, level) {
            return cells;
        }
    }
    unreachable!("the coarsest route grid covers every finite f32 coordinate")
}

fn route_cells_for_segment_at_level(
    from: CanvasPoint,
    to: CanvasPoint,
    level: u8,
) -> Option<Vec<SpatialCell>> {
    let cell_size = spatial_cell_size(level);
    let start = spatial_cell_coords(from, cell_size);
    let end = spatial_cell_coords(to, cell_size);
    let mut cells = BTreeSet::new();
    let mut insert = |x: i32, y: i32| {
        cells.insert(SpatialCell { level, x, y });
        cells.len() <= MAX_CELLS_PER_SEGMENT
    };

    if !insert(start.0, start.1) {
        return None;
    }
    if start == end {
        return Some(cells.into_iter().collect());
    }

    let dx = f64::from(to.x) - f64::from(from.x);
    let dy = f64::from(to.y) - f64::from(from.y);
    let step_x = if dx > 0.0 {
        1
    } else if dx < 0.0 {
        -1
    } else {
        0
    };
    let step_y = if dy > 0.0 {
        1
    } else if dy < 0.0 {
        -1
    } else {
        0
    };
    let mut x = start.0;
    let mut y = start.1;
    let t_delta_x = if step_x == 0 {
        f64::INFINITY
    } else {
        cell_size / dx.abs()
    };
    let t_delta_y = if step_y == 0 {
        f64::INFINITY
    } else {
        cell_size / dy.abs()
    };
    let next_x = if step_x > 0 {
        (f64::from(x) + 1.0) * cell_size
    } else {
        f64::from(x) * cell_size
    };
    let next_y = if step_y > 0 {
        (f64::from(y) + 1.0) * cell_size
    } else {
        f64::from(y) * cell_size
    };
    let mut t_max_x = if step_x == 0 {
        f64::INFINITY
    } else {
        (next_x - f64::from(from.x)) / dx
    };
    let mut t_max_y = if step_y == 0 {
        f64::INFINITY
    } else {
        (next_y - f64::from(from.y)) / dy
    };

    while (x, y) != end {
        match t_max_x.total_cmp(&t_max_y) {
            std::cmp::Ordering::Less => {
                x = x.checked_add(step_x)?;
                t_max_x += t_delta_x;
            }
            std::cmp::Ordering::Greater => {
                y = y.checked_add(step_y)?;
                t_max_y += t_delta_y;
            }
            std::cmp::Ordering::Equal => {
                let next_x = x.checked_add(step_x)?;
                let next_y = y.checked_add(step_y)?;
                if !insert(next_x, y) || !insert(x, next_y) {
                    return None;
                }
                x = next_x;
                y = next_y;
                t_max_x += t_delta_x;
                t_max_y += t_delta_y;
            }
        }
        if !insert(x, y) {
            return None;
        }
    }
    Some(cells.into_iter().collect())
}

fn route_cells_for_bounds(bounds: CanvasRect) -> Vec<SpatialCell> {
    bounded_spatial_cells_for_rect(bounds, MAX_CELLS_PER_SEGMENT as i64)
}

fn tolerance_bucket(desired: f32) -> (u32, f32) {
    let bits = desired.to_bits();
    let bucket = bits & !0x0fff;
    let tolerance = f32::from_bits(bucket);
    if tolerance.is_finite() && tolerance > 0.0 && tolerance <= desired {
        (bucket, tolerance)
    } else {
        (bits, desired)
    }
}

fn rects_intersect(left: CanvasRect, right: CanvasRect) -> bool {
    let left_max_x = left.origin.x + left.size.width;
    let left_max_y = left.origin.y + left.size.height;
    let right_max_x = right.origin.x + right.size.width;
    let right_max_y = right.origin.y + right.size.height;
    left.origin.x <= right_max_x
        && left_max_x >= right.origin.x
        && left.origin.y <= right_max_y
        && left_max_y >= right.origin.y
}

fn flattened_path_intersects_rect(path: &FlattenedEdgePath, rect: CanvasRect) -> bool {
    path.segments()
        .iter()
        .any(|segment| segment_intersects_rect(segment.from(), segment.to(), rect))
}

fn segment_intersects_rect(from: CanvasPoint, to: CanvasPoint, rect: CanvasRect) -> bool {
    let min_x = rect.origin.x;
    let min_y = rect.origin.y;
    let max_x = rect.origin.x + rect.size.width;
    let max_y = rect.origin.y + rect.size.height;
    if point_in_rect(from, min_x, min_y, max_x, max_y)
        || point_in_rect(to, min_x, min_y, max_x, max_y)
    {
        return true;
    }
    let dx = to.x - from.x;
    let dy = to.y - from.y;
    let mut t0 = 0.0_f32;
    let mut t1 = 1.0_f32;
    for (p, q) in [
        (-dx, from.x - min_x),
        (dx, max_x - from.x),
        (-dy, from.y - min_y),
        (dy, max_y - from.y),
    ] {
        if p.abs() <= f32::EPSILON {
            if q < 0.0 {
                return false;
            }
            continue;
        }
        let ratio = q / p;
        if p < 0.0 {
            t0 = t0.max(ratio);
        } else {
            t1 = t1.min(ratio);
        }
        if t0 > t1 {
            return false;
        }
    }
    true
}

fn point_in_rect(point: CanvasPoint, min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> bool {
    point.x >= min_x && point.x <= max_x && point.y >= min_y && point.y <= max_y
}

#[cfg(test)]
mod tests {
    use jellyflow_core::core::{Edge, EdgeKind, EdgeRouteKind, EdgeViewDescriptor, PortId};

    use super::*;
    use crate::runtime::adapter_protocol::PublishedEdgeRoute;
    use crate::runtime::geometry::{
        EdgeEndpointPosition, EdgePosition, HandlePosition, resolve_edge_route_path,
    };

    #[test]
    fn route_index_resumes_without_omission_and_updates_atomically() {
        let mut index = RouteGeometryIndex::default();
        let routes = (0..4)
            .map(|offset| test_route(EdgeId::from_u128(offset + 1), offset as f32 * 120.0))
            .collect::<Vec<_>>();
        for route in &routes {
            assert!(index.upsert(route.edge(), route).index_changed);
        }

        let epoch = index.index_epoch;
        let cursor = index.new_cursor(
            ViewId::unavailable(),
            ViewGeneration::initial(),
            0,
            CanvasRect {
                origin: CanvasPoint { x: -10.0, y: -10.0 },
                size: CanvasSize {
                    width: 500.0,
                    height: 40.0,
                },
            },
        );
        let (found, total_work) = finish_query(&mut index, cursor, 1);
        assert_eq!(found, routes.iter().map(PublishedEdgeRoute::edge).collect());
        assert!(total_work >= routes.len() as u64);
        assert_eq!(index.flatten_cache.len(), routes.len());

        let selected = routes[0].clone().with_selected(true);
        assert!(!index.upsert(selected.edge(), &selected).index_changed);
        assert_eq!(index.index_epoch, epoch);
        assert_eq!(index.flatten_cache.len(), routes.len());

        assert!(index.remove(routes[1].edge()).index_changed);
        let cursor = index.new_cursor(
            ViewId::unavailable(),
            ViewGeneration::initial(),
            0,
            CanvasRect {
                origin: CanvasPoint { x: -10.0, y: -10.0 },
                size: CanvasSize {
                    width: 500.0,
                    height: 40.0,
                },
            },
        );
        let (found, _) = finish_query(&mut index, cursor, 2);
        assert!(!found.contains(&routes[1].edge()));
        assert_eq!(found.len(), 3);

        assert!(index.clear().index_changed);
        let cursor = index.new_cursor(
            ViewId::unavailable(),
            ViewGeneration::initial(),
            0,
            CanvasRect {
                origin: CanvasPoint { x: -10.0, y: -10.0 },
                size: CanvasSize {
                    width: 500.0,
                    height: 40.0,
                },
            },
        );
        let (found, work) = finish_query(&mut index, cursor, 1);
        assert!(found.is_empty());
        assert_eq!(work, 0);
    }

    #[test]
    fn long_diagonal_uses_bounded_segment_cells_instead_of_its_bounding_box() {
        let cells = route_cells_for_segment(
            CanvasPoint {
                x: -1_000_000.0,
                y: -1_000_000.0,
            },
            CanvasPoint {
                x: 1_000_000.0,
                y: 1_000_000.0,
            },
        );

        assert!(!cells.is_empty());
        assert!(cells.len() <= MAX_CELLS_PER_SEGMENT);
        assert!(cells[0].level > 0);
        assert!(cells.iter().all(|cell| (cell.x - cell.y).abs() <= 1));
    }

    #[test]
    fn ten_thousand_spatial_routes_query_only_the_local_hundred() {
        const GRID_SIDE: u128 = 100;
        const SPACING: f32 = 512.0;

        let mut index = RouteGeometryIndex::default();
        for row in 0..GRID_SIDE {
            for column in 0..GRID_SIDE {
                let ordinal = row * GRID_SIDE + column;
                let route = test_route_at(
                    EdgeId::from_u128(ordinal + 1),
                    column as f32 * SPACING,
                    row as f32 * SPACING,
                );
                assert!(index.upsert(route.edge(), &route).index_changed);
            }
        }

        let cursor = index.new_cursor(
            ViewId::unavailable(),
            ViewGeneration::initial(),
            0,
            CanvasRect {
                origin: CanvasPoint { x: -16.0, y: -16.0 },
                size: CanvasSize {
                    width: 9.0 * SPACING + 132.0,
                    height: 9.0 * SPACING + 32.0,
                },
            },
        );
        let InternalRouteQueryOutcome::Complete { edges, work, .. } =
            index.query_page(cursor, 256, 1.0)
        else {
            panic!("the local route query must finish inside one frame budget");
        };

        assert_eq!(edges.len(), 100);
        assert!(work.entries_examined <= 256);
        assert_eq!(work.flatten_cache_misses, 100);
        assert_eq!(index.flatten_cache.len(), 100);
    }

    fn finish_query(
        index: &mut RouteGeometryIndex,
        mut cursor: VisibleRouteCursor,
        page_budget: usize,
    ) -> (BTreeSet<EdgeId>, u64) {
        let mut total_work = 0;
        for _ in 0..128 {
            match index.query_page(cursor, page_budget, 1.0) {
                InternalRouteQueryOutcome::Complete { edges, work, .. } => {
                    total_work += work.entries_examined;
                    return (edges, total_work);
                }
                InternalRouteQueryOutcome::Incomplete { cursor: next, work } => {
                    assert!(work.entries_examined <= page_budget as u64);
                    total_work += work.entries_examined;
                    cursor = next;
                }
                InternalRouteQueryOutcome::BudgetExhausted { .. } => {
                    panic!("small indexed fixture must not exhaust the hard budget")
                }
            }
        }
        panic!("route query did not complete")
    }

    fn test_route(edge: EdgeId, x: f32) -> PublishedEdgeRoute {
        test_route_at(edge, x, 0.0)
    }

    fn test_route_at(edge: EdgeId, x: f32, y: f32) -> PublishedEdgeRoute {
        let mut model = Edge::new(EdgeKind::Data, PortId::from_u128(1), PortId::from_u128(2));
        model.view = EdgeViewDescriptor::new().with_route_kind(EdgeRouteKind::Straight);
        let facts = resolve_edge_route_path(
            &model,
            EdgePosition {
                source: EdgeEndpointPosition {
                    point: CanvasPoint { x, y },
                    position: HandlePosition::Right,
                },
                target: EdgeEndpointPosition {
                    point: CanvasPoint { x: x + 100.0, y },
                    position: HandlePosition::Left,
                },
            },
        )
        .expect("route");
        PublishedEdgeRoute::new(edge, facts)
    }
}
