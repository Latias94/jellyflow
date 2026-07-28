//! Per-view geometry publication, indexing, and route derivation.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Bound::{Excluded, Unbounded};
use std::sync::Arc;

use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize, Graph, Node, NodeId, NodeKindKey};

use crate::runtime::adapter_protocol::{
    FeedbackLayoutWork, FrameRequest, GeometrySnapshot, LayoutNodeStatus, LayoutOperationReceipt,
    LayoutPublicationReceipt, PortLayoutOutcome, PublishedNodeGeometry, PublishedPortGeometry,
    PublishedPortOutcome,
};
use crate::runtime::geometry::CanvasBounds;
use crate::runtime::graph_runtime::{
    AdapterNodeRuntimeFacts, GeometryImpact, GraphSnapshot, IncidentEdgeSnapshot,
};
use crate::runtime::interaction::{
    AlignmentSnapQuery, AlignmentSnapQueryOutcome, AlignmentSnapWork, resolve_alignment_snap,
};
use crate::runtime::viewport::ViewportTransform;

use super::derived_geometry::LayoutWithdrawal;
use super::protocol::{ExpectedNodeLayout, PendingFrame, PendingLayout};
use super::{
    AlignmentSnapQueryReceipt, InteractionQueryBinding, InteractionQueryProof,
    NodeIntersectionQueryReceipt, ViewQueryIdentity, ViewQueryIdentityStaleReason, ViewSession,
    ViewSessionError, ViewShared, ViewState, WeakViewSession, ensure_open,
};

const HARD_NODE_INTERSECTION_ENTRY_LIMIT: u64 = 16_384;

#[derive(Debug)]
pub struct NodeIntersectionQuery {
    request: NodeIntersectionRequest,
    entry_budget: usize,
    interaction_binding: Option<InteractionQueryBinding>,
}

impl NodeIntersectionQuery {
    pub fn new(bounds: CanvasRect, entry_budget: usize) -> Self {
        Self {
            request: NodeIntersectionRequest::Start(bounds),
            entry_budget,
            interaction_binding: None,
        }
    }

