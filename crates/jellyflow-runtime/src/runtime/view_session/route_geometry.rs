use std::collections::{BTreeMap, BTreeSet};
use std::ops::Bound::{Excluded, Unbounded};
use std::sync::Arc;

use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize, EdgeId};

use crate::runtime::adapter_protocol::{
    GeometrySnapshot, PublishedEdgeRoute, RouteGeometryRevision, ViewRect,
};
use crate::runtime::geometry::{
    EdgeFlattenOptions, EdgePath, EdgePathFlattenError, EdgePathSegment, FlattenedEdgePath,
    edge_path_conservative_bounds, flatten_edge_path,
};
use crate::runtime::graph_runtime::EdgeInstanceKey;
use crate::runtime::viewport::{ViewportRevision, ViewportTransform};

use super::order::sort_render_order;
use super::spatial_grid::{
    HierarchicalSpatialIndex, SpatialCell, SpatialQueryCursor, SpatialQuerySource,
    SpatialQueryStep, bounded_spatial_cells_for_rect, max_spatial_cell_level,
    max_spatial_query_setup_work, spatial_cell_coords, spatial_cell_size,
};
use super::{
    ViewGeneration, ViewId, ViewProvenance, ViewQueryIdentity, ViewQueryIdentityStaleReason,
    ViewSession, ViewSessionError, ensure_open,
};

const ROUTE_INDEX_EPSILON: f32 = 0.001;
const MAX_CELLS_PER_SEGMENT: usize = 128;
const MAX_QUERY_CELLS_PER_LEVEL: i64 = 4_096;
const INDEX_FLATTEN_ERROR: f32 = 8.0;
const HARD_ROUTE_QUERY_ENTRY_LIMIT: u64 = 16_384;
pub(super) const MAX_ROUTE_QUERY_SETUP_WORK: u64 =
    max_spatial_query_setup_work(MAX_QUERY_CELLS_PER_LEVEL as u64);
const HARD_ROUTE_QUERY_WORK_LIMIT: u64 = HARD_ROUTE_QUERY_ENTRY_LIMIT + MAX_ROUTE_QUERY_SETUP_WORK;
const ROUTE_FLATTEN_CACHE_CAPACITY: usize = 4_096;

/// Renderer-provided bounds for one runtime-owned visible-route work stream.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VisibleRouteWorkRequest {
    bounds: ViewRect,
    margin: f32,
}

impl VisibleRouteWorkRequest {
    pub fn new(bounds: ViewRect) -> Self {
        Self {
            bounds,
            margin: 0.0,
        }
    }

    pub fn with_margin(mut self, margin: f32) -> Self {
        self.margin = margin;
        self
    }
}

#[derive(Debug)]
struct VisibleRouteQuery {
    request: VisibleRouteRequest,
    entry_budget: usize,
}

impl VisibleRouteQuery {
    fn new(canvas_bounds: CanvasRect, entry_budget: usize) -> Self {
        Self {
            request: VisibleRouteRequest::Start(canvas_bounds),
            entry_budget,
        }
    }

    fn resume(cursor: Box<VisibleRouteCursor>, entry_budget: usize) -> Self {
        Self {
            request: VisibleRouteRequest::Resume(cursor),
            entry_budget,
        }
    }
}

#[derive(Debug)]
enum VisibleRouteRequest {
    Start(CanvasRect),
    Resume(Box<VisibleRouteCursor>),
}

#[derive(Debug)]
pub(super) struct VisibleRouteCursor {
    identity: ViewQueryIdentity,
    index_epoch: u128,
    canvas_bounds: CanvasRect,
    scan: RouteScan,
    active_source: Option<RouteQuerySource>,
    next_entry: Option<RouteSegmentKey>,
    checked: BTreeSet<EdgeId>,
    matched: BTreeSet<EdgeId>,
    flatten_overflows: BTreeMap<EdgeId, EdgePathFlattenError>,
    total_entries_examined: u64,
    total_spatial_work: u64,
}

#[derive(Debug)]
pub(super) struct RouteScan {
    spatial: SpatialQueryCursor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum RouteQuerySource {
    Cell(SpatialCell),
    Level(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RouteQueryStep {
    LevelInitialized,
    Source(RouteQuerySource),
    Exhausted,
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
    /// Level-discovery steps, including the terminal exhausted check.
    pub spatial_level_checks: u64,
    /// Cell or whole-level sources initialized by the spatial cursor.
    pub spatial_source_setups: u64,
    /// Cell sources initialized; this is a subset of `spatial_source_setups`.
    pub cells_examined: u64,
    /// Route-segment entries consumed from initialized sources.
    pub entries_examined: u64,
    pub candidates_matched: u64,
    pub flatten_cache_hits: u64,
    pub flatten_cache_misses: u64,
    pub flatten_cache_evictions: u64,
    pub flatten_segments_built: u64,
    pub flatten_overflows: u64,
}

impl VisibleRouteQueryWork {
    /// Returns the level, source, and entry units charged against the page budget.
    #[must_use]
    pub const fn spatial_work_units(self) -> u64 {
        self.spatial_level_checks
            .saturating_add(self.spatial_source_setups)
            .saturating_add(self.entries_examined)
    }
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
    geometry_overflows: Vec<VisibleRouteGeometryOverflow>,
}

/// Stable invalidation stamp for one view's visible-route publication.
///
/// Adapters can retain a complete or truncated publication while this value and their query bounds
/// remain unchanged. The stamp advances for viewport, route-fact, selection, or draw-order changes
/// even when the resolved path geometry itself remains identical.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VisibleRouteQueryRevision {
    view: ViewId,
    generation: ViewGeneration,
    viewport_revision: ViewportRevision,
    route_facts_revision: u128,
    interaction_order_revision: u128,
}

/// One edge from a visible-route membership publication.
#[derive(Debug, Clone, Copy)]
pub struct VisibleRoute<'a> {
    edge: EdgeId,
    route: &'a PublishedEdgeRoute,
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

/// Terminal or pending state returned by [`VisibleRouteWorkSession::advance`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisibleRouteWorkStatus {
    Pending,
    Complete,
    Truncated,
}

/// One bounded visible-route advancement and its currently publishable route set.
#[derive(Debug, Clone)]
pub struct VisibleRouteWorkUpdate {
    routes: Option<Arc<VisibleRoutes>>,
    query_revision: VisibleRouteQueryRevision,
    publication_revision: Option<VisibleRouteQueryRevision>,
    generation: u64,
    status: VisibleRouteWorkStatus,
    work: VisibleRouteQueryWork,
    reused_previous_publication: bool,
}

impl VisibleRouteWorkUpdate {
    pub fn routes(&self) -> Option<&VisibleRoutes> {
        self.routes.as_deref()
    }

    pub fn query_revision(&self) -> VisibleRouteQueryRevision {
        self.query_revision
    }

