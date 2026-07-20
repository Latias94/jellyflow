use std::collections::{BTreeMap, BTreeSet};
use std::ops::Bound::{Excluded, Unbounded};

use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize, NodeId, PortDirection, PortId};

use crate::runtime::adapter_protocol::{PublishedPortGeometry, ViewPoint};
use crate::runtime::graph_runtime::GraphRevision;
use crate::runtime::interaction::{
    ConnectionGestureIntent, ConnectionProbe, ConnectionProbeTarget,
};
use crate::runtime::viewport::ViewportTransform;

use super::order::render_order_key;
use super::spatial_grid::{
    HierarchicalSpatialIndex, SpatialQuerySource, bounded_spatial_cells_for_rect,
};
use super::{ViewGeneration, ViewId, ViewSession, ViewSessionError, ensure_open};

const MAX_CELLS_PER_PORT: i64 = 64;
const MAX_QUERY_CELLS_PER_LEVEL: i64 = 1_024;
const HARD_PORT_QUERY_ENTRY_LIMIT: u64 = 16_384;
const ANCHOR_INDEX_EPSILON: f32 = 0.001;

#[derive(Debug)]
pub struct ConnectionProbeQuery {
    request: ConnectionProbeRequest,
    entry_budget: usize,
}

impl ConnectionProbeQuery {
    pub fn new(
        intent: ConnectionGestureIntent,
        pointer: ViewPoint,
        radius_in_view: f32,
        entry_budget: usize,
    ) -> Self {
        Self {
            request: ConnectionProbeRequest::Start {
                intent,
                pointer,
                radius_in_view,
            },
            entry_budget,
        }
    }

    pub fn resume(cursor: Box<ConnectionProbeCursor>, entry_budget: usize) -> Self {
        Self {
            request: ConnectionProbeRequest::Resume(cursor),
            entry_budget,
        }
    }
}

#[derive(Debug)]
enum ConnectionProbeRequest {
    Start {
        intent: ConnectionGestureIntent,
        pointer: ViewPoint,
        radius_in_view: f32,
    },
    Resume(Box<ConnectionProbeCursor>),
}

