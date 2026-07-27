use std::collections::{BTreeMap, BTreeSet};
use std::mem::{size_of, size_of_val};
use std::rc::Rc;

use jellyflow_core::EdgeId;
use jellyflow_runtime::{
    geometry::ViewRect, view::RouteGeometryRevision, view::ViewSession, view::ViewSessionError,
    view::VisibleRouteCursor, view::VisibleRouteCursorStaleReason, view::VisibleRouteQuery,
    view::VisibleRouteQueryError, view::VisibleRouteQueryOutcome, view::VisibleRouteQueryRevision,
    view::VisibleRouteQueryWork,
};
use open_gpui::{Path, PathVertex, Pixels};

use crate::chrome::{OpenGpuiEdgePainterBatchKey, OpenGpuiEdgeStrokePattern};

const OPEN_GPUI_EDGE_PATH_CACHE_CAPACITY: usize = super::OPEN_GPUI_MAX_EDGE_LAYER_BUDGET;
const OPEN_GPUI_EDGE_PATH_CACHE_VERTEX_BUDGET: usize = 1_048_576;
const OPEN_GPUI_EDGE_PATH_CACHE_ESTIMATED_BYTE_BUDGET: usize = 64 * 1_024 * 1_024;
const OPEN_GPUI_EDGE_PLAN_CACHE_CAPACITY: usize = 8;
const OPEN_GPUI_EDGE_PLAN_CACHE_VERTEX_BUDGET: usize = 2_097_152;
const OPEN_GPUI_EDGE_PLAN_CACHE_ESTIMATED_BYTE_BUDGET: usize = 64 * 1_024 * 1_024;
const OPEN_GPUI_EDGE_PLAN_RESERVED_VERTICES_PER_ITEM: usize = 30;
// During a draw Open GPUI keeps the cached path, the previous scene, the caller-owned paint
// clone, the scaled temporary, and the next-scene clone alive at the same time.
const OPEN_GPUI_EDGE_PLAN_LIVE_PATH_COPY_FACTOR: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct OpenGpuiEdgeStrokeTessellationKey {
    stroke_width_bits: u32,
    stroke_pattern: OpenGpuiEdgeStrokePattern,
    round_line_cap: bool,
    round_line_join: bool,
}

