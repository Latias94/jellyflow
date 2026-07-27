use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::sync::Arc;

use jellyflow_core::core::{CanvasPoint, CanvasRect, NodeId};

use crate::runtime::adapter_protocol::{GeometrySnapshot, ViewSize};
use crate::runtime::view_session::{
    AlignmentSnapQueryReceipt, InteractionQueryBinding, ViewGeneration, ViewId,
};
use crate::runtime::viewport::ViewportTransform;

use super::{AlignmentGuide, AlignmentGuideAxis, InteractionGeneration};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct AlignmentSnapLocks {
    x: Option<AxisSnapLock>,
    y: Option<AxisSnapLock>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AxisSnapLock {
    moving: NodeId,
    candidate: NodeId,
    moving_feature: usize,
    candidate_feature: usize,
}

/// One bounded, renderer-neutral node-alignment query.
#[derive(Debug, Clone)]
pub struct AlignmentSnapQuery {
    view: ViewId,
    view_generation: ViewGeneration,
    generation: InteractionGeneration,
    moving_nodes: Arc<[NodeId]>,
    delta: CanvasPoint,
    acquire_tolerance_in_view: f32,
    release_tolerance_in_view: f32,
    entry_budget: usize,
    locks: AlignmentSnapLocks,
    interaction_binding: InteractionQueryBinding,
}

impl AlignmentSnapQuery {
    pub(super) fn new(
        view: ViewId,
        view_generation: ViewGeneration,
        generation: InteractionGeneration,
        moving_nodes: Arc<[NodeId]>,
        delta: CanvasPoint,
        acquire_tolerance_in_view: f32,
        release_tolerance_in_view: f32,
        entry_budget: usize,
        locks: AlignmentSnapLocks,
        interaction_binding: InteractionQueryBinding,
    ) -> Self {
        Self {
            view,
            view_generation,
            generation,
            moving_nodes,
            delta,
            acquire_tolerance_in_view,
            release_tolerance_in_view,
            entry_budget,
            locks,
            interaction_binding,
        }
    }

    pub fn view(&self) -> ViewId {
        self.view
    }

    pub fn view_generation(&self) -> ViewGeneration {
        self.view_generation
    }

    pub fn generation(&self) -> InteractionGeneration {
        self.generation
    }

    pub fn moving_nodes(&self) -> &[NodeId] {
        &self.moving_nodes
    }

    pub fn delta(&self) -> CanvasPoint {
        self.delta
    }

    pub fn acquire_tolerance_in_view(&self) -> f32 {
        self.acquire_tolerance_in_view
    }

    pub fn release_tolerance_in_view(&self) -> f32 {
        self.release_tolerance_in_view
    }

    pub fn entry_budget(&self) -> usize {
        self.entry_budget
    }

    pub(crate) fn interaction_binding(&self) -> InteractionQueryBinding {
        self.interaction_binding
    }
}

/// Bounded work evidence for a node-alignment query that could not produce a snap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlignmentSnapWork {
    generation: InteractionGeneration,
    entries_examined: u64,
    receipt: Option<AlignmentSnapQueryReceipt>,
}

impl AlignmentSnapWork {
    pub(crate) fn new(generation: InteractionGeneration, entries_examined: u64) -> Self {
        Self {
            generation,
            entries_examined,
            receipt: None,
        }
    }

    pub fn generation(self) -> InteractionGeneration {
        self.generation
    }

    pub fn entries_examined(self) -> u64 {
        self.entries_examined
    }

    pub fn receipt(self) -> Option<AlignmentSnapQueryReceipt> {
        self.receipt
    }

    pub(crate) fn with_receipt(mut self, receipt: Option<AlignmentSnapQueryReceipt>) -> Self {
        self.receipt = receipt;
        self
    }
}

/// Terminal result of one bounded node-alignment query.
#[derive(Debug, Clone, PartialEq)]
pub enum AlignmentSnapQueryOutcome {
    Complete(AlignmentSnapResult),
    BudgetExhausted(AlignmentSnapWork),
    GeometryIndeterminate(AlignmentSnapWork),
}