    pub fn resume(cursor: Box<NodeIntersectionCursor>, entry_budget: usize) -> Self {
        Self {
            request: NodeIntersectionRequest::Resume(cursor),
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
enum NodeIntersectionRequest {
    Start(CanvasRect),
    Resume(Box<NodeIntersectionCursor>),
}

#[derive(Debug)]
pub struct NodeIntersectionCursor {
    identity: ViewQueryIdentity,
    index_epoch: u128,
    bounds: CanvasBounds,
    sources: Vec<NodeSpatialQuerySource>,
    source_index: usize,
    source_started: bool,
    last_node: Option<NodeId>,
    checked: BTreeSet<NodeId>,
    matched: BTreeSet<NodeId>,
    total_entries_examined: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NodeIntersectionWork {
    pub cells_examined: u64,
    pub entries_examined: u64,
    pub exact_tests: u64,
}

#[derive(Debug)]
pub enum NodeIntersectionQueryOutcome {
    Complete {
        nodes: Vec<NodeId>,
        receipt: NodeIntersectionQueryReceipt,
    },
    Incomplete {
        cursor: Box<NodeIntersectionCursor>,
        receipt: NodeIntersectionQueryReceipt,
    },
    BudgetExhausted {
        receipt: NodeIntersectionQueryReceipt,
    },
}

impl NodeIntersectionQueryOutcome {
    pub fn receipt(&self) -> NodeIntersectionQueryReceipt {
        match self {
            Self::Complete { receipt, .. }
            | Self::Incomplete { receipt, .. }
            | Self::BudgetExhausted { receipt } => *receipt,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum NodeIntersectionCursorStaleReason {
    #[error("the cursor belongs to another view")]
    ForeignView,
    #[error("the view generation changed")]
    ViewGeneration,
    #[error("the viewport transform changed")]
    ViewportTransform,
    #[error("published node geometry changed")]
    NodeGeometry,
}

impl NodeIntersectionCursorStaleReason {
    pub const fn is_restartable(self) -> bool {
        matches!(self, Self::ViewportTransform | Self::NodeGeometry)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum NodeIntersectionQueryError {
    #[error(transparent)]
    View(#[from] ViewSessionError),
    #[error("the node-intersection query is invalid")]
    InvalidQuery,
    #[error("the node-intersection cursor is stale: {0}")]
    StaleCursor(NodeIntersectionCursorStaleReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NodeSpatialQuerySource {
    Overflow,
    Cell(SpatialCell),
    All,
}

pub(super) enum InternalNodeIntersectionOutcome {
    Complete {
        nodes: BTreeSet<NodeId>,
        work: NodeIntersectionWork,
    },
    Incomplete {
        cursor: NodeIntersectionCursor,
        work: NodeIntersectionWork,
    },
    BudgetExhausted {
        work: NodeIntersectionWork,
    },
}

impl ViewSession {
    pub fn geometry_snapshot(&self) -> Result<GeometrySnapshot, ViewSessionError> {
        self.runtime()?;
        self.shared.geometry_snapshot()
    }

    pub fn nodes_intersecting(
        &self,
        query: NodeIntersectionQuery,
    ) -> Result<NodeIntersectionQueryOutcome, NodeIntersectionQueryError> {
        self.runtime()?;
        self.shared.nodes_intersecting(query)
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

    pub(crate) fn nodes_intersecting(
        &self,
        query: NodeIntersectionQuery,
    ) -> Result<NodeIntersectionQueryOutcome, NodeIntersectionQueryError> {
        let (view, _) = self.open_capability()?;
        view.nodes_intersecting(query)
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
    fn nodes_intersecting(
        &self,
        query: NodeIntersectionQuery,
    ) -> Result<NodeIntersectionQueryOutcome, NodeIntersectionQueryError> {
        let (outcome, proof) = {
            let state = self
                .state
                .try_borrow()
                .map_err(|_| ViewSessionError::Busy)?;
            ensure_open(state.status)?;
            let identity = state.query_identity();
            let index_epoch = state.derived.node_spatial_index_epoch();
            let proof = InteractionQueryProof::issue(
                query.interaction_binding,
                identity,
                [index_epoch, 0, 0, 0],
            )?;
            let cursor = match query.request {
                NodeIntersectionRequest::Start(bounds) => {
                    if !bounds.is_finite() || bounds.size.width < 0.0 || bounds.size.height < 0.0 {
                        return Err(NodeIntersectionQueryError::InvalidQuery);
                    }
                    if bounds.size.width == 0.0 || bounds.size.height == 0.0 {
                        return Ok(NodeIntersectionQueryOutcome::Complete {
                            nodes: Vec::new(),
                            receipt: NodeIntersectionQueryReceipt::new(
                                NodeIntersectionWork::default(),
                                proof,
                            ),
                        });
                    }
                    let Some(canvas_bounds) = CanvasBounds::from_rect(bounds) else {
                        return Err(NodeIntersectionQueryError::InvalidQuery);
                    };
                    state.derived.new_node_intersection_cursor(
                        state.query_identity(),
                        bounds,
                        canvas_bounds,
                    )
                }
                NodeIntersectionRequest::Resume(cursor) => {
                    validate_node_intersection_cursor(&state, &cursor)?;
                    *cursor
                }
            };
            let outcome = state
                .derived
                .query_node_intersection_page(cursor, query.entry_budget);
            (outcome, proof)
        };
        Ok(match outcome {
            InternalNodeIntersectionOutcome::Complete { nodes, work } => {
                NodeIntersectionQueryOutcome::Complete {
                    nodes: nodes.into_iter().collect(),
                    receipt: NodeIntersectionQueryReceipt::new(work, proof),
                }
            }
            InternalNodeIntersectionOutcome::Incomplete { cursor, work } => {
                NodeIntersectionQueryOutcome::Incomplete {
                    cursor: Box::new(cursor),
                    receipt: NodeIntersectionQueryReceipt::new(work, proof),
                }
            }
            InternalNodeIntersectionOutcome::BudgetExhausted { work } => {
                NodeIntersectionQueryOutcome::BudgetExhausted {
                    receipt: NodeIntersectionQueryReceipt::new(work, proof),
                }
            }
        })
    }

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
        let proof = InteractionQueryProof::issue(
            Some(query.interaction_binding()),
            state.query_identity(),
            [
                state.derived.node_spatial_index_epoch(),
                state.interaction_order_revision,
                u128::from(state.seen_revision),
                0,
            ],
        )?;
        let receipt = |cells_examined, entries_examined, candidates_examined| {
            AlignmentSnapQueryReceipt::new(
                AlignmentSnapWork::new(cells_examined, entries_examined, candidates_examined),
                proof,
            )
        };
        if query.view() != state.id || query.view_generation() != state.generation {
            return Ok(AlignmentSnapQueryOutcome::GeometryIndeterminate {
                generation: query.generation(),
                receipt: receipt(0, 0, 0),
            });
        }
        let Some(transform) = ViewportTransform::new(state.facts.pan, state.facts.zoom) else {
            return Ok(AlignmentSnapQueryOutcome::GeometryIndeterminate {
                generation: query.generation(),
                receipt: receipt(0, 0, 0),
            });
        };
        let acquire_tolerance = query.acquire_tolerance_in_view() / transform.zoom;
        let release_tolerance = query.release_tolerance_in_view() / transform.zoom;
        if !acquire_tolerance.is_finite()
            || acquire_tolerance <= 0.0
            || !release_tolerance.is_finite()
            || release_tolerance < acquire_tolerance
            || query.entry_budget() == 0
        {
            return Ok(AlignmentSnapQueryOutcome::GeometryIndeterminate {
                generation: query.generation(),
                receipt: receipt(0, 0, 0),
            });
        }
        let geometry = state.derived.snapshot();
        if query.moving_nodes().len() > query.entry_budget() {
            return Ok(AlignmentSnapQueryOutcome::BudgetExhausted {
                generation: query.generation(),
                receipt: receipt(0, 0, 0),
            });
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
            return Ok(AlignmentSnapQueryOutcome::GeometryIndeterminate {
                generation: query.generation(),
                receipt: receipt(0, 0, 0),
            });
        };
        let candidates = state.derived.nodes_near_alignment_with_limit(
            &moving_bounds,
            release_tolerance,
            query.entry_budget(),
        );
        if candidates.truncated {
            return Ok(AlignmentSnapQueryOutcome::BudgetExhausted {
                generation: query.generation(),
                receipt: receipt(candidates.cells_examined, candidates.entries_examined, 0),
            });
        }
        let (result, candidates_examined) = resolve_alignment_snap(
            &geometry,
            &query,
            &candidates.nodes,
            acquire_tolerance,
            release_tolerance,
        );
        Ok(AlignmentSnapQueryOutcome::Complete {
            result,
            receipt: receipt(
                candidates.cells_examined,
                candidates.entries_examined,
                candidates_examined,
            ),
        })
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
    index_epoch: u128,
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
        self.index_epoch = self.index_epoch.saturating_add(1);
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
        self.index_epoch = self.index_epoch.saturating_add(1);
        true
    }

    pub(super) fn clear(&mut self) {
        let changed = !self.nodes.is_empty();
        self.cells.clear();
        self.alignment_x.clear();
        self.alignment_y.clear();
        self.nodes.clear();
        self.overflow_nodes.clear();
        if changed {
            self.index_epoch = self.index_epoch.saturating_add(1);
        }
    }

    pub(super) fn index_epoch(&self) -> u128 {
        self.index_epoch
    }

    pub(super) fn new_intersection_cursor(
        &self,
        identity: ViewQueryIdentity,
        bounds: CanvasRect,
        canvas_bounds: CanvasBounds,
    ) -> NodeIntersectionCursor {
        let sources = covered_spatial_cells(bounds).map_or_else(
            || vec![NodeSpatialQuerySource::All],
            |cells| {
                std::iter::once(NodeSpatialQuerySource::Overflow)
                    .chain(cells.into_iter().map(NodeSpatialQuerySource::Cell))
                    .collect()
            },
        );
        NodeIntersectionCursor {
            identity,
            index_epoch: self.index_epoch,
            bounds: canvas_bounds,
            sources,
            source_index: 0,
            source_started: false,
            last_node: None,
            checked: BTreeSet::new(),
            matched: BTreeSet::new(),
            total_entries_examined: 0,
        }
    }

    pub(super) fn query_intersection_page(
        &self,
        mut cursor: NodeIntersectionCursor,
        entry_budget: usize,
    ) -> InternalNodeIntersectionOutcome {
        let mut work = NodeIntersectionWork::default();
        let mut remaining = entry_budget;
        let query = cursor.bounds;
        loop {
            if cursor.source_index >= cursor.sources.len() {
                return InternalNodeIntersectionOutcome::Complete {
                    nodes: cursor.matched,
                    work,
                };
            }
            let source = cursor.sources[cursor.source_index];
            if !cursor.source_started {
                if matches!(source, NodeSpatialQuerySource::Cell(_)) {
                    work.cells_examined = work.cells_examined.saturating_add(1);
                }
                cursor.source_started = true;
            }
            let next = match source {
                NodeSpatialQuerySource::Overflow => cursor.last_node.map_or_else(
                    || self.overflow_nodes.iter().next().copied(),
                    |last| {
                        self.overflow_nodes
                            .range((Excluded(last), Unbounded))
                            .next()
                            .copied()
                    },
                ),
                NodeSpatialQuerySource::Cell(cell) => self.cells.get(&cell).and_then(|nodes| {
                    cursor.last_node.map_or_else(
                        || nodes.iter().next().copied(),
                        |last| nodes.range((Excluded(last), Unbounded)).next().copied(),
                    )
                }),
                NodeSpatialQuerySource::All => cursor.last_node.map_or_else(
                    || self.nodes.keys().next().copied(),
                    |last| {
                        self.nodes
                            .range((Excluded(last), Unbounded))
                            .next()
                            .map(|(node, _)| *node)
                    },
                ),
            };
            let Some(node) = next else {
                cursor.source_index += 1;
                cursor.source_started = false;
                cursor.last_node = None;
                continue;
            };
            if remaining == 0 {
                return InternalNodeIntersectionOutcome::Incomplete { cursor, work };
            }
            if cursor.total_entries_examined >= HARD_NODE_INTERSECTION_ENTRY_LIMIT {
                return InternalNodeIntersectionOutcome::BudgetExhausted { work };
            }
            remaining -= 1;
            cursor.last_node = Some(node);
            cursor.total_entries_examined = cursor.total_entries_examined.saturating_add(1);
            work.entries_examined = work.entries_examined.saturating_add(1);
            if !cursor.checked.insert(node) {
                continue;
            }
            work.exact_tests = work.exact_tests.saturating_add(1);
            if self
                .nodes
                .get(&node)
                .and_then(|indexed| CanvasBounds::from_rect(indexed.bounds))
                .is_some_and(|bounds| bounds.intersects(query))
            {
                cursor.matched.insert(node);
            }
        }
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
        let mut truncated = extend_spatial_candidates(
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
                truncated = extend_spatial_candidates(
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
                truncated = extend_spatial_candidates(
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
    current_node_facts: &BTreeMap<NodeId, AdapterNodeRuntimeFacts>,
    geometry_impact: &GeometryImpact,
    layouts: Vec<PendingLayout>,
    receipts: &mut [LayoutOperationReceipt],
    global_stale: bool,
) -> (Option<LayoutPublicationReceipt>, FeedbackLayoutWork) {
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
    let layouts_examined = layouts.len() as u64;
    let mut layouts_stale = 0_u64;
    let mut impact_candidates_checked = 0_u64;
    let mut impact_candidates_invalidated = 0_u64;

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
        if status == LayoutNodeStatus::Stale {
            layouts_stale = layouts_stale.saturating_add(1);
        }
        if impact_stale && status == LayoutNodeStatus::Stale {
            impact_candidates_invalidated = impact_candidates_invalidated.saturating_add(1);
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
        receipts[pending_layout.receipt_index].set_status(status);
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
                    .map(|facts| facts.incident_edges.clone()),
            )
        })
        .collect();
    let policy = state.policy;
    let ViewState {
        derived,
        selected_edge_membership,
        ..
    } = state;
    let (publication, work) = derived.publish_complete_layout(
        snapshot,
        staged,
        withdrawals,
        geometry_impact.incident_edges(),
        request_refresh,
        policy,
        selected_edge_membership,
    );
    (
        publication,
        FeedbackLayoutWork::new(
            layouts_examined,
            layouts_stale,
            impact_candidates_checked,
            impact_candidates_invalidated,
            work.incident_snapshot.entries_read(),
            work.incident_snapshot.materializations(),
            work.spatial_updates,
            work.route_invalidations,
            work.route_recomputations,
            work.refresh_requested,
        ),
    )
}

fn evaluate_layout_candidate(
    state: &ViewState,
    snapshot: &GraphSnapshot,
    current_node_facts: &BTreeMap<NodeId, AdapterNodeRuntimeFacts>,
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
        || target.instance().lifetime() != expected.lifetime
        || target.instance().renderer() != &expected.renderer
        || target.layout_generation() != expected.layout_generation
        || target.frame_sequence() != frame_sequence
    {
        return (LayoutNodeStatus::Stale, None);
    }
    let Some(runtime_facts) = current_node_facts.get(&node) else {
        return (LayoutNodeStatus::MissingNode, None);
    };
    if runtime_facts.lifetime != expected.lifetime
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
        if runtime_facts.port_instances.get(&port_id).copied()
            != Some(outcome.target().port_instance())
        {
            return (LayoutNodeStatus::Stale, None);
        }
        let published = match outcome {
            PortLayoutOutcome::Rendered(port) => {
                let Some(visual_bounds) = port.visual_bounds.to_canvas_rect(transform) else {
                    return (LayoutNodeStatus::InvalidBounds, None);
                };
                if port.hit_bounds.to_canvas_rect(transform).is_none() {
                    return (LayoutNodeStatus::InvalidBounds, None);
                }
                let interaction_bounds = match port.interaction_bounds {
                    Some(bounds) => {
                        if !view_rect_contains(port.hit_bounds, bounds) {
                            return (LayoutNodeStatus::InvalidBounds, None);
                        }
                        let Some(bounds) = bounds.to_canvas_rect(transform) else {
                            return (LayoutNodeStatus::InvalidBounds, None);
                        };
                        Some(bounds)
                    }
                    None => None,
                };
                let Some(route_anchor) = port.route_anchor.to_canvas_point(transform) else {
                    return (LayoutNodeStatus::InvalidBounds, None);
                };
                PublishedPortOutcome::Rendered(PublishedPortGeometry::new(
                    port_id,
                    node,
                    visual_bounds,
                    interaction_bounds,
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
        expected.lifetime,
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

fn view_rect_contains(
    outer: crate::runtime::adapter_protocol::ViewRect,
    inner: crate::runtime::adapter_protocol::ViewRect,
) -> bool {
    let outer_right = outer.origin.x + outer.size.width;
    let outer_bottom = outer.origin.y + outer.size.height;
    let inner_right = inner.origin.x + inner.size.width;
    let inner_bottom = inner.origin.y + inner.size.height;
    inner.origin.x >= outer.origin.x
        && inner.origin.y >= outer.origin.y
        && inner_right <= outer_right
        && inner_bottom <= outer_bottom
}

fn validate_node_intersection_cursor(
    state: &ViewState,
    cursor: &NodeIntersectionCursor,
) -> Result<(), NodeIntersectionQueryError> {
    let stale = match cursor.identity.stale_reason(state.query_identity()) {
        Some(ViewQueryIdentityStaleReason::ForeignView) => {
            Some(NodeIntersectionCursorStaleReason::ForeignView)
        }
        Some(ViewQueryIdentityStaleReason::ViewGeneration) => {
            Some(NodeIntersectionCursorStaleReason::ViewGeneration)
        }
        Some(ViewQueryIdentityStaleReason::ViewportTransform) => {
            Some(NodeIntersectionCursorStaleReason::ViewportTransform)
        }
        None if cursor.index_epoch != state.derived.node_spatial_index_epoch() => {
            Some(NodeIntersectionCursorStaleReason::NodeGeometry)
        }
        None => None,
    };
    stale
        .map(NodeIntersectionQueryError::StaleCursor)
        .map_or(Ok(()), Err)
}