#[derive(Debug)]
pub struct ConnectionProbeCursor {
    view: ViewId,
    generation: ViewGeneration,
    transform_revision: u64,
    graph_revision: GraphRevision,
    index_epoch: u128,
    order_revision: u128,
    intent: ConnectionGestureIntent,
    canvas_pointer: CanvasPoint,
    radius_in_view: f32,
    radius_in_canvas: f32,
    sources: Vec<SpatialQuerySource>,
    source_index: usize,
    source_started: bool,
    last_port: Option<PortId>,
    checked: BTreeSet<PortId>,
    best: Option<PortProbeCandidate>,
    total_entries_examined: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ConnectionProbeWork {
    pub cells_examined: u64,
    pub entries_examined: u64,
    pub exact_tests: u64,
    pub policy_evaluations: u64,
}

#[derive(Debug)]
pub enum ConnectionProbeQueryOutcome {
    Complete {
        probe: ConnectionProbe,
        work: ConnectionProbeWork,
    },
    Incomplete {
        cursor: Box<ConnectionProbeCursor>,
        work: ConnectionProbeWork,
    },
    BudgetExhausted {
        work: ConnectionProbeWork,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ConnectionProbeCursorStaleReason {
    #[error("the cursor belongs to another view")]
    ForeignView,
    #[error("the view generation changed")]
    ViewGeneration,
    #[error("the viewport transform changed")]
    ViewportTransform,
    #[error("the graph revision changed")]
    GraphRevision,
    #[error("published port geometry changed")]
    PortGeometry,
    #[error("node stacking or selection changed")]
    PaintOrder,
}

impl ConnectionProbeCursorStaleReason {
    /// Returns whether normal mutable view state can invalidate this cursor between pages.
    ///
    /// Foreign-view and generation mismatches are capability integrity failures. They must not be
    /// retried against a session that never owned the cursor or no longer owns the view generation.
    pub const fn is_restartable(self) -> bool {
        matches!(
            self,
            Self::ViewportTransform | Self::GraphRevision | Self::PortGeometry | Self::PaintOrder
        )
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConnectionProbeQueryError {
    #[error(transparent)]
    Session(#[from] ViewSessionError),
    #[error("connection probe point or radius is invalid")]
    InvalidQuery,
    #[error(transparent)]
    StaleCursor(ConnectionProbeCursorStaleReason),
}

#[derive(Debug, Default)]
pub(super) struct PortGeometryIndex {
    spatial: HierarchicalSpatialIndex<PortId>,
    indexed: BTreeMap<PortId, IndexedPort>,
    index_epoch: u128,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct IndexedPort {
    node: NodeId,
    hit_bounds: CanvasRect,
    route_anchor: CanvasPoint,
    outward: crate::runtime::geometry::HandlePosition,
}

impl From<&PublishedPortGeometry> for IndexedPort {
    fn from(geometry: &PublishedPortGeometry) -> Self {
        Self {
            node: geometry.node(),
            hit_bounds: geometry.hit_bounds(),
            route_anchor: geometry.route_anchor(),
            outward: geometry.outward(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct PortProbeCandidate {
    target: ConnectionProbeTarget,
    direct_hit: bool,
    opposite_direction: bool,
    paint_order: (bool, bool, usize, NodeId),
}

pub(super) enum InternalProbeOutcome {
    Complete {
        intent: ConnectionGestureIntent,
        graph_revision: GraphRevision,
        candidate: Option<PortProbeCandidate>,
        work: ConnectionProbeWork,
    },
    Incomplete {
        cursor: ConnectionProbeCursor,
        work: ConnectionProbeWork,
    },
    BudgetExhausted {
        work: ConnectionProbeWork,
    },
}

impl PortGeometryIndex {
    pub(super) fn upsert(&mut self, geometry: &PublishedPortGeometry) -> bool {
        let port = geometry.port();
        let indexed = IndexedPort::from(geometry);
        if self.indexed.get(&port) == Some(&indexed) {
            return false;
        }
        let bounds = port_index_bounds(indexed.hit_bounds, indexed.route_anchor);
        self.spatial.upsert(
            port,
            bounded_spatial_cells_for_rect(bounds, MAX_CELLS_PER_PORT),
        );
        self.indexed.insert(port, indexed);
        self.index_epoch = self.index_epoch.saturating_add(1);
        true
    }

    pub(super) fn remove(&mut self, port: PortId) -> bool {
        let indexed = self.indexed.remove(&port).is_some();
        let spatial = self.spatial.remove(port);
        if indexed || spatial {
            self.index_epoch = self.index_epoch.saturating_add(1);
        }
        indexed || spatial
    }

    pub(super) fn clear(&mut self) -> bool {
        if self.indexed.is_empty() {
            return false;
        }
        self.spatial.clear();
        self.indexed.clear();
        self.index_epoch = self.index_epoch.saturating_add(1);
        true
    }

    pub(super) fn index_epoch(&self) -> u128 {
        self.index_epoch
    }

    #[cfg(test)]
    pub(super) fn indexed_ports_for_test(&self) -> BTreeSet<PortId> {
        self.indexed.keys().copied().collect()
    }

    #[cfg(test)]
    pub(super) fn matches_geometry_for_test(&self, geometry: &PublishedPortGeometry) -> bool {
        self.indexed.get(&geometry.port()) == Some(&IndexedPort::from(geometry))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn new_cursor(
        &self,
        view: ViewId,
        generation: ViewGeneration,
        transform_revision: u64,
        graph_revision: GraphRevision,
        order_revision: u128,
        intent: ConnectionGestureIntent,
        pointer: ViewPoint,
        radius_in_view: f32,
        transform: ViewportTransform,
    ) -> Option<ConnectionProbeCursor> {
        if !pointer.is_finite() || !radius_in_view.is_finite() || radius_in_view <= 0.0 {
            return None;
        }
        let canvas_pointer = pointer.to_canvas_point(transform)?;
        let radius_in_canvas = radius_in_view / transform.zoom;
        if !radius_in_canvas.is_finite() || radius_in_canvas <= 0.0 {
            return None;
        }
        let query_bounds = CanvasRect {
            origin: CanvasPoint {
                x: canvas_pointer.x - radius_in_canvas,
                y: canvas_pointer.y - radius_in_canvas,
            },
            size: CanvasSize {
                width: radius_in_canvas * 2.0,
                height: radius_in_canvas * 2.0,
            },
        };
        if !query_bounds.is_positive_finite() {
            return None;
        }
        Some(ConnectionProbeCursor {
            view,
            generation,
            transform_revision,
            graph_revision,
            index_epoch: self.index_epoch,
            order_revision,
            intent,
            canvas_pointer,
            radius_in_view,
            radius_in_canvas,
            sources: self
                .spatial
                .query_sources(query_bounds, MAX_QUERY_CELLS_PER_LEVEL),
            source_index: 0,
            source_started: false,
            last_port: None,
            checked: BTreeSet::new(),
            best: None,
            total_entries_examined: 0,
        })
    }

    pub(super) fn query_page(
        &self,
        graph: &jellyflow_core::core::Graph,
        node_draw_order_ranks: &BTreeMap<NodeId, usize>,
        selected_nodes: &std::collections::HashSet<NodeId>,
        mut cursor: ConnectionProbeCursor,
        entry_budget: usize,
    ) -> InternalProbeOutcome {
        let mut work = ConnectionProbeWork::default();
        let mut remaining = entry_budget;
        loop {
            if cursor.source_index >= cursor.sources.len() {
                return InternalProbeOutcome::Complete {
                    intent: cursor.intent,
                    graph_revision: cursor.graph_revision,
                    candidate: cursor.best,
                    work,
                };
            }
            if !cursor.source_started {
                if matches!(
                    cursor.sources[cursor.source_index],
                    SpatialQuerySource::Cell(_)
                ) {
                    work.cells_examined = work.cells_examined.saturating_add(1);
                }
                cursor.source_started = true;
            }
            let source = cursor.sources[cursor.source_index];
            let next = self.spatial.source(source).and_then(|ports| {
                cursor.last_port.map_or_else(
                    || ports.iter().next().copied(),
                    |last| ports.range((Excluded(last), Unbounded)).next().copied(),
                )
            });
            let Some(port) = next else {
                cursor.source_index += 1;
                cursor.source_started = false;
                cursor.last_port = None;
                continue;
            };
            if remaining == 0 {
                return InternalProbeOutcome::Incomplete { cursor, work };
            }
            if cursor.total_entries_examined >= HARD_PORT_QUERY_ENTRY_LIMIT {
                return InternalProbeOutcome::BudgetExhausted { work };
            }
            remaining -= 1;
            cursor.last_port = Some(port);
            cursor.total_entries_examined = cursor.total_entries_examined.saturating_add(1);
            work.entries_examined = work.entries_examined.saturating_add(1);
            if !cursor.checked.insert(port) || port == cursor.intent.fixed_port() {
                continue;
            }
            let Some(indexed) = self.indexed.get(&port).copied() else {
                continue;
            };
            work.exact_tests = work.exact_tests.saturating_add(1);
            let direct_hit = rect_contains(indexed.hit_bounds, cursor.canvas_pointer);
            let distance_in_canvas = point_distance(indexed.route_anchor, cursor.canvas_pointer);
            if !direct_hit && distance_in_canvas > cursor.radius_in_canvas {
                continue;
            }
            let distance_in_view =
                distance_in_canvas * (cursor.radius_in_view / cursor.radius_in_canvas);
            let source_direction = graph
                .port(&cursor.intent.fixed_port())
                .map(|source| source.dir);
            let target_direction = graph.port(&port).map(|target| target.dir);
            let opposite_direction = matches!(
                (source_direction, target_direction),
                (Some(PortDirection::In), Some(PortDirection::Out))
                    | (Some(PortDirection::Out), Some(PortDirection::In))
            );
            let candidate = PortProbeCandidate {
                target: ConnectionProbeTarget::new(
                    port,
                    indexed.node,
                    indexed.route_anchor,
                    indexed.outward,
                    distance_in_view,
                ),
                direct_hit,
                opposite_direction,
                paint_order: render_order_key(
                    indexed.node,
                    selected_nodes.contains(&indexed.node),
                    node_draw_order_ranks,
                ),
            };
            if cursor
                .best
                .is_none_or(|current| port_candidate_is_better(candidate, current))
            {
                cursor.best = Some(candidate);
            }
        }
    }
}

impl ViewSession {
    pub fn connection_probe(
        &self,
        query: ConnectionProbeQuery,
    ) -> Result<ConnectionProbeQueryOutcome, ConnectionProbeQueryError> {
        self.runtime()?;
        let graph = self.graph_snapshot()?;
        let internal = {
            let mut state = self
                .shared
                .state
                .try_borrow_mut()
                .map_err(|_| ViewSessionError::Busy)?;
            ensure_open(state.status)?;
            let transform = ViewportTransform::new(state.facts.pan, state.facts.zoom)
                .ok_or(ViewSessionError::InvalidViewport)?;
            let cursor = match query.request {
                ConnectionProbeRequest::Start {
                    intent,
                    pointer,
                    radius_in_view,
                } => state
                    .derived
                    .new_connection_probe_cursor(
                        state.id,
                        state.generation,
                        state.transform_revision,
                        state.seen_revision,
                        state.interaction_order_revision,
                        intent,
                        pointer,
                        radius_in_view,
                        transform,
                    )
                    .ok_or(ConnectionProbeQueryError::InvalidQuery)?,
                ConnectionProbeRequest::Resume(cursor) => {
                    validate_probe_cursor(&state, &cursor)?;
                    *cursor
                }
            };
            if graph.revision() != state.seen_revision {
                return Err(ConnectionProbeQueryError::StaleCursor(
                    ConnectionProbeCursorStaleReason::GraphRevision,
                ));
            }
            let state = &mut *state;
            state.derived.query_connection_probe_page(
                graph.graph(),
                &state.node_draw_order_ranks,
                &state.selected_node_membership,
                cursor,
                query.entry_budget,
            )
        };

        match internal {
            InternalProbeOutcome::Complete {
                intent,
                graph_revision,
                candidate,
                mut work,
            } => {
                let probe = if let Some(candidate) = candidate {
                    let evaluation =
                        self.evaluate_connection(intent.edit_for_target(candidate.target.port()))?;
                    work.policy_evaluations = 1;
                    {
                        let mut state = self
                            .shared
                            .state
                            .try_borrow_mut()
                            .map_err(|_| ViewSessionError::Busy)?;
                        ensure_open(state.status)?;
                        state.control_stats.connection_probe_policy_evaluations = state
                            .control_stats
                            .connection_probe_policy_evaluations
                            .saturating_add(1);
                    }
                    if evaluation.revision() != graph_revision {
                        return Err(ConnectionProbeQueryError::StaleCursor(
                            ConnectionProbeCursorStaleReason::GraphRevision,
                        ));
                    }
                    if evaluation.is_accept() {
                        ConnectionProbe::Valid {
                            revision: graph_revision,
                            target: candidate.target,
                        }
                    } else {
                        ConnectionProbe::Invalid {
                            revision: graph_revision,
                            target: candidate.target,
                            diagnostics: evaluation.diagnostics().to_vec(),
                        }
                    }
                } else {
                    ConnectionProbe::None
                };
                Ok(ConnectionProbeQueryOutcome::Complete { probe, work })
            }
            InternalProbeOutcome::Incomplete { cursor, work } => {
                Ok(ConnectionProbeQueryOutcome::Incomplete {
                    cursor: Box::new(cursor),
                    work,
                })
            }
            InternalProbeOutcome::BudgetExhausted { work } => {
                Ok(ConnectionProbeQueryOutcome::BudgetExhausted { work })
            }
        }
    }
}

fn validate_probe_cursor(
    state: &super::ViewState,
    cursor: &ConnectionProbeCursor,
) -> Result<(), ConnectionProbeQueryError> {
    let reason = if cursor.view != state.id {
        Some(ConnectionProbeCursorStaleReason::ForeignView)
    } else if cursor.generation != state.generation {
        Some(ConnectionProbeCursorStaleReason::ViewGeneration)
    } else if cursor.transform_revision != state.transform_revision {
        Some(ConnectionProbeCursorStaleReason::ViewportTransform)
    } else if cursor.graph_revision != state.seen_revision {
        Some(ConnectionProbeCursorStaleReason::GraphRevision)
    } else if cursor.index_epoch != state.derived.connection_probe_index_epoch() {
        Some(ConnectionProbeCursorStaleReason::PortGeometry)
    } else if cursor.order_revision != state.interaction_order_revision {
        Some(ConnectionProbeCursorStaleReason::PaintOrder)
    } else {
        None
    };
    reason
        .map(ConnectionProbeQueryError::StaleCursor)
        .map_or(Ok(()), Err)
}

pub(super) fn record_probe_query_stats(
    stats: &mut super::ViewSessionStats,
    outcome: &InternalProbeOutcome,
) {
    let work = match outcome {
        InternalProbeOutcome::Complete { work, .. }
        | InternalProbeOutcome::Incomplete { work, .. }
        | InternalProbeOutcome::BudgetExhausted { work } => *work,
    };
    stats.port_query_cells_examined = stats
        .port_query_cells_examined
        .saturating_add(work.cells_examined);
    stats.port_query_entries_examined = stats
        .port_query_entries_examined
        .saturating_add(work.entries_examined);
    stats.port_query_exact_tests = stats
        .port_query_exact_tests
        .saturating_add(work.exact_tests);
    match outcome {
        InternalProbeOutcome::Incomplete { .. } => {
            stats.port_query_incomplete = stats.port_query_incomplete.saturating_add(1);
        }
        InternalProbeOutcome::BudgetExhausted { .. } => {
            stats.port_query_budget_exhausted = stats.port_query_budget_exhausted.saturating_add(1);
        }
        InternalProbeOutcome::Complete { .. } => {}
    }
}

fn port_candidate_is_better(candidate: PortProbeCandidate, current: PortProbeCandidate) -> bool {
    if candidate.direct_hit != current.direct_hit {
        return candidate.direct_hit;
    }
    let distance = candidate
        .target
        .distance_in_view()
        .total_cmp(&current.target.distance_in_view());
    if candidate.direct_hit {
        candidate
            .paint_order
            .cmp(&current.paint_order)
            .then_with(|| distance.reverse())
            .then_with(|| {
                candidate
                    .opposite_direction
                    .cmp(&current.opposite_direction)
            })
            .then_with(|| current.target.port().cmp(&candidate.target.port()))
            .is_gt()
    } else {
        distance
            .reverse()
            .then_with(|| {
                candidate
                    .opposite_direction
                    .cmp(&current.opposite_direction)
            })
            .then_with(|| candidate.paint_order.cmp(&current.paint_order))
            .then_with(|| current.target.port().cmp(&candidate.target.port()))
            .is_gt()
    }
}

fn port_index_bounds(hit_bounds: CanvasRect, anchor: CanvasPoint) -> CanvasRect {
    let min_x = hit_bounds.origin.x.min(anchor.x - ANCHOR_INDEX_EPSILON);
    let min_y = hit_bounds.origin.y.min(anchor.y - ANCHOR_INDEX_EPSILON);
    let max_x = (hit_bounds.origin.x + hit_bounds.size.width).max(anchor.x + ANCHOR_INDEX_EPSILON);
    let max_y = (hit_bounds.origin.y + hit_bounds.size.height).max(anchor.y + ANCHOR_INDEX_EPSILON);
    CanvasRect {
        origin: CanvasPoint { x: min_x, y: min_y },
        size: CanvasSize {
            width: max_x - min_x,
            height: max_y - min_y,
        },
    }
}

fn rect_contains(bounds: CanvasRect, point: CanvasPoint) -> bool {
    point.x >= bounds.origin.x
        && point.y >= bounds.origin.y
        && point.x <= bounds.origin.x + bounds.size.width
        && point.y <= bounds.origin.y + bounds.size.height
}

fn point_distance(left: CanvasPoint, right: CanvasPoint) -> f32 {
    let x = left.x - right.x;
    let y = left.y - right.y;
    x.mul_add(x, y * y).sqrt()
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashSet};

    use jellyflow_core::core::{
        Graph, GraphBuilder, GraphId, Node, NodeKindKey, Port, PortCapacity, PortKey, PortKind,
    };
    use jellyflow_core::interaction::NodeGraphConnectionMode;

    use super::*;
    use crate::runtime::geometry::HandlePosition;

    #[test]
    fn overlapping_direct_hit_uses_topmost_port_even_when_it_is_not_direction_compatible() {
        let valid_node = NodeId::from_u128(2);
        let invalid_node = NodeId::from_u128(3);
        let valid_port = PortId::from_u128(2);
        let invalid_port = PortId::from_u128(3);
        let graph = graph_with_ports(&[
            (source_node(), source_port(), PortDirection::Out),
            (valid_node, valid_port, PortDirection::In),
            (invalid_node, invalid_port, PortDirection::Out),
        ]);
        let mut index = PortGeometryIndex::default();
        index.upsert(&geometry(valid_node, valid_port, 100.0, 100.0));
        index.upsert(&geometry(invalid_node, invalid_port, 100.0, 100.0));
        let ranks = BTreeMap::from([(valid_node, 0), (invalid_node, 1)]);

        let outcome = query(
            &index,
            &graph,
            &ranks,
            ViewPoint::new(100.0, 100.0),
            12.0,
            1.0,
            32,
        );

        assert_eq!(completed_candidate(outcome), Some(invalid_port));
    }

    #[test]
    fn fixed_port_is_excluded_before_candidate_ranking() {
        let target_node = NodeId::from_u128(2);
        let target_port = PortId::from_u128(2);
        let graph = graph_with_ports(&[
            (source_node(), source_port(), PortDirection::Out),
            (target_node, target_port, PortDirection::In),
        ]);
        let mut index = PortGeometryIndex::default();
        index.upsert(&geometry(source_node(), source_port(), 100.0, 100.0));
        index.upsert(&geometry(target_node, target_port, 106.0, 100.0));

        let outcome = query(
            &index,
            &graph,
            &BTreeMap::new(),
            ViewPoint::new(100.0, 100.0),
            12.0,
            1.0,
            32,
        );

        assert_eq!(completed_candidate(outcome), Some(target_port));
    }

    #[test]
    fn screen_space_radius_is_invariant_across_zoom_levels() {
        let target_node = NodeId::from_u128(2);
        let target_port = PortId::from_u128(2);
        let graph = graph_with_ports(&[
            (source_node(), source_port(), PortDirection::Out),
            (target_node, target_port, PortDirection::In),
        ]);
        let mut index = PortGeometryIndex::default();

        for zoom in [0.5, 1.0, 2.0] {
            index.upsert(&geometry_for_zoom(
                target_node,
                target_port,
                100.0,
                100.0,
                zoom,
            ));
            let inside = query(
                &index,
                &graph,
                &BTreeMap::new(),
                ViewPoint::new(100.0 * zoom + 11.0, 100.0 * zoom),
                12.0,
                zoom,
                32,
            );
            assert_eq!(completed_candidate(inside), Some(target_port));

            let outside = query(
                &index,
                &graph,
                &BTreeMap::new(),
                ViewPoint::new(100.0 * zoom + 13.0, 100.0 * zoom),
                12.0,
                zoom,
                32,
            );
            assert_eq!(completed_candidate(outside), None);
        }
    }

    #[test]
    fn zero_budget_resumes_without_losing_the_candidate() {
        let target_node = NodeId::from_u128(2);
        let target_port = PortId::from_u128(2);
        let graph = graph_with_ports(&[
            (source_node(), source_port(), PortDirection::Out),
            (target_node, target_port, PortDirection::In),
        ]);
        let mut index = PortGeometryIndex::default();
        index.upsert(&geometry(target_node, target_port, 100.0, 100.0));
        let cursor = cursor(&index, ViewPoint::new(100.0, 100.0), 12.0, 1.0);

        let InternalProbeOutcome::Incomplete { cursor, work } =
            index.query_page(&graph, &BTreeMap::new(), &HashSet::new(), cursor, 0)
        else {
            panic!("zero budget must preserve resumable work");
        };
        assert_eq!(work.entries_examined, 0);

        let resumed = index.query_page(&graph, &BTreeMap::new(), &HashSet::new(), cursor, 1);
        assert_eq!(completed_candidate(resumed), Some(target_port));
    }

    #[test]
    fn remove_and_clear_delete_all_spatial_membership() {
        let first_node = NodeId::from_u128(2);
        let second_node = NodeId::from_u128(3);
        let first_port = PortId::from_u128(2);
        let second_port = PortId::from_u128(3);
        let graph = graph_with_ports(&[
            (source_node(), source_port(), PortDirection::Out),
            (first_node, first_port, PortDirection::In),
            (second_node, second_port, PortDirection::In),
        ]);
        let mut index = PortGeometryIndex::default();
        index.upsert(&geometry(first_node, first_port, 100.0, 100.0));
        index.upsert(&geometry(second_node, second_port, 300.0, 100.0));

        assert!(index.remove(first_port));
        assert_eq!(
            completed_candidate(query(
                &index,
                &graph,
                &BTreeMap::new(),
                ViewPoint::new(100.0, 100.0),
                12.0,
                1.0,
                32,
            )),
            None
        );
        assert!(index.clear());
        assert_eq!(
            completed_candidate(query(
                &index,
                &graph,
                &BTreeMap::new(),
                ViewPoint::new(300.0, 100.0),
                12.0,
                1.0,
                32,
            )),
            None
        );
    }

    #[test]
    fn distributed_ten_thousand_port_probe_stays_local() {
        let mut builder = GraphBuilder::new(GraphId::from_u128(1));
        insert_port(
            &mut builder,
            source_node(),
            source_port(),
            PortDirection::Out,
        );
        let mut index = PortGeometryIndex::default();
        let mut expected = None;
        for ordinal in 0..10_000_u128 {
            let node = NodeId::from_u128(ordinal + 2);
            let port = PortId::from_u128(ordinal + 2);
            insert_port(&mut builder, node, port, PortDirection::In);
            let x = (ordinal % 100) as f32 * 512.0;
            let y = (ordinal / 100) as f32 * 512.0;
            index.upsert(&geometry(node, port, x, y));
            if ordinal == 5_050 {
                expected = Some((port, x, y));
            }
        }
        let graph = builder.build_unchecked();
        let (expected_port, x, y) = expected.expect("fixture target");

        let outcome = query(
            &index,
            &graph,
            &BTreeMap::new(),
            ViewPoint::new(x, y),
            12.0,
            1.0,
            256,
        );
        let InternalProbeOutcome::Complete {
            candidate, work, ..
        } = outcome
        else {
            panic!("a local page must complete");
        };

        assert_eq!(
            candidate.map(|candidate| candidate.target.port()),
            Some(expected_port)
        );
        assert!(work.entries_examined <= 256);
        assert!(work.exact_tests <= 4);
    }

    fn query(
        index: &PortGeometryIndex,
        graph: &Graph,
        ranks: &BTreeMap<NodeId, usize>,
        pointer: ViewPoint,
        radius: f32,
        zoom: f32,
        budget: usize,
    ) -> InternalProbeOutcome {
        index.query_page(
            graph,
            ranks,
            &HashSet::new(),
            cursor(index, pointer, radius, zoom),
            budget,
        )
    }

    fn cursor(
        index: &PortGeometryIndex,
        pointer: ViewPoint,
        radius: f32,
        zoom: f32,
    ) -> ConnectionProbeCursor {
        index
            .new_cursor(
                ViewId::unavailable(),
                ViewGeneration::initial(),
                0,
                0,
                0,
                ConnectionGestureIntent::connect(
                    test_edge(),
                    source_port(),
                    NodeGraphConnectionMode::Strict,
                ),
                pointer,
                radius,
                ViewportTransform::new(CanvasPoint { x: 0.0, y: 0.0 }, zoom)
                    .expect("valid transform"),
            )
            .expect("valid query")
    }

    fn completed_candidate(outcome: InternalProbeOutcome) -> Option<PortId> {
        match outcome {
            InternalProbeOutcome::Complete { candidate, .. } => {
                candidate.map(|candidate| candidate.target.port())
            }
            InternalProbeOutcome::Incomplete { .. } => panic!("query did not complete"),
            InternalProbeOutcome::BudgetExhausted { .. } => panic!("query exhausted its budget"),
        }
    }

    fn source_node() -> NodeId {
        NodeId::from_u128(1)
    }

    fn source_port() -> PortId {
        PortId::from_u128(1)
    }

    fn test_edge() -> jellyflow_core::core::EdgeId {
        jellyflow_core::core::EdgeId::from_u128(1)
    }

    fn graph_with_ports(entries: &[(NodeId, PortId, PortDirection)]) -> Graph {
        let mut builder = GraphBuilder::new(GraphId::from_u128(1));
        for &(node, port, direction) in entries {
            insert_port(&mut builder, node, port, direction);
        }
        builder.build().expect("valid fixture graph")
    }

    fn insert_port(
        builder: &mut GraphBuilder,
        node: NodeId,
        port: PortId,
        direction: PortDirection,
    ) {
        builder.insert_node(node, test_node(port));
        builder.insert_port(
            port,
            Port {
                node,
                key: PortKey::new("port"),
                dir: direction,
                kind: PortKind::Data,
                capacity: PortCapacity::Multi,
                connectable: None,
                connectable_start: None,
                connectable_end: None,
                ty: None,
                data: serde_json::Value::Null,
            },
        );
    }

    fn test_node(port: PortId) -> Node {
        Node {
            kind: NodeKindKey::new("test.node"),
            kind_version: 1,
            pos: CanvasPoint { x: 0.0, y: 0.0 },
            origin: None,
            selectable: None,
            draggable: None,
            connectable: None,
            deletable: None,
            parent: None,
            extent: None,
            expand_parent: None,
            size: Some(CanvasSize {
                width: 160.0,
                height: 96.0,
            }),
            hidden: false,
            collapsed: false,
            ports: vec![port],
            data: serde_json::Value::Null,
        }
    }

    fn geometry(node: NodeId, port: PortId, x: f32, y: f32) -> PublishedPortGeometry {
        geometry_for_zoom(node, port, x, y, 1.0)
    }

    fn geometry_for_zoom(
        node: NodeId,
        port: PortId,
        x: f32,
        y: f32,
        zoom: f32,
    ) -> PublishedPortGeometry {
        let visual_half = 5.0 / zoom;
        let hit_half = 10.0 / zoom;
        PublishedPortGeometry::new(
            port,
            node,
            CanvasRect {
                origin: CanvasPoint {
                    x: x - visual_half,
                    y: y - visual_half,
                },
                size: CanvasSize {
                    width: visual_half * 2.0,
                    height: visual_half * 2.0,
                },
            },
            CanvasRect {
                origin: CanvasPoint {
                    x: x - hit_half,
                    y: y - hit_half,
                },
                size: CanvasSize {
                    width: hit_half * 2.0,
                    height: hit_half * 2.0,
                },
            },
            CanvasPoint { x, y },
            HandlePosition::Left,
        )
    }
}