impl AlignmentSnapQueryOutcome {
    pub fn generation(&self) -> InteractionGeneration {
        match self {
            Self::Complete(result) => result.generation(),
            Self::BudgetExhausted(work) | Self::GeometryIndeterminate(work) => work.generation(),
        }
    }

    pub fn receipt(&self) -> Option<AlignmentSnapQueryReceipt> {
        match self {
            Self::Complete(result) => result.receipt(),
            Self::BudgetExhausted(work) | Self::GeometryIndeterminate(work) => work.receipt(),
        }
    }
}

/// Bounded alignment result for one node-drag presentation update.
#[derive(Debug, Clone, PartialEq)]
pub struct AlignmentSnapResult {
    generation: InteractionGeneration,
    delta: CanvasPoint,
    guides: Box<[AlignmentGuide]>,
    candidates_examined: usize,
    locks: AlignmentSnapLocks,
    receipt: Option<AlignmentSnapQueryReceipt>,
}

impl AlignmentSnapResult {
    pub fn generation(&self) -> InteractionGeneration {
        self.generation
    }

    /// Returns the canvas-space delta after alignment snapping.
    pub fn delta(&self) -> CanvasPoint {
        self.delta
    }

    /// Returns the deterministic alignment guides associated with the snapped axes.
    pub fn guides(&self) -> &[AlignmentGuide] {
        &self.guides
    }

    /// Returns the number of spatially admitted, non-moving nodes inspected.
    pub fn candidates_examined(&self) -> usize {
        self.candidates_examined
    }

    pub fn receipt(&self) -> Option<AlignmentSnapQueryReceipt> {
        self.receipt
    }

    pub(crate) fn with_receipt(mut self, receipt: Option<AlignmentSnapQueryReceipt>) -> Self {
        self.receipt = receipt;
        self
    }

    pub(crate) fn into_parts(
        self,
    ) -> (
        InteractionGeneration,
        CanvasPoint,
        Box<[AlignmentGuide]>,
        AlignmentSnapLocks,
    ) {
        (self.generation, self.delta, self.guides, self.locks)
    }
}

#[derive(Debug, Clone, Copy)]
struct AxisSnap {
    distance: f32,
    adjustment: f32,
    coordinate: f32,
    moving: NodeId,
    candidate: NodeId,
    moving_feature: usize,
    candidate_feature: usize,
}

impl AxisSnap {
    fn lock(self) -> AxisSnapLock {
        AxisSnapLock {
            moving: self.moving,
            candidate: self.candidate,
            moving_feature: self.moving_feature,
            candidate_feature: self.candidate_feature,
        }
    }
}

impl AxisSnap {
    fn is_better_than(self, current: Self) -> bool {
        self.distance
            .total_cmp(&current.distance)
            .then_with(|| self.candidate.cmp(&current.candidate))
            .then_with(|| self.moving.cmp(&current.moving))
            .then_with(|| self.candidate_feature.cmp(&current.candidate_feature))
            .then_with(|| self.moving_feature.cmp(&current.moving_feature))
            == Ordering::Less
    }
}

