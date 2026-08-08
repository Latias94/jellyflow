use std::collections::{BTreeMap, BTreeSet};
use std::ops::Bound::{Excluded, Unbounded};
use std::rc::Rc;

use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize, NodeId, PortDirection, PortId};

use crate::runtime::adapter_protocol::{PublishedPortGeometry, ViewPoint};
use crate::runtime::graph_runtime::{
    ConnectionAdmissionCursor, ConnectionAdmissionOutcome, ConnectionAdmissionWork, GraphRevision,
    PortInstanceKey, RuntimeShared,
};
use crate::runtime::interaction::{
    ConnectionGestureBinding, ConnectionProbe, ConnectionProbeTarget, ConnectionProbeTargetIdentity,
};
use crate::runtime::viewport::ViewportTransform;

use super::order::render_order_key;
use super::spatial_grid::{
    HierarchicalSpatialIndex, SpatialQueryCursor, SpatialQuerySource, SpatialQueryStep,
    bounded_spatial_cells_for_rect, max_spatial_query_setup_work,
};
use super::{
    ConnectionProbeQueryReceipt, InteractionQueryBinding, InteractionQueryProof, ViewQueryIdentity,
    ViewQueryIdentityStaleReason, ViewSession, ViewSessionError, ViewShared, WeakViewSession,
    ensure_open,
};
#[cfg(test)]
use super::{ViewGeneration, ViewId};

const MAX_CELLS_PER_PORT: i64 = 64;
const MAX_QUERY_CELLS_PER_LEVEL: i64 = 1_024;
const HARD_PORT_QUERY_ENTRY_LIMIT: u64 = 16_384;
const MAX_PORT_QUERY_SETUP_WORK: u64 =
    max_spatial_query_setup_work(MAX_QUERY_CELLS_PER_LEVEL as u64);
const HARD_PORT_QUERY_WORK_LIMIT: u64 = HARD_PORT_QUERY_ENTRY_LIMIT + MAX_PORT_QUERY_SETUP_WORK;

/// Screen-space target bands and the optional target lifetime retained by a connection drag.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ConnectionProbeTargeting {
    acquire_radius_in_view: f32,
    release_radius_in_view: f32,
    preferred: Option<ConnectionProbeTargetIdentity>,
}

impl ConnectionProbeTargeting {
    pub(crate) fn new(acquire_radius_in_view: f32, release_radius_in_view: f32) -> Option<Self> {
        (acquire_radius_in_view.is_finite()
            && acquire_radius_in_view > 0.0
            && release_radius_in_view.is_finite()
            && release_radius_in_view >= acquire_radius_in_view)
            .then_some(Self {
                acquire_radius_in_view,
                release_radius_in_view,
                preferred: None,
            })
    }

    #[must_use]
    pub(crate) const fn with_preferred(mut self, preferred: ConnectionProbeTargetIdentity) -> Self {
        self.preferred = Some(preferred);
        self
    }

    pub(crate) const fn acquire_radius_in_view(self) -> f32 {
        self.acquire_radius_in_view
    }

    pub(crate) const fn release_radius_in_view(self) -> f32 {
        self.release_radius_in_view
    }

    pub(crate) const fn preferred(self) -> Option<ConnectionProbeTargetIdentity> {
        self.preferred
    }
}

#[derive(Debug)]
pub(crate) struct ConnectionProbeQuery {
    request: ConnectionProbeRequest,
    entry_budget: usize,
    interaction_binding: Option<InteractionQueryBinding>,
}

impl ConnectionProbeQuery {
    pub(crate) fn new(
        intent: ConnectionGestureBinding,
        pointer: ViewPoint,
        targeting: ConnectionProbeTargeting,
        entry_budget: usize,
    ) -> Self {
        Self {
            request: ConnectionProbeRequest::Start {
                intent,
                pointer,
                targeting,
            },
            entry_budget,
            interaction_binding: None,
        }
    }

