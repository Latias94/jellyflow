use std::collections::{BTreeMap, BTreeSet};
use std::mem::{size_of, size_of_val};
use std::rc::Rc;

use jellyflow_core::EdgeId;
use jellyflow_runtime::{
    geometry::ViewRect, view::RouteGeometryRevision, view::ViewSession, view::ViewSessionError,
    view::VisibleRouteQueryRevision, view::VisibleRouteQueryWork, view::VisibleRouteWorkError,
    view::VisibleRouteWorkRequest, view::VisibleRouteWorkSession, view::VisibleRouteWorkStatus,
};
use open_gpui::{Path, PathVertex, Pixels};

use crate::chrome::{OpenGpuiEdgePainterBatchKey, OpenGpuiEdgeStrokePattern};

const OPEN_GPUI_EDGE_PATH_CACHE_CAPACITY: usize = super::OPEN_GPUI_MAX_EDGE_LAYER_BUDGET;
const OPEN_GPUI_EDGE_PATH_CACHE_VERTEX_BUDGET: usize = 1_048_576;
const OPEN_GPUI_EDGE_PATH_CACHE_ESTIMATED_BYTE_BUDGET: usize = 64 * 1_024 * 1_024;
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
}

impl OpenGpuiEdgeStrokeTessellationKey {
    fn new(stroke_width: f32, stroke_pattern: OpenGpuiEdgeStrokePattern) -> Self {
        Self {
            stroke_width_bits: stroke_width.to_bits(),
            stroke_pattern,
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
    ) -> Self {
        Self {
            edge,
            geometry_revision: geometry_revision.get(),
            pan_x_bits: pan_x.to_bits(),
            pan_y_bits: pan_y.to_bits(),
            zoom_bits: zoom.to_bits(),
            origin_x_bits: origin_x.to_bits(),
            origin_y_bits: origin_y.to_bits(),
            stroke: OpenGpuiEdgeStrokeTessellationKey::new(stroke_width, stroke_pattern),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OpenGpuiEdgePaintPlanSegment {
    Ordinary,
    MarkerOrdinary,
}

impl OpenGpuiEdgePaintPlanSegment {
    fn is_marker(self) -> bool {
        matches!(self, Self::MarkerOrdinary)
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

    #[cfg(test)]
    pub(super) fn rejected_items(&self) -> usize {
        match self.completeness {
            OpenGpuiEdgePaintPlanCompleteness::Complete => 0,
            OpenGpuiEdgePaintPlanCompleteness::Incomplete { rejected_items } => rejected_items,
        }
    }

    #[cfg(test)]
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
    let path = OpenGpuiEdgeCacheFootprint::for_path_storage(
        OPEN_GPUI_EDGE_PLAN_RESERVED_VERTICES_PER_ITEM,
        OPEN_GPUI_EDGE_PLAN_RESERVED_VERTICES_PER_ITEM,
    );
    // A future item may start a new style run in the subsequent marker plan.
    let item = OpenGpuiEdgeCacheFootprint {
        vertices: path.vertices,
        estimated_bytes: path
            .estimated_bytes
            .saturating_add(size_of::<OpenGpuiEdgePaintBatch>())
            .saturating_add(2 * size_of::<usize>()),
    };
    item.saturating_mul(items)
}

pub(super) struct OpenGpuiEdgePaintPlanBuilder {
    batches: Vec<OpenGpuiEdgePaintBatch>,
    footprint: OpenGpuiEdgeCacheFootprint,
    limit: OpenGpuiEdgeCacheFootprint,
    rejected_items: usize,
}

impl OpenGpuiEdgePaintPlanBuilder {
    fn new(limit: OpenGpuiEdgeCacheFootprint) -> Self {
        Self {
            batches: Vec::new(),
            footprint: OpenGpuiEdgeCacheFootprint::default(),
            limit,
            rejected_items: 0,
        }
    }

    pub(super) fn merge_shared(
        &mut self,
        key: OpenGpuiEdgePainterBatchKey,
        path: Rc<Path<Pixels>>,
    ) -> bool {
        let Some(batch) = self.batches.last().filter(|batch| batch.key == key) else {
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
            self.batches.push(OpenGpuiEdgePaintBatch {
                key,
                path,
                edge_count: 1,
            });
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
                .last_mut()
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
            let removed = self
                .batches
                .pop()
                .expect("the over-allocated native edge batch remains present");
            let plan_overhead =
                size_of::<OpenGpuiEdgePaintBatch>().saturating_add(if self.batches.is_empty() {
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
            .last_mut()
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
    }

    fn finish(self) -> OpenGpuiEdgePaintPlan {
        let batches = Rc::<[OpenGpuiEdgePaintBatch]>::from(self.batches);
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
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct OpenGpuiEdgeCacheBudgets {
    path_entries: usize,
    path_vertices: usize,
    path_estimated_bytes: usize,
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
    paint_plan: Option<OpenGpuiCachedEdgePaintPlan>,
    marker_plan: Option<OpenGpuiCachedEdgePaintPlan>,
    path_footprint: OpenGpuiEdgeCacheFootprint,
    plan_footprint: OpenGpuiEdgeCacheFootprint,
    next_access: u64,
    budgets: OpenGpuiEdgeCacheBudgets,
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
            self.touch(key, previous_access);
            return Some(path);
        }

        let path = Rc::new(build()?);
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
            return Some(path);
        }

        while self.paths.len() >= self.budgets.path_entries
            || !self.path_footprint.saturating_add(footprint).fits(
                self.budgets.path_vertices,
                self.budgets.path_estimated_bytes,
            )
        {
            if !self.evict_least_recently_used_path() {
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
        if let Some(cached) = self.paint_plan.as_ref()
            && cached.key == key
        {
            return cached.plan.clone();
        }
        self.remove_paint_plan();

        let mut builder = OpenGpuiEdgePaintPlanBuilder::new(self.plan_build_limit(reserved_items));
        build(self, &mut builder);
        let plan = builder.finish();
        if plan.is_complete() {
            self.retain_plan(false, key, plan.clone());
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
        if let Some(cached) = self.marker_plan.as_ref()
            && cached.key == key
        {
            return cached.plan.clone();
        }
        self.remove_marker_plan();

        let mut builder = OpenGpuiEdgePaintPlanBuilder::new(self.plan_build_limit(reserved_items));
        build(&mut builder);
        let plan = builder.finish();
        if plan.is_complete() {
            self.retain_plan(true, key, plan.clone());
        }
        plan
    }

    fn plan_build_limit(&self, reserved_items: usize) -> OpenGpuiEdgeCacheFootprint {
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

    fn retain_plan(
        &mut self,
        marker: bool,
        key: OpenGpuiEdgePaintPlanKey,
        plan: OpenGpuiEdgePaintPlan,
    ) {
        let footprint = plan.footprint();
        let retained_candidate = self.plan_footprint.saturating_add(footprint);
        let live_candidate =
            retained_candidate.saturating_mul(OPEN_GPUI_EDGE_PLAN_LIVE_PATH_COPY_FACTOR);
        if !retained_candidate.fits(
            self.budgets.plan_vertices,
            self.budgets.plan_estimated_bytes,
        ) || !live_candidate.fits(
            self.budgets.plan_live_vertices,
            self.budgets.plan_live_estimated_bytes,
        ) {
            return;
        }

        let cached = OpenGpuiCachedEdgePaintPlan {
            key,
            plan,
            footprint,
        };
        if marker {
            debug_assert!(self.marker_plan.is_none());
            self.marker_plan = Some(cached);
        } else {
            debug_assert!(self.paint_plan.is_none());
            self.paint_plan = Some(cached);
        }
        self.plan_footprint = retained_candidate;
    }

    fn purge_stale_plan_generations(&mut self, generation: OpenGpuiEdgePaintPlanGeneration) {
        if self
            .paint_plan
            .as_ref()
            .is_some_and(|cached| cached.key.generation != generation)
        {
            self.remove_paint_plan();
        }
        if self
            .marker_plan
            .as_ref()
            .is_some_and(|cached| cached.key.generation != generation)
        {
            self.remove_marker_plan();
        }
    }

    fn remove_paint_plan(&mut self) {
        if let Some(cached) = self.paint_plan.take() {
            self.plan_footprint = self.plan_footprint.saturating_sub(cached.footprint);
        }
    }

    fn remove_marker_plan(&mut self) {
        if let Some(cached) = self.marker_plan.take() {
            self.plan_footprint = self.plan_footprint.saturating_sub(cached.footprint);
        }
    }

    pub(super) fn clear(&mut self) {
        self.paths.clear();
        self.least_recently_used.clear();
        self.paint_plan = None;
        self.marker_plan = None;
        self.path_footprint = OpenGpuiEdgeCacheFootprint::default();
        self.plan_footprint = OpenGpuiEdgeCacheFootprint::default();
        self.next_access = 0;
    }

    pub(super) fn usage(&self) -> OpenGpuiEdgeCacheUsage {
        OpenGpuiEdgeCacheUsage {
            resident_paths: self.paths.len(),
            path_footprint: self.path_footprint,
            resident_paint_plans: self.paint_plan.iter().count(),
            resident_marker_plans: self.marker_plan.iter().count(),
            plan_generations: self
                .paint_plan
                .iter()
                .map(|cached| cached.key.generation)
                .chain(self.marker_plan.iter().map(|cached| cached.key.generation))
                .collect::<BTreeSet<_>>()
                .len(),
            plan_footprint: self.plan_footprint,
            plan_live_footprint: self
                .plan_footprint
                .saturating_mul(OPEN_GPUI_EDGE_PLAN_LIVE_PATH_COPY_FACTOR),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct OpenGpuiEdgeCacheUsage {
    pub(super) resident_paths: usize,
    pub(super) path_footprint: OpenGpuiEdgeCacheFootprint,
    pub(super) resident_paint_plans: usize,
    pub(super) resident_marker_plans: usize,
    pub(super) plan_generations: usize,
    pub(super) plan_footprint: OpenGpuiEdgeCacheFootprint,
    pub(super) plan_live_footprint: OpenGpuiEdgeCacheFootprint,
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
    pub(super) routes: Rc<[EdgeId]>,
    /// Current query facts invalidate prepared paint even while membership is still pending.
    pub(super) query_revision: VisibleRouteQueryRevision,
    pub(super) generation: u64,
    pub(super) report: OpenGpuiVisibleRouteQueryReport,
}

#[derive(Default)]
pub(super) struct OpenGpuiVisibleRouteCache {
    work: Option<VisibleRouteWorkSession>,
    complete: Rc<[EdgeId]>,
    mapped_generation: Option<u64>,
    complete_geometry_overflows: usize,
}

impl OpenGpuiVisibleRouteCache {
    pub(super) fn clear(&mut self) {
        if let Some(work) = &mut self.work {
            work.clear();
        }
        self.complete = Rc::default();
        self.mapped_generation = None;
        self.complete_geometry_overflows = 0;
    }

    pub(super) fn query(
        &mut self,
        view: &ViewSession,
        bounds: ViewRect,
        entry_budget: usize,
    ) -> Result<OpenGpuiVisibleRoutePlan, ViewSessionError> {
        let work = match &mut self.work {
            Some(work) => work,
            slot @ None => slot.insert(view.visible_route_work_session()?),
        };
        let update = work
            .advance(
                view,
                VisibleRouteWorkRequest::new(bounds).with_margin(4.0),
                entry_budget,
            )
            .map_err(map_query_error)?;
        let generation = update.generation();
        if self.mapped_generation != Some(generation) {
            self.complete = update
                .routes()
                .map(|routes| {
                    routes
                        .iter()
                        .map(|route| route.edge())
                        .collect::<Vec<_>>()
                        .into()
                })
                .unwrap_or_default();
            self.complete_geometry_overflows = update
                .routes()
                .map_or(0, |routes| routes.geometry_overflows().len());
            self.mapped_generation = Some(generation);
        }
        let status = update.status();
        Ok(OpenGpuiVisibleRoutePlan {
            routes: self.complete.clone(),
            query_revision: update.query_revision(),
            generation,
            report: OpenGpuiVisibleRouteQueryReport {
                work: update.work(),
                pending: status == VisibleRouteWorkStatus::Pending,
                budget_exhausted: status == VisibleRouteWorkStatus::Truncated,
                geometry_overflows: self.complete_geometry_overflows,
                reused_previous_plan: update.reused_previous_publication(),
            },
        })
    }
}

fn map_query_error(error: VisibleRouteWorkError) -> ViewSessionError {
    match error {
        VisibleRouteWorkError::Session(error) => error,
        VisibleRouteWorkError::ForeignView { .. } => ViewSessionError::ForeignInteractionFacts,
        VisibleRouteWorkError::InvalidRequest => ViewSessionError::InvalidViewport,
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
        native_batch_key_for(None)
    }

    fn native_batch_key_for(style_slot: Option<&str>) -> OpenGpuiEdgePainterBatchKey {
        let style = OpenGpuiGraphTheme::default()
            .resolve_edge_style(OpenGpuiEdgeVisualFacts::default(), style_slot);
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
            stroke: OpenGpuiEdgeStrokeTessellationKey::new(2.0, OpenGpuiEdgeStrokePattern::Solid),
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
    fn cleared_cache_has_no_mapped_route_publication() {
        let mut cache = OpenGpuiVisibleRouteCache::default();
        cache.clear();

        assert!(cache.complete.is_empty());
        assert!(cache.mapped_generation.is_none());
        assert_eq!(cache.complete_geometry_overflows, 0);
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
    }

    #[test]
    fn paint_and_marker_plans_keep_one_current_generation_each() {
        let mut cache = OpenGpuiEdgePathCache::default();
        let ordinary = plan_key(1, OpenGpuiEdgePaintPlanSegment::Ordinary, 0.0);
        let marker_ordinary = plan_key(1, OpenGpuiEdgePaintPlanSegment::MarkerOrdinary, 0.0);

        let first = cache.resolve_paint_plan(ordinary, |_, builder| {
            assert!(builder.merge_owned(
                native_batch_key(),
                representative_path(OpenGpuiEdgeStrokePattern::Solid),
            ));
        });
        let reused = cache.resolve_paint_plan(ordinary, |_, _| unreachable!("plan must reuse"));
        assert!(Rc::ptr_eq(&first.batches, &reused.batches));
        cache.resolve_marker_plan(marker_ordinary, |builder| {
            assert!(builder.merge_owned(native_batch_key(), marker_path(false)));
        });

        let first_generation = cache.usage();
        assert_eq!(first_generation.resident_paint_plans, 1);
        assert_eq!(first_generation.resident_marker_plans, 1);
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
        assert_eq!(
            cache
                .paint_plan
                .as_ref()
                .map(|cached| cached.key.generation),
            Some(next.generation)
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
        let path_vertices = paths
            .iter()
            .map(|path| path.vertices.len())
            .collect::<Vec<_>>();
        assert!(path_vertices.iter().all(|vertices| *vertices > 0));
        assert!(path_vertices.windows(2).all(|pair| pair[0] < pair[1]));
        let arrow = marker_path(false);
        let circle = marker_path(true);
        assert!(!arrow.vertices.is_empty());
        assert!(circle.vertices.len() > arrow.vertices.len());

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
        const EDGE_COUNT: usize = 2;

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
                assert!(paint.footprint().fits(
                    cache.budgets.plan_vertices,
                    cache.budgets.plan_estimated_bytes,
                ));
                assert!(markers.footprint().fits(
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
    fn paint_batches_merge_only_adjacent_styles_and_preserve_runtime_order() {
        let first = native_batch_key_for(Some("data"));
        let second = native_batch_key_for(Some("control"));
        assert_ne!(first, second);
        let path = Rc::new(representative_path(OpenGpuiEdgeStrokePattern::Solid));
        let mut cache = OpenGpuiEdgePathCache::default();

        let plan = cache.resolve_paint_plan(
            plan_key(1, OpenGpuiEdgePaintPlanSegment::Ordinary, 0.0),
            |_, builder| {
                assert!(builder.merge_shared(first, Rc::clone(&path)));
                assert!(builder.merge_shared(first, Rc::clone(&path)));
                assert!(builder.merge_shared(second, Rc::clone(&path)));
                assert!(builder.merge_shared(first, Rc::clone(&path)));
            },
        );

        assert_eq!(
            plan.batches()
                .iter()
                .map(|batch| (batch.key, batch.edge_count))
                .collect::<Vec<_>>(),
            [(first, 2), (second, 1), (first, 1)],
        );
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
    fn committed_plan_reservation_keeps_minimum_marker_geometry_admissible() {
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

        let marker = cache.resolve_marker_plan(
            plan_key(1, OpenGpuiEdgePaintPlanSegment::MarkerOrdinary, 0.0),
            |builder| assert!(builder.merge_owned(native_batch_key(), marker_path(false))),
        );
        assert!(marker.is_complete());
        assert_eq!(marker.edge_count(), 1);
        assert_eq!(cache.usage().resident_paint_plans, 1);
        assert_eq!(cache.usage().resident_marker_plans, 1);
    }

    #[test]
    fn alternating_style_reservation_preserves_markers_under_tight_budget() {
        let first = native_batch_key_for(Some("data"));
        let second = native_batch_key_for(Some("control"));
        assert_ne!(first, second);

        let ordinary_paths = [Rc::new(marker_path(false)), Rc::new(marker_path(false))];
        let ordinary_batches = [
            OpenGpuiEdgePaintBatch {
                key: first,
                path: Rc::clone(&ordinary_paths[0]),
                edge_count: 1,
            },
            OpenGpuiEdgePaintBatch {
                key: second,
                path: Rc::clone(&ordinary_paths[1]),
                edge_count: 1,
            },
        ];
        let ordinary_footprint = OpenGpuiEdgeCacheFootprint::for_plan(&ordinary_batches);
        let future_items = 1;
        let future_reservation = minimum_plan_reservation(1).saturating_mul(future_items);
        assert_eq!(minimum_plan_reservation(future_items), future_reservation);
        let total = ordinary_footprint.saturating_add(future_reservation);
        let mut cache = OpenGpuiEdgePathCache::default();
        cache.budgets.plan_vertices = total.vertices;
        cache.budgets.plan_estimated_bytes = total.estimated_bytes;
        cache.budgets.plan_live_vertices = total
            .vertices
            .saturating_mul(OPEN_GPUI_EDGE_PLAN_LIVE_PATH_COPY_FACTOR);
        cache.budgets.plan_live_estimated_bytes = total
            .estimated_bytes
            .saturating_mul(OPEN_GPUI_EDGE_PLAN_LIVE_PATH_COPY_FACTOR);
        assert_eq!(cache.plan_build_limit(future_items), ordinary_footprint);

        let ordinary = cache.resolve_paint_plan_reserving(
            plan_key(1, OpenGpuiEdgePaintPlanSegment::Ordinary, 0.0),
            future_items,
            |_, builder| {
                assert!(builder.merge_shared(first, Rc::clone(&ordinary_paths[0])));
                assert!(builder.merge_shared(second, Rc::clone(&ordinary_paths[1])));
            },
        );
        assert!(ordinary.is_complete());
        assert_eq!(ordinary.batches().len(), 2);

        let markers = cache.resolve_marker_plan(
            plan_key(1, OpenGpuiEdgePaintPlanSegment::MarkerOrdinary, 0.0),
            |builder| {
                assert!(builder.merge_owned(first, marker_path(false)));
            },
        );
        assert!(markers.is_complete());
        assert_eq!(markers.edge_count(), 1);

        let usage = cache.usage();
        assert_eq!(usage.resident_paint_plans, 1);
        assert_eq!(usage.resident_marker_plans, 1);
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
    fn retained_plan_footprint_accounts_for_open_gpui_path_copies() {
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
        assert!(usage.plan_live_footprint.fits(
            cache.budgets.plan_live_vertices,
            cache.budgets.plan_live_estimated_bytes,
        ));
    }
}