pub(crate) fn resolve_alignment_snap(
    geometry: &GeometrySnapshot,
    query: &AlignmentSnapQuery,
    candidate_nodes: &BTreeSet<NodeId>,
    acquire_tolerance_in_canvas: f32,
    release_tolerance_in_canvas: f32,
) -> AlignmentSnapResult {
    let moving_membership = query.moving_nodes.iter().copied().collect::<BTreeSet<_>>();
    let moving = query
        .moving_nodes
        .iter()
        .filter_map(|node| node_features(geometry, *node, query.delta))
        .collect::<Vec<_>>();
    let mut x_snap = query.locks.x.and_then(|lock| {
        retained_axis_snap(geometry, &moving, lock, true, release_tolerance_in_canvas)
    });
    let mut y_snap = query.locks.y.and_then(|lock| {
        retained_axis_snap(geometry, &moving, lock, false, release_tolerance_in_canvas)
    });
    let x_locked = x_snap.is_some();
    let y_locked = y_snap.is_some();
    let mut candidates_examined = 0_usize;
    for candidate in candidate_nodes.iter().copied() {
        if moving_membership.contains(&candidate) {
            continue;
        }
        let Some(candidate_geometry) = geometry.node(candidate).filter(|node| node.is_fresh())
        else {
            continue;
        };
        candidates_examined = candidates_examined.saturating_add(1);
        let candidate_bounds = candidate_geometry.bounds();
        let candidate_x = axis_features(candidate_bounds.origin.x, candidate_bounds.size.width);
        let candidate_y = axis_features(candidate_bounds.origin.y, candidate_bounds.size.height);

        for moving in &moving {
            if !x_locked {
                x_snap = best_axis_snap(
                    x_snap,
                    moving.node,
                    candidate,
                    moving.x,
                    candidate_x,
                    acquire_tolerance_in_canvas,
                );
            }
            if !y_locked {
                y_snap = best_axis_snap(
                    y_snap,
                    moving.node,
                    candidate,
                    moving.y,
                    candidate_y,
                    acquire_tolerance_in_canvas,
                );
            }
        }
    }

    let mut guides = Vec::with_capacity(2);
    let mut snapped = query.delta;
    if let Some(x) = x_snap {
        snapped.x += x.adjustment;
        if let Some(guide) = AlignmentGuide::new(AlignmentGuideAxis::Vertical, x.coordinate) {
            guides.push(guide);
        }
    }
    if let Some(y) = y_snap {
        snapped.y += y.adjustment;
        if let Some(guide) = AlignmentGuide::new(AlignmentGuideAxis::Horizontal, y.coordinate) {
            guides.push(guide);
        }
    }
    AlignmentSnapResult {
        generation: query.generation,
        delta: snapped,
        guides: guides.into_boxed_slice(),
        candidates_examined,
        locks: AlignmentSnapLocks {
            x: x_snap.map(AxisSnap::lock),
            y: y_snap.map(AxisSnap::lock),
        },
        receipt: None,
    }
}

#[derive(Debug, Clone, Copy)]
struct NodeFeatures {
    node: NodeId,
    x: [f32; 3],
    y: [f32; 3],
}

fn node_features(
    geometry: &GeometrySnapshot,
    node: NodeId,
    delta: CanvasPoint,
) -> Option<NodeFeatures> {
    let bounds = geometry.node(node).filter(|node| node.is_fresh())?.bounds();
    Some(NodeFeatures {
        node,
        x: axis_features(bounds.origin.x + delta.x, bounds.size.width),
        y: axis_features(bounds.origin.y + delta.y, bounds.size.height),
    })
}

fn retained_axis_snap(
    geometry: &GeometrySnapshot,
    moving: &[NodeFeatures],
    lock: AxisSnapLock,
    horizontal_axis: bool,
    tolerance: f32,
) -> Option<AxisSnap> {
    let moving = moving.iter().find(|node| node.node == lock.moving)?;
    let candidate = node_features(geometry, lock.candidate, CanvasPoint::default())?;
    let moving_features = if horizontal_axis { moving.x } else { moving.y };
    let candidate_features = if horizontal_axis {
        candidate.x
    } else {
        candidate.y
    };
    let moving_value = *moving_features.get(lock.moving_feature)?;
    let candidate_value = *candidate_features.get(lock.candidate_feature)?;
    let adjustment = candidate_value - moving_value;
    (adjustment.abs() <= tolerance).then_some(AxisSnap {
        distance: adjustment.abs(),
        adjustment,
        coordinate: candidate_value,
        moving: lock.moving,
        candidate: lock.candidate,
        moving_feature: lock.moving_feature,
        candidate_feature: lock.candidate_feature,
    })
}