    pub(crate) fn resume(cursor: Box<ConnectionProbeCursor>, entry_budget: usize) -> Self {
        Self {
            request: ConnectionProbeRequest::Resume(cursor),
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
enum ConnectionProbeRequest {
    Start {
        intent: ConnectionGestureBinding,
        pointer: ViewPoint,
        targeting: ConnectionProbeTargeting,
    },
    Resume(Box<ConnectionProbeCursor>),
}

#[derive(Debug)]
pub(crate) struct ConnectionProbeCursor {
    identity: ViewQueryIdentity,
    graph_revision: GraphRevision,
    index_epoch: u128,
    order_revision: u128,
    intent: ConnectionGestureBinding,
    canvas_pointer: CanvasPoint,
    targeting: ConnectionProbeTargeting,
    acquire_radius_in_canvas: f32,
    release_radius_in_canvas: f32,
    spatial_query: SpatialQueryCursor,
    active_source: Option<SpatialQuerySource>,
    next_port: Option<PortId>,
    checked: BTreeSet<PortId>,
    best: Option<PortProbeCandidate>,
    admission: Option<ConnectionAdmissionCursor>,
    total_entries_examined: u64,
    total_spatial_work: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ConnectionProbeWork {
    /// Level-discovery steps, including the terminal exhausted check.
    pub spatial_level_checks: u64,
    /// Cell or whole-level sources initialized by the spatial cursor.
    pub spatial_source_setups: u64,
    /// Cell sources initialized; this is a subset of `spatial_source_setups`.
    pub cells_examined: u64,
    /// Port entries consumed from initialized sources.
    pub entries_examined: u64,
    /// Exact geometry tests performed; this is a subset of `entries_examined`.
    pub exact_tests: u64,
    pub admission_base_rules_examined: u64,
    pub admission_incident_buckets_examined: u64,
    pub admission_incident_entries_examined: u64,
    pub admission_cycle_nodes_examined: u64,
    pub admission_cycle_ports_examined: u64,
    pub admission_cycle_buckets_examined: u64,
    pub admission_cycle_edges_examined: u64,
}

impl ConnectionProbeWork {
    /// Returns the spatial level, source, and entry units charged against the shared page budget.
    #[must_use]
    pub const fn spatial_work_units(self) -> u64 {
        self.spatial_level_checks
            .saturating_add(self.spatial_source_setups)
            .saturating_add(self.entries_examined)
    }

    #[must_use]
    pub const fn admission_work_units(self) -> u64 {
        self.admission_base_rules_examined
            .saturating_add(self.admission_incident_buckets_examined)
            .saturating_add(self.admission_incident_entries_examined)
            .saturating_add(self.admission_cycle_nodes_examined)
            .saturating_add(self.admission_cycle_ports_examined)
            .saturating_add(self.admission_cycle_buckets_examined)
            .saturating_add(self.admission_cycle_edges_examined)
    }

    /// Returns all spatial and connection-admission units charged against the shared page budget.
    #[must_use]
    pub const fn total_work_units(self) -> u64 {
        self.spatial_work_units()
            .saturating_add(self.admission_work_units())
    }

    fn record_admission(&mut self, admission: ConnectionAdmissionWork) {
        let before = self.admission_work_units();
        let expected = admission.total();
        self.admission_base_rules_examined = self
            .admission_base_rules_examined
            .saturating_add(admission.base_rules_examined);
        self.admission_incident_buckets_examined = self
            .admission_incident_buckets_examined
            .saturating_add(admission.incident_buckets_examined);
        self.admission_incident_entries_examined = self
            .admission_incident_entries_examined
            .saturating_add(admission.incident_entries_examined);
        self.admission_cycle_nodes_examined = self
            .admission_cycle_nodes_examined
            .saturating_add(admission.cycle_nodes_examined);
        self.admission_cycle_ports_examined = self
            .admission_cycle_ports_examined
            .saturating_add(admission.cycle_ports_examined);
        self.admission_cycle_buckets_examined = self
            .admission_cycle_buckets_examined
            .saturating_add(admission.cycle_buckets_examined);
        self.admission_cycle_edges_examined = self
            .admission_cycle_edges_examined
            .saturating_add(admission.cycle_edges_examined);
        debug_assert_eq!(self.admission_work_units().saturating_sub(before), expected);
    }
}

#[derive(Debug)]
pub(crate) enum ConnectionProbeQueryOutcome {
    Complete {
        probe: ConnectionProbe,
        receipt: ConnectionProbeQueryReceipt,
    },
    Incomplete {
        cursor: Box<ConnectionProbeCursor>,
        receipt: ConnectionProbeQueryReceipt,
    },
    BudgetExhausted {
        receipt: ConnectionProbeQueryReceipt,
    },
}

impl ConnectionProbeQueryOutcome {
    pub(crate) fn receipt(&self) -> ConnectionProbeQueryReceipt {
        match self {
            Self::Complete { receipt, .. }
            | Self::Incomplete { receipt, .. }
            | Self::BudgetExhausted { receipt } => *receipt,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum ConnectionProbeCursorStaleReason {
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
    pub(crate) const fn is_restartable(self) -> bool {
        matches!(
            self,
            Self::ViewportTransform | Self::GraphRevision | Self::PortGeometry | Self::PaintOrder
        )
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ConnectionProbeQueryError {
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

impl IndexedPort {
    fn from_geometry(geometry: &PublishedPortGeometry) -> Option<Self> {
        Some(Self {
            node: geometry.node(),
            hit_bounds: geometry.hit_bounds()?,
            route_anchor: geometry.route_anchor(),
            outward: geometry.outward(),
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct PortProbeCandidate {
    target: ConnectionProbeTarget,
    direct_hit: bool,
    preferred: bool,
    opposite_direction: bool,
    paint_order: (bool, bool, bool, usize, NodeId),
}

pub(super) enum InternalProbeOutcome {
    Complete {
        cursor: Box<ConnectionProbeCursor>,
        candidate: Option<PortProbeCandidate>,
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

impl PortGeometryIndex {
    pub(super) fn upsert(&mut self, geometry: &PublishedPortGeometry) -> bool {
        let port = geometry.port();
        let Some(indexed) = IndexedPort::from_geometry(geometry) else {
            return self.remove(port);
        };
        if self.indexed.get(&port) == Some(&indexed) {
            return false;
        }
        self.spatial.upsert(
            port,
            bounded_spatial_cells_for_rect(indexed.hit_bounds, MAX_CELLS_PER_PORT),
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

    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.indexed.is_empty() && self.spatial.is_empty()
    }

    pub(super) fn index_epoch(&self) -> u128 {
        self.index_epoch
    }

    /// Invalidates resumable queries after an external visibility gate changes.
    ///
    /// The retained spatial membership is intentionally left intact. Callers may hide an entire
    /// owner in constant work and reject its entries while advancing a query.
    pub(super) fn invalidate_visibility(&mut self) {
        self.index_epoch = self.index_epoch.saturating_add(1);
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn new_cursor(
        &self,
        identity: ViewQueryIdentity,
        graph_revision: GraphRevision,
        order_revision: u128,
        intent: ConnectionGestureBinding,
        pointer: ViewPoint,
        targeting: ConnectionProbeTargeting,
        transform: ViewportTransform,
    ) -> Option<Box<ConnectionProbeCursor>> {
        if !pointer.is_finite()
            || ConnectionProbeTargeting::new(
                targeting.acquire_radius_in_view(),
                targeting.release_radius_in_view(),
            )
            .is_none()
        {
            return None;
        }
        let canvas_pointer = pointer.to_canvas_point(transform)?;
        let acquire_radius_in_canvas = targeting.acquire_radius_in_view() / transform.zoom;
        let release_radius_in_canvas = targeting.release_radius_in_view() / transform.zoom;
        if !acquire_radius_in_canvas.is_finite()
            || acquire_radius_in_canvas <= 0.0
            || !release_radius_in_canvas.is_finite()
            || release_radius_in_canvas < acquire_radius_in_canvas
        {
            return None;
        }
        let query_bounds = CanvasRect {
            origin: CanvasPoint {
                x: canvas_pointer.x - release_radius_in_canvas,
                y: canvas_pointer.y - release_radius_in_canvas,
            },
            size: CanvasSize {
                width: release_radius_in_canvas * 2.0,
                height: release_radius_in_canvas * 2.0,
            },
        };
        if !query_bounds.is_positive_finite() {
            return None;
        }
        Some(Box::new(ConnectionProbeCursor {
            identity,
            graph_revision,
            index_epoch: self.index_epoch,
            order_revision,
            intent,
            canvas_pointer,
            targeting,
            acquire_radius_in_canvas,
            release_radius_in_canvas,
            spatial_query: SpatialQueryCursor::new(query_bounds, MAX_QUERY_CELLS_PER_LEVEL),
            active_source: None,
            next_port: None,
            checked: BTreeSet::new(),
            best: None,
            admission: None,
            total_entries_examined: 0,
            total_spatial_work: 0,
        }))
    }

    pub(super) fn query_page(
        &self,
        graph: &jellyflow_core::core::Graph,
        port_instance: impl Fn(PortId) -> Option<PortInstanceKey>,
        node_draw_order_ranks: &BTreeMap<NodeId, usize>,
        selected_nodes: &std::collections::HashSet<NodeId>,
        recent_interaction_node: Option<NodeId>,
        mut cursor: Box<ConnectionProbeCursor>,
        entry_budget: usize,
    ) -> InternalProbeOutcome {
        let mut work = ConnectionProbeWork::default();
        let mut remaining = entry_budget;
        loop {
            let Some(port) = cursor.next_port else {
                if remaining == 0 {
                    return InternalProbeOutcome::Incomplete { cursor, work };
                }
                if cursor.total_spatial_work >= HARD_PORT_QUERY_WORK_LIMIT {
                    return InternalProbeOutcome::BudgetExhausted { work };
                }
                remaining -= 1;
                cursor.total_spatial_work = cursor.total_spatial_work.saturating_add(1);
                match self.spatial.advance_query(&mut cursor.spatial_query) {
                    SpatialQueryStep::LevelInitialized => {
                        work.spatial_level_checks = work.spatial_level_checks.saturating_add(1);
                    }
                    SpatialQueryStep::Source(source) => {
                        work.spatial_source_setups = work.spatial_source_setups.saturating_add(1);
                        if matches!(source, SpatialQuerySource::Cell(_)) {
                            work.cells_examined = work.cells_examined.saturating_add(1);
                        }
                        cursor.active_source = Some(source);
                        cursor.next_port = self
                            .spatial
                            .source(source)
                            .and_then(|ports| ports.iter().next().copied());
                        if cursor.next_port.is_none() {
                            cursor.active_source = None;
                        }
                    }
                    SpatialQueryStep::Exhausted => {
                        work.spatial_level_checks = work.spatial_level_checks.saturating_add(1);
                        let candidate = cursor.best;
                        return InternalProbeOutcome::Complete {
                            cursor,
                            candidate,
                            work,
                        };
                    }
                }
                continue;
            };
            if remaining == 0 {
                return InternalProbeOutcome::Incomplete { cursor, work };
            }
            if cursor.total_entries_examined >= HARD_PORT_QUERY_ENTRY_LIMIT
                || cursor.total_spatial_work >= HARD_PORT_QUERY_WORK_LIMIT
            {
                return InternalProbeOutcome::BudgetExhausted { work };
            }
            remaining -= 1;
            cursor.total_entries_examined = cursor.total_entries_examined.saturating_add(1);
            cursor.total_spatial_work = cursor.total_spatial_work.saturating_add(1);
            work.entries_examined = work.entries_examined.saturating_add(1);
            let source = cursor
                .active_source
                .expect("a pending port always belongs to an active spatial source");
            cursor.next_port = self
                .spatial
                .source(source)
                .and_then(|ports| ports.range((Excluded(port), Unbounded)).next().copied());
            if cursor.next_port.is_none() {
                cursor.active_source = None;
            }
            if !cursor.checked.insert(port) || port == cursor.intent.fixed_port_instance().port() {
                continue;
            }
            let Some(indexed) = self.indexed.get(&port).copied() else {
                continue;
            };
            let Some(instance) = port_instance(port) else {
                continue;
            };
            if instance.node() != indexed.node {
                continue;
            }
            work.exact_tests = work.exact_tests.saturating_add(1);
            let direct_hit = rect_contains(indexed.hit_bounds, cursor.canvas_pointer);
            let hit_distance_in_canvas =
                point_rect_distance(cursor.canvas_pointer, indexed.hit_bounds);
            let distance_in_canvas = point_distance(indexed.route_anchor, cursor.canvas_pointer);
            let identity = ConnectionProbeTargetIdentity::new(instance);
            let preferred = cursor.targeting.preferred() == Some(identity);
            let radius_in_canvas = if preferred {
                cursor.release_radius_in_canvas
            } else {
                cursor.acquire_radius_in_canvas
            };
            if !direct_hit
                && (distance_in_canvas > radius_in_canvas
                    || hit_distance_in_canvas > radius_in_canvas)
            {
                continue;
            }
            let distance_in_view = distance_in_canvas
                * (cursor.targeting.release_radius_in_view() / cursor.release_radius_in_canvas);
            let source_direction = graph
                .port(&cursor.intent.fixed_port_instance().port())
                .map(|source| source.dir);
            let target_direction = graph.port(&port).map(|target| target.dir);
            let opposite_direction = matches!(
                (source_direction, target_direction),
                (Some(PortDirection::In), Some(PortDirection::Out))
                    | (Some(PortDirection::Out), Some(PortDirection::In))
            );
            let candidate = PortProbeCandidate {
                target: ConnectionProbeTarget::new(
                    instance,
                    indexed.route_anchor,
                    indexed.outward,
                    distance_in_view,
                ),
                direct_hit,
                preferred,
                opposite_direction,
                paint_order: render_order_key(
                    indexed.node,
                    selected_nodes.contains(&indexed.node),
                    recent_interaction_node == Some(indexed.node),
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
    #[cfg(test)]
    pub(crate) fn connection_probe(
        &self,
        query: ConnectionProbeQuery,
    ) -> Result<ConnectionProbeQueryOutcome, ConnectionProbeQueryError> {
        let runtime = self.runtime()?;
        connection_probe_page(&self.shared, &runtime, query)
    }
}

impl WeakViewSession {
    pub(crate) fn connection_probe(
        &self,
        query: ConnectionProbeQuery,
    ) -> Result<ConnectionProbeQueryOutcome, ConnectionProbeQueryError> {
        let (view, runtime) = self.open_capability()?;
        connection_probe_page(&view, &runtime, query)
    }
}

fn connection_probe_page(
    view: &ViewShared,
    runtime: &Rc<RuntimeShared>,
    query: ConnectionProbeQuery,
) -> Result<ConnectionProbeQueryOutcome, ConnectionProbeQueryError> {
    let graph = runtime.snapshot().map_err(ViewSessionError::from)?;
    let entry_budget = query.entry_budget;
    if let ConnectionProbeRequest::Start { intent, .. } = &query.request {
        if intent.provenance() != graph.provenance() {
            return Err(ViewSessionError::ForeignInteractionFacts.into());
        }
        let policy = runtime.policy().map_err(ViewSessionError::from)?;
        if !intent.is_interactive(&graph, policy) {
            return Err(ViewSessionError::StaleInteractionFacts.into());
        }
    }
    let (internal, proof) = {
        let mut state = view
            .state
            .try_borrow_mut()
            .map_err(|_| ViewSessionError::Busy)?;
        ensure_open(state.status)?;
        let transform = ViewportTransform::new(state.facts.pan, state.facts.zoom)
            .ok_or(ViewSessionError::InvalidViewport)?;
        let (cursor, fixed_endpoint_visible) = match query.request {
            ConnectionProbeRequest::Start {
                intent,
                pointer,
                targeting,
            } => {
                let fixed_endpoint_visible = state
                    .derived
                    .connection_endpoint_owner_is_frame_visible(intent.fixed_port_instance());
                let cursor = state
                    .derived
                    .new_connection_probe_cursor(
                        state.query_identity(),
                        state.seen_revision,
                        state.interaction_order_revision,
                        intent,
                        pointer,
                        targeting,
                        transform,
                    )
                    .ok_or(ConnectionProbeQueryError::InvalidQuery)?;
                (cursor, fixed_endpoint_visible)
            }
            ConnectionProbeRequest::Resume(cursor) => {
                validate_probe_cursor(&state, &cursor)?;
                (cursor, true)
            }
        };
        let proof = InteractionQueryProof::issue(
            query.interaction_binding,
            cursor.identity,
            [
                u128::from(cursor.graph_revision),
                cursor.index_epoch,
                cursor.order_revision,
                0,
            ],
        )?;
        if graph.revision() != state.seen_revision {
            return Err(ConnectionProbeQueryError::StaleCursor(
                ConnectionProbeCursorStaleReason::GraphRevision,
            ));
        }
        let state = &mut *state;
        let internal = if !fixed_endpoint_visible {
            InternalProbeOutcome::Complete {
                cursor,
                candidate: None,
                work: ConnectionProbeWork::default(),
            }
        } else if cursor.admission.is_some() {
            let candidate = cursor.best;
            InternalProbeOutcome::Complete {
                cursor,
                candidate,
                work: ConnectionProbeWork::default(),
            }
        } else {
            state.derived.query_connection_probe_page(
                &graph,
                &state.node_draw_order_ranks,
                &state.selected_node_membership,
                state.recent_interaction_node_id(),
                cursor,
                entry_budget,
            )
        };
        (internal, proof)
    };

    match internal {
        InternalProbeOutcome::Complete {
            mut cursor,
            candidate,
            mut work,
        } => {
            let Some(candidate) = candidate else {
                return Ok(ConnectionProbeQueryOutcome::Complete {
                    probe: ConnectionProbe::None,
                    receipt: ConnectionProbeQueryReceipt::new(work, proof),
                });
            };
            if cursor.admission.is_none() {
                let edit = cursor.intent.edit_for_target(candidate.target.port());
                cursor.admission = Some(ConnectionAdmissionCursor::new(edit));
            }
            let spatial_units = usize::try_from(work.spatial_work_units()).unwrap_or(usize::MAX);
            let remaining = entry_budget.saturating_sub(spatial_units);
            let admission = runtime
                .advance_connection_admission(
                    cursor
                        .admission
                        .as_mut()
                        .expect("connection admission was initialized above"),
                    graph.version(),
                    remaining,
                )
                .map_err(ViewSessionError::from)?;
            match admission {
                ConnectionAdmissionOutcome::Stale => Err(ConnectionProbeQueryError::StaleCursor(
                    ConnectionProbeCursorStaleReason::GraphRevision,
                )),
                ConnectionAdmissionOutcome::Incomplete {
                    work: admission_work,
                } => {
                    work.record_admission(admission_work);
                    Ok(ConnectionProbeQueryOutcome::Incomplete {
                        cursor,
                        receipt: ConnectionProbeQueryReceipt::new(work, proof),
                    })
                }
                ConnectionAdmissionOutcome::BudgetExhausted {
                    work: admission_work,
                } => {
                    work.record_admission(admission_work);
                    Ok(ConnectionProbeQueryOutcome::BudgetExhausted {
                        receipt: ConnectionProbeQueryReceipt::new(work, proof),
                    })
                }
                ConnectionAdmissionOutcome::Invalid {
                    version,
                    diagnostics,
                    work: admission_work,
                } => {
                    work.record_admission(admission_work);
                    Ok(ConnectionProbeQueryOutcome::Complete {
                        probe: ConnectionProbe::Invalid {
                            version,
                            target: candidate.target,
                            diagnostics,
                        },
                        receipt: ConnectionProbeQueryReceipt::new(work, proof),
                    })
                }
                ConnectionAdmissionOutcome::Accepted {
                    version,
                    deferred,
                    work: admission_work,
                } => {
                    work.record_admission(admission_work);
                    let probe = if deferred {
                        ConnectionProbe::Deferred {
                            version,
                            target: candidate.target,
                        }
                    } else {
                        ConnectionProbe::Valid {
                            version,
                            target: candidate.target,
                        }
                    };
                    Ok(ConnectionProbeQueryOutcome::Complete {
                        probe,
                        receipt: ConnectionProbeQueryReceipt::new(work, proof),
                    })
                }
            }
        }
        InternalProbeOutcome::Incomplete { cursor, work } => {
            Ok(ConnectionProbeQueryOutcome::Incomplete {
                cursor,
                receipt: ConnectionProbeQueryReceipt::new(work, proof),
            })
        }
        InternalProbeOutcome::BudgetExhausted { work } => {
            Ok(ConnectionProbeQueryOutcome::BudgetExhausted {
                receipt: ConnectionProbeQueryReceipt::new(work, proof),
            })
        }
    }
}

fn validate_probe_cursor(
    state: &super::ViewState,
    cursor: &ConnectionProbeCursor,
) -> Result<(), ConnectionProbeQueryError> {
    probe_cursor_stale_reason(
        state.query_identity(),
        state.seen_revision,
        state.derived.connection_probe_index_epoch(),
        state.interaction_order_revision,
        cursor,
    )
    .map(ConnectionProbeQueryError::StaleCursor)
    .map_or(Ok(()), Err)
}

fn probe_cursor_stale_reason(
    identity: ViewQueryIdentity,
    graph_revision: GraphRevision,
    index_epoch: u128,
    order_revision: u128,
    cursor: &ConnectionProbeCursor,
) -> Option<ConnectionProbeCursorStaleReason> {
    match cursor.identity.stale_reason(identity) {
        Some(ViewQueryIdentityStaleReason::ForeignView) => {
            Some(ConnectionProbeCursorStaleReason::ForeignView)
        }
        Some(ViewQueryIdentityStaleReason::ViewGeneration) => {
            Some(ConnectionProbeCursorStaleReason::ViewGeneration)
        }
        Some(ViewQueryIdentityStaleReason::ViewportTransform) => {
            Some(ConnectionProbeCursorStaleReason::ViewportTransform)
        }
        None if cursor.graph_revision != graph_revision => {
            Some(ConnectionProbeCursorStaleReason::GraphRevision)
        }
        None if cursor.index_epoch != index_epoch => {
            Some(ConnectionProbeCursorStaleReason::PortGeometry)
        }
        None if cursor.order_revision != order_revision => {
            Some(ConnectionProbeCursorStaleReason::PaintOrder)
        }
        None => None,
    }
}

fn port_candidate_is_better(candidate: PortProbeCandidate, current: PortProbeCandidate) -> bool {
    if candidate.direct_hit != current.direct_hit {
        return candidate.direct_hit;
    }
    if !candidate.direct_hit && candidate.preferred != current.preferred {
        return candidate.preferred;
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

fn rect_contains(bounds: CanvasRect, point: CanvasPoint) -> bool {
    point.x >= bounds.origin.x
        && point.y >= bounds.origin.y
        && point.x <= bounds.origin.x + bounds.size.width
        && point.y <= bounds.origin.y + bounds.size.height
}

fn point_rect_distance(point: CanvasPoint, bounds: CanvasRect) -> f32 {
    let right = bounds.origin.x + bounds.size.width;
    let bottom = bounds.origin.y + bounds.size.height;
    let dx = if point.x < bounds.origin.x {
        bounds.origin.x - point.x
    } else if point.x > right {
        point.x - right
    } else {
        0.0
    };
    let dy = if point.y < bounds.origin.y {
        bounds.origin.y - point.y
    } else if point.y > bottom {
        point.y - bottom
    } else {
        0.0
    };
    dx.mul_add(dx, dy * dy).sqrt()
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
    use crate::runtime::viewport::ViewportRevision;

    #[test]
    fn probe_cursor_identity_and_epoch_staleness_matrix() {
        let index = PortGeometryIndex::default();
        let cursor = cursor(&index, ViewPoint::new(0.0, 0.0), 8.0, 1.0);
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
        for (current_identity, graph_revision, index_epoch, order_revision, expected) in [
            (
                foreign_identity,
                cursor.graph_revision,
                cursor.index_epoch,
                cursor.order_revision,
                Some(ConnectionProbeCursorStaleReason::ForeignView),
            ),
            (
                next_generation,
                cursor.graph_revision,
                cursor.index_epoch,
                cursor.order_revision,
                Some(ConnectionProbeCursorStaleReason::ViewGeneration),
            ),
            (
                next_transform,
                cursor.graph_revision,
                cursor.index_epoch,
                cursor.order_revision,
                Some(ConnectionProbeCursorStaleReason::ViewportTransform),
            ),
            (
                identity,
                cursor.graph_revision.saturating_add(1),
                cursor.index_epoch,
                cursor.order_revision,
                Some(ConnectionProbeCursorStaleReason::GraphRevision),
            ),
            (
                identity,
                cursor.graph_revision,
                cursor.index_epoch.saturating_add(1),
                cursor.order_revision,
                Some(ConnectionProbeCursorStaleReason::PortGeometry),
            ),
            (
                identity,
                cursor.graph_revision,
                cursor.index_epoch,
                cursor.order_revision.saturating_add(1),
                Some(ConnectionProbeCursorStaleReason::PaintOrder),
            ),
            (
                identity,
                cursor.graph_revision,
                cursor.index_epoch,
                cursor.order_revision,
                None,
            ),
        ] {
            assert_eq!(
                probe_cursor_stale_reason(
                    current_identity,
                    graph_revision,
                    index_epoch,
                    order_revision,
                    &cursor,
                ),
                expected,
            );
        }
    }

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
    fn recent_selected_node_elevation_also_controls_overlapping_port_hits() {
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
        index.upsert(&geometry(second_node, second_port, 100.0, 100.0));
        let ranks = BTreeMap::from([(first_node, 0), (second_node, 1)]);
        let selected = HashSet::from([first_node, second_node]);

        let baseline = index.query_page(
            &graph,
            |port| test_port_instance(&graph, port),
            &ranks,
            &selected,
            None,
            cursor(&index, ViewPoint::new(100.0, 100.0), 12.0, 1.0),
            32,
        );
        assert_eq!(completed_candidate(baseline), Some(second_port));

        let elevated = index.query_page(
            &graph,
            |port| test_port_instance(&graph, port),
            &ranks,
            &selected,
            Some(first_node),
            cursor(&index, ViewPoint::new(100.0, 100.0), 12.0, 1.0),
            32,
        );
        assert_eq!(completed_candidate(elevated), Some(first_port));
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
    fn preferred_target_uses_release_band_but_direct_hit_still_wins() {
        let first_node = NodeId::from_u128(2);
        let second_node = NodeId::from_u128(3);
        let first_port = PortId::from_u128(2);
        let second_port = PortId::from_u128(3);
        let graph = graph_with_ports(&[
            (source_node(), source_port(), PortDirection::Out),
            (first_node, first_port, PortDirection::In),
            (second_node, second_port, PortDirection::In),
        ]);
        let preferred = ConnectionProbeTargetIdentity::new(PortInstanceKey::unavailable(
            first_node, first_port,
        ));
        for zoom in [0.5, 1.0, 2.0] {
            let first_x = 100.0;
            let second_x = first_x + 12.0 / zoom;
            let pointer_x = first_x * zoom + 7.0;
            let small_hit = |node, port, x| {
                geometry_for_zoom(node, port, x, 100.0, zoom).with_hit_bounds_for_test(Some(
                    CanvasRect {
                        origin: CanvasPoint {
                            x: x - 1.0 / zoom,
                            y: 100.0 - 1.0 / zoom,
                        },
                        size: CanvasSize {
                            width: 2.0 / zoom,
                            height: 2.0 / zoom,
                        },
                    },
                ))
            };
            let mut index = PortGeometryIndex::default();
            index.upsert(&small_hit(first_node, first_port, first_x));
            index.upsert(&small_hit(second_node, second_port, second_x));

            let acquired_without_lock = query_with_targeting(
                &index,
                &graph,
                &BTreeMap::new(),
                ViewPoint::new(pointer_x, 100.0 * zoom),
                ConnectionProbeTargeting::new(6.0, 10.0).unwrap(),
                zoom,
                32,
            );
            assert_eq!(
                completed_candidate(acquired_without_lock),
                Some(second_port)
            );

            let retained = query_with_targeting(
                &index,
                &graph,
                &BTreeMap::new(),
                ViewPoint::new(pointer_x, 100.0 * zoom),
                ConnectionProbeTargeting::new(6.0, 10.0)
                    .unwrap()
                    .with_preferred(preferred),
                zoom,
                32,
            );
            assert_eq!(completed_candidate(retained), Some(first_port));

            let direct = query_with_targeting(
                &index,
                &graph,
                &BTreeMap::new(),
                ViewPoint::new(second_x * zoom, 100.0 * zoom),
                ConnectionProbeTargeting::new(6.0, 14.0)
                    .unwrap()
                    .with_preferred(preferred),
                zoom,
                32,
            );
            assert_eq!(completed_candidate(direct), Some(second_port));
        }
    }

    #[test]
    fn replaced_preferred_target_loses_release_authority() {
        let target_node = NodeId::from_u128(2);
        let target_port = PortId::from_u128(2);
        let graph = graph_with_ports(&[
            (source_node(), source_port(), PortDirection::Out),
            (target_node, target_port, PortDirection::In),
        ]);
        let mut index = PortGeometryIndex::default();
        index.upsert(
            &geometry(target_node, target_port, 100.0, 100.0).with_hit_bounds_for_test(Some(
                CanvasRect {
                    origin: CanvasPoint { x: 99.0, y: 99.0 },
                    size: CanvasSize {
                        width: 2.0,
                        height: 2.0,
                    },
                },
            )),
        );
        let stale = ConnectionProbeTargetIdentity::new(PortInstanceKey::unavailable(
            NodeId::from_u128(999),
            target_port,
        ));
        let cursor = index
            .new_cursor(
                ViewQueryIdentity::new(
                    ViewId::unavailable(),
                    ViewGeneration::initial(),
                    ViewportRevision::initial(),
                ),
                0,
                0,
                ConnectionGestureBinding::connect_for_test(
                    test_edge(),
                    source_port(),
                    NodeGraphConnectionMode::Strict,
                ),
                ViewPoint::new(108.0, 100.0),
                ConnectionProbeTargeting::new(6.0, 10.0)
                    .unwrap()
                    .with_preferred(stale),
                ViewportTransform::new(CanvasPoint::default(), 1.0).unwrap(),
            )
            .unwrap();
        let outcome = index.query_page(
            &graph,
            |port| test_port_instance(&graph, port),
            &BTreeMap::new(),
            &HashSet::new(),
            None,
            cursor,
            32,
        );

        assert_eq!(completed_candidate(outcome), None);
    }

    #[test]
    fn zero_budget_does_not_initialize_spatial_query_work() {
        let target_node = NodeId::from_u128(2);
        let target_port = PortId::from_u128(2);
        let graph = graph_with_ports(&[
            (source_node(), source_port(), PortDirection::Out),
            (target_node, target_port, PortDirection::In),
        ]);
        let mut index = PortGeometryIndex::default();
        index.upsert(&geometry(target_node, target_port, 100.0, 100.0));
        let cursor = cursor(&index, ViewPoint::new(100.0, 100.0), 12.0, 1.0);

        let InternalProbeOutcome::Incomplete { cursor, work } = index.query_page(
            &graph,
            |port| test_port_instance(&graph, port),
            &BTreeMap::new(),
            &HashSet::new(),
            None,
            cursor,
            0,
        ) else {
            panic!("zero budget must preserve resumable work");
        };
        assert_eq!(work.spatial_work_units(), 0);
        assert_eq!(work.spatial_level_checks, 0);
        assert_eq!(work.spatial_source_setups, 0);
        assert_eq!(work.cells_examined, 0);
        assert_eq!(cursor.total_spatial_work, 0);

        let InternalProbeOutcome::Incomplete { cursor, work } = index.query_page(
            &graph,
            |port| test_port_instance(&graph, port),
            &BTreeMap::new(),
            &HashSet::new(),
            None,
            cursor,
            1,
        ) else {
            panic!("one unit initializes only the first occupied level");
        };
        assert_eq!(work.spatial_level_checks, 1);
        assert_eq!(work.spatial_source_setups, 0);
        assert_eq!(work.entries_examined, 0);

        let resumed = index.query_page(
            &graph,
            |port| test_port_instance(&graph, port),
            &BTreeMap::new(),
            &HashSet::new(),
            None,
            cursor,
            3,
        );
        assert_eq!(completed_candidate(resumed), Some(target_port));
    }

    #[test]
    fn one_unit_pages_receipt_every_spatial_setup_and_entry() {
        let target_node = NodeId::from_u128(2);
        let target_port = PortId::from_u128(2);
        let graph = graph_with_ports(&[
            (source_node(), source_port(), PortDirection::Out),
            (target_node, target_port, PortDirection::In),
        ]);
        let mut index = PortGeometryIndex::default();
        index.upsert(&geometry(target_node, target_port, 100.0, 100.0));
        let mut cursor = cursor(&index, ViewPoint::new(100.0, 100.0), 12.0, 1.0);
        let mut total = ConnectionProbeWork::default();
        let mut pages = 0;

        loop {
            let outcome = index.query_page(
                &graph,
                |port| test_port_instance(&graph, port),
                &BTreeMap::new(),
                &HashSet::new(),
                None,
                cursor,
                1,
            );
            let (next, candidate, work) = match outcome {
                InternalProbeOutcome::Complete {
                    candidate, work, ..
                } => (None, candidate, work),
                InternalProbeOutcome::Incomplete { cursor, work } => (Some(cursor), None, work),
                InternalProbeOutcome::BudgetExhausted { .. } => {
                    panic!("a local query cannot exhaust its hard budget")
                }
            };
            assert_eq!(work.spatial_work_units(), 1);
            accumulate_spatial_work(&mut total, work);
            pages += 1;
            let Some(next) = next else {
                assert_eq!(
                    candidate.map(|candidate| candidate.target.port()),
                    Some(target_port)
                );
                break;
            };
            cursor = next;
        }

        assert_eq!(pages, 4);
        assert_eq!(total.spatial_level_checks, 2);
        assert_eq!(total.spatial_source_setups, 1);
        assert_eq!(total.cells_examined, 1);
        assert_eq!(total.entries_examined, 1);
        assert_eq!(total.exact_tests, 1);
    }

    #[test]
    fn multi_level_and_whole_level_probe_is_invariant_between_page_splits() {
        let fine_node = NodeId::from_u128(2);
        let coarse_node = NodeId::from_u128(3);
        let fine_port = PortId::from_u128(2);
        let coarse_port = PortId::from_u128(3);
        let graph = graph_with_ports(&[
            (source_node(), source_port(), PortDirection::Out),
            (fine_node, fine_port, PortDirection::In),
            (coarse_node, coarse_port, PortDirection::In),
        ]);
        let mut index = PortGeometryIndex::default();
        index.upsert(&geometry(fine_node, fine_port, 100.0, 100.0));
        index.upsert(
            &geometry(coarse_node, coarse_port, 102.0, 100.0).with_hit_bounds_for_test(Some(
                CanvasRect {
                    origin: CanvasPoint {
                        x: -5_000.0,
                        y: -5_000.0,
                    },
                    size: CanvasSize {
                        width: 10_000.0,
                        height: 10_000.0,
                    },
                },
            )),
        );

        let InternalProbeOutcome::Complete {
            candidate: single_candidate,
            work: single_work,
            ..
        } = query(
            &index,
            &graph,
            &BTreeMap::new(),
            ViewPoint::new(100.0, 100.0),
            5_000.0,
            1.0,
            64,
        )
        else {
            panic!("the multi-level query must fit in one large page");
        };

        let mut split_cursor = cursor(&index, ViewPoint::new(100.0, 100.0), 5_000.0, 1.0);
        let mut split_work = ConnectionProbeWork::default();
        let split_candidate = loop {
            match index.query_page(
                &graph,
                |port| test_port_instance(&graph, port),
                &BTreeMap::new(),
                &HashSet::new(),
                None,
                split_cursor,
                1,
            ) {
                InternalProbeOutcome::Complete {
                    candidate, work, ..
                } => {
                    accumulate_spatial_work(&mut split_work, work);
                    break candidate;
                }
                InternalProbeOutcome::Incomplete { cursor, work } => {
                    accumulate_spatial_work(&mut split_work, work);
                    split_cursor = cursor;
                }
                InternalProbeOutcome::BudgetExhausted { .. } => {
                    panic!("split pages cannot change the hard-budget outcome")
                }
            }
        };

        assert_eq!(
            split_candidate.map(|candidate| candidate.target.port()),
            single_candidate.map(|candidate| candidate.target.port())
        );
        assert_eq!(split_work, single_work);
        assert_eq!(single_work.spatial_level_checks, 3);
        assert_eq!(single_work.spatial_source_setups, 17);
        assert_eq!(single_work.cells_examined, 16);
        assert_eq!(single_work.entries_examined, 17);
        assert_eq!(single_work.exact_tests, 2);
        assert_eq!(single_work.spatial_work_units(), 37);
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
    fn fully_clipped_port_leaves_probe_index_and_returns_with_same_route_geometry() {
        let target_node = NodeId::from_u128(2);
        let target_port = PortId::from_u128(2);
        let graph = graph_with_ports(&[
            (source_node(), source_port(), PortDirection::Out),
            (target_node, target_port, PortDirection::In),
        ]);
        let visible = geometry(target_node, target_port, 100.0, 100.0);
        let clipped = visible.clone().with_hit_bounds_for_test(None);
        let mut index = PortGeometryIndex::default();

        assert!(index.upsert(&visible));
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
            Some(target_port)
        );

        assert!(index.upsert(&clipped));
        assert_eq!(clipped.port(), visible.port());
        assert_eq!(clipped.route_anchor(), visible.route_anchor());
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

        assert!(index.upsert(&visible));
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
            Some(target_port)
        );
    }

    #[test]
    fn proximity_probe_is_bounded_by_clipped_interaction_region() {
        let target_node = NodeId::from_u128(2);
        let target_port = PortId::from_u128(2);
        let graph = graph_with_ports(&[
            (source_node(), source_port(), PortDirection::Out),
            (target_node, target_port, PortDirection::In),
        ]);
        let clipped = geometry(target_node, target_port, 100.0, 100.0).with_hit_bounds_for_test(
            Some(CanvasRect {
                origin: CanvasPoint { x: 104.0, y: 96.0 },
                size: CanvasSize {
                    width: 6.0,
                    height: 8.0,
                },
            }),
        );
        let mut index = PortGeometryIndex::default();
        assert!(index.upsert(&clipped));

        assert_eq!(
            completed_candidate(query(
                &index,
                &graph,
                &BTreeMap::new(),
                ViewPoint::new(91.0, 100.0),
                4.0,
                1.0,
                32,
            )),
            None
        );
        assert_eq!(
            completed_candidate(query(
                &index,
                &graph,
                &BTreeMap::new(),
                ViewPoint::new(106.0, 100.0),
                4.0,
                1.0,
                32,
            )),
            Some(target_port)
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

    fn accumulate_spatial_work(total: &mut ConnectionProbeWork, page: ConnectionProbeWork) {
        total.spatial_level_checks = total
            .spatial_level_checks
            .saturating_add(page.spatial_level_checks);
        total.spatial_source_setups = total
            .spatial_source_setups
            .saturating_add(page.spatial_source_setups);
        total.cells_examined = total.cells_examined.saturating_add(page.cells_examined);
        total.entries_examined = total.entries_examined.saturating_add(page.entries_examined);
        total.exact_tests = total.exact_tests.saturating_add(page.exact_tests);
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
        query_with_targeting(
            index,
            graph,
            ranks,
            pointer,
            ConnectionProbeTargeting::new(radius, radius).expect("valid target radius"),
            zoom,
            budget,
        )
    }

    fn query_with_targeting(
        index: &PortGeometryIndex,
        graph: &Graph,
        ranks: &BTreeMap<NodeId, usize>,
        pointer: ViewPoint,
        targeting: ConnectionProbeTargeting,
        zoom: f32,
        budget: usize,
    ) -> InternalProbeOutcome {
        let cursor = index
            .new_cursor(
                ViewQueryIdentity::new(
                    ViewId::unavailable(),
                    ViewGeneration::initial(),
                    ViewportRevision::initial(),
                ),
                0,
                0,
                ConnectionGestureBinding::connect_for_test(
                    test_edge(),
                    source_port(),
                    NodeGraphConnectionMode::Strict,
                ),
                pointer,
                targeting,
                ViewportTransform::new(CanvasPoint::default(), zoom).expect("valid transform"),
            )
            .expect("valid query");
        index.query_page(
            graph,
            |port| test_port_instance(graph, port),
            ranks,
            &HashSet::new(),
            None,
            cursor,
            budget,
        )
    }

    fn cursor(
        index: &PortGeometryIndex,
        pointer: ViewPoint,
        radius: f32,
        zoom: f32,
    ) -> Box<ConnectionProbeCursor> {
        index
            .new_cursor(
                ViewQueryIdentity::new(
                    ViewId::unavailable(),
                    ViewGeneration::initial(),
                    ViewportRevision::initial(),
                ),
                0,
                0,
                ConnectionGestureBinding::connect_for_test(
                    test_edge(),
                    source_port(),
                    NodeGraphConnectionMode::Strict,
                ),
                pointer,
                ConnectionProbeTargeting::new(radius, radius).expect("valid target radius"),
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

    fn test_port_instance(graph: &Graph, port: PortId) -> Option<PortInstanceKey> {
        graph
            .port(&port)
            .map(|model| PortInstanceKey::unavailable(model.node, port))
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
            Some(CanvasRect {
                origin: CanvasPoint {
                    x: x - hit_half,
                    y: y - hit_half,
                },
                size: CanvasSize {
                    width: hit_half * 2.0,
                    height: hit_half * 2.0,
                },
            }),
            CanvasPoint { x, y },
            HandlePosition::Left,
        )
    }
}
