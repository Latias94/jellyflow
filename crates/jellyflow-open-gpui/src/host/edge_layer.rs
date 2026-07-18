use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use jellyflow::core::EdgeId;
use jellyflow::runtime::{
    RouteGeometryRevision, ViewRect, ViewSession, ViewSessionError, VisibleRouteCursor,
    VisibleRouteCursorStaleReason, VisibleRouteQuery, VisibleRouteQueryError,
    VisibleRouteQueryOutcome, VisibleRouteQueryRevision, VisibleRouteQueryWork,
};
use open_gpui::{Path, Pixels};

use crate::chrome::{OpenGpuiEdgePainterBatchKey, OpenGpuiEdgeStrokePattern};

const OPEN_GPUI_EDGE_PATH_CACHE_CAPACITY: usize = super::OPEN_GPUI_MAX_EDGE_LAYER_BUDGET;
const OPEN_GPUI_EDGE_PAINT_ITEM_CACHE_CAPACITY: usize = super::OPEN_GPUI_MAX_EDGE_LAYER_BUDGET;
const OPEN_GPUI_EDGE_PAINT_PLAN_CACHE_CAPACITY: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct OpenGpuiEdgePathCacheKey {
    edge: EdgeId,
    geometry_revision: RouteGeometryRevision,
    pan_x_bits: u32,
    pan_y_bits: u32,
    zoom_bits: u32,
    origin_x_bits: u32,
    origin_y_bits: u32,
    stroke_width_bits: u32,
    stroke_pattern: OpenGpuiEdgeStrokePattern,
}