    pub fn publication_revision(&self) -> Option<VisibleRouteQueryRevision> {
        self.publication_revision
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn status(&self) -> VisibleRouteWorkStatus {
        self.status
    }

    pub fn work(&self) -> VisibleRouteQueryWork {
        self.work
    }

    pub fn reused_previous_publication(&self) -> bool {
        self.reused_previous_publication
    }
}

/// Runtime-owned visible-route continuation and last complete or truncated publication.
///
/// Native adapters retain this owner and choose only the request bounds and per-tick budget. Query
/// cursors, stale restarts, accumulated matches, and terminal reuse remain private to the runtime.
#[derive(Debug)]
pub struct VisibleRouteWorkSession {
    owner: ViewProvenance,
    continuation: Option<VisibleRouteContinuation>,
    publication: Option<VisibleRoutePublication>,
    generation: u64,
}

impl VisibleRouteWorkSession {
    fn attach(view: &ViewSession) -> Result<Self, ViewSessionError> {
        Ok(Self {
            owner: view.provenance()?,
            continuation: None,
            publication: None,
            generation: 0,
        })
    }

    pub fn clear(&mut self) {
        self.continuation = None;
        self.publication = None;
        self.generation = self.generation.saturating_add(1);
    }

    pub fn advance(
        &mut self,
        view: &ViewSession,
        request: VisibleRouteWorkRequest,
        entry_budget: usize,
    ) -> Result<VisibleRouteWorkUpdate, VisibleRouteWorkError> {
        let provided = view.provenance()?;
        if provided != self.owner {
            return Err(VisibleRouteWorkError::ForeignView {
                owner: self.owner,
                provided,
            });
        }
        view.advance_visible_route_work(self, request, entry_budget)
    }

    fn update(
        &self,
        query_revision: VisibleRouteQueryRevision,
        status: VisibleRouteWorkStatus,
        work: VisibleRouteQueryWork,
        reused_previous_publication: bool,
    ) -> VisibleRouteWorkUpdate {
        VisibleRouteWorkUpdate {
            routes: self
                .publication
                .as_ref()
                .map(|publication| Arc::clone(&publication.routes)),
            query_revision,
            publication_revision: self
                .publication
                .as_ref()
                .map(|publication| publication.key.revision),
            generation: self.generation,
            status,
            work,
            reused_previous_publication,
        }
    }