impl OpenGpuiEdgeStrokeTessellationKey {
    fn new(
        stroke_width: f32,
        stroke_pattern: OpenGpuiEdgeStrokePattern,
        round_line_cap: bool,
        round_line_join: bool,
    ) -> Self {
        Self {
            stroke_width_bits: stroke_width.to_bits(),
            stroke_pattern,
            round_line_cap,
            round_line_join,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct OpenGpuiEdgePathCacheKey {
    edge: EdgeId,
    geometry_revision: u128,
    pan_x_bits: u32,
    pan_y_bits: u32,
    zoom_bits: u32,
    origin_x_bits: u32,
    origin_y_bits: u32,
    stroke: OpenGpuiEdgeStrokeTessellationKey,
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
        round_line_cap: bool,
        round_line_join: bool,
    ) -> Self {
        Self {
            edge,
            geometry_revision: geometry_revision.get(),
            pan_x_bits: pan_x.to_bits(),
            pan_y_bits: pan_y.to_bits(),
            zoom_bits: zoom.to_bits(),
            origin_x_bits: origin_x.to_bits(),
            origin_y_bits: origin_y.to_bits(),
            stroke: OpenGpuiEdgeStrokeTessellationKey::new(
                stroke_width,
                stroke_pattern,
                round_line_cap,
                round_line_join,
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OpenGpuiEdgePaintPlanSegment {
    Ordinary,
    Emphasized,
    Hover(EdgeId),
    Focus(EdgeId),
    FocusBase(EdgeId),
    MarkerOrdinary,
    MarkerEmphasized,
    MarkerHover(EdgeId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum OpenGpuiEdgePaintPlanSlot {
    Ordinary,
    Emphasized,
    Hover,
    Focus,
    FocusBase,
}

impl OpenGpuiEdgePaintPlanSegment {
    fn slot(self) -> OpenGpuiEdgePaintPlanSlot {
        match self {
            Self::Ordinary | Self::MarkerOrdinary => OpenGpuiEdgePaintPlanSlot::Ordinary,
            Self::Emphasized | Self::MarkerEmphasized => OpenGpuiEdgePaintPlanSlot::Emphasized,
            Self::Hover(_) | Self::MarkerHover(_) => OpenGpuiEdgePaintPlanSlot::Hover,
            Self::Focus(_) => OpenGpuiEdgePaintPlanSlot::Focus,
            Self::FocusBase(_) => OpenGpuiEdgePaintPlanSlot::FocusBase,
        }
    }

    fn is_marker(self) -> bool {
        matches!(
            self,
            Self::MarkerOrdinary | Self::MarkerEmphasized | Self::MarkerHover(_)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct OpenGpuiEdgePaintPlanGeneration {
    plan_id: u64,
    pan_x_bits: u32,
    pan_y_bits: u32,
    zoom_bits: u32,
    origin_x_bits: u32,
    origin_y_bits: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct OpenGpuiEdgePaintPlanKey {
    generation: OpenGpuiEdgePaintPlanGeneration,
    segment: OpenGpuiEdgePaintPlanSegment,
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
            generation: OpenGpuiEdgePaintPlanGeneration {
                plan_id,
                pan_x_bits: pan_x.to_bits(),
                pan_y_bits: pan_y.to_bits(),
                zoom_bits: zoom.to_bits(),
                origin_x_bits: origin_x.to_bits(),
                origin_y_bits: origin_y.to_bits(),
            },
            segment,
        }
    }
}

#[derive(Clone)]
pub(super) struct OpenGpuiEdgePaintBatch {
    pub(super) key: OpenGpuiEdgePainterBatchKey,
    pub(super) path: Rc<Path<Pixels>>,
    pub(super) edge_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OpenGpuiEdgePaintPlanCompleteness {
    Complete,
    Incomplete { rejected_items: usize },
}

#[derive(Clone)]
pub(super) struct OpenGpuiEdgePaintPlan {
    batches: Rc<[OpenGpuiEdgePaintBatch]>,
    completeness: OpenGpuiEdgePaintPlanCompleteness,
    footprint: OpenGpuiEdgeCacheFootprint,
    build_peak_footprint: OpenGpuiEdgeCacheFootprint,
}

impl OpenGpuiEdgePaintPlan {
    pub(super) fn batches(&self) -> &[OpenGpuiEdgePaintBatch] {
        &self.batches
    }

    pub(super) fn completeness(&self) -> OpenGpuiEdgePaintPlanCompleteness {
        self.completeness
    }

    pub(super) fn is_complete(&self) -> bool {
        self.completeness() == OpenGpuiEdgePaintPlanCompleteness::Complete
    }

    pub(super) fn rejected_items(&self) -> usize {
        match self.completeness {
            OpenGpuiEdgePaintPlanCompleteness::Complete => 0,
            OpenGpuiEdgePaintPlanCompleteness::Incomplete { rejected_items } => rejected_items,
        }
    }

    pub(super) fn edge_count(&self) -> usize {
        self.batches.iter().map(|batch| batch.edge_count).sum()
    }

    pub(super) fn footprint(&self) -> OpenGpuiEdgeCacheFootprint {
        self.footprint
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct OpenGpuiEdgeCacheFootprint {
    pub(super) vertices: usize,
    pub(super) estimated_bytes: usize,
}

impl OpenGpuiEdgeCacheFootprint {
    fn for_path_storage(vertices: usize, capacity: usize) -> Self {
        Self {
            vertices,
            estimated_bytes: size_of::<Path<Pixels>>()
                .saturating_add(2 * size_of::<usize>())
                .saturating_add(capacity.saturating_mul(size_of::<PathVertex<Pixels>>())),
        }
    }

    fn for_path_allocation(path: &Path<Pixels>) -> Self {
        Self::for_path_storage(path.vertices.len(), path.vertices.capacity())
    }

    fn for_cached_path(path: &Path<Pixels>) -> Self {
        let allocation = Self::for_path_allocation(path);
        Self {
            vertices: allocation.vertices,
            estimated_bytes: allocation
                .estimated_bytes
                .saturating_add(size_of::<Rc<Path<Pixels>>>())
                .saturating_add(2 * size_of::<OpenGpuiEdgePathCacheKey>())
                .saturating_add(2 * size_of::<u64>()),
        }
    }

    fn for_plan(plan: &[OpenGpuiEdgePaintBatch]) -> Self {
        if plan.is_empty() {
            return Self::default();
        }
        plan.iter().fold(
            Self {
                vertices: 0,
                estimated_bytes: (2 * size_of::<usize>()).saturating_add(size_of_val(plan)),
            },
            |footprint, batch| footprint.saturating_add(Self::for_path_allocation(&batch.path)),
        )
    }

    fn saturating_add(self, other: Self) -> Self {
        Self {
            vertices: self.vertices.saturating_add(other.vertices),
            estimated_bytes: self.estimated_bytes.saturating_add(other.estimated_bytes),
        }
    }

    fn saturating_sub(self, other: Self) -> Self {
        Self {
            vertices: self.vertices.saturating_sub(other.vertices),
            estimated_bytes: self.estimated_bytes.saturating_sub(other.estimated_bytes),
        }
    }

    fn componentwise_max(self, other: Self) -> Self {
        Self {
            vertices: self.vertices.max(other.vertices),
            estimated_bytes: self.estimated_bytes.max(other.estimated_bytes),
        }
    }

    fn saturating_mul(self, factor: usize) -> Self {
        Self {
            vertices: self.vertices.saturating_mul(factor),
            estimated_bytes: self.estimated_bytes.saturating_mul(factor),
        }
    }

    fn componentwise_min(self, other: Self) -> Self {
        Self {
            vertices: self.vertices.min(other.vertices),
            estimated_bytes: self.estimated_bytes.min(other.estimated_bytes),
        }
    }

    fn divided_by(self, divisor: usize) -> Self {
        debug_assert!(divisor > 0);
        Self {
            vertices: self.vertices / divisor,
            estimated_bytes: self.estimated_bytes / divisor,
        }
    }

    fn fits(self, vertex_budget: usize, estimated_byte_budget: usize) -> bool {
        self.vertices <= vertex_budget && self.estimated_bytes <= estimated_byte_budget
    }
}

fn minimum_plan_reservation(items: usize) -> OpenGpuiEdgeCacheFootprint {
    if items == 0 {
        return OpenGpuiEdgeCacheFootprint::default();
    }
    let vertices = items.saturating_mul(OPEN_GPUI_EDGE_PLAN_RESERVED_VERTICES_PER_ITEM);
    let path = OpenGpuiEdgeCacheFootprint::for_path_storage(vertices, vertices);
    OpenGpuiEdgeCacheFootprint {
        vertices: path.vertices,
        estimated_bytes: path
            .estimated_bytes
            .saturating_add(size_of::<OpenGpuiEdgePaintBatch>()),
    }
}

pub(super) struct OpenGpuiEdgePaintPlanBuilder {
    batches: BTreeMap<OpenGpuiEdgePainterBatchKey, OpenGpuiEdgePaintBatch>,
    footprint: OpenGpuiEdgeCacheFootprint,
    build_peak_footprint: OpenGpuiEdgeCacheFootprint,
    limit: OpenGpuiEdgeCacheFootprint,
    rejected_items: usize,
}

impl OpenGpuiEdgePaintPlanBuilder {
    fn new(limit: OpenGpuiEdgeCacheFootprint) -> Self {
        Self {
            batches: BTreeMap::new(),
            footprint: OpenGpuiEdgeCacheFootprint::default(),
            build_peak_footprint: OpenGpuiEdgeCacheFootprint::default(),
            limit,
            rejected_items: 0,
        }
    }

    pub(super) fn merge_shared(
        &mut self,
        key: OpenGpuiEdgePainterBatchKey,
        path: Rc<Path<Pixels>>,
    ) -> bool {
        let Some(batch) = self.batches.get(&key) else {
            let path_footprint = OpenGpuiEdgeCacheFootprint::for_path_allocation(&path);
            let plan_overhead = if self.batches.is_empty() {
                (2 * size_of::<usize>()).saturating_add(size_of::<OpenGpuiEdgePaintBatch>())
            } else {
                size_of::<OpenGpuiEdgePaintBatch>()
            };
            let candidate = self.footprint.saturating_add(OpenGpuiEdgeCacheFootprint {
                vertices: path_footprint.vertices,
                estimated_bytes: path_footprint.estimated_bytes.saturating_add(plan_overhead),
            });
            if !candidate.fits(self.limit.vertices, self.limit.estimated_bytes) {
                self.reject_unbuilt_items(1);
                return false;
            }
            self.batches.insert(
                key,
                OpenGpuiEdgePaintBatch {
                    key,
                    path,
                    edge_count: 1,
                },
            );
            self.record_accepted_footprint(candidate);
            return true;
        };

        let previous_path_footprint = OpenGpuiEdgeCacheFootprint::for_path_allocation(&batch.path);
        let required_vertices = batch
            .path
            .vertices
            .len()
            .saturating_add(path.vertices.len());
        let predicted_capacity = if required_vertices <= batch.path.vertices.capacity() {
            batch.path.vertices.capacity()
        } else {
            required_vertices.max(batch.path.vertices.capacity().saturating_mul(2))
        };
        let predicted_path_footprint =
            OpenGpuiEdgeCacheFootprint::for_path_storage(required_vertices, predicted_capacity);
        let candidate = self
            .footprint
            .saturating_sub(previous_path_footprint)
            .saturating_add(predicted_path_footprint);
        if !candidate.fits(self.limit.vertices, self.limit.estimated_bytes) {
            self.reject_unbuilt_items(1);
            return false;
        }

        let additional = path.vertices.len();
        let actual_path_footprint = {
            let batch = self
                .batches
                .get_mut(&key)
                .expect("the checked native edge batch remains present");
            let batch_path = Rc::make_mut(&mut batch.path);
            if batch_path.vertices.try_reserve(additional).is_err() {
                None
            } else {
                Some(OpenGpuiEdgeCacheFootprint::for_path_storage(
                    required_vertices,
                    batch_path.vertices.capacity(),
                ))
            }
        };
        let Some(actual_path_footprint) = actual_path_footprint else {
            self.reject_unbuilt_items(1);
            return false;
        };
        let actual_candidate = self
            .footprint
            .saturating_sub(previous_path_footprint)
            .saturating_add(actual_path_footprint);
        if !actual_candidate.fits(self.limit.vertices, self.limit.estimated_bytes) {
            let batch_count = self.batches.len();
            let removed = self
                .batches
                .remove(&key)
                .expect("the over-allocated native edge batch remains present");
            let plan_overhead =
                size_of::<OpenGpuiEdgePaintBatch>().saturating_add(if batch_count == 1 {
                    2 * size_of::<usize>()
                } else {
                    0
                });
            self.footprint = self.footprint.saturating_sub(OpenGpuiEdgeCacheFootprint {
                vertices: previous_path_footprint.vertices,
                estimated_bytes: previous_path_footprint
                    .estimated_bytes
                    .saturating_add(plan_overhead),
            });
            self.reject_unbuilt_items(removed.edge_count.saturating_add(1));
            return false;
        }

        let batch = self
            .batches
            .get_mut(&key)
            .expect("the reserved native edge batch remains present");
        let batch_path = Rc::make_mut(&mut batch.path);
        batch_path.bounds = batch_path.bounds.union(&path.bounds);
        batch_path.vertices.extend_from_slice(&path.vertices);
        batch.edge_count = batch.edge_count.saturating_add(1);
        self.record_accepted_footprint(actual_candidate);
        true
    }

    pub(super) fn merge_owned(
        &mut self,
        key: OpenGpuiEdgePainterBatchKey,
        path: Path<Pixels>,
    ) -> bool {
        self.merge_shared(key, Rc::new(path))
    }

    pub(super) fn reject_unbuilt_items(&mut self, count: usize) {
        self.rejected_items = self.rejected_items.saturating_add(count);
    }

    pub(super) fn next_path_vertex_budget(&self, remaining_items: usize) -> usize {
        let remaining = self.limit.saturating_sub(self.footprint);
        let item_count = remaining_items.max(1);
        let fixed_bytes = size_of::<Path<Pixels>>()
            .saturating_add(2 * size_of::<usize>())
            .saturating_add(size_of::<OpenGpuiEdgePaintBatch>());
        let vertices_by_bytes = remaining.estimated_bytes.saturating_sub(fixed_bytes)
            / size_of::<PathVertex<Pixels>>().max(1);
        remaining.vertices.min(vertices_by_bytes) / item_count
    }

    fn record_accepted_footprint(&mut self, footprint: OpenGpuiEdgeCacheFootprint) {
        self.footprint = footprint;
        self.build_peak_footprint = self.build_peak_footprint.componentwise_max(footprint);
    }

    fn finish(self) -> OpenGpuiEdgePaintPlan {
        let batches =
            Rc::<[OpenGpuiEdgePaintBatch]>::from(self.batches.into_values().collect::<Vec<_>>());
        let footprint = OpenGpuiEdgeCacheFootprint::for_plan(&batches);
        debug_assert!(footprint.fits(self.limit.vertices, self.limit.estimated_bytes));
        OpenGpuiEdgePaintPlan {
            batches,
            completeness: if self.rejected_items == 0 {
                OpenGpuiEdgePaintPlanCompleteness::Complete
            } else {
                OpenGpuiEdgePaintPlanCompleteness::Incomplete {
                    rejected_items: self.rejected_items,
                }
            },
            footprint,
            build_peak_footprint: self.build_peak_footprint,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct OpenGpuiEdgeCacheBudgets {
    path_entries: usize,
    path_vertices: usize,
    path_estimated_bytes: usize,
    plan_entries: usize,
    plan_vertices: usize,
    plan_estimated_bytes: usize,
    plan_live_vertices: usize,
    plan_live_estimated_bytes: usize,
}

impl Default for OpenGpuiEdgeCacheBudgets {
    fn default() -> Self {
        Self {
            path_entries: OPEN_GPUI_EDGE_PATH_CACHE_CAPACITY,
            path_vertices: OPEN_GPUI_EDGE_PATH_CACHE_VERTEX_BUDGET,
            path_estimated_bytes: OPEN_GPUI_EDGE_PATH_CACHE_ESTIMATED_BYTE_BUDGET,
            plan_entries: OPEN_GPUI_EDGE_PLAN_CACHE_CAPACITY,
            plan_vertices: OPEN_GPUI_EDGE_PLAN_CACHE_VERTEX_BUDGET,
            plan_estimated_bytes: OPEN_GPUI_EDGE_PLAN_CACHE_ESTIMATED_BYTE_BUDGET,
            plan_live_vertices: OPEN_GPUI_EDGE_PLAN_CACHE_VERTEX_BUDGET
                .saturating_mul(OPEN_GPUI_EDGE_PLAN_LIVE_PATH_COPY_FACTOR),
            plan_live_estimated_bytes: OPEN_GPUI_EDGE_PLAN_CACHE_ESTIMATED_BYTE_BUDGET
                .saturating_mul(OPEN_GPUI_EDGE_PLAN_LIVE_PATH_COPY_FACTOR),
        }
    }
}

struct OpenGpuiCachedEdgePath {
    path: Rc<Path<Pixels>>,
    access: u64,
    footprint: OpenGpuiEdgeCacheFootprint,
}

struct OpenGpuiCachedEdgePaintPlan {
    key: OpenGpuiEdgePaintPlanKey,
    plan: OpenGpuiEdgePaintPlan,
    footprint: OpenGpuiEdgeCacheFootprint,
}

#[derive(Default)]
pub(super) struct OpenGpuiEdgePathCache {
    paths: BTreeMap<OpenGpuiEdgePathCacheKey, OpenGpuiCachedEdgePath>,
    least_recently_used: BTreeSet<(u64, OpenGpuiEdgePathCacheKey)>,
    paint_plans: BTreeMap<OpenGpuiEdgePaintPlanSlot, OpenGpuiCachedEdgePaintPlan>,
    marker_plans: BTreeMap<OpenGpuiEdgePaintPlanSlot, OpenGpuiCachedEdgePaintPlan>,
    path_footprint: OpenGpuiEdgeCacheFootprint,
    plan_footprint: OpenGpuiEdgeCacheFootprint,
    next_access: u64,
    budgets: OpenGpuiEdgeCacheBudgets,
    hits: u64,
    misses: u64,
    path_budget_evictions: u64,
    path_budget_rejections: u64,
    plan_budget_rejections: u64,
    plan_incomplete_builds: u64,
    plan_rejected_items: u64,
    plan_build_peak_footprint: OpenGpuiEdgeCacheFootprint,
    plan_live_peak_footprint: OpenGpuiEdgeCacheFootprint,
    paint_preparations: u64,
    paint_batches: u64,
    painted_paths: u64,
    paint_plan_reuses: u64,
    native_paint_calls: u64,
    native_edges_painted: u64,
    marker_preparations: u64,
    marker_batches: u64,
    marker_plan_reuses: u64,
    native_marker_paint_calls: u64,
    native_markers_painted: u64,
}

impl OpenGpuiEdgePathCache {
    pub(super) fn resolve_path(
        &mut self,
        key: OpenGpuiEdgePathCacheKey,
        vertex_budget: usize,
        build: impl FnOnce() -> Option<Path<Pixels>>,
    ) -> Option<Rc<Path<Pixels>>> {
        if let Some(cached) = self
            .paths
            .get(&key)
            .filter(|cached| cached.path.vertices.len() <= vertex_budget)
        {
            let path = Rc::clone(&cached.path);
            let previous_access = cached.access;
            self.hits = self.hits.saturating_add(1);
            self.touch(key, previous_access);
            return Some(path);
        }

        let path = Rc::new(build()?);
        self.misses = self.misses.saturating_add(1);
        if self.paths.contains_key(&key) {
            // Keep the higher-fidelity resident path. This bounded path serves only the current
            // plan so an earlier cache fill cannot consume the fair share of every later edge.
            return Some(path);
        }
        let footprint = OpenGpuiEdgeCacheFootprint::for_cached_path(&path);
        if self.budgets.path_entries == 0
            || !footprint.fits(
                self.budgets.path_vertices,
                self.budgets.path_estimated_bytes,
            )
        {
            self.path_budget_rejections = self.path_budget_rejections.saturating_add(1);
            return Some(path);
        }

        while self.paths.len() >= self.budgets.path_entries
            || !self.path_footprint.saturating_add(footprint).fits(
                self.budgets.path_vertices,
                self.budgets.path_estimated_bytes,
            )
        {
            if !self.evict_least_recently_used_path() {
                self.path_budget_rejections = self.path_budget_rejections.saturating_add(1);
                return Some(path);
            }
        }

        let access = self.reserve_access();
        self.least_recently_used.insert((access, key));
        self.paths.insert(
            key,
            OpenGpuiCachedEdgePath {
                path: Rc::clone(&path),
                access,
                footprint,
            },
        );
        self.path_footprint = self.path_footprint.saturating_add(footprint);
        Some(path)
    }

    fn evict_least_recently_used_path(&mut self) -> bool {
        while let Some((access, key)) = self.least_recently_used.pop_first() {
            let is_current = self
                .paths
                .get(&key)
                .is_some_and(|cached| cached.access == access);
            if !is_current {
                continue;
            }
            if let Some(cached) = self.paths.remove(&key) {
                self.path_footprint = self.path_footprint.saturating_sub(cached.footprint);
                self.path_budget_evictions = self.path_budget_evictions.saturating_add(1);
                return true;
            }
        }
        false
    }

    fn touch(&mut self, key: OpenGpuiEdgePathCacheKey, previous_access: u64) {
        self.least_recently_used.remove(&(previous_access, key));
        let access = self.reserve_access();
        if let Some(cached) = self.paths.get_mut(&key) {
            cached.access = access;
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
            if let Some(cached) = self.paths.get_mut(&key) {
                cached.access = access;
            }
            self.least_recently_used.insert((access, key));
        }
        self.next_access = self.paths.len() as u64;
    }

    #[cfg(test)]
    pub(super) fn resolve_paint_plan(
        &mut self,
        key: OpenGpuiEdgePaintPlanKey,
        build: impl FnOnce(&mut Self, &mut OpenGpuiEdgePaintPlanBuilder),
    ) -> OpenGpuiEdgePaintPlan {
        self.resolve_paint_plan_reserving(key, 0, build)
    }

    pub(super) fn resolve_paint_plan_reserving(
        &mut self,
        key: OpenGpuiEdgePaintPlanKey,
        reserved_items: usize,
        build: impl FnOnce(&mut Self, &mut OpenGpuiEdgePaintPlanBuilder),
    ) -> OpenGpuiEdgePaintPlan {
        debug_assert!(!key.segment.is_marker());
        self.purge_stale_plan_generations(key.generation);
        let slot = key.segment.slot();
        if let Some(cached) = self.paint_plans.get(&slot)
            && cached.key == key
        {
            self.paint_plan_reuses = self.paint_plan_reuses.saturating_add(1);
            return cached.plan.clone();
        }
        self.remove_paint_plan(slot);

        let mut builder = OpenGpuiEdgePaintPlanBuilder::new(self.plan_build_limit(reserved_items));
        build(self, &mut builder);
        let plan = builder.finish();
        self.record_paint_plan(
            plan.batches().len(),
            plan.batches().iter().map(|batch| batch.edge_count).sum(),
        );
        self.record_plan_build(&plan);
        if plan.is_complete() {
            self.retain_plan(false, slot, key, plan.clone());
        }
        plan
    }

    #[cfg(test)]
    pub(super) fn resolve_marker_plan(
        &mut self,
        key: OpenGpuiEdgePaintPlanKey,
        build: impl FnOnce(&mut OpenGpuiEdgePaintPlanBuilder),
    ) -> OpenGpuiEdgePaintPlan {
        self.resolve_marker_plan_reserving(key, 0, build)
    }

    pub(super) fn resolve_marker_plan_reserving(
        &mut self,
        key: OpenGpuiEdgePaintPlanKey,
        reserved_items: usize,
        build: impl FnOnce(&mut OpenGpuiEdgePaintPlanBuilder),
    ) -> OpenGpuiEdgePaintPlan {
        debug_assert!(key.segment.is_marker());
        self.purge_stale_plan_generations(key.generation);
        let slot = key.segment.slot();
        if let Some(cached) = self.marker_plans.get(&slot)
            && cached.key == key
        {
            self.marker_plan_reuses = self.marker_plan_reuses.saturating_add(1);
            return cached.plan.clone();
        }
        self.remove_marker_plan(slot);

        let mut builder = OpenGpuiEdgePaintPlanBuilder::new(self.plan_build_limit(reserved_items));
        build(&mut builder);
        let plan = builder.finish();
        self.marker_preparations = self.marker_preparations.saturating_add(1);
        self.marker_batches = self
            .marker_batches
            .saturating_add(plan.batches().len() as u64);
        self.record_plan_build(&plan);
        if plan.is_complete() {
            self.retain_plan(true, slot, key, plan.clone());
        }
        plan
    }

    fn plan_build_limit(&self, reserved_items: usize) -> OpenGpuiEdgeCacheFootprint {
        let entry_count = self
            .paint_plans
            .len()
            .saturating_add(self.marker_plans.len());
        if entry_count >= self.budgets.plan_entries {
            return OpenGpuiEdgeCacheFootprint::default();
        }
        let retained_limit = OpenGpuiEdgeCacheFootprint {
            vertices: self
                .budgets
                .plan_vertices
                .saturating_sub(self.plan_footprint.vertices),
            estimated_bytes: self
                .budgets
                .plan_estimated_bytes
                .saturating_sub(self.plan_footprint.estimated_bytes),
        };
        let retained_live = self
            .plan_footprint
            .saturating_mul(OPEN_GPUI_EDGE_PLAN_LIVE_PATH_COPY_FACTOR);
        let live_limit = OpenGpuiEdgeCacheFootprint {
            vertices: self
                .budgets
                .plan_live_vertices
                .saturating_sub(retained_live.vertices),
            estimated_bytes: self
                .budgets
                .plan_live_estimated_bytes
                .saturating_sub(retained_live.estimated_bytes),
        }
        .divided_by(OPEN_GPUI_EDGE_PLAN_LIVE_PATH_COPY_FACTOR);
        retained_limit
            .componentwise_min(live_limit)
            .saturating_sub(minimum_plan_reservation(reserved_items))
    }

    fn record_plan_build(&mut self, plan: &OpenGpuiEdgePaintPlan) {
        self.plan_build_peak_footprint = self
            .plan_build_peak_footprint
            .componentwise_max(plan.build_peak_footprint);
        let live_footprint = self
            .plan_footprint
            .saturating_add(plan.footprint())
            .saturating_mul(OPEN_GPUI_EDGE_PLAN_LIVE_PATH_COPY_FACTOR);
        self.plan_live_peak_footprint = self
            .plan_live_peak_footprint
            .componentwise_max(live_footprint);
        if !plan.is_complete() {
            self.plan_incomplete_builds = self.plan_incomplete_builds.saturating_add(1);
            self.plan_budget_rejections = self.plan_budget_rejections.saturating_add(1);
            self.plan_rejected_items = self
                .plan_rejected_items
                .saturating_add(plan.rejected_items() as u64);
        }
    }

    fn retain_plan(
        &mut self,
        marker: bool,
        slot: OpenGpuiEdgePaintPlanSlot,
        key: OpenGpuiEdgePaintPlanKey,
        plan: OpenGpuiEdgePaintPlan,
    ) {
        let footprint = plan.footprint();
        let entry_count = self
            .paint_plans
            .len()
            .saturating_add(self.marker_plans.len());
        let retained_candidate = self.plan_footprint.saturating_add(footprint);
        let live_candidate =
            retained_candidate.saturating_mul(OPEN_GPUI_EDGE_PLAN_LIVE_PATH_COPY_FACTOR);
        if entry_count >= self.budgets.plan_entries
            || !retained_candidate.fits(
                self.budgets.plan_vertices,
                self.budgets.plan_estimated_bytes,
            )
            || !live_candidate.fits(
                self.budgets.plan_live_vertices,
                self.budgets.plan_live_estimated_bytes,
            )
        {
            if plan.is_complete() {
                self.plan_budget_rejections = self.plan_budget_rejections.saturating_add(1);
            }
            return;
        }

        let cached = OpenGpuiCachedEdgePaintPlan {
            key,
            plan,
            footprint,
        };
        if marker {
            self.marker_plans.insert(slot, cached);
        } else {
            self.paint_plans.insert(slot, cached);
        }
        self.plan_footprint = retained_candidate;
    }

    fn purge_stale_plan_generations(&mut self, generation: OpenGpuiEdgePaintPlanGeneration) {
        let stale_paint = self
            .paint_plans
            .iter()
            .filter_map(|(slot, cached)| (cached.key.generation != generation).then_some(*slot))
            .collect::<Vec<_>>();
        for slot in stale_paint {
            self.remove_paint_plan(slot);
        }
        let stale_markers = self
            .marker_plans
            .iter()
            .filter_map(|(slot, cached)| (cached.key.generation != generation).then_some(*slot))
            .collect::<Vec<_>>();
        for slot in stale_markers {
            self.remove_marker_plan(slot);
        }
    }

    fn remove_paint_plan(&mut self, slot: OpenGpuiEdgePaintPlanSlot) {
        if let Some(cached) = self.paint_plans.remove(&slot) {
            self.plan_footprint = self.plan_footprint.saturating_sub(cached.footprint);
        }
    }

    fn remove_marker_plan(&mut self, slot: OpenGpuiEdgePaintPlanSlot) {
        if let Some(cached) = self.marker_plans.remove(&slot) {
            self.plan_footprint = self.plan_footprint.saturating_sub(cached.footprint);
        }
    }

    pub(super) fn clear(&mut self) {
        self.paths.clear();
        self.least_recently_used.clear();
        self.paint_plans.clear();
        self.marker_plans.clear();
        self.path_footprint = OpenGpuiEdgeCacheFootprint::default();
        self.plan_footprint = OpenGpuiEdgeCacheFootprint::default();
        self.next_access = 0;
    }

    pub(super) fn usage(&self) -> OpenGpuiEdgeCacheUsage {
        OpenGpuiEdgeCacheUsage {
            path_hits: self.hits,
            path_misses: self.misses,
            resident_paths: self.paths.len(),
            path_footprint: self.path_footprint,
            resident_paint_plans: self.paint_plans.len(),
            resident_marker_plans: self.marker_plans.len(),
            plan_generations: self
                .paint_plans
                .values()
                .map(|cached| cached.key.generation)
                .chain(
                    self.marker_plans
                        .values()
                        .map(|cached| cached.key.generation),
                )
                .collect::<BTreeSet<_>>()
                .len(),
            plan_footprint: self.plan_footprint,
            plan_live_footprint: self
                .plan_footprint
                .saturating_mul(OPEN_GPUI_EDGE_PLAN_LIVE_PATH_COPY_FACTOR),
            path_budget_evictions: self.path_budget_evictions,
            path_budget_rejections: self.path_budget_rejections,
            plan_budget_rejections: self.plan_budget_rejections,
            plan_incomplete_builds: self.plan_incomplete_builds,
            plan_rejected_items: self.plan_rejected_items,
            plan_build_peak_footprint: self.plan_build_peak_footprint,
            plan_live_peak_footprint: self.plan_live_peak_footprint,
        }
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
pub(super) struct OpenGpuiEdgeCacheUsage {
    pub(super) path_hits: u64,
    pub(super) path_misses: u64,
    pub(super) resident_paths: usize,
    pub(super) path_footprint: OpenGpuiEdgeCacheFootprint,
    pub(super) resident_paint_plans: usize,
    pub(super) resident_marker_plans: usize,
    pub(super) plan_generations: usize,
    pub(super) plan_footprint: OpenGpuiEdgeCacheFootprint,
    pub(super) plan_live_footprint: OpenGpuiEdgeCacheFootprint,
    pub(super) path_budget_evictions: u64,
    pub(super) path_budget_rejections: u64,
    pub(super) plan_budget_rejections: u64,
    pub(super) plan_incomplete_builds: u64,
    pub(super) plan_rejected_items: u64,
    pub(super) plan_build_peak_footprint: OpenGpuiEdgeCacheFootprint,
    pub(super) plan_live_peak_footprint: OpenGpuiEdgeCacheFootprint,
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct OpenGpuiEdgePaintUsage {
    pub(super) paint_preparations: u64,
    pub(super) paint_batches: u64,
    pub(super) painted_paths: u64,
    pub(super) paint_plan_reuses: u64,
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
    use lyon::tessellation::{LineCap, LineJoin};
    use open_gpui::{PathBuilder, PathStyle, point, px};

    use crate::chrome::OpenGpuiEdgeVisualFacts;
    use crate::theme::OpenGpuiGraphTheme;

    fn native_batch_key() -> OpenGpuiEdgePainterBatchKey {
        let style = OpenGpuiGraphTheme::default()
            .resolve_edge_style(OpenGpuiEdgeVisualFacts::default(), None);
        OpenGpuiEdgePainterBatchKey::native_fallback(style.batch_key())
    }

    fn representative_path(pattern: OpenGpuiEdgeStrokePattern) -> Path<Pixels> {
        let mut builder = match pattern {
            OpenGpuiEdgeStrokePattern::Solid => PathBuilder::stroke(px(2.0)),
            OpenGpuiEdgeStrokePattern::Dashed => {
                PathBuilder::stroke(px(2.0)).dash_array(&[px(6.0), px(4.0)])
            }
            OpenGpuiEdgeStrokePattern::Dotted => {
                PathBuilder::stroke(px(2.0)).dash_array(&[px(1.0), px(3.0)])
            }
        };
        if let PathStyle::Stroke(options) = &mut builder.style {
            options.start_cap = LineCap::Round;
            options.end_cap = LineCap::Round;
            options.line_join = LineJoin::Round;
        }
        builder.move_to(point(px(0.0), px(0.0)));
        builder.cubic_bezier_to(
            point(px(240.0), px(80.0)),
            point(px(96.0), px(0.0)),
            point(px(144.0), px(80.0)),
        );
        builder.build().expect("representative native edge path")
    }

    fn marker_path(circle: bool) -> Path<Pixels> {
        let points = if circle {
            (0..12)
                .map(|index| {
                    let angle = std::f32::consts::TAU * index as f32 / 12.0;
                    point(px(angle.cos() * 4.0), px(angle.sin() * 4.0))
                })
                .collect::<Vec<_>>()
        } else {
            vec![
                point(px(4.0), px(0.0)),
                point(px(-4.0), px(2.88)),
                point(px(-1.8), px(0.0)),
                point(px(-4.0), px(-2.88)),
            ]
        };
        let mut builder = PathBuilder::fill();
        builder.add_polygon(&points, true);
        builder.build().expect("representative native marker path")
    }

    fn batch(path: Path<Pixels>) -> OpenGpuiEdgePaintBatch {
        OpenGpuiEdgePaintBatch {
            key: native_batch_key(),
            path: Rc::new(path),
            edge_count: 1,
        }
    }

    fn plan_key(
        plan_id: u64,
        segment: OpenGpuiEdgePaintPlanSegment,
        pan: f32,
    ) -> OpenGpuiEdgePaintPlanKey {
        OpenGpuiEdgePaintPlanKey::new(plan_id, segment, pan, 0.0, 1.0, 0.0, 0.0)
    }

    fn path_key() -> OpenGpuiEdgePathCacheKey {
        OpenGpuiEdgePathCacheKey {
            edge: EdgeId::from_u128(1),
            geometry_revision: 1,
            pan_x_bits: 0.0_f32.to_bits(),
            pan_y_bits: 0.0_f32.to_bits(),
            zoom_bits: 1.0_f32.to_bits(),
            origin_x_bits: 0.0_f32.to_bits(),
            origin_y_bits: 0.0_f32.to_bits(),
            stroke: OpenGpuiEdgeStrokeTessellationKey::new(
                2.0,
                OpenGpuiEdgeStrokePattern::Solid,
                true,
                true,
            ),
        }
    }

    fn retained_homogeneous_paths(
        item: OpenGpuiEdgeCacheFootprint,
        insertions: usize,
        budgets: OpenGpuiEdgeCacheBudgets,
    ) -> OpenGpuiEdgeCacheFootprint {
        let retained = insertions
            .min(budgets.path_entries)
            .min(budgets.path_vertices / item.vertices.max(1))
            .min(budgets.path_estimated_bytes / item.estimated_bytes.max(1));
        item.saturating_mul(retained)
    }

    #[test]
    fn empty_cache_has_no_stale_query_capability() {
        let mut cache = OpenGpuiVisibleRouteCache::default();
        cache.clear();

        assert!(cache.bounds.is_none());
        assert!(cache.cursor.is_none());
        assert!(cache.exhausted_key.is_none());
        assert!(cache.complete.is_empty());
    }

    #[test]
    fn path_tessellation_key_separates_round_caps_and_joins() {
        let square = OpenGpuiEdgeStrokeTessellationKey::new(
            2.0,
            OpenGpuiEdgeStrokePattern::Solid,
            false,
            false,
        );
        let round_cap = OpenGpuiEdgeStrokeTessellationKey::new(
            2.0,
            OpenGpuiEdgeStrokePattern::Solid,
            true,
            false,
        );
        let round_join = OpenGpuiEdgeStrokeTessellationKey::new(
            2.0,
            OpenGpuiEdgeStrokePattern::Solid,
            false,
            true,
        );

        assert_ne!(square, round_cap);
        assert_ne!(square, round_join);
        assert_ne!(round_cap, round_join);
    }

    #[test]
    fn oversized_cached_path_cannot_consume_a_later_edges_fair_share() {
        let key = path_key();
        let resident = representative_path(OpenGpuiEdgeStrokePattern::Dotted);
        let resident_vertices = resident.vertices.len();
        let bounded = marker_path(false);
        let bounded_vertices = bounded.vertices.len();
        assert!(resident_vertices > bounded_vertices);

        let mut cache = OpenGpuiEdgePathCache::default();
        let first = cache
            .resolve_path(key, resident_vertices, || Some(resident))
            .expect("the detailed path is cached");
        let current = cache
            .resolve_path(key, bounded_vertices, || Some(bounded))
            .expect("the current plan receives a bounded temporary path");

        assert_eq!(current.vertices.len(), bounded_vertices);
        assert_eq!(cache.usage().resident_paths, 1);
        let reused = cache
            .resolve_path(key, resident_vertices, || {
                unreachable!("the higher-fidelity resident path remains cached")
            })
            .expect("the detailed path remains reusable");
        assert!(Rc::ptr_eq(&first, &reused));
        assert_eq!(cache.usage().path_hits, 1);
        assert_eq!(cache.usage().path_misses, 2);
    }

    #[test]
    fn paint_and_marker_plans_keep_only_one_current_generation_per_slot() {
        let mut cache = OpenGpuiEdgePathCache::default();
        let ordinary = plan_key(1, OpenGpuiEdgePaintPlanSegment::Ordinary, 0.0);
        let emphasized = plan_key(1, OpenGpuiEdgePaintPlanSegment::Emphasized, 0.0);
        let hover = plan_key(
            1,
            OpenGpuiEdgePaintPlanSegment::Hover(EdgeId::from_u128(1)),
            0.0,
        );
        let focus = plan_key(
            1,
            OpenGpuiEdgePaintPlanSegment::Focus(EdgeId::from_u128(1)),
            0.0,
        );
        let marker_ordinary = plan_key(1, OpenGpuiEdgePaintPlanSegment::MarkerOrdinary, 0.0);
        let marker_emphasized = plan_key(1, OpenGpuiEdgePaintPlanSegment::MarkerEmphasized, 0.0);
        let marker_hover = plan_key(
            1,
            OpenGpuiEdgePaintPlanSegment::MarkerHover(EdgeId::from_u128(1)),
            0.0,
        );

        let first = cache.resolve_paint_plan(ordinary, |_, builder| {
            assert!(builder.merge_owned(
                native_batch_key(),
                representative_path(OpenGpuiEdgeStrokePattern::Solid),
            ));
        });
        let reused = cache.resolve_paint_plan(ordinary, |_, _| unreachable!("plan must reuse"));
        assert!(Rc::ptr_eq(&first.batches, &reused.batches));
        cache.resolve_paint_plan(emphasized, |_, builder| {
            assert!(builder.merge_owned(
                native_batch_key(),
                representative_path(OpenGpuiEdgeStrokePattern::Solid),
            ));
        });
        cache.resolve_paint_plan(hover, |_, builder| {
            assert!(builder.merge_owned(
                native_batch_key(),
                representative_path(OpenGpuiEdgeStrokePattern::Solid),
            ));
        });
        cache.resolve_paint_plan(focus, |_, builder| {
            assert!(builder.merge_owned(
                native_batch_key(),
                representative_path(OpenGpuiEdgeStrokePattern::Solid),
            ));
        });
        cache.resolve_marker_plan(marker_ordinary, |builder| {
            assert!(builder.merge_owned(native_batch_key(), marker_path(false)));
        });
        cache.resolve_marker_plan(marker_emphasized, |builder| {
            assert!(builder.merge_owned(native_batch_key(), marker_path(false)));
        });
        cache.resolve_marker_plan(marker_hover, |builder| {
            assert!(builder.merge_owned(native_batch_key(), marker_path(false)));
        });

        let first_generation = cache.usage();
        assert_eq!(first_generation.resident_paint_plans, 4);
        assert_eq!(first_generation.resident_marker_plans, 3);
        assert_eq!(first_generation.plan_generations, 1);

        let next = plan_key(2, OpenGpuiEdgePaintPlanSegment::Ordinary, 20.0);
        cache.resolve_paint_plan(next, |_, builder| {
            assert!(builder.merge_owned(
                native_batch_key(),
                representative_path(OpenGpuiEdgeStrokePattern::Solid),
            ));
        });
        let next_generation = cache.usage();
        assert_eq!(next_generation.resident_paint_plans, 1);
        assert_eq!(next_generation.resident_marker_plans, 0);
        assert_eq!(next_generation.plan_generations, 1);
        assert!(
            cache
                .paint_plans
                .values()
                .all(|cached| cached.key.generation == next.generation)
        );
    }

    #[test]
    fn paint_and_marker_plans_share_one_hard_current_frame_budget() {
        let candidate = vec![batch(representative_path(OpenGpuiEdgeStrokePattern::Solid))];
        let footprint = OpenGpuiEdgeCacheFootprint::for_plan(&candidate);
        let mut cache = OpenGpuiEdgePathCache::default();
        cache.budgets.plan_vertices = footprint.vertices;
        cache.budgets.plan_estimated_bytes = footprint.estimated_bytes;
        cache.budgets.plan_live_vertices = footprint
            .vertices
            .saturating_mul(OPEN_GPUI_EDGE_PLAN_LIVE_PATH_COPY_FACTOR);
        cache.budgets.plan_live_estimated_bytes = footprint
            .estimated_bytes
            .saturating_mul(OPEN_GPUI_EDGE_PLAN_LIVE_PATH_COPY_FACTOR);
        assert_eq!(cache.plan_build_limit(0), footprint);

        let paint_key = plan_key(1, OpenGpuiEdgePaintPlanSegment::Ordinary, 0.0);
        let retained = cache.resolve_paint_plan(paint_key, |_, builder| {
            assert!(builder.merge_owned(
                native_batch_key(),
                representative_path(OpenGpuiEdgeStrokePattern::Solid),
            ));
        });
        assert_eq!(retained.batches().len(), 1);
        assert!(retained.is_complete());
        assert_eq!(cache.usage().resident_paint_plans, 1);

        let marker_key = plan_key(1, OpenGpuiEdgePaintPlanSegment::MarkerOrdinary, 0.0);
        let current = cache.resolve_marker_plan(marker_key, |builder| {
            assert!(!builder.merge_owned(native_batch_key(), marker_path(false)));
        });
        assert!(current.batches().is_empty());
        assert!(!current.is_complete());
        assert_eq!(current.rejected_items(), 1);
        let usage = cache.usage();
        assert_eq!(usage.resident_paint_plans, 1);
        assert_eq!(usage.resident_marker_plans, 0);
        assert_eq!(usage.plan_budget_rejections, 1);
        assert_eq!(usage.plan_incomplete_builds, 1);
        assert!(usage.plan_footprint.fits(
            cache.budgets.plan_vertices,
            cache.budgets.plan_estimated_bytes,
        ));
        assert!(usage.plan_live_footprint.fits(
            cache.budgets.plan_live_vertices,
            cache.budgets.plan_live_estimated_bytes,
        ));
    }

    #[test]
    fn deterministic_edge_cache_budget_matrix_plateaus_for_all_native_recipes() {
        let paths = [
            representative_path(OpenGpuiEdgeStrokePattern::Solid),
            representative_path(OpenGpuiEdgeStrokePattern::Dashed),
            representative_path(OpenGpuiEdgeStrokePattern::Dotted),
        ];
        assert_eq!(
            paths
                .iter()
                .map(|path| path.vertices.len())
                .collect::<Vec<_>>(),
            vec![174, 624, 1_560],
            "the pinned Open GPUI tessellation contract changed",
        );
        let arrow = marker_path(false);
        let circle = marker_path(true);
        assert_eq!(arrow.vertices.len(), 6);
        assert_eq!(circle.vertices.len(), 30);

        let budgets = OpenGpuiEdgeCacheBudgets::default();
        for edge_count in [100_usize, 4_096, 16_384] {
            for path in &paths {
                let item = OpenGpuiEdgeCacheFootprint::for_cached_path(path);
                let plateau =
                    retained_homogeneous_paths(item, edge_count.saturating_mul(200), budgets);
                let later =
                    retained_homogeneous_paths(item, edge_count.saturating_mul(400), budgets);
                assert_eq!(plateau, later, "continuous pan must reach a hard plateau");
                assert!(plateau.fits(budgets.path_vertices, budgets.path_estimated_bytes,));

                let shared_cold = item.saturating_mul(edge_count);
                let former_duplicate_cold = shared_cold.saturating_mul(2);
                assert!(shared_cold.estimated_bytes < former_duplicate_cold.estimated_bytes);

                let route_plan = OpenGpuiEdgeCacheFootprint::for_path_allocation(path)
                    .saturating_mul(edge_count);
                let marker_footprints = [
                    OpenGpuiEdgeCacheFootprint::default(),
                    OpenGpuiEdgeCacheFootprint::for_path_allocation(&arrow)
                        .saturating_mul(edge_count),
                    OpenGpuiEdgeCacheFootprint::for_path_allocation(&circle)
                        .saturating_mul(edge_count.saturating_mul(2)),
                ];
                assert!(marker_footprints.windows(2).all(|pair| {
                    route_plan.saturating_add(pair[0]).estimated_bytes
                        < route_plan.saturating_add(pair[1]).estimated_bytes
                }));
            }
        }
    }

    #[test]
    fn ordinary_native_plan_matrix_remains_complete() {
        const EDGE_COUNT: usize = 100;

        for pattern in [
            OpenGpuiEdgeStrokePattern::Solid,
            OpenGpuiEdgeStrokePattern::Dashed,
            OpenGpuiEdgeStrokePattern::Dotted,
        ] {
            for marker_count in [0_usize, 1, 2] {
                let path = Rc::new(representative_path(pattern));
                let marker = marker_path(marker_count == 2);
                let mut cache = OpenGpuiEdgePathCache::default();
                let paint = cache.resolve_paint_plan(
                    plan_key(1, OpenGpuiEdgePaintPlanSegment::Ordinary, 0.0),
                    |_, builder| {
                        for _ in 0..EDGE_COUNT {
                            assert!(builder.merge_shared(native_batch_key(), Rc::clone(&path)));
                        }
                    },
                );
                let markers = cache.resolve_marker_plan(
                    plan_key(1, OpenGpuiEdgePaintPlanSegment::MarkerOrdinary, 0.0),
                    |builder| {
                        for _ in 0..EDGE_COUNT * marker_count {
                            assert!(builder.merge_owned(native_batch_key(), marker.clone()));
                        }
                    },
                );

                assert!(paint.is_complete());
                assert!(markers.is_complete());
                assert_eq!(paint.rejected_items(), 0);
                assert_eq!(markers.rejected_items(), 0);
                let usage = cache.usage();
                assert_eq!(usage.plan_incomplete_builds, 0);
                assert_eq!(usage.plan_rejected_items, 0);
                assert!(usage.plan_build_peak_footprint.fits(
                    cache.budgets.plan_vertices,
                    cache.budgets.plan_estimated_bytes,
                ));
                assert!(usage.plan_footprint.fits(
                    cache.budgets.plan_vertices,
                    cache.budgets.plan_estimated_bytes,
                ));
            }
        }
    }

    #[test]
    fn single_edge_shares_cached_path_and_merge_clone_stays_inside_plan_budget() {
        let shared = Rc::new(representative_path(OpenGpuiEdgeStrokePattern::Solid));
        let original_vertices = shared.vertices.len();
        let mut cache = OpenGpuiEdgePathCache::default();

        let single = cache.resolve_paint_plan(
            plan_key(1, OpenGpuiEdgePaintPlanSegment::Ordinary, 0.0),
            |_, builder| {
                assert!(builder.merge_shared(native_batch_key(), Rc::clone(&shared)));
            },
        );
        assert!(Rc::ptr_eq(&single.batches()[0].path, &shared));

        let merged = cache.resolve_paint_plan(
            plan_key(2, OpenGpuiEdgePaintPlanSegment::Ordinary, 0.0),
            |_, builder| {
                assert!(builder.merge_shared(native_batch_key(), Rc::clone(&shared)));
                assert!(builder.merge_shared(native_batch_key(), Rc::clone(&shared)));
            },
        );
        assert!(!Rc::ptr_eq(&merged.batches()[0].path, &shared));
        assert_eq!(
            merged.batches()[0].path.vertices.len(),
            original_vertices * 2
        );
        assert_eq!(shared.vertices.len(), original_vertices);
        assert!(merged.footprint().fits(
            cache.budgets.plan_vertices,
            cache.budgets.plan_estimated_bytes,
        ));
    }

    #[test]
    fn incomplete_plans_are_not_cached_and_recover_on_the_next_complete_build() {
        const EDGE_COUNT: usize = 16_384;

        let dotted = Rc::new(representative_path(OpenGpuiEdgeStrokePattern::Dotted));
        let marker = marker_path(true);
        let mut cache = OpenGpuiEdgePathCache::default();

        let paint_key = plan_key(1, OpenGpuiEdgePaintPlanSegment::Ordinary, 0.0);
        let paint = cache.resolve_paint_plan(paint_key, |_, builder| {
            for index in 0..EDGE_COUNT {
                if !builder.merge_shared(native_batch_key(), Rc::clone(&dotted)) {
                    builder.reject_unbuilt_items(EDGE_COUNT - index - 1);
                    break;
                }
            }
        });
        assert!(matches!(
            paint.completeness(),
            OpenGpuiEdgePaintPlanCompleteness::Incomplete { .. }
        ));
        assert!(paint.rejected_items() > 0);
        assert_eq!(
            paint
                .batches()
                .iter()
                .map(|batch| batch.edge_count)
                .sum::<usize>()
                .saturating_add(paint.rejected_items()),
            EDGE_COUNT,
        );
        assert!(paint.footprint().fits(
            cache.budgets.plan_vertices,
            cache.budgets.plan_estimated_bytes,
        ));
        assert_eq!(cache.usage().resident_paint_plans, 0);

        let recovered = cache.resolve_paint_plan(paint_key, |_, builder| {
            assert!(builder.merge_owned(
                native_batch_key(),
                representative_path(OpenGpuiEdgeStrokePattern::Solid),
            ));
        });
        assert!(recovered.is_complete());
        assert_eq!(recovered.edge_count(), 1);
        let reused = cache.resolve_paint_plan(paint_key, |_, _| {
            unreachable!("the recovered complete plan must be reusable")
        });
        assert!(Rc::ptr_eq(&recovered.batches, &reused.batches));

        let markers = cache.resolve_marker_plan(
            plan_key(1, OpenGpuiEdgePaintPlanSegment::MarkerOrdinary, 0.0),
            |builder| {
                for index in 0..EDGE_COUNT * 2 {
                    if !builder.merge_owned(native_batch_key(), marker.clone()) {
                        builder.reject_unbuilt_items(EDGE_COUNT * 2 - index - 1);
                        break;
                    }
                }
            },
        );
        assert!(markers.is_complete());
        assert_eq!(markers.rejected_items(), 0);
        assert_eq!(cache.usage().resident_marker_plans, 1);
        assert_eq!(
            markers
                .batches()
                .iter()
                .map(|batch| batch.edge_count)
                .sum::<usize>()
                .saturating_add(markers.rejected_items()),
            EDGE_COUNT * 2,
        );

        let usage = cache.usage();
        assert_eq!(usage.plan_incomplete_builds, 1);
        assert_eq!(usage.plan_budget_rejections, 1);
        assert_eq!(usage.plan_rejected_items, paint.rejected_items() as u64);
        assert!(usage.plan_build_peak_footprint.fits(
            cache.budgets.plan_vertices,
            cache.budgets.plan_estimated_bytes,
        ));
        assert!(usage.plan_footprint.fits(
            cache.budgets.plan_vertices,
            cache.budgets.plan_estimated_bytes,
        ));
        assert!(usage.plan_live_footprint.fits(
            cache.budgets.plan_live_vertices,
            cache.budgets.plan_live_estimated_bytes,
        ));
        assert!(usage.plan_live_peak_footprint.fits(
            cache.budgets.plan_live_vertices,
            cache.budgets.plan_live_estimated_bytes,
        ));
    }

    #[test]
    fn one_rejected_edge_does_not_starve_later_edges_or_poison_the_slot() {
        let path = representative_path(OpenGpuiEdgeStrokePattern::Solid);
        let mut cache = OpenGpuiEdgePathCache::default();
        let key = plan_key(1, OpenGpuiEdgePaintPlanSegment::Ordinary, 0.0);

        let incomplete = cache.resolve_paint_plan(key, |_, builder| {
            assert!(builder.merge_owned(native_batch_key(), path.clone()));
            builder.reject_unbuilt_items(1);
            assert!(builder.merge_owned(native_batch_key(), path.clone()));
        });

        assert_eq!(incomplete.edge_count(), 2);
        assert_eq!(incomplete.rejected_items(), 1);
        assert_eq!(cache.usage().resident_paint_plans, 0);

        let recovered = cache.resolve_paint_plan(key, |_, builder| {
            assert!(builder.merge_owned(native_batch_key(), path));
        });
        assert!(recovered.is_complete());
        assert_eq!(recovered.edge_count(), 1);
        assert_eq!(cache.usage().resident_paint_plans, 1);
    }

    #[test]
    fn incomplete_ordinary_plan_releases_budget_for_emphasized_edges() {
        let path = representative_path(OpenGpuiEdgeStrokePattern::Solid);
        let footprint = OpenGpuiEdgeCacheFootprint::for_plan(&[batch(path.clone())]);
        let mut cache = OpenGpuiEdgePathCache::default();
        cache.budgets.plan_vertices = footprint.vertices;
        cache.budgets.plan_estimated_bytes = footprint.estimated_bytes;
        cache.budgets.plan_live_vertices = footprint
            .vertices
            .saturating_mul(OPEN_GPUI_EDGE_PLAN_LIVE_PATH_COPY_FACTOR);
        cache.budgets.plan_live_estimated_bytes = footprint
            .estimated_bytes
            .saturating_mul(OPEN_GPUI_EDGE_PLAN_LIVE_PATH_COPY_FACTOR);

        let ordinary = cache.resolve_paint_plan(
            plan_key(1, OpenGpuiEdgePaintPlanSegment::Ordinary, 0.0),
            |_, builder| {
                assert!(builder.merge_owned(native_batch_key(), path.clone()));
                builder.reject_unbuilt_items(1);
            },
        );
        assert!(!ordinary.is_complete());
        assert_eq!(cache.usage().resident_paint_plans, 0);

        let emphasized = cache.resolve_paint_plan(
            plan_key(1, OpenGpuiEdgePaintPlanSegment::Emphasized, 0.0),
            |_, builder| assert!(builder.merge_owned(native_batch_key(), path.clone())),
        );
        assert!(emphasized.is_complete());
        assert_eq!(emphasized.edge_count(), 1);
        assert_eq!(cache.usage().resident_paint_plans, 1);
    }

    #[test]
    fn ordinary_plan_reservation_keeps_minimum_emphasized_geometry_admissible() {
        let ordinary_path = representative_path(OpenGpuiEdgeStrokePattern::Solid);
        let ordinary_footprint =
            OpenGpuiEdgeCacheFootprint::for_plan(&[batch(ordinary_path.clone())]);
        let reserved = minimum_plan_reservation(1);
        let total = ordinary_footprint.saturating_add(reserved);
        let mut cache = OpenGpuiEdgePathCache::default();
        cache.budgets.plan_vertices = total.vertices;
        cache.budgets.plan_estimated_bytes = total.estimated_bytes;
        cache.budgets.plan_live_vertices = total
            .vertices
            .saturating_mul(OPEN_GPUI_EDGE_PLAN_LIVE_PATH_COPY_FACTOR);
        cache.budgets.plan_live_estimated_bytes = total
            .estimated_bytes
            .saturating_mul(OPEN_GPUI_EDGE_PLAN_LIVE_PATH_COPY_FACTOR);
        assert_eq!(cache.plan_build_limit(1), ordinary_footprint);

        let ordinary = cache.resolve_paint_plan_reserving(
            plan_key(1, OpenGpuiEdgePaintPlanSegment::Ordinary, 0.0),
            1,
            |_, builder| {
                assert!(builder.merge_owned(native_batch_key(), ordinary_path.clone()));
            },
        );
        assert!(ordinary.is_complete());

        let emphasized = cache.resolve_paint_plan(
            plan_key(1, OpenGpuiEdgePaintPlanSegment::Emphasized, 0.0),
            |_, builder| assert!(builder.merge_owned(native_batch_key(), marker_path(false))),
        );
        assert!(emphasized.is_complete());
        assert_eq!(emphasized.edge_count(), 1);
        assert_eq!(cache.usage().resident_paint_plans, 2);
    }

    #[test]
    fn live_paint_peak_accounts_for_every_simultaneously_owned_open_gpui_path_copy() {
        let mut cache = OpenGpuiEdgePathCache::default();
        let plan = cache.resolve_paint_plan(
            plan_key(1, OpenGpuiEdgePaintPlanSegment::Ordinary, 0.0),
            |_, builder| {
                assert!(builder.merge_owned(
                    native_batch_key(),
                    representative_path(OpenGpuiEdgeStrokePattern::Dotted),
                ));
            },
        );
        let usage = cache.usage();
        let expected_live = plan
            .footprint()
            .saturating_mul(OPEN_GPUI_EDGE_PLAN_LIVE_PATH_COPY_FACTOR);

        assert_eq!(usage.plan_live_footprint, expected_live);
        assert_eq!(usage.plan_live_peak_footprint, expected_live);
        assert!(usage.plan_live_peak_footprint.fits(
            cache.budgets.plan_live_vertices,
            cache.budgets.plan_live_estimated_bytes,
        ));
    }
}