/// Fits one positive canvas rectangle into a native viewport with screen-pixel padding.
pub fn fit_viewport(
    bounds: CanvasRect,
    viewport: ViewSize,
    padding_in_view: f32,
    min_zoom: f32,
    max_zoom: f32,
) -> Option<ViewportTransform> {
    if !bounds.is_positive_finite()
        || !viewport.is_positive_finite()
        || !padding_in_view.is_finite()
        || padding_in_view < 0.0
        || !min_zoom.is_finite()
        || min_zoom <= 0.0
        || !max_zoom.is_finite()
        || max_zoom <= 0.0
    {
        return None;
    }
    let available_width = viewport.width - padding_in_view * 2.0;
    let available_height = viewport.height - padding_in_view * 2.0;
    if available_width <= 0.0 || available_height <= 0.0 {
        return None;
    }
    let (minimum, maximum) = if min_zoom <= max_zoom {
        (min_zoom, max_zoom)
    } else {
        (max_zoom, min_zoom)
    };
    let zoom = (available_width / bounds.size.width)
        .min(available_height / bounds.size.height)
        .clamp(minimum, maximum);
    let center_x = bounds.origin.x + bounds.size.width * 0.5;
    let center_y = bounds.origin.y + bounds.size.height * 0.5;
    ViewportTransform::new(
        CanvasPoint {
            x: viewport.width * 0.5 / zoom - center_x,
            y: viewport.height * 0.5 / zoom - center_y,
        },
        zoom,
    )
}

fn axis_features(origin: f32, extent: f32) -> [f32; 3] {
    [origin, origin + extent * 0.5, origin + extent]
}

fn best_axis_snap(
    mut best: Option<AxisSnap>,
    moving: NodeId,
    candidate: NodeId,
    moving_features: [f32; 3],
    candidate_features: [f32; 3],
    tolerance: f32,
) -> Option<AxisSnap> {
    for (moving_feature, moving_value) in moving_features.into_iter().enumerate() {
        for (candidate_feature, candidate_value) in candidate_features.into_iter().enumerate() {
            let adjustment = candidate_value - moving_value;
            let distance = adjustment.abs();
            if distance > tolerance {
                continue;
            }
            let incoming = AxisSnap {
                distance,
                adjustment,
                coordinate: candidate_value,
                moving,
                candidate,
                moving_feature,
                candidate_feature,
            };
            if best.is_none_or(|current| incoming.is_better_than(current)) {
                best = Some(incoming);
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_viewport_centers_bounds_with_screen_pixel_padding() {
        let bounds = CanvasRect {
            origin: CanvasPoint { x: 40.0, y: 80.0 },
            size: jellyflow_core::core::CanvasSize {
                width: 500.0,
                height: 100.0,
            },
        };
        let transform = fit_viewport(bounds, ViewSize::new(800.0, 600.0), 48.0, 0.1, 4.0)
            .expect("valid fit transform");
        let top_left = transform
            .screen_point_for_canvas(bounds.origin)
            .expect("project fit origin");
        let bottom_right = transform
            .screen_point_for_canvas(CanvasPoint { x: 540.0, y: 180.0 })
            .expect("project fit extent");

        assert!((top_left.x - 48.0).abs() < 0.001);
        assert!((bottom_right.x - 752.0).abs() < 0.001);
        assert!(((top_left.y + bottom_right.y) * 0.5 - 300.0).abs() < 0.001);
    }

    #[test]
    fn fit_viewport_rejects_empty_available_space_and_clamps_zoom() {
        let bounds = CanvasRect {
            origin: CanvasPoint::default(),
            size: jellyflow_core::core::CanvasSize {
                width: 10.0,
                height: 10.0,
            },
        };
        assert!(fit_viewport(bounds, ViewSize::new(80.0, 80.0), 40.0, 0.1, 4.0,).is_none());
        assert_eq!(
            fit_viewport(bounds, ViewSize::new(800.0, 600.0), 48.0, 0.1, 2.0,)
                .expect("clamped fit")
                .zoom,
            2.0,
        );
    }
}