    fn publish(
        &mut self,
        key: VisibleRouteWorkKey,
        routes: VisibleRoutes,
        status: VisibleRouteWorkStatus,
        work: VisibleRouteQueryWork,
    ) -> VisibleRouteWorkUpdate {
        debug_assert!(matches!(
            status,
            VisibleRouteWorkStatus::Complete | VisibleRouteWorkStatus::Truncated
        ));
        self.continuation = None;
        self.generation = self.generation.saturating_add(1);
        self.publication = Some(VisibleRoutePublication {
            key,
            routes: Arc::new(routes),
            status,
        });
        self.update(key.revision, status, work, false)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct VisibleRouteWorkKey {
    request: VisibleRouteWorkRequest,
    revision: VisibleRouteQueryRevision,
}

#[derive(Debug)]
struct VisibleRouteContinuation {
    key: VisibleRouteWorkKey,
    cursor: Box<VisibleRouteCursor>,
}

#[derive(Debug)]
struct VisibleRoutePublication {
    key: VisibleRouteWorkKey,
    routes: Arc<VisibleRoutes>,
    status: VisibleRouteWorkStatus,
}

#[derive(Debug)]
enum VisibleRouteQueryOutcome {
    Complete {
        routes: VisibleRoutes,
        work: VisibleRouteQueryWork,
    },
    Incomplete {
        cursor: Box<VisibleRouteCursor>,
        work: VisibleRouteQueryWork,
    },
    Truncated {
        routes: VisibleRoutes,
        work: VisibleRouteQueryWork,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
enum VisibleRouteCursorStaleReason {
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
    const fn is_restartable(self) -> bool {
        matches!(self, Self::ViewportTransform | Self::RouteGeometry)
    }
}

#[derive(Debug, thiserror::Error)]
enum VisibleRouteQueryError {
    #[error(transparent)]
    Session(#[from] ViewSessionError),
    #[error(transparent)]
    StaleCursor(VisibleRouteCursorStaleReason),
}

/// Failure to advance a runtime-owned visible-route work stream.
#[derive(Debug, thiserror::Error)]
pub enum VisibleRouteWorkError {
    #[error(transparent)]
    Session(#[from] ViewSessionError),
    #[error("visible-route work belongs to {owner:?}, not {provided:?}")]
    ForeignView {
        owner: ViewProvenance,
        provided: ViewProvenance,
    },
    #[error("visible-route work bounds or margin are invalid")]
    InvalidRequest,
}

#[derive(Debug, Default)]
pub(super) struct RouteGeometryIndex {
    spatial: RouteSpatialIndex,
    indexed: BTreeMap<EdgeId, IndexedRoute>,
    flatten_cache: BTreeMap<EdgeId, CachedFlatten>,
    flatten_lru: BTreeSet<(u64, EdgeId)>,
    interaction_half_widths: BTreeMap<u32, usize>,
    index_epoch: u128,
    pick_epoch: u128,
    next_path_revision: u128,
    next_flatten_access: u64,
}

#[derive(Debug, Clone)]
struct IndexedRoute {
    instance: EdgeInstanceKey,
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
    access: u64,
}

#[derive(Debug, Clone)]
pub(super) struct FlattenedRouteApproximation {
    pub(super) path: Arc<FlattenedEdgePath>,
    pub(super) max_error_in_canvas: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct RouteFlattenWork {
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub cache_evictions: u64,
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
}

#[derive(Debug, Clone, Copy)]
pub(super) struct RouteIndexAdmission {
    #[cfg(test)]
    pub index_changed: bool,
    pub geometry_revision: RouteGeometryRevision,
}

enum PreparedRouteSpatial {
    Segments(FlattenedEdgePath),
    Bounds(CanvasRect),
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

    #[cfg(test)]
    fn is_empty(&self) -> bool {
        self.spatial.is_empty() && self.routes.is_empty()
    }

    fn new_scan(&self, bounds: CanvasRect) -> RouteScan {
        let query_bounds = inflate_canvas_rect(bounds, INDEX_FLATTEN_ERROR).unwrap_or(bounds);
        RouteScan {
            spatial: SpatialQueryCursor::new(query_bounds, MAX_QUERY_CELLS_PER_LEVEL),
        }
    }

    fn advance_scan(&self, scan: &mut RouteScan) -> RouteQueryStep {
        match self.spatial.advance_query(&mut scan.spatial) {
            SpatialQueryStep::LevelInitialized => RouteQueryStep::LevelInitialized,
            SpatialQueryStep::Source(SpatialQuerySource::Cell(cell)) => {
                RouteQueryStep::Source(RouteQuerySource::Cell(cell))
            }
            SpatialQueryStep::Source(SpatialQuerySource::Level(level)) => {
                RouteQueryStep::Source(RouteQuerySource::Level(level))
            }
            SpatialQueryStep::Exhausted => RouteQueryStep::Exhausted,
        }
    }

    fn source(&self, source: RouteQuerySource) -> Option<&BTreeSet<RouteSegmentKey>> {
        match source {
            RouteQuerySource::Cell(cell) => self.spatial.source(SpatialQuerySource::Cell(cell)),
            RouteQuerySource::Level(level) => self.spatial.source(SpatialQuerySource::Level(level)),
        }
    }
}

impl RouteGeometryIndex {
    pub(super) fn try_upsert(
        &mut self,
        edge: EdgeId,
        route: &PublishedEdgeRoute,
    ) -> Option<RouteIndexAdmission> {
        let pick = IndexedRoutePickFacts::from(route);
        if let Some(indexed) = self.indexed.get(&edge)
            && indexed.instance == route.instance()
            && indexed.path == route.facts().path
        {
            let geometry_revision = RouteGeometryRevision::new(indexed.path_revision);
            if indexed.pick == pick {
                return Some(RouteIndexAdmission {
                    #[cfg(test)]
                    index_changed: false,
                    geometry_revision,
                });
            }
            let old_width = indexed.pick.interaction_width;
            self.replace_interaction_half_width(old_width, pick.interaction_width);
            self.indexed
                .get_mut(&edge)
                .expect("the route was read immediately before mutation")
                .pick = pick;
            self.pick_epoch = self.pick_epoch.saturating_add(1);
            return Some(RouteIndexAdmission {
                #[cfg(test)]
                index_changed: false,
                geometry_revision,
            });
        }
        if !route.facts().path.is_finite() {
            return None;
        }
        let Some(bounds) = edge_path_conservative_bounds(&route.facts().path)
            .ok()
            .flatten()
            .and_then(expand_route_bounds)
        else {
            return None;
        };
        let spatial = match flatten_edge_path(
            &route.facts().path,
            EdgeFlattenOptions::new(INDEX_FLATTEN_ERROR),
        ) {
            Ok(path) => PreparedRouteSpatial::Segments(path),
            Err(_) => PreparedRouteSpatial::Bounds(bounds),
        };

        self.remove_cached_flatten(edge);
        self.next_path_revision = self.next_path_revision.saturating_add(1);
        let path_revision = self.next_path_revision;
        if let Some(indexed) = self.indexed.get(&edge) {
            self.remove_interaction_half_width(indexed.pick.interaction_width);
        }
        self.insert_interaction_half_width(pick.interaction_width);
        match spatial {
            PreparedRouteSpatial::Segments(path) => {
                self.spatial.upsert_segments(edge, path.segments());
            }
            PreparedRouteSpatial::Bounds(bounds) => self.spatial.upsert_bounds(edge, bounds),
        }
        self.indexed.insert(
            edge,
            IndexedRoute {
                instance: route.instance(),
                path_revision,
                path: route.facts().path.clone(),
                bounds,
                pick,
            },
        );
        self.index_epoch = self.index_epoch.saturating_add(1);
        self.pick_epoch = self.pick_epoch.saturating_add(1);
        Some(RouteIndexAdmission {
            #[cfg(test)]
            index_changed: true,
            geometry_revision: RouteGeometryRevision::new(path_revision),
        })
    }

    #[cfg(test)]
    pub(super) fn upsert(
        &mut self,
        edge: EdgeId,
        route: &PublishedEdgeRoute,
    ) -> RouteIndexAdmission {
        self.try_upsert(edge, route)
            .expect("test routes must be admissible")
    }

    pub(super) fn edge_instance(&self, edge: EdgeId) -> Option<EdgeInstanceKey> {
        self.indexed.get(&edge).map(|route| route.instance)
    }

    pub(super) fn remove(&mut self, edge: EdgeId) -> RouteIndexMutation {
        let indexed = self.indexed.remove(&edge);
        if let Some(indexed) = &indexed {
            self.remove_interaction_half_width(indexed.pick.interaction_width);
        }
        let spatial = self.spatial.remove(edge);
        self.remove_cached_flatten(edge);
        if indexed.is_some() || spatial {
            self.index_epoch = self.index_epoch.saturating_add(1);
            self.pick_epoch = self.pick_epoch.saturating_add(1);
        }
        RouteIndexMutation {
            index_changed: indexed.is_some() || spatial,
        }
    }

    pub(super) fn clear(&mut self) -> RouteIndexMutation {
        let index_changed = !self.indexed.is_empty();
        self.spatial.clear();
        self.indexed.clear();
        self.flatten_cache.clear();
        self.flatten_lru.clear();
        self.next_flatten_access = 0;
        self.interaction_half_widths.clear();
        if index_changed {
            self.index_epoch = self.index_epoch.saturating_add(1);
            self.pick_epoch = self.pick_epoch.saturating_add(1);
        }
        RouteIndexMutation { index_changed }
    }

    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.indexed.is_empty()
            && self.spatial.is_empty()
            && self.flatten_cache.is_empty()
            && self.flatten_lru.is_empty()
            && self.interaction_half_widths.is_empty()
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

    pub(super) fn new_route_scan(&self, bounds: CanvasRect) -> RouteScan {
        self.spatial.new_scan(bounds)
    }

    pub(super) fn advance_route_scan(&self, scan: &mut RouteScan) -> RouteQueryStep {
        self.spatial.advance_scan(scan)
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
    ) -> Result<FlattenedRouteApproximation, EdgePathFlattenError> {
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

    pub(super) fn new_cursor(
        &self,
        identity: ViewQueryIdentity,
        canvas_bounds: CanvasRect,
    ) -> Box<VisibleRouteCursor> {
        Box::new(VisibleRouteCursor {
            identity,
            index_epoch: self.index_epoch,
            canvas_bounds,
            scan: self.spatial.new_scan(canvas_bounds),
            active_source: None,
            next_entry: None,
            checked: BTreeSet::new(),
            matched: BTreeSet::new(),
            flatten_overflows: BTreeMap::new(),
            total_entries_examined: 0,
            total_spatial_work: 0,
        })
    }

    pub(super) fn query_page(
        &mut self,
        mut cursor: Box<VisibleRouteCursor>,
        entry_budget: usize,
        zoom: f32,
    ) -> InternalRouteQueryOutcome {
        let mut work = VisibleRouteQueryWork::default();
        let mut remaining = entry_budget;
        loop {
            let Some(entry) = cursor.next_entry else {
                if remaining == 0 {
                    return InternalRouteQueryOutcome::Incomplete { cursor, work };
                }
                if cursor.total_spatial_work >= HARD_ROUTE_QUERY_WORK_LIMIT {
                    let cursor = *cursor;
                    return InternalRouteQueryOutcome::Truncated {
                        edges: cursor.matched,
                        overflows: cursor.flatten_overflows,
                        work,
                    };
                }
                remaining -= 1;
                cursor.total_spatial_work = cursor.total_spatial_work.saturating_add(1);
                match self.spatial.advance_scan(&mut cursor.scan) {
                    RouteQueryStep::LevelInitialized => {
                        work.spatial_level_checks = work.spatial_level_checks.saturating_add(1);
                    }
                    RouteQueryStep::Source(source) => {
                        work.spatial_source_setups = work.spatial_source_setups.saturating_add(1);
                        if matches!(source, RouteQuerySource::Cell(_)) {
                            work.cells_examined = work.cells_examined.saturating_add(1);
                        }
                        cursor.active_source = Some(source);
                        cursor.next_entry = self
                            .spatial
                            .source(source)
                            .and_then(|entries| entries.iter().next().copied());
                        if cursor.next_entry.is_none() {
                            cursor.active_source = None;
                        }
                    }
                    RouteQueryStep::Exhausted => {
                        work.spatial_level_checks = work.spatial_level_checks.saturating_add(1);
                        let cursor = *cursor;
                        return InternalRouteQueryOutcome::Complete {
                            edges: cursor.matched,
                            overflows: cursor.flatten_overflows,
                            work,
                        };
                    }
                }
                continue;
            };
            if remaining == 0 {
                return InternalRouteQueryOutcome::Incomplete { cursor, work };
            }
            if cursor.total_entries_examined >= HARD_ROUTE_QUERY_ENTRY_LIMIT
                || cursor.total_spatial_work >= HARD_ROUTE_QUERY_WORK_LIMIT
            {
                let cursor = *cursor;
                return InternalRouteQueryOutcome::Truncated {
                    edges: cursor.matched,
                    overflows: cursor.flatten_overflows,
                    work,
                };
            }
            remaining -= 1;
            cursor.total_entries_examined = cursor.total_entries_examined.saturating_add(1);
            cursor.total_spatial_work = cursor.total_spatial_work.saturating_add(1);
            work.entries_examined = work.entries_examined.saturating_add(1);
            let source = cursor
                .active_source
                .expect("a pending route entry always belongs to an active spatial source");
            cursor.next_entry = self
                .spatial
                .source(source)
                .and_then(|entries| entries.range((Excluded(entry), Unbounded)).next().copied());
            if cursor.next_entry.is_none() {
                cursor.active_source = None;
            }

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
        work.flatten_cache_evictions = work
            .flatten_cache_evictions
            .saturating_add(flatten_work.cache_evictions);
        work.flatten_segments_built = work
            .flatten_segments_built
            .saturating_add(flatten_work.segments_built);
        work.flatten_overflows = work
            .flatten_overflows
            .saturating_add(flatten_work.overflows);
        match result {
            Ok(approximation) => {
                let Some(conservative_query) =
                    inflate_canvas_rect(query, approximation.max_error_in_canvas)
                else {
                    return RouteIntersection::Overflow(EdgePathFlattenError::InvalidGeometry);
                };
                if flattened_path_intersects_rect(&approximation.path, conservative_query) {
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
    ) -> Result<FlattenedRouteApproximation, EdgePathFlattenError> {
        let mut options = EdgeFlattenOptions::default()
            .for_zoom(zoom)
            .ok_or(EdgePathFlattenError::InvalidGeometry)?;
        let (tolerance_bucket, tolerance) = tolerance_bucket(options.max_error);
        let Some(path_revision) = self.indexed.get(&edge).map(|route| route.path_revision) else {
            return Err(EdgePathFlattenError::InvalidGeometry);
        };
        if let Some(cached) = self.flatten_cache.get(&edge)
            && cached.path_revision == path_revision
            && cached.tolerance_bucket == tolerance_bucket
        {
            let result = cached.result.clone();
            let previous_access = cached.access;
            work.cache_hits = work.cache_hits.saturating_add(1);
            self.touch_cached_flatten(edge, previous_access);
            return result.map(|path| FlattenedRouteApproximation {
                path,
                max_error_in_canvas: tolerance,
            });
        }
        work.cache_misses = work.cache_misses.saturating_add(1);
        options.max_error = tolerance;
        let result = flatten_edge_path(
            &self
                .indexed
                .get(&edge)
                .expect("the indexed route was read immediately before flattening")
                .path,
            options,
        )
        .map(Arc::new);
        if let Ok(path) = &result {
            work.segments_built = work
                .segments_built
                .saturating_add(path.segments().len() as u64);
        } else {
            work.overflows = work.overflows.saturating_add(1);
        }
        self.remove_cached_flatten(edge);
        while self.flatten_cache.len() >= ROUTE_FLATTEN_CACHE_CAPACITY {
            let Some((access, cached_edge)) = self.flatten_lru.pop_first() else {
                break;
            };
            if self
                .flatten_cache
                .get(&cached_edge)
                .is_some_and(|cached| cached.access == access)
            {
                self.flatten_cache.remove(&cached_edge);
                work.cache_evictions = work.cache_evictions.saturating_add(1);
            }
        }
        let access = self.reserve_flatten_access();
        self.flatten_lru.insert((access, edge));
        self.flatten_cache.insert(
            edge,
            CachedFlatten {
                path_revision,
                tolerance_bucket,
                result: result.clone(),
                access,
            },
        );
        result.map(|path| FlattenedRouteApproximation {
            path,
            max_error_in_canvas: tolerance,
        })
    }

    fn remove_cached_flatten(&mut self, edge: EdgeId) -> bool {
        let Some(cached) = self.flatten_cache.remove(&edge) else {
            return false;
        };
        self.flatten_lru.remove(&(cached.access, edge));
        true
    }

    fn touch_cached_flatten(&mut self, edge: EdgeId, previous_access: u64) {
        self.flatten_lru.remove(&(previous_access, edge));
        let access = self.reserve_flatten_access();
        if let Some(cached) = self.flatten_cache.get_mut(&edge) {
            cached.access = access;
        }
        self.flatten_lru.insert((access, edge));
    }

    fn reserve_flatten_access(&mut self) -> u64 {
        if self.next_flatten_access == u64::MAX {
            self.rebase_flatten_access_order();
        }
        let access = self.next_flatten_access;
        self.next_flatten_access = self.next_flatten_access.saturating_add(1);
        access
    }

    fn rebase_flatten_access_order(&mut self) {
        let edges = self
            .flatten_lru
            .iter()
            .map(|(_, edge)| *edge)
            .collect::<Vec<_>>();
        self.flatten_lru.clear();
        for (access, edge) in edges.into_iter().enumerate() {
            let access = access as u64;
            if let Some(cached) = self.flatten_cache.get_mut(&edge) {
                cached.access = access;
            }
            self.flatten_lru.insert((access, edge));
        }
        self.next_flatten_access = self.flatten_cache.len() as u64;
    }
}

enum RouteIntersection {
    Visible,
    Hidden,
    Overflow(EdgePathFlattenError),
}

pub(super) enum InternalRouteQueryOutcome {
    Complete {
        edges: BTreeSet<EdgeId>,
        overflows: BTreeMap<EdgeId, EdgePathFlattenError>,
        work: VisibleRouteQueryWork,
    },
    Incomplete {
        cursor: Box<VisibleRouteCursor>,
        work: VisibleRouteQueryWork,
    },
    Truncated {
        edges: BTreeSet<EdgeId>,
        overflows: BTreeMap<EdgeId, EdgePathFlattenError>,
        work: VisibleRouteQueryWork,
    },
}

impl ViewSession {
    /// Creates a visible-route work owner bound to this exact view generation.
    pub fn visible_route_work_session(&self) -> Result<VisibleRouteWorkSession, ViewSessionError> {
        VisibleRouteWorkSession::attach(self)
    }

    fn prepare_visible_route_work(
        &self,
        request: VisibleRouteWorkRequest,
    ) -> Result<(VisibleRouteWorkKey, CanvasRect), VisibleRouteWorkError> {
        self.runtime()?;
        let state = self
            .shared
            .state
            .try_borrow()
            .map_err(|_| ViewSessionError::Busy)?;
        ensure_open(state.status)?;
        let transform = ViewportTransform::new(state.facts.pan, state.facts.zoom)
            .ok_or(ViewSessionError::InvalidViewport)?;
        let canvas_bounds = expanded_view_rect(request.bounds, request.margin)
            .and_then(|bounds| bounds.to_canvas_rect(transform))
            .ok_or(VisibleRouteWorkError::InvalidRequest)?;
        let revision = VisibleRouteQueryRevision {
            view: state.id,
            generation: state.generation,
            viewport_revision: state.viewport_revision,
            route_facts_revision: state.derived.route_facts_revision(),
            interaction_order_revision: state.interaction_order_revision,
        };
        Ok((VisibleRouteWorkKey { request, revision }, canvas_bounds))
    }

    fn advance_visible_route_work(
        &self,
        session: &mut VisibleRouteWorkSession,
        request: VisibleRouteWorkRequest,
        entry_budget: usize,
    ) -> Result<VisibleRouteWorkUpdate, VisibleRouteWorkError> {
        let (mut key, canvas_bounds) = self.prepare_visible_route_work(request)?;

        if session.continuation.is_none()
            && let Some(status) = session
                .publication
                .as_ref()
                .filter(|publication| publication.key == key)
                .map(|publication| publication.status)
        {
            return Ok(session.update(
                key.revision,
                status,
                VisibleRouteQueryWork::default(),
                true,
            ));
        }

        let cursor = session
            .continuation
            .take()
            .filter(|continuation| continuation.key == key)
            .map(|continuation| continuation.cursor);
        let query = cursor.map_or_else(
            || VisibleRouteQuery::new(canvas_bounds, entry_budget),
            |cursor| VisibleRouteQuery::resume(cursor, entry_budget),
        );
        let outcome = match self.visible_route_page(query) {
            Ok(outcome) => outcome,
            Err(VisibleRouteQueryError::StaleCursor(reason)) if reason.is_restartable() => {
                let (fresh_key, fresh_canvas_bounds) = self.prepare_visible_route_work(request)?;
                key = fresh_key;
                self.visible_route_page(VisibleRouteQuery::new(fresh_canvas_bounds, entry_budget))
                    .map_err(map_visible_route_work_error)?
            }
            Err(error) => return Err(map_visible_route_work_error(error)),
        };

        match outcome {
            VisibleRouteQueryOutcome::Complete { routes, work } => {
                Ok(session.publish(key, routes, VisibleRouteWorkStatus::Complete, work))
            }
            VisibleRouteQueryOutcome::Incomplete { cursor, work } => {
                session.continuation = Some(VisibleRouteContinuation { key, cursor });
                Ok(session.update(
                    key.revision,
                    VisibleRouteWorkStatus::Pending,
                    work,
                    session.publication.is_some(),
                ))
            }
            VisibleRouteQueryOutcome::Truncated { routes, work } => {
                Ok(session.publish(key, routes, VisibleRouteWorkStatus::Truncated, work))
            }
        }
    }

    fn visible_route_page(
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
                VisibleRouteRequest::Start(canvas_bounds) => state
                    .derived
                    .new_visible_route_cursor(state.query_identity(), canvas_bounds),
                VisibleRouteRequest::Resume(cursor) => {
                    validate_cursor(&state, &cursor)?;
                    cursor
                }
            };
            state
                .derived
                .query_visible_route_page(cursor, query.entry_budget, transform.zoom)
        };

        match result {
            InternalRouteQueryOutcome::Complete {
                edges,
                overflows,
                work,
            } => Ok(VisibleRouteQueryOutcome::Complete {
                routes: self.materialize_visible_routes(edges, overflows)?,
                work,
            }),
            InternalRouteQueryOutcome::Incomplete { cursor, work } => {
                Ok(VisibleRouteQueryOutcome::Incomplete { cursor, work })
            }
            InternalRouteQueryOutcome::Truncated {
                edges,
                overflows,
                work,
            } => Ok(VisibleRouteQueryOutcome::Truncated {
                routes: self.materialize_visible_routes(edges, overflows)?,
                work,
            }),
        }
    }

    fn materialize_visible_routes(
        &self,
        edges: BTreeSet<EdgeId>,
        overflows: BTreeMap<EdgeId, EdgePathFlattenError>,
    ) -> Result<VisibleRoutes, VisibleRouteQueryError> {
        let mut edges = edges.into_iter().collect::<Vec<_>>();
        {
            let state = self
                .shared
                .state
                .try_borrow()
                .map_err(|_| ViewSessionError::Busy)?;
            ensure_open(state.status)?;
            sort_render_order(
                &mut edges,
                |edge| *edge,
                |edge| state.selected_edge_membership.contains_id(*edge),
                &state.edge_draw_order_ranks,
                None,
            );
        }
        let snapshot = self.shared.geometry_snapshot()?;
        let geometry_overflows = overflows
            .into_iter()
            .map(|(edge, error)| VisibleRouteGeometryOverflow { edge, error })
            .collect();
        Ok(VisibleRoutes {
            snapshot,
            edges,
            geometry_overflows,
        })
    }
}

fn map_visible_route_work_error(error: VisibleRouteQueryError) -> VisibleRouteWorkError {
    match error {
        VisibleRouteQueryError::Session(error) => VisibleRouteWorkError::Session(error),
        VisibleRouteQueryError::StaleCursor(
            VisibleRouteCursorStaleReason::ForeignView
            | VisibleRouteCursorStaleReason::ViewGeneration,
        ) => VisibleRouteWorkError::Session(ViewSessionError::InvalidViewport),
        VisibleRouteQueryError::StaleCursor(
            VisibleRouteCursorStaleReason::ViewportTransform
            | VisibleRouteCursorStaleReason::RouteGeometry,
        ) => VisibleRouteWorkError::Session(ViewSessionError::Busy),
    }
}

fn validate_cursor(
    state: &super::ViewState,
    cursor: &VisibleRouteCursor,
) -> Result<(), VisibleRouteQueryError> {
    visible_route_cursor_stale_reason(
        state.query_identity(),
        state.derived.visible_route_index_epoch(),
        cursor,
    )
    .map(VisibleRouteQueryError::StaleCursor)
    .map_or(Ok(()), Err)
}

fn visible_route_cursor_stale_reason(
    identity: ViewQueryIdentity,
    index_epoch: u128,
    cursor: &VisibleRouteCursor,
) -> Option<VisibleRouteCursorStaleReason> {
    match cursor.identity.stale_reason(identity) {
        Some(ViewQueryIdentityStaleReason::ForeignView) => {
            Some(VisibleRouteCursorStaleReason::ForeignView)
        }
        Some(ViewQueryIdentityStaleReason::ViewGeneration) => {
            Some(VisibleRouteCursorStaleReason::ViewGeneration)
        }
        Some(ViewQueryIdentityStaleReason::ViewportTransform) => {
            Some(VisibleRouteCursorStaleReason::ViewportTransform)
        }
        None if cursor.index_epoch != index_epoch => {
            Some(VisibleRouteCursorStaleReason::RouteGeometry)
        }
        None => None,
    }
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
    use jellyflow_core::core::{
        Edge, EdgeKind, EdgeRouteSpec, EdgeViewDescriptor, GraphBuilder, GraphId, PortId,
    };

    use super::*;
    use crate::runtime::adapter_protocol::PublishedEdgeRoute;
    use crate::runtime::geometry::{
        EdgeEndpointPosition, EdgePathLabel, EdgePosition, HandlePosition, PathCommand,
        resolve_edge_route_path,
    };
    use crate::runtime::graph_runtime::GraphRuntime;
    use crate::runtime::view_session::ViewSessionOptions;

    #[test]
    fn visible_route_cursor_identity_and_epoch_staleness_matrix() {
        let index = RouteGeometryIndex::default();
        let identity = ViewQueryIdentity::new(
            ViewId::unavailable(),
            ViewGeneration::initial(),
            ViewportRevision::initial(),
        );
        let cursor = index.new_cursor(
            identity,
            CanvasRect {
                origin: CanvasPoint::default(),
                size: CanvasSize {
                    width: 10.0,
                    height: 10.0,
                },
            },
        );
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
        for (current_identity, index_epoch, expected) in [
            (
                foreign_identity,
                cursor.index_epoch,
                Some(VisibleRouteCursorStaleReason::ForeignView),
            ),
            (
                next_generation,
                cursor.index_epoch,
                Some(VisibleRouteCursorStaleReason::ViewGeneration),
            ),
            (
                next_transform,
                cursor.index_epoch,
                Some(VisibleRouteCursorStaleReason::ViewportTransform),
            ),
            (
                identity,
                cursor.index_epoch.saturating_add(1),
                Some(VisibleRouteCursorStaleReason::RouteGeometry),
            ),
            (identity, cursor.index_epoch, None),
        ] {
            assert_eq!(
                visible_route_cursor_stale_reason(current_identity, index_epoch, &cursor),
                expected,
            );
        }
        assert!(!VisibleRouteCursorStaleReason::ForeignView.is_restartable());
        assert!(!VisibleRouteCursorStaleReason::ViewGeneration.is_restartable());
        assert!(VisibleRouteCursorStaleReason::ViewportTransform.is_restartable());
        assert!(VisibleRouteCursorStaleReason::RouteGeometry.is_restartable());
    }

    #[test]
    fn invalid_work_request_preserves_the_existing_continuation() {
        let graph = GraphBuilder::new(GraphId::from_u128(0x1a11_d001))
            .build()
            .unwrap();
        let runtime = GraphRuntime::builder(Default::default())
            .build(graph)
            .unwrap();
        let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
        let valid = VisibleRouteWorkRequest::new(ViewRect::new(
            crate::runtime::adapter_protocol::ViewPoint::new(0.0, 0.0),
            crate::runtime::adapter_protocol::ViewSize::new(800.0, 600.0),
        ));
        let (key, canvas_bounds) = view.prepare_visible_route_work(valid).unwrap();
        let identity = view.shared.state.borrow().query_identity();
        let cursor = RouteGeometryIndex::default().new_cursor(identity, canvas_bounds);
        let mut work = view.visible_route_work_session().unwrap();
        work.continuation = Some(VisibleRouteContinuation { key, cursor });

        let error = work
            .advance(&view, valid.with_margin(f32::NAN), 64)
            .unwrap_err();

        assert!(matches!(error, VisibleRouteWorkError::InvalidRequest));
        let continuation = work
            .continuation
            .as_ref()
            .expect("invalid input must not consume valid pending work");
        assert_eq!(continuation.key, key);
        assert_eq!(continuation.cursor.canvas_bounds, canvas_bounds);
        assert!(work.publication.is_none());
        assert_eq!(work.generation, 0);
    }

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
        let full_cursor = index.new_cursor(
            ViewQueryIdentity::new(
                ViewId::unavailable(),
                ViewGeneration::initial(),
                ViewportRevision::initial(),
            ),
            CanvasRect {
                origin: CanvasPoint { x: -10.0, y: -10.0 },
                size: CanvasSize {
                    width: 500.0,
                    height: 40.0,
                },
            },
        );
        let (full_found, full_work) = finish_query(&mut index, full_cursor, 64);
        let cursor = index.new_cursor(
            ViewQueryIdentity::new(
                ViewId::unavailable(),
                ViewGeneration::initial(),
                ViewportRevision::initial(),
            ),
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
        assert_eq!(found, full_found);
        assert_eq!(
            total_work.spatial_level_checks,
            full_work.spatial_level_checks
        );
        assert_eq!(
            total_work.spatial_source_setups,
            full_work.spatial_source_setups
        );
        assert_eq!(total_work.cells_examined, full_work.cells_examined);
        assert_eq!(total_work.entries_examined, full_work.entries_examined);
        assert!(total_work.entries_examined >= routes.len() as u64);
        assert_eq!(index.flatten_cache.len(), routes.len());

        let selected = routes[0].clone().with_selected(true);
        assert!(!index.upsert(selected.edge(), &selected).index_changed);
        assert_eq!(index.index_epoch, epoch);
        assert_eq!(index.flatten_cache.len(), routes.len());

        assert!(index.remove(routes[1].edge()).index_changed);
        let cursor = index.new_cursor(
            ViewQueryIdentity::new(
                ViewId::unavailable(),
                ViewGeneration::initial(),
                ViewportRevision::initial(),
            ),
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
            ViewQueryIdentity::new(
                ViewId::unavailable(),
                ViewGeneration::initial(),
                ViewportRevision::initial(),
            ),
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
        assert_eq!(work.spatial_level_checks, 1);
        assert_eq!(work.spatial_source_setups, 0);
        assert_eq!(work.entries_examined, 0);
    }

    #[test]
    fn rejected_route_admission_preserves_the_previous_complete_index_entry() {
        let edge = EdgeId::from_u128(1);
        let valid = test_route(edge, 0.0);
        let mut index = RouteGeometryIndex::default();
        let admitted = index.upsert(edge, &valid);
        let revision = admitted.geometry_revision;
        let index_epoch = index.index_epoch();
        let pick_epoch = index.pick_epoch();

        let mut invalid_facts = valid.facts().clone();
        invalid_facts.path = EdgePath {
            commands: vec![
                PathCommand::MoveTo(CanvasPoint {
                    x: -f32::MAX,
                    y: 0.0,
                }),
                PathCommand::LineTo(CanvasPoint {
                    x: f32::MAX,
                    y: 0.0,
                }),
            ],
            label: EdgePathLabel {
                point: CanvasPoint { x: 0.0, y: 0.0 },
                offset_x: f32::MAX,
                offset_y: 0.0,
            },
        };
        let invalid = PublishedEdgeRoute::new_for_test(edge, invalid_facts);

        assert!(index.try_upsert(edge, &invalid).is_none());
        assert_eq!(index.index_epoch(), index_epoch);
        assert_eq!(index.pick_epoch(), pick_epoch);
        let retained = index
            .indexed
            .get(&edge)
            .expect("valid route remains indexed");
        assert_eq!(RouteGeometryRevision::new(retained.path_revision), revision);
        assert_eq!(retained.instance, valid.instance());
        assert_eq!(retained.path, valid.facts().path);
        assert_eq!(retained.pick, IndexedRoutePickFacts::from(&valid));
        assert_eq!(
            edge_path_conservative_bounds(&valid.facts().path)
                .expect("valid route bounds")
                .and_then(expand_route_bounds),
            Some(retained.bounds),
        );
        assert!(index.spatial.routes.contains_key(&edge));
    }

    #[test]
    fn zero_budget_does_not_initialize_empty_visible_route_sources() {
        let edge = EdgeId::from_u128(1);
        let route = test_route_at(edge, 10_000.0, 10_000.0);
        let mut index = RouteGeometryIndex::default();
        index.upsert(edge, &route);
        let cursor = index.new_cursor(
            ViewQueryIdentity::new(
                ViewId::unavailable(),
                ViewGeneration::initial(),
                ViewportRevision::initial(),
            ),
            CanvasRect {
                origin: CanvasPoint { x: -9.0, y: -9.0 },
                size: CanvasSize {
                    width: 18.0,
                    height: 18.0,
                },
            },
        );

        let InternalRouteQueryOutcome::Incomplete { cursor, work } =
            index.query_page(cursor, 0, 1.0)
        else {
            panic!("zero budget must preserve an untouched visible-route cursor");
        };
        assert_eq!(work.spatial_work_units(), 0);
        assert_eq!(cursor.total_spatial_work, 0);

        let InternalRouteQueryOutcome::Incomplete { cursor, work } =
            index.query_page(cursor, 1, 1.0)
        else {
            panic!("one unit initializes only the occupied route level");
        };
        assert_eq!(work.spatial_level_checks, 1);
        assert_eq!(work.spatial_source_setups, 0);
        assert_eq!(work.entries_examined, 0);

        let (edges, work) = finish_query(&mut index, cursor, 1);
        assert!(edges.is_empty());
        assert_eq!(work.spatial_level_checks, 1);
        assert_eq!(work.spatial_source_setups, 4);
        assert_eq!(work.cells_examined, 4);
        assert_eq!(work.entries_examined, 0);
        assert_eq!(work.spatial_work_units(), 5);
    }

    #[test]
    fn cross_level_whole_level_route_scan_is_split_invariant() {
        let fine_edge = EdgeId::from_u128(1);
        let coarse_edge = EdgeId::from_u128(2);
        let fine = test_route(fine_edge, 0.0);
        let coarse = test_route_between(coarse_edge, -1_000_000.0, 1_000_000.0);
        let mut index = RouteGeometryIndex::default();
        index.upsert(fine_edge, &fine);
        index.upsert(coarse_edge, &coarse);
        let bounds = CanvasRect {
            origin: CanvasPoint {
                x: -1_000_000.0,
                y: -100.0,
            },
            size: CanvasSize {
                width: 2_000_000.0,
                height: 200.0,
            },
        };
        let identity = || {
            ViewQueryIdentity::new(
                ViewId::unavailable(),
                ViewGeneration::initial(),
                ViewportRevision::initial(),
            )
        };

        let full_cursor = index.new_cursor(identity(), bounds);
        let (full_edges, full_work) = finish_query(&mut index, full_cursor, 512);
        let split_cursor = index.new_cursor(identity(), bounds);
        let (split_edges, split_work) = finish_query(&mut index, split_cursor, 1);

        assert_eq!(full_edges, BTreeSet::from([fine_edge, coarse_edge]));
        assert_eq!(split_edges, full_edges);
        assert_eq!(
            split_work.spatial_level_checks,
            full_work.spatial_level_checks
        );
        assert_eq!(
            split_work.spatial_source_setups,
            full_work.spatial_source_setups
        );
        assert_eq!(split_work.cells_examined, full_work.cells_examined);
        assert_eq!(split_work.entries_examined, full_work.entries_examined);
        assert_eq!(split_work.spatial_level_checks, 3);
        assert_eq!(split_work.spatial_source_setups, 65);
        assert_eq!(split_work.cells_examined, 64);
        assert_eq!(split_work.entries_examined, 33);
        assert_eq!(split_work.spatial_work_units(), 101);
    }

    #[test]
    fn visible_route_query_compensates_for_flattening_error_at_curve_boundary() {
        let edge = EdgeId::from_u128(1);
        let mut route = test_route(edge, 0.0);
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
        route = PublishedEdgeRoute::new_for_test(edge, facts);
        let mut index = RouteGeometryIndex::default();
        assert!(index.upsert(edge, &route).index_changed);
        let cursor = index.new_cursor(
            ViewQueryIdentity::new(
                ViewId::unavailable(),
                ViewGeneration::initial(),
                ViewportRevision::initial(),
            ),
            CanvasRect {
                origin: CanvasPoint { x: 21.59, y: 62.99 },
                size: CanvasSize {
                    width: 0.02,
                    height: 0.02,
                },
            },
        );

        let (visible, _) = finish_query(&mut index, cursor, 64);

        assert_eq!(visible, BTreeSet::from([edge]));
    }

    #[test]
    fn extreme_finite_cubic_visibility_is_conservative_and_reports_overflow() {
        let edge = EdgeId::from_u128(1);
        let extent = 1.0e20;
        let mut route = test_route(edge, -extent);
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
        route = PublishedEdgeRoute::new_for_test(edge, facts);
        let mut index = RouteGeometryIndex::default();
        assert!(index.upsert(edge, &route).index_changed);
        let cursor = index.new_cursor(
            ViewQueryIdentity::new(
                ViewId::unavailable(),
                ViewGeneration::initial(),
                ViewportRevision::initial(),
            ),
            CanvasRect {
                origin: CanvasPoint { x: -1.0, y: -1.0 },
                size: CanvasSize {
                    width: 2.0,
                    height: 2.0,
                },
            },
        );

        let InternalRouteQueryOutcome::Complete {
            edges,
            overflows,
            work,
        } = index.query_page(cursor, 64, 1.0)
        else {
            panic!("small conservative visibility query must complete");
        };
        assert_eq!(edges, BTreeSet::from([edge]));
        assert_eq!(
            overflows.get(&edge),
            Some(&EdgePathFlattenError::InvalidGeometry)
        );
        assert_eq!(work.flatten_overflows, 1);
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
            ViewQueryIdentity::new(
                ViewId::unavailable(),
                ViewGeneration::initial(),
                ViewportRevision::initial(),
            ),
            CanvasRect {
                origin: CanvasPoint { x: -16.0, y: -16.0 },
                size: CanvasSize {
                    width: 9.0 * SPACING + 132.0,
                    height: 9.0 * SPACING + 32.0,
                },
            },
        );
        let InternalRouteQueryOutcome::Complete { edges, work, .. } =
            index.query_page(cursor, 1_024, 1.0)
        else {
            panic!("the local route query must finish inside one frame budget");
        };

        assert_eq!(edges.len(), 100);
        assert!(work.spatial_work_units() <= 1_024);
        assert!(work.entries_examined <= 256);
        assert_eq!(work.flatten_cache_misses, 100);
        assert_eq!(index.flatten_cache.len(), 100);
    }

    #[test]
    fn flatten_cache_evicts_the_least_recently_used_route_at_capacity() {
        let mut index = RouteGeometryIndex::default();
        for ordinal in 0..ROUTE_FLATTEN_CACHE_CAPACITY {
            let edge = EdgeId::from_u128(ordinal as u128 + 1);
            let route = test_route_at(
                edge,
                (ordinal % 64) as f32 * 160.0,
                (ordinal / 64) as f32 * 48.0,
            );
            assert!(index.upsert(edge, &route).index_changed);
            let mut work = RouteFlattenWork::default();
            index
                .flattened(edge, 1.0, &mut work)
                .expect("fixture route must flatten");
            assert_eq!(work.cache_misses, 1);
            assert_eq!(work.cache_evictions, 0);
        }

        let first = EdgeId::from_u128(1);
        let second = EdgeId::from_u128(2);
        let mut touch_work = RouteFlattenWork::default();
        index
            .flattened(first, 1.0, &mut touch_work)
            .expect("cached fixture route must flatten");
        assert_eq!(touch_work.cache_hits, 1);

        let extra = EdgeId::from_u128(ROUTE_FLATTEN_CACHE_CAPACITY as u128 + 1);
        let route = test_route(extra, 12_000.0);
        assert!(index.upsert(extra, &route).index_changed);
        let mut eviction_work = RouteFlattenWork::default();
        index
            .flattened(extra, 1.0, &mut eviction_work)
            .expect("extra fixture route must flatten");

        assert_eq!(eviction_work.cache_evictions, 1);
        assert_eq!(index.flatten_cache.len(), ROUTE_FLATTEN_CACHE_CAPACITY);
        assert_eq!(index.flatten_lru.len(), ROUTE_FLATTEN_CACHE_CAPACITY);
        assert!(index.flatten_cache.contains_key(&first));
        assert!(!index.flatten_cache.contains_key(&second));
        assert!(index.flatten_cache.contains_key(&extra));
    }

    fn finish_query(
        index: &mut RouteGeometryIndex,
        mut cursor: Box<VisibleRouteCursor>,
        page_budget: usize,
    ) -> (BTreeSet<EdgeId>, VisibleRouteQueryWork) {
        let mut total_work = VisibleRouteQueryWork::default();
        for _ in 0..4_096 {
            match index.query_page(cursor, page_budget, 1.0) {
                InternalRouteQueryOutcome::Complete { edges, work, .. } => {
                    assert!(work.spatial_work_units() <= page_budget as u64);
                    accumulate_query_work(&mut total_work, work);
                    return (edges, total_work);
                }
                InternalRouteQueryOutcome::Incomplete { cursor: next, work } => {
                    assert!(work.spatial_work_units() <= page_budget as u64);
                    accumulate_query_work(&mut total_work, work);
                    cursor = next;
                }
                InternalRouteQueryOutcome::Truncated { .. } => {
                    panic!("small indexed fixture must not exhaust the hard budget")
                }
            }
        }
        panic!("route query did not complete")
    }

    fn accumulate_query_work(total: &mut VisibleRouteQueryWork, page: VisibleRouteQueryWork) {
        total.spatial_level_checks = total
            .spatial_level_checks
            .saturating_add(page.spatial_level_checks);
        total.spatial_source_setups = total
            .spatial_source_setups
            .saturating_add(page.spatial_source_setups);
        total.cells_examined = total.cells_examined.saturating_add(page.cells_examined);
        total.entries_examined = total.entries_examined.saturating_add(page.entries_examined);
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

    fn test_route(edge: EdgeId, x: f32) -> PublishedEdgeRoute {
        test_route_at(edge, x, 0.0)
    }

    fn test_route_at(edge: EdgeId, x: f32, y: f32) -> PublishedEdgeRoute {
        test_route_between_at(edge, x, y, x + 100.0, y)
    }

    fn test_route_between(edge: EdgeId, from_x: f32, to_x: f32) -> PublishedEdgeRoute {
        test_route_between_at(edge, from_x, 0.0, to_x, 0.0)
    }

    fn test_route_between_at(
        edge: EdgeId,
        from_x: f32,
        from_y: f32,
        to_x: f32,
        to_y: f32,
    ) -> PublishedEdgeRoute {
        let mut model = Edge::new(EdgeKind::Data, PortId::from_u128(1), PortId::from_u128(2));
        model.view = EdgeViewDescriptor::new().with_route(EdgeRouteSpec::straight());
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
}