impl OpenGpuiEdgePathCacheKey {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        edge: EdgeId,
        geometry_revision: RouteGeometryRevision,
        pan_x: f32,
        pan_y: f32,
        zoom: f32,
        origin_x: f32,
        origin_y: f32,
        stroke_width: f32,
        stroke_pattern: OpenGpuiEdgeStrokePattern,
    ) -> Self {
        Self {
            edge,
            geometry_revision,
            pan_x_bits: pan_x.to_bits(),
            pan_y_bits: pan_y.to_bits(),
            zoom_bits: zoom.to_bits(),
            origin_x_bits: origin_x.to_bits(),
            origin_y_bits: origin_y.to_bits(),
            stroke_width_bits: stroke_width.to_bits(),
            stroke_pattern,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct OpenGpuiEdgePaintItemKey {
    path: OpenGpuiEdgePathCacheKey,
    batch: OpenGpuiEdgePainterBatchKey,
}

impl OpenGpuiEdgePaintItemKey {
    pub(super) fn new(path: OpenGpuiEdgePathCacheKey, batch: OpenGpuiEdgePainterBatchKey) -> Self {
        Self { path, batch }
    }

    pub(super) fn batch(self) -> OpenGpuiEdgePainterBatchKey {
        self.batch
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OpenGpuiEdgePaintPlanSegment {
    Ordinary,
    Emphasized,
    Hover(EdgeId),
    MarkerOrdinary,
    MarkerEmphasized,
    MarkerHover(EdgeId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct OpenGpuiEdgePaintPlanKey {
    plan_id: u64,
    segment: OpenGpuiEdgePaintPlanSegment,
    pan_x_bits: u32,
    pan_y_bits: u32,
    zoom_bits: u32,
    origin_x_bits: u32,
    origin_y_bits: u32,
}

impl OpenGpuiEdgePaintPlanKey {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        plan_id: u64,
        segment: OpenGpuiEdgePaintPlanSegment,
        pan_x: f32,
        pan_y: f32,
        zoom: f32,
        origin_x: f32,
        origin_y: f32,
    ) -> Self {
        Self {
            plan_id,
            segment,
            pan_x_bits: pan_x.to_bits(),
            pan_y_bits: pan_y.to_bits(),
            zoom_bits: zoom.to_bits(),
            origin_x_bits: origin_x.to_bits(),
            origin_y_bits: origin_y.to_bits(),
        }
    }
}

#[derive(Clone)]
pub(super) struct OpenGpuiEdgePaintBatch {
    pub(super) key: OpenGpuiEdgePainterBatchKey,
    pub(super) path: Path<Pixels>,
    pub(super) edge_count: usize,
}

pub(super) struct OpenGpuiEdgePathCache {
    paths: BTreeMap<OpenGpuiEdgePathCacheKey, (Path<Pixels>, u64)>,
    least_recently_used: BTreeSet<(u64, OpenGpuiEdgePathCacheKey)>,
    paint_items: BTreeMap<OpenGpuiEdgePaintItemKey, (Path<Pixels>, u64)>,
    paint_items_lru: BTreeSet<(u64, OpenGpuiEdgePaintItemKey)>,
    paint_plans: Vec<(OpenGpuiEdgePaintPlanKey, Rc<[OpenGpuiEdgePaintBatch]>)>,
    marker_plans: Vec<(OpenGpuiEdgePaintPlanKey, Rc<[OpenGpuiEdgePaintBatch]>)>,
    next_access: u64,
    capacity: usize,
    hits: u64,
    misses: u64,
    paint_preparations: u64,
    paint_batches: u64,
    painted_paths: u64,
    paint_plan_reuses: u64,
    paint_item_reuses: u64,
    paint_item_builds: u64,
    native_paint_calls: u64,
    native_edges_painted: u64,
    marker_preparations: u64,
    marker_batches: u64,
    marker_plan_reuses: u64,
    native_marker_paint_calls: u64,
    native_markers_painted: u64,
}

impl Default for OpenGpuiEdgePathCache {
    fn default() -> Self {
        Self {
            paths: BTreeMap::new(),
            least_recently_used: BTreeSet::new(),
            paint_items: BTreeMap::new(),
            paint_items_lru: BTreeSet::new(),
            paint_plans: Vec::new(),
            marker_plans: Vec::new(),
            next_access: 0,
            capacity: OPEN_GPUI_EDGE_PATH_CACHE_CAPACITY,
            hits: 0,
            misses: 0,
            paint_preparations: 0,
            paint_batches: 0,
            painted_paths: 0,
            paint_plan_reuses: 0,
            paint_item_reuses: 0,
            paint_item_builds: 0,
            native_paint_calls: 0,
            native_edges_painted: 0,
            marker_preparations: 0,
            marker_batches: 0,
            marker_plan_reuses: 0,
            native_marker_paint_calls: 0,
            native_markers_painted: 0,
        }
    }
}

impl OpenGpuiEdgePathCache {
    pub(super) fn resolve(
        &mut self,
        key: OpenGpuiEdgePathCacheKey,
        build: impl FnOnce() -> Option<Path<Pixels>>,
    ) -> Option<Path<Pixels>> {
        if let Some((path, previous_access)) = self.paths.get(&key) {
            let path = path.clone();
            let previous_access = *previous_access;
            self.hits = self.hits.saturating_add(1);
            self.touch(key, previous_access);
            return Some(path);
        }
        let path = build()?;
        self.misses = self.misses.saturating_add(1);
        while self.paths.len() >= self.capacity {
            let Some((access, cached_key)) = self.least_recently_used.pop_first() else {
                break;
            };
            if self
                .paths
                .get(&cached_key)
                .is_some_and(|(_, current_access)| *current_access == access)
            {
                self.paths.remove(&cached_key);
            }
        }
        let access = self.reserve_access();
        self.least_recently_used.insert((access, key));
        self.paths.insert(key, (path.clone(), access));
        Some(path)
    }

    fn touch(&mut self, key: OpenGpuiEdgePathCacheKey, previous_access: u64) {
        self.least_recently_used.remove(&(previous_access, key));
        let access = self.reserve_access();
        if let Some((_, current_access)) = self.paths.get_mut(&key) {
            *current_access = access;
        }
        self.least_recently_used.insert((access, key));
    }

    fn reserve_access(&mut self) -> u64 {
        if self.next_access == u64::MAX {
            self.rebase_access_order();
        }
        let access = self.next_access;
        self.next_access = self.next_access.saturating_add(1);
        access
    }

    fn rebase_access_order(&mut self) {
        let keys = self
            .least_recently_used
            .iter()
            .map(|(_, key)| *key)
            .collect::<Vec<_>>();
        self.least_recently_used.clear();
        for (access, key) in keys.into_iter().enumerate() {
            let access = access as u64;
            if let Some((_, current_access)) = self.paths.get_mut(&key) {
                *current_access = access;
            }
            self.least_recently_used.insert((access, key));
        }
        let item_keys = self
            .paint_items_lru
            .iter()
            .map(|(_, key)| *key)
            .collect::<Vec<_>>();
        self.paint_items_lru.clear();
        let offset = self.paths.len() as u64;
        for (index, key) in item_keys.into_iter().enumerate() {
            let access = offset.saturating_add(index as u64);
            if let Some((_, current_access)) = self.paint_items.get_mut(&key) {
                *current_access = access;
            }
            self.paint_items_lru.insert((access, key));
        }
        self.next_access = offset.saturating_add(self.paint_items.len() as u64);
    }

    pub(super) fn resolve_paint_item(
        &mut self,
        key: OpenGpuiEdgePaintItemKey,
        build: impl FnOnce() -> Option<Path<Pixels>>,
    ) -> Option<Path<Pixels>> {
        if let Some((path, previous_access)) = self.paint_items.get(&key) {
            let path = path.clone();
            let previous_access = *previous_access;
            self.paint_item_reuses = self.paint_item_reuses.saturating_add(1);
            self.touch_paint_item(key, previous_access);
            return Some(path);
        }
        let path = self.resolve(key.path, build)?;
        self.paint_item_builds = self.paint_item_builds.saturating_add(1);
        while self.paint_items.len() >= OPEN_GPUI_EDGE_PAINT_ITEM_CACHE_CAPACITY {
            let Some((access, cached_key)) = self.paint_items_lru.pop_first() else {
                break;
            };
            if self
                .paint_items
                .get(&cached_key)
                .is_some_and(|(_, current_access)| *current_access == access)
            {
                self.paint_items.remove(&cached_key);
            }
        }
        let access = self.reserve_access();
        self.paint_items_lru.insert((access, key));
        self.paint_items.insert(key, (path.clone(), access));
        Some(path)
    }

    fn touch_paint_item(&mut self, key: OpenGpuiEdgePaintItemKey, previous_access: u64) {
        self.paint_items_lru.remove(&(previous_access, key));
        let access = self.reserve_access();
        if let Some((_, current_access)) = self.paint_items.get_mut(&key) {
            *current_access = access;
        }
        self.paint_items_lru.insert((access, key));
    }

    pub(super) fn resolve_paint_plan(
        &mut self,
        key: Option<OpenGpuiEdgePaintPlanKey>,
        build: impl FnOnce(&mut Self) -> Vec<OpenGpuiEdgePaintBatch>,
    ) -> Rc<[OpenGpuiEdgePaintBatch]> {
        if let Some(key) = key.as_ref()
            && let Some(index) = self
                .paint_plans
                .iter()
                .position(|(cached_key, _)| cached_key == key)
        {
            let cached = self.paint_plans.remove(index);
            let plan = Rc::clone(&cached.1);
            self.paint_plans.push(cached);
            self.paint_plan_reuses = self.paint_plan_reuses.saturating_add(1);
            return plan;
        }
        let batches = build(self);
        self.record_paint_plan(
            batches.len(),
            batches.iter().map(|batch| batch.edge_count).sum(),
        );
        let plan = Rc::<[OpenGpuiEdgePaintBatch]>::from(batches);
        if let Some(key) = key {
            if self.paint_plans.len() >= OPEN_GPUI_EDGE_PAINT_PLAN_CACHE_CAPACITY {
                self.paint_plans.remove(0);
            }
            self.paint_plans.push((key, Rc::clone(&plan)));
        }
        plan
    }

    pub(super) fn resolve_marker_plan(
        &mut self,
        key: OpenGpuiEdgePaintPlanKey,
        build: impl FnOnce() -> Vec<OpenGpuiEdgePaintBatch>,
    ) -> Rc<[OpenGpuiEdgePaintBatch]> {
        if let Some(index) = self
            .marker_plans
            .iter()
            .position(|(cached_key, _)| *cached_key == key)
        {
            let cached = self.marker_plans.remove(index);
            let plan = Rc::clone(&cached.1);
            self.marker_plans.push(cached);
            self.marker_plan_reuses = self.marker_plan_reuses.saturating_add(1);
            return plan;
        }
        let batches = build();
        self.marker_preparations = self.marker_preparations.saturating_add(1);
        self.marker_batches = self.marker_batches.saturating_add(batches.len() as u64);
        let plan = Rc::<[OpenGpuiEdgePaintBatch]>::from(batches);
        if self.marker_plans.len() >= OPEN_GPUI_EDGE_PAINT_PLAN_CACHE_CAPACITY {
            self.marker_plans.remove(0);
        }
        self.marker_plans.push((key, Rc::clone(&plan)));
        plan
    }

    pub(super) fn clear(&mut self) {
        self.paths.clear();
        self.least_recently_used.clear();
        self.paint_items.clear();
        self.paint_items_lru.clear();
        self.paint_plans.clear();
        self.marker_plans.clear();
        self.next_access = 0;
    }

    pub(super) fn usage(&self) -> (u64, u64, usize) {
        (self.hits, self.misses, self.paths.len())
    }

    pub(super) fn record_paint_plan(&mut self, batches: usize, paths: usize) {
        self.paint_preparations = self.paint_preparations.saturating_add(1);
        self.paint_batches = self.paint_batches.saturating_add(batches as u64);
        self.painted_paths = self.painted_paths.saturating_add(paths as u64);
    }

    pub(super) fn record_native_paint(&mut self, calls: usize, edges: usize) {
        self.native_paint_calls = self.native_paint_calls.saturating_add(calls as u64);
        self.native_edges_painted = self.native_edges_painted.saturating_add(edges as u64);
    }

    pub(super) fn record_native_marker_paint(&mut self, calls: usize, markers: usize) {
        self.native_marker_paint_calls =
            self.native_marker_paint_calls.saturating_add(calls as u64);
        self.native_markers_painted = self.native_markers_painted.saturating_add(markers as u64);
    }

    pub(super) fn paint_usage(&self) -> OpenGpuiEdgePaintUsage {
        OpenGpuiEdgePaintUsage {
            paint_preparations: self.paint_preparations,
            paint_batches: self.paint_batches,
            painted_paths: self.painted_paths,
            paint_plan_reuses: self.paint_plan_reuses,
            paint_item_reuses: self.paint_item_reuses,
            paint_item_builds: self.paint_item_builds,
            native_paint_calls: self.native_paint_calls,
            native_edges_painted: self.native_edges_painted,
            marker_preparations: self.marker_preparations,
            marker_batches: self.marker_batches,
            marker_plan_reuses: self.marker_plan_reuses,
            native_marker_paint_calls: self.native_marker_paint_calls,
            native_markers_painted: self.native_markers_painted,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct OpenGpuiEdgePaintUsage {
    pub(super) paint_preparations: u64,
    pub(super) paint_batches: u64,
    pub(super) painted_paths: u64,
    pub(super) paint_plan_reuses: u64,
    pub(super) paint_item_reuses: u64,
    pub(super) paint_item_builds: u64,
    pub(super) native_paint_calls: u64,
    pub(super) native_edges_painted: u64,
    pub(super) marker_preparations: u64,
    pub(super) marker_batches: u64,
    pub(super) marker_plan_reuses: u64,
    pub(super) native_marker_paint_calls: u64,
    pub(super) native_markers_painted: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct OpenGpuiVisibleRouteIdentity {
    pub(super) edge: EdgeId,
    pub(super) geometry_revision: Option<RouteGeometryRevision>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct OpenGpuiVisibleRouteQueryReport {
    pub(super) work: VisibleRouteQueryWork,
    pub(super) pending: bool,
    pub(super) budget_exhausted: bool,
    pub(super) geometry_overflows: usize,
    pub(super) reused_previous_plan: bool,
}

pub(super) struct OpenGpuiVisibleRoutePlan {
    pub(super) routes: Rc<[OpenGpuiVisibleRouteIdentity]>,
    pub(super) revision: VisibleRouteQueryRevision,
    pub(super) generation: u64,
    pub(super) report: OpenGpuiVisibleRouteQueryReport,
}

#[derive(Default)]
pub(super) struct OpenGpuiVisibleRouteCache {
    bounds: Option<ViewRect>,
    cursor: Option<Box<VisibleRouteCursor>>,
    exhausted_key: Option<(ViewRect, VisibleRouteQueryRevision)>,
    complete: Rc<[OpenGpuiVisibleRouteIdentity]>,
    complete_key: Option<(ViewRect, VisibleRouteQueryRevision)>,
    complete_geometry_overflows: usize,
    complete_generation: u64,
}

impl OpenGpuiVisibleRouteCache {
    pub(super) fn clear(&mut self) {
        self.bounds = None;
        self.cursor = None;
        self.exhausted_key = None;
        self.complete = Rc::default();
        self.complete_key = None;
        self.complete_geometry_overflows = 0;
        self.complete_generation = self.complete_generation.saturating_add(1);
    }

    pub(super) fn query(
        &mut self,
        view: &ViewSession,
        bounds: ViewRect,
        entry_budget: usize,
    ) -> Result<OpenGpuiVisibleRoutePlan, ViewSessionError> {
        let revision = view.visible_route_query_revision()?;
        if self.cursor.is_none() && self.complete_key == Some((bounds, revision)) {
            return Ok(OpenGpuiVisibleRoutePlan {
                routes: self.complete.clone(),
                revision,
                generation: self.complete_generation,
                report: OpenGpuiVisibleRouteQueryReport {
                    geometry_overflows: self.complete_geometry_overflows,
                    reused_previous_plan: true,
                    ..OpenGpuiVisibleRouteQueryReport::default()
                },
            });
        }
        if self.cursor.is_none() && self.exhausted_key == Some((bounds, revision)) {
            return Ok(OpenGpuiVisibleRoutePlan {
                routes: self.complete.clone(),
                revision,
                generation: self.complete_generation,
                report: OpenGpuiVisibleRouteQueryReport {
                    budget_exhausted: true,
                    reused_previous_plan: self.complete_key.is_some(),
                    ..OpenGpuiVisibleRouteQueryReport::default()
                },
            });
        }
        if self.bounds != Some(bounds) {
            self.bounds = Some(bounds);
            self.cursor = None;
            self.exhausted_key = None;
        }

        let query = self.cursor.take().map_or_else(
            || VisibleRouteQuery::new(bounds, entry_budget).with_margin(4.0),
            |cursor| VisibleRouteQuery::resume(cursor, entry_budget),
        );
        let outcome = match view.visible_routes(query) {
            Ok(outcome) => outcome,
            Err(VisibleRouteQueryError::StaleCursor(reason)) if reason.is_restartable() => view
                .visible_routes(VisibleRouteQuery::new(bounds, entry_budget).with_margin(4.0))
                .map_err(map_query_error)?,
            Err(error) => return Err(map_query_error(error)),
        };

        Ok(match outcome {
            VisibleRouteQueryOutcome::Complete { routes, work } => {
                self.exhausted_key = None;
                self.complete = routes
                    .iter()
                    .map(|route| OpenGpuiVisibleRouteIdentity {
                        edge: route.edge(),
                        geometry_revision: Some(route.geometry_revision()),
                    })
                    .collect::<Vec<_>>()
                    .into();
                let revision = view.visible_route_query_revision()?;
                self.complete_key = Some((bounds, revision));
                self.complete_geometry_overflows = routes.geometry_overflows().len();
                self.complete_generation = self.complete_generation.saturating_add(1);
                OpenGpuiVisibleRoutePlan {
                    routes: self.complete.clone(),
                    revision,
                    generation: self.complete_generation,
                    report: OpenGpuiVisibleRouteQueryReport {
                        work,
                        geometry_overflows: self.complete_geometry_overflows,
                        ..OpenGpuiVisibleRouteQueryReport::default()
                    },
                }
            }
            VisibleRouteQueryOutcome::Incomplete { cursor, work } => {
                self.exhausted_key = None;
                self.cursor = Some(cursor);
                OpenGpuiVisibleRoutePlan {
                    routes: self.complete.clone(),
                    revision,
                    generation: self.complete_generation,
                    report: OpenGpuiVisibleRouteQueryReport {
                        work,
                        pending: true,
                        reused_previous_plan: self.complete_key.is_some(),
                        ..OpenGpuiVisibleRouteQueryReport::default()
                    },
                }
            }
            VisibleRouteQueryOutcome::BudgetExhausted { work } => {
                self.exhausted_key = Some((bounds, revision));
                OpenGpuiVisibleRoutePlan {
                    routes: self.complete.clone(),
                    revision,
                    generation: self.complete_generation,
                    report: OpenGpuiVisibleRouteQueryReport {
                        work,
                        budget_exhausted: true,
                        reused_previous_plan: self.complete_key.is_some(),
                        ..OpenGpuiVisibleRouteQueryReport::default()
                    },
                }
            }
        })
    }
}

fn map_query_error(error: VisibleRouteQueryError) -> ViewSessionError {
    match error {
        VisibleRouteQueryError::Session(error) => error,
        VisibleRouteQueryError::InvalidQuery
        | VisibleRouteQueryError::StaleCursor(
            VisibleRouteCursorStaleReason::ForeignView
            | VisibleRouteCursorStaleReason::ViewGeneration,
        ) => ViewSessionError::InvalidViewport,
        VisibleRouteQueryError::StaleCursor(
            VisibleRouteCursorStaleReason::ViewportTransform
            | VisibleRouteCursorStaleReason::RouteGeometry,
        ) => ViewSessionError::Busy,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_cache_has_no_stale_query_capability() {
        let mut cache = OpenGpuiVisibleRouteCache::default();
        cache.clear();

        assert!(cache.bounds.is_none());
        assert!(cache.cursor.is_none());
        assert!(cache.exhausted_key.is_none());
        assert!(cache.complete.is_empty());
    }
}
