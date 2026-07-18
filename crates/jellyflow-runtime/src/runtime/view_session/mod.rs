//! View-local capability issued by an authoritative [`GraphRuntime`](super::graph_runtime::GraphRuntime).

mod edge_pick;
mod facts;
mod geometry;
mod port_geometry;
mod protocol;
mod route_geometry;
mod spatial_grid;
mod subscriptions;

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::rc::{Rc, Weak};
use std::sync::Arc;

use jellyflow_core::core::{CanvasPoint, CanvasRect, EdgeId, Graph, GroupId, NodeId, PortId};
use jellyflow_core::interaction::NodeGraphConnectionMode;

use crate::io::{
    ViewSelectionSeed, ViewSessionSeed, ViewSessionSeedGraphMismatch, edge_is_visible,
};
use crate::runtime::adapter_protocol::{
    AdapterEnvironment, GeometrySnapshot, PublishedEdgeRoute, PublishedNodeGeometry,
    PublishedPortGeometry, PublishedPortOutcome, RendererStamp, SharedGeometryMap,
};
use crate::runtime::commands::{
    ConnectionEdit, ConnectionEvaluation, GraphCommand, GraphCommandOutcome,
};
use crate::runtime::graph_runtime::{
    CreateFromWireCandidates, CreateFromWireQueryError, GraphRevision, GraphRuntimeError,
    GraphRuntimePolicy, GraphSnapshot, NodeIncarnation, PortConnectionDragIntent,
    PortConnectionFacts, RuntimeFactKey, RuntimeId, RuntimePatch, RuntimePatchKind, RuntimeShared,
};
use crate::runtime::unwind::{PanicCollector, PanicPayload, resume, suppress};
use crate::runtime::viewport::ViewportTransform;
use crate::schema::NodeCatalog;

pub use self::edge_pick::{
    EdgePick, EdgePickCursor, EdgePickCursorStaleReason, EdgePickGeometryFailure, EdgePickQuery,
    EdgePickQueryError, EdgePickQueryOutcome, EdgePickWork,
};
use self::geometry::{AdmissionIndex, ViewSpatialIndex, recompute_routes};
use self::order::index_draw_order;
use self::port_geometry::PortGeometryIndex;
pub use self::port_geometry::{
    ConnectionProbeCursor, ConnectionProbeCursorStaleReason, ConnectionProbeQuery,
    ConnectionProbeQueryError, ConnectionProbeQueryOutcome, ConnectionProbeWork,
};
use self::protocol::PendingFrame;
use self::route_geometry::RouteGeometryIndex;
pub use self::route_geometry::{
    VisibleRouteCursor, VisibleRouteCursorStaleReason, VisibleRouteGeometryOverflow,
    VisibleRouteQuery, VisibleRouteQueryError, VisibleRouteQueryOutcome, VisibleRouteQueryWork,
    VisibleRoutes,
};
pub use self::subscriptions::ViewSubscriptionToken;
use facts::ViewFacts;

mod order;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ViewId {
    runtime: RuntimeId,
    local: u64,
}

impl ViewId {
    pub(crate) fn new(runtime: RuntimeId, local: u64) -> Self {
        Self { runtime, local }
    }

    const fn unavailable() -> Self {
        Self {
            runtime: RuntimeId::UNAVAILABLE,
            local: 0,
        }
    }

    pub fn get(self) -> u64 {
        self.local
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ViewGeneration(u64);

impl ViewGeneration {
    pub(crate) fn initial() -> Self {
        Self(1)
    }

    fn terminal(self) -> Self {
        Self(self.0.saturating_add(1))
    }

    pub fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Default)]
pub struct ViewSessionOptions {
    pub seed: Option<ViewSessionSeed>,
}

impl ViewSessionOptions {
    pub fn with_seed(seed: ViewSessionSeed) -> Self {
        Self { seed: Some(seed) }
    }

    pub(crate) fn sanitized_seed(&self, graph: &Graph) -> Result<ViewSessionSeed, AttachViewError> {
        let Some(seed) = &self.seed else {
            return Ok(ViewSessionSeed::new(graph.graph_id()));
        };
        let mut seed = seed.clone();
        seed.sanitize_for_graph(graph)?;
        Ok(seed)
    }
}

/// Stable prefix of the selected edge IDs for bounded rendering queries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedEdgeIds {
    edges: Vec<EdgeId>,
    total_count: usize,
}

impl SelectedEdgeIds {
    fn from_selection(edges: &[EdgeId], limit: usize) -> Self {
        Self {
            edges: edges.iter().copied().take(limit).collect(),
            total_count: edges.len(),
        }
    }

    /// Returns the selected edge prefix in stable selection order.
    pub fn edges(&self) -> &[EdgeId] {
        &self.edges
    }

    /// Returns the number of selected edges before the prefix limit was applied.
    pub const fn total_count(&self) -> usize {
        self.total_count
    }

    /// Returns whether selected edges were omitted by the requested limit.
    pub fn is_truncated(&self) -> bool {
        self.edges.len() < self.total_count
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AttachViewError {
    #[error("graph runtime is unavailable")]
    RuntimeUnavailable,
    #[error("graph runtime state is currently borrowed")]
    RuntimeBusy,
    #[error("view identity allocator is exhausted")]
    IdentityExhausted,
    #[error(transparent)]
    SeedGraphMismatch(#[from] ViewSessionSeedGraphMismatch),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewCloseReason {
    HostClosed,
    RuntimeDropped,
}

/// Ordered graph commands that a host drains immediately before closing a view.
///
/// An empty request performs a plain terminal close. Layout feedback remains bound to its
/// originating frame and must be submitted through [`ViewSession::submit_feedback`].
#[derive(Debug, Clone, Default)]
pub struct ViewCloseRequest {
    commands: Vec<GraphCommand>,
}

impl ViewCloseRequest {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_command(mut self, command: GraphCommand) -> Self {
        self.commands.push(command);
        self
    }

    pub fn commands(&self) -> &[GraphCommand] {
        &self.commands
    }
}

#[derive(Debug)]
pub enum ViewCloseReceipt {
    Command { outcome: GraphCommandOutcome },
    CommandFailed { error: GraphRuntimeError },
}

impl ViewCloseReceipt {
    pub fn patch(&self) -> Option<&RuntimePatch> {
        match self {
            Self::Command { outcome } => outcome.applied().map(|receipt| receipt.patch()),
            Self::CommandFailed { .. } => None,
        }
    }

    pub fn command_outcome(&self) -> Option<&GraphCommandOutcome> {
        match self {
            Self::Command { outcome } => Some(outcome),
            Self::CommandFailed { .. } => None,
        }
    }

    pub fn command_error(&self) -> Option<&GraphRuntimeError> {
        match self {
            Self::CommandFailed { error } => Some(error),
            Self::Command { .. } => None,
        }
    }

    /// Consumes this close receipt into its exact command result.
    pub fn into_command_result(self) -> Result<GraphCommandOutcome, GraphRuntimeError> {
        match self {
            Self::Command { outcome } => Ok(outcome),
            Self::CommandFailed { error } => Err(error),
        }
    }
}

#[derive(Debug)]
pub struct ViewCloseOutcome {
    reason: ViewCloseReason,
    receipts: Vec<ViewCloseReceipt>,
}

impl ViewCloseOutcome {
    pub(crate) fn new(reason: ViewCloseReason, receipts: Vec<ViewCloseReceipt>) -> Self {
        Self { reason, receipts }
    }

    pub fn reason(&self) -> ViewCloseReason {
        self.reason
    }

    pub fn receipts(&self) -> &[ViewCloseReceipt] {
        &self.receipts
    }

    /// Consumes the close outcome while preserving final-command receipt order.
    pub fn into_parts(self) -> (ViewCloseReason, Vec<ViewCloseReceipt>) {
        (self.reason, self.receipts)
    }

    /// Splits an adapter-owned command prefix from caller-owned final-command receipts.
    pub fn split_receipt_prefix(mut self, count: usize) -> (Vec<ViewCloseReceipt>, Self) {
        let remaining = self.receipts.split_off(count.min(self.receipts.len()));
        let prefix = std::mem::replace(&mut self.receipts, remaining);
        (prefix, self)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ViewSessionError {
    #[error("view session is closed: {reason:?}")]
    Closed { reason: ViewCloseReason },
    #[error("view session state is currently borrowed")]
    Busy,
    #[error("view session is closing")]
    Closing,
    #[error("view subscription identity allocator is exhausted")]
    IdentityExhausted,
    #[error("view session viewport is invalid")]
    InvalidViewport,
    #[error("node is not present in the current graph: {node:?}")]
    NodeUnavailable { node: NodeId },
    #[error(transparent)]
    Runtime(#[from] GraphRuntimeError),
}

#[derive(Debug, thiserror::Error)]
pub enum ViewCreateFromWireQueryError {
    #[error(transparent)]
    Session(#[from] ViewSessionError),
    #[error(transparent)]
    Query(#[from] CreateFromWireQueryError),
}

#[derive(Debug, Clone)]
pub enum ViewSessionEvent {
    ViewportChanged {
        transform: ViewportTransform,
    },
    SelectionChanged {
        selection: ViewSelectionSeed,
    },
    RuntimePatched {
        patch: RuntimePatch,
    },
    LayoutPublished {
        layout_revision: u64,
        nodes: Vec<NodeId>,
    },
    Closed {
        reason: ViewCloseReason,
    },
}

#[derive(Debug, Clone)]
pub struct NodeBindingEvent {
    key: NodeBindingKey,
    patch: RuntimePatch,
}

impl NodeBindingEvent {
    pub fn node(&self) -> NodeId {
        self.key.node
    }

    pub fn incarnation(&self) -> NodeIncarnation {
        self.key.incarnation
    }

    pub fn key(&self) -> NodeBindingKey {
        self.key
    }

    pub fn patch(&self) -> &RuntimePatch {
        &self.patch
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeBindingKey {
    node: NodeId,
    incarnation: NodeIncarnation,
}

impl NodeBindingKey {
    pub fn node(self) -> NodeId {
        self.node
    }

    pub fn incarnation(self) -> NodeIncarnation {
        self.incarnation
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ViewSessionStats {
    pub runtime_patches: u64,
    pub fact_records_visited: u64,
    pub node_invalidations: u64,
    pub node_binding_notifications: u64,
    pub spatial_updates: u64,
    pub geometry_impact_patches_merged: u64,
    pub geometry_impact_global_fallbacks: u64,
    pub layout_impact_candidates_checked: u64,
    pub layout_impact_candidates_stale: u64,
    pub port_index_updates: u64,
    pub port_query_cells_examined: u64,
    pub port_query_entries_examined: u64,
    pub port_query_exact_tests: u64,
    pub port_query_incomplete: u64,
    pub port_query_budget_exhausted: u64,
    pub connection_probe_policy_evaluations: u64,
    pub edge_pick_cells_examined: u64,
    pub edge_pick_entries_examined: u64,
    pub edge_pick_exact_tests: u64,
    pub edge_pick_candidates_matched: u64,
    pub edge_pick_incomplete: u64,
    pub edge_pick_budget_exhausted: u64,
    pub edge_pick_geometry_indeterminate: u64,
    pub route_invalidations: u64,
    pub route_recomputations: u64,
    pub route_index_updates: u64,
    pub route_query_cells_examined: u64,
    pub route_query_entries_examined: u64,
    pub route_query_candidates_matched: u64,
    pub route_query_incomplete: u64,
    pub route_query_budget_exhausted: u64,
    pub route_flatten_cache_hits: u64,
    pub route_flatten_cache_misses: u64,
    pub route_flatten_cache_invalidations: u64,
    pub route_flatten_segments_built: u64,
    pub route_flatten_overflows: u64,
    pub view_state_sanitizations: u64,
    pub frame_nodes_materialized: u64,
    /// Admitted node candidates ranked with one cached selection-membership lookup each.
    pub frame_order_candidates_ranked: u64,
    /// Cached selection-membership lookups performed for admitted frame candidates.
    pub frame_selection_membership_lookups: u64,
    /// Nodes visited while rebuilding or incrementally patching the provisional admission index.
    pub admission_nodes_visited: u64,
    /// Spatial cells examined by provisional-admission and published-layout frame queries.
    pub admission_query_cells_examined: u64,
    /// Raw index entries examined by frame queries before candidate de-duplication.
    pub admission_query_entries_examined: u64,
    pub admission_index_rebuilds: u64,
    pub geometry_snapshot_builds: u64,
    pub geometry_snapshot_fact_updates: u64,
    pub layout_publications: u64,
    pub refresh_requests: u64,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct DerivedFactCounts {
    pub nodes: usize,
    pub routes: usize,
}

#[derive(Debug, Clone, Copy)]
enum ViewStatus {
    Open,
    Closing,
    Closed(ViewCloseReason),
}

struct ViewState {
    id: ViewId,
    generation: ViewGeneration,
    status: ViewStatus,
    seen_revision: GraphRevision,
    policy: GraphRuntimePolicy,
    facts: ViewFacts,
    node_draw_order_ranks: BTreeMap<NodeId, usize>,
    edge_draw_order_ranks: BTreeMap<EdgeId, usize>,
    selected_node_membership: HashSet<NodeId>,
    selected_edge_membership: HashSet<EdgeId>,
    selected_group_membership: HashSet<GroupId>,
    explicit_draw_order_nodes: BTreeSet<NodeId>,
    explicit_draw_order_edges: BTreeSet<EdgeId>,
    explicit_draw_order_groups: BTreeSet<GroupId>,
    retired_draw_order_nodes: BTreeSet<NodeId>,
    retired_draw_order_edges: BTreeSet<EdgeId>,
    retired_draw_order_groups: BTreeSet<GroupId>,
    layout_revision: u64,
    transform_revision: u64,
    interaction_order_revision: u128,
    environment: AdapterEnvironment,
    layout_generations: BTreeMap<NodeId, u64>,
    binding_revisions: BTreeMap<NodeId, GraphRevision>,
    renderer_bindings: BTreeMap<NodeId, RendererStamp>,
    geometry: SharedGeometryMap<NodeId, PublishedNodeGeometry>,
    geometry_ports: SharedGeometryMap<PortId, PublishedPortGeometry>,
    geometry_port_outcomes: SharedGeometryMap<PortId, PublishedPortOutcome>,
    port_geometry: PortGeometryIndex,
    spatial: ViewSpatialIndex,
    admission: AdmissionIndex,
    active_frame_nodes: BTreeSet<NodeId>,
    routes: SharedGeometryMap<EdgeId, PublishedEdgeRoute>,
    route_geometry: RouteGeometryIndex,
    unresolved_routes: BTreeSet<EdgeId>,
    geometry_snapshot_cache: Option<GeometrySnapshot>,
    pending_frame: Option<PendingFrame>,
    last_issued_frame: u64,
    last_superseded_frame: u64,
    stats: ViewSessionStats,
}

impl ViewState {
    fn advance_layout_revision(&mut self) {
        self.geometry_snapshot_cache = None;
        self.layout_revision = self.layout_revision.saturating_add(1);
    }

    fn record_geometry_snapshot_updates(&mut self, count: usize) {
        self.geometry_snapshot_cache = None;
        self.stats.geometry_snapshot_fact_updates = self
            .stats
            .geometry_snapshot_fact_updates
            .saturating_add(count as u64);
    }

    fn upsert_node_geometry(&mut self, node: NodeId, geometry: PublishedNodeGeometry) -> bool {
        debug_assert_eq!(node, geometry.node());
        if self
            .geometry
            .get(&node)
            .is_some_and(|current| current.as_ref() == &geometry)
        {
            return false;
        }

        let old_ports = self
            .geometry
            .get(&node)
            .map(|current| current.ports().keys().copied().collect::<BTreeSet<_>>())
            .unwrap_or_default();
        let new_ports = geometry.ports().keys().copied().collect::<BTreeSet<_>>();
        let touched_ports = old_ports.union(&new_ports).count();
        let old_outcomes = self
            .geometry
            .get(&node)
            .map(|current| {
                current
                    .port_outcomes()
                    .keys()
                    .copied()
                    .collect::<BTreeSet<_>>()
            })
            .unwrap_or_default();
        let new_outcomes = geometry
            .port_outcomes()
            .keys()
            .copied()
            .collect::<BTreeSet<_>>();
        let touched_outcomes = old_outcomes.union(&new_outcomes).count();

        for port in &old_ports {
            self.geometry_ports.remove_mut(port);
        }
        for port in old_outcomes {
            self.geometry_port_outcomes.remove_mut(&port);
        }
        for (port, published) in geometry.ports() {
            if self.port_geometry.upsert(published) {
                self.stats.port_index_updates = self.stats.port_index_updates.saturating_add(1);
            }
            self.geometry_ports
                .insert_mut(*port, Arc::new(published.clone()));
        }
        for port in old_ports.difference(&new_ports) {
            if self.port_geometry.remove(*port) {
                self.stats.port_index_updates = self.stats.port_index_updates.saturating_add(1);
            }
        }
        for (port, outcome) in geometry.port_outcomes() {
            self.geometry_port_outcomes
                .insert_mut(*port, Arc::new(outcome.clone()));
        }
        self.geometry.insert_mut(node, Arc::new(geometry));
        self.record_geometry_snapshot_updates(1 + touched_ports + touched_outcomes);
        true
    }

    fn remove_node_geometry(&mut self, node: NodeId) -> bool {
        let Some(current) = self.geometry.get(&node).cloned() else {
            return false;
        };
        self.geometry.remove_mut(&node);
        for port in current.ports().keys() {
            self.geometry_ports.remove_mut(port);
            if self.port_geometry.remove(*port) {
                self.stats.port_index_updates = self.stats.port_index_updates.saturating_add(1);
            }
        }
        for port in current.port_outcomes().keys() {
            self.geometry_port_outcomes.remove_mut(port);
        }
        self.record_geometry_snapshot_updates(
            1 + current.ports().len() + current.port_outcomes().len(),
        );
        true
    }

    fn translate_node_geometry(&mut self, node: NodeId, x: f32, y: f32) -> Option<CanvasRect> {
        let mut translated = self.geometry.get(&node)?.as_ref().clone();
        translated.translate(x, y);
        let bounds = translated.bounds();
        self.upsert_node_geometry(node, translated);
        Some(bounds)
    }

    fn mark_node_ports_culled(
        &mut self,
        node: NodeId,
        ports: impl IntoIterator<Item = PortId>,
    ) -> bool {
        let Some(current) = self.geometry.get(&node) else {
            return false;
        };
        let mut culled = current.as_ref().clone();
        if !culled.mark_ports_culled(ports) {
            return false;
        }
        self.upsert_node_geometry(node, culled)
    }

    fn mark_node_geometry_stale(
        &mut self,
        node: NodeId,
        ports: impl IntoIterator<Item = PortId>,
    ) -> bool {
        let Some(current) = self.geometry.get(&node) else {
            return false;
        };
        let mut stale = current.as_ref().clone();
        if !stale.mark_stale(ports) {
            return false;
        }
        self.upsert_node_geometry(node, stale)
    }

    fn mark_all_node_geometry_stale(
        &mut self,
        ports_by_node: &BTreeMap<NodeId, Vec<PortId>>,
    ) -> usize {
        ports_by_node
            .iter()
            .filter(|(node, ports)| self.mark_node_geometry_stale(**node, ports.iter().copied()))
            .count()
    }

    fn clear_node_geometry(&mut self) {
        if self.geometry.is_empty()
            && self.geometry_ports.is_empty()
            && self.geometry_port_outcomes.is_empty()
        {
            return;
        }
        let removed = self
            .geometry
            .size()
            .saturating_add(self.geometry_ports.size())
            .saturating_add(self.geometry_port_outcomes.size());
        self.geometry = SharedGeometryMap::new_sync();
        self.geometry_ports = SharedGeometryMap::new_sync();
        self.geometry_port_outcomes = SharedGeometryMap::new_sync();
        if self.port_geometry.clear() {
            self.stats.port_index_updates = self.stats.port_index_updates.saturating_add(1);
        }
        self.record_geometry_snapshot_updates(removed);
    }

    fn upsert_route(&mut self, edge: EdgeId, route: PublishedEdgeRoute) -> bool {
        if self
            .routes
            .get(&edge)
            .is_some_and(|current| current.as_ref() == &route)
        {
            return false;
        }
        let index = self.route_geometry.upsert(edge, &route);
        self.stats.route_index_updates = self
            .stats
            .route_index_updates
            .saturating_add(u64::from(index.index_changed));
        self.stats.route_flatten_cache_invalidations = self
            .stats
            .route_flatten_cache_invalidations
            .saturating_add(index.cache_invalidations);
        self.routes.insert_mut(edge, Arc::new(route));
        self.record_geometry_snapshot_updates(1);
        true
    }

    fn update_route_selection(&mut self, edges: &BTreeSet<EdgeId>) {
        for edge in edges {
            let Some(route) = self.routes.get(edge).map(|route| route.as_ref().clone()) else {
                continue;
            };
            let selected = self.selected_edge_membership.contains(edge);
            self.upsert_route(*edge, route.with_selected(selected));
        }
    }

    fn remove_route(&mut self, edge: EdgeId) -> bool {
        let route_removed = self.routes.remove_mut(&edge);
        let index = self.route_geometry.remove(edge);
        if !route_removed && !index.index_changed {
            return false;
        }
        self.stats.route_index_updates = self
            .stats
            .route_index_updates
            .saturating_add(u64::from(index.index_changed));
        self.stats.route_flatten_cache_invalidations = self
            .stats
            .route_flatten_cache_invalidations
            .saturating_add(index.cache_invalidations);
        self.record_geometry_snapshot_updates(1);
        true
    }

    fn clear_routes(&mut self) {
        let index = self.route_geometry.clear();
        if self.routes.is_empty() && !index.index_changed {
            return;
        }
        let removed = self.routes.size();
        self.routes = SharedGeometryMap::new_sync();
        self.stats.route_index_updates = self
            .stats
            .route_index_updates
            .saturating_add(u64::from(index.index_changed));
        self.stats.route_flatten_cache_invalidations = self
            .stats
            .route_flatten_cache_invalidations
            .saturating_add(index.cache_invalidations);
        self.record_geometry_snapshot_updates(removed);
    }

    fn record_draw_order_retirements(&mut self, patch: &RuntimePatch) {
        let graph = patch.snapshot().graph();
        for fact in patch.facts() {
            match fact.key() {
                RuntimeFactKey::Node(node) => {
                    update_draw_order_retirement(
                        &mut self.explicit_draw_order_nodes,
                        &mut self.retired_draw_order_nodes,
                        node,
                        graph.node(&node).is_some_and(|node| !node.hidden),
                    );
                }
                RuntimeFactKey::Edge(edge) => {
                    update_draw_order_retirement(
                        &mut self.explicit_draw_order_edges,
                        &mut self.retired_draw_order_edges,
                        edge,
                        edge_is_visible(graph, edge),
                    );
                }
                RuntimeFactKey::Group(group) => {
                    update_draw_order_retirement(
                        &mut self.explicit_draw_order_groups,
                        &mut self.retired_draw_order_groups,
                        group,
                        graph.group(&group).is_some(),
                    );
                }
                _ => {}
            }
        }
        for edge in &patch.impact().route_edges {
            update_draw_order_retirement(
                &mut self.explicit_draw_order_edges,
                &mut self.retired_draw_order_edges,
                *edge,
                edge_is_visible(graph, *edge),
            );
        }
    }

    fn rebuild_explicit_draw_order_membership(&mut self) {
        self.node_draw_order_ranks = index_draw_order(&self.facts.draw_order);
        self.edge_draw_order_ranks = index_draw_order(&self.facts.edge_draw_order);
        self.explicit_draw_order_nodes = self.facts.draw_order.iter().copied().collect();
        self.explicit_draw_order_edges = self.facts.edge_draw_order.iter().copied().collect();
        self.explicit_draw_order_groups = self.facts.group_draw_order.iter().copied().collect();
    }

    fn rebuild_selection_membership(&mut self) {
        self.selected_node_membership = self.facts.selected_nodes.iter().copied().collect();
        self.selected_edge_membership = self.facts.selected_edges.iter().copied().collect();
        self.selected_group_membership = self.facts.selected_groups.iter().copied().collect();
    }

    fn patch_may_change_selection(&self, patch: &RuntimePatch) -> bool {
        patch.facts().iter().any(|fact| match fact.key() {
            RuntimeFactKey::Node(node) => self.selected_node_membership.contains(&node),
            RuntimeFactKey::Edge(edge) => self.selected_edge_membership.contains(&edge),
            RuntimeFactKey::Group(group) => self.selected_group_membership.contains(&group),
            RuntimeFactKey::Import(_)
            | RuntimeFactKey::Symbol(_)
            | RuntimeFactKey::Port(_)
            | RuntimeFactKey::StickyNote(_)
            | RuntimeFactKey::Binding(_) => false,
        })
    }

    fn export_seed(&self, graph: &Graph) -> ViewSessionSeed {
        let mut facts = self.facts.clone();
        facts
            .draw_order
            .retain(|node| !self.retired_draw_order_nodes.contains(node));
        facts
            .edge_draw_order
            .retain(|edge| !self.retired_draw_order_edges.contains(edge));
        facts
            .group_draw_order
            .retain(|group| !self.retired_draw_order_groups.contains(group));
        facts.sanitize_for_graph(graph);
        facts.into_seed(graph.graph_id())
    }
}

fn update_draw_order_retirement<T: Copy + Ord>(
    explicit: &mut BTreeSet<T>,
    retired: &mut BTreeSet<T>,
    id: T,
    visible: bool,
) {
    if visible {
        if retired.remove(&id) {
            explicit.insert(id);
        }
    } else if explicit.remove(&id) {
        retired.insert(id);
    }
}

pub(crate) struct ViewShared {
    state: RefCell<ViewState>,
    subscriptions: subscriptions::ViewEventHub,
}

/// Revocable capability for one view-local graph session.
///
/// Raw transactions remain private to the runtime mutation kernel.
///
/// ```compile_fail
/// use jellyflow_runtime::graph_kernel::GraphTransaction;
/// ```
pub struct ViewSession {
    runtime: Weak<RuntimeShared>,
    shared: Rc<ViewShared>,
    catalog: Arc<NodeCatalog>,
}

#[derive(Clone)]
pub struct WeakViewSession {
    runtime: Weak<RuntimeShared>,
    view: Weak<ViewShared>,
    catalog: Arc<NodeCatalog>,
}

impl std::fmt::Debug for ViewSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = self
            .shared
            .state
            .try_borrow()
            .map_err(|_| std::fmt::Error)?;
        f.debug_struct("ViewSession")
            .field("id", &state.id)
            .field("generation", &state.generation)
            .field("status", &state.status)
            .field("seen_revision", &state.seen_revision)
            .finish()
    }
}

impl ViewSession {
    pub(crate) fn new(
        runtime: Weak<RuntimeShared>,
        shared: Rc<ViewShared>,
        catalog: Arc<NodeCatalog>,
    ) -> Self {
        Self {
            runtime,
            shared,
            catalog,
        }
    }

    pub fn id(&self) -> ViewId {
        self.shared.id()
    }

    pub fn generation(&self) -> Result<ViewGeneration, ViewSessionError> {
        let state = self
            .shared
            .state
            .try_borrow()
            .map_err(|_| ViewSessionError::Busy)?;
        ensure_open(state.status)?;
        Ok(state.generation)
    }

    pub fn graph_snapshot(&self) -> Result<GraphSnapshot, ViewSessionError> {
        self.runtime()?.snapshot().map_err(Into::into)
    }

    /// Returns the exact semantic catalog snapshot that canonicalized this view's runtime.
    pub fn catalog(&self) -> &NodeCatalog {
        &self.catalog
    }

    /// Clones this view's read-only semantic catalog provenance.
    pub fn catalog_owned(&self) -> Arc<NodeCatalog> {
        Arc::clone(&self.catalog)
    }

    /// Returns the current canvas-to-screen transform for native viewport projection.
    pub fn viewport_transform(&self) -> Result<ViewportTransform, ViewSessionError> {
        self.runtime()?;
        self.shared.viewport_transform()
    }

    /// Returns the runtime-assigned incarnation for a node visible to this session capability.
    pub fn node_incarnation(
        &self,
        node: NodeId,
    ) -> Result<Option<NodeIncarnation>, ViewSessionError> {
        self.runtime()?.node_incarnation(node).map_err(Into::into)
    }

    pub fn set_viewport(&self, pan: CanvasPoint, zoom: f32) -> Result<(), ViewSessionError> {
        self.runtime()?;
        self.shared.set_viewport(pan, zoom)
    }

    /// Returns the current normalized selection in stable selection order.
    pub fn selection(&self) -> Result<ViewSelectionSeed, ViewSessionError> {
        self.runtime()?;
        self.shared.selection()
    }

    /// Returns a bounded prefix of selected edges without cloning the full selection.
    pub fn selected_edge_ids(&self, limit: usize) -> Result<SelectedEdgeIds, ViewSessionError> {
        self.runtime()?;
        self.shared.selected_edge_ids(limit)
    }

    pub fn set_selection(
        &self,
        nodes: Vec<NodeId>,
        edges: Vec<EdgeId>,
        groups: Vec<GroupId>,
    ) -> Result<(), ViewSessionError> {
        let snapshot = self.graph_snapshot()?;
        self.shared
            .set_selection(snapshot.graph(), nodes, edges, groups)
    }

    pub fn export_seed(&self) -> Result<ViewSessionSeed, ViewSessionError> {
        let snapshot = self.graph_snapshot()?;
        self.shared.export_seed(snapshot.graph())
    }

    /// Executes one revision-checked graph command through this session's runtime capability.
    pub fn execute(&self, command: GraphCommand) -> Result<GraphCommandOutcome, ViewSessionError> {
        self.runtime()?.execute_command(command).map_err(Into::into)
    }

    pub fn evaluate_connection(
        &self,
        edit: ConnectionEdit,
    ) -> Result<ConnectionEvaluation, ViewSessionError> {
        self.runtime()?
            .evaluate_connection(edit)
            .map_err(Into::into)
    }

    pub fn create_from_wire_candidates(
        &self,
        source: PortId,
        mode: NodeGraphConnectionMode,
    ) -> Result<CreateFromWireCandidates, ViewCreateFromWireQueryError> {
        Ok(self.runtime()?.create_from_wire_candidates(source, mode)?)
    }

    pub fn port_connection_facts(
        &self,
        port: PortId,
    ) -> Result<Option<PortConnectionFacts>, ViewSessionError> {
        self.runtime()?
            .port_connection_facts(port)
            .map_err(Into::into)
    }

    pub fn port_connection_drag_intent(
        &self,
        port: PortId,
    ) -> Result<Option<PortConnectionDragIntent>, ViewSessionError> {
        self.runtime()?
            .port_connection_drag_intent(port)
            .map_err(Into::into)
    }

    pub fn subscribe(
        &self,
        callback: impl FnMut(ViewSessionEvent) + 'static,
    ) -> Result<ViewSubscriptionToken, ViewSessionError> {
        self.ensure_open()?;
        self.shared
            .subscriptions
            .subscribe_event(callback)
            .ok_or(ViewSessionError::IdentityExhausted)
    }

    pub fn subscribe_node(
        &self,
        node: NodeId,
        callback: impl FnMut(NodeBindingEvent) + 'static,
    ) -> Result<ViewSubscriptionToken, ViewSessionError> {
        let runtime = self.runtime()?;
        let incarnation = runtime
            .node_incarnation(node)?
            .ok_or(ViewSessionError::NodeUnavailable { node })?;
        self.shared
            .subscriptions
            .subscribe_node(NodeBindingKey { node, incarnation }, callback)
            .ok_or(ViewSessionError::IdentityExhausted)
    }

    pub fn unsubscribe(&self, token: ViewSubscriptionToken) -> Result<bool, ViewSessionError> {
        self.ensure_open()?;
        Ok(self.shared.subscriptions.unsubscribe(token))
    }

    pub fn downgrade(&self) -> WeakViewSession {
        WeakViewSession {
            runtime: self.runtime.clone(),
            view: Rc::downgrade(&self.shared),
            catalog: Arc::clone(&self.catalog),
        }
    }

    pub fn close(&self) -> Result<ViewCloseOutcome, ViewSessionError> {
        self.close_with_request(ViewCloseRequest::default())
    }

    pub fn close_with_request(
        &self,
        request: ViewCloseRequest,
    ) -> Result<ViewCloseOutcome, ViewSessionError> {
        if let Some(reason) = self.close_reason() {
            return Err(ViewSessionError::Closed { reason });
        }
        let runtime = self.runtime()?;
        if request.commands().is_empty() {
            let (terminalized, callback_panic) =
                self.shared.terminalize(ViewCloseReason::HostClosed);
            if !terminalized {
                resume(callback_panic);
                return self
                    .close_reason()
                    .map_or(Err(ViewSessionError::Busy), |reason| {
                        Err(ViewSessionError::Closed { reason })
                    });
            }
            runtime.unregister_view(self.id());
            let outcome = ViewCloseOutcome::new(ViewCloseReason::HostClosed, Vec::new());
            resume(callback_panic);
            return Ok(outcome);
        }
        runtime.close_view(&self.shared, request)
    }

    pub fn close_reason(&self) -> Option<ViewCloseReason> {
        self.shared
            .state
            .try_borrow()
            .ok()
            .and_then(|state| match state.status {
                ViewStatus::Open => None,
                ViewStatus::Closing => None,
                ViewStatus::Closed(reason) => Some(reason),
            })
    }

    pub fn stats(&self) -> Result<ViewSessionStats, ViewSessionError> {
        let state = self
            .shared
            .state
            .try_borrow()
            .map_err(|_| ViewSessionError::Busy)?;
        ensure_open(state.status)?;
        Ok(state.stats)
    }

    #[cfg(test)]
    pub(crate) fn layout_revision_for_test(&self) -> u64 {
        self.shared.state.borrow().layout_revision
    }

    #[cfg(test)]
    pub(crate) fn draw_order_storage_counts_for_test(&self) -> (usize, usize) {
        let state = self.shared.state.borrow();
        (
            state.facts.draw_order.len(),
            state.retired_draw_order_nodes.len(),
        )
    }

    #[cfg(test)]
    pub(crate) fn with_state_borrowed_for_test<R>(&self, callback: impl FnOnce() -> R) -> R {
        let _state = self.shared.state.borrow_mut();
        callback()
    }

    fn ensure_open(&self) -> Result<(), ViewSessionError> {
        let state = self
            .shared
            .state
            .try_borrow()
            .map_err(|_| ViewSessionError::Busy)?;
        ensure_open(state.status)
    }

    fn runtime(&self) -> Result<Rc<RuntimeShared>, ViewSessionError> {
        self.ensure_open()?;
        let Some(runtime) = self.runtime.upgrade() else {
            let (_, callback_panic) = self.shared.terminalize(ViewCloseReason::RuntimeDropped);
            resume(callback_panic);
            return Err(ViewSessionError::Closed {
                reason: ViewCloseReason::RuntimeDropped,
            });
        };
        if !runtime.alive.get() {
            let (_, callback_panic) = self.shared.terminalize(ViewCloseReason::RuntimeDropped);
            resume(callback_panic);
            return Err(ViewSessionError::Closed {
                reason: ViewCloseReason::RuntimeDropped,
            });
        }
        Ok(runtime)
    }
}

impl Drop for ViewSession {
    fn drop(&mut self) {
        let (terminalized, callback_panic) = self.shared.terminalize(ViewCloseReason::HostClosed);
        suppress(callback_panic);
        if terminalized && let Some(runtime) = self.runtime.upgrade() {
            runtime.unregister_view(self.id());
        }
    }
}

impl WeakViewSession {
    pub fn is_open(&self) -> bool {
        self.runtime
            .upgrade()
            .is_some_and(|runtime| runtime.alive.get())
            && self.view.upgrade().is_some_and(|view| view.is_open())
    }

    pub fn graph_snapshot(&self) -> Result<GraphSnapshot, ViewSessionError> {
        let (_, runtime) = self.open_capability()?;
        runtime.snapshot().map_err(Into::into)
    }

    /// Returns the exact semantic catalog snapshot without reviving the view or runtime.
    pub fn catalog(&self) -> &NodeCatalog {
        &self.catalog
    }

    /// Clones the read-only catalog provenance without reviving the view or runtime.
    pub fn catalog_owned(&self) -> Arc<NodeCatalog> {
        Arc::clone(&self.catalog)
    }

    /// Returns the current canvas-to-screen transform without owning the view session.
    pub fn viewport_transform(&self) -> Result<ViewportTransform, ViewSessionError> {
        let (view, _) = self.open_capability()?;
        view.viewport_transform()
    }

    /// Updates the viewport without owning or reviving the view session.
    pub fn set_viewport(&self, pan: CanvasPoint, zoom: f32) -> Result<(), ViewSessionError> {
        let (view, _) = self.open_capability()?;
        view.set_viewport(pan, zoom)
    }

    /// Returns the current normalized selection in stable selection order.
    pub fn selection(&self) -> Result<ViewSelectionSeed, ViewSessionError> {
        let (view, _) = self.open_capability()?;
        view.selection()
    }

    /// Updates the selection without owning or reviving the view session.
    pub fn set_selection(
        &self,
        nodes: Vec<NodeId>,
        edges: Vec<EdgeId>,
        groups: Vec<GroupId>,
    ) -> Result<(), ViewSessionError> {
        let (view, runtime) = self.open_capability()?;
        let snapshot = runtime.snapshot()?;
        view.set_selection(snapshot.graph(), nodes, edges, groups)
    }

    /// Exports the current persisted view facts without owning the view session.
    pub fn export_seed(&self) -> Result<ViewSessionSeed, ViewSessionError> {
        let (view, runtime) = self.open_capability()?;
        let snapshot = runtime.snapshot()?;
        view.export_seed(snapshot.graph())
    }

    /// Returns the current incarnation without reviving a closed view capability.
    pub fn node_incarnation(
        &self,
        node: NodeId,
    ) -> Result<Option<NodeIncarnation>, ViewSessionError> {
        let (_, runtime) = self.open_capability()?;
        runtime.node_incarnation(node).map_err(Into::into)
    }

    /// Executes one revision-checked graph command without reviving a closed view capability.
    pub fn execute(&self, command: GraphCommand) -> Result<GraphCommandOutcome, ViewSessionError> {
        let (_, runtime) = self.open_capability()?;
        runtime.execute_command(command).map_err(Into::into)
    }

    pub fn evaluate_connection(
        &self,
        edit: ConnectionEdit,
    ) -> Result<ConnectionEvaluation, ViewSessionError> {
        let (_, runtime) = self.open_capability()?;
        runtime.evaluate_connection(edit).map_err(Into::into)
    }

    pub fn create_from_wire_candidates(
        &self,
        source: PortId,
        mode: NodeGraphConnectionMode,
    ) -> Result<CreateFromWireCandidates, ViewCreateFromWireQueryError> {
        let (_, runtime) = self.open_capability()?;
        Ok(runtime.create_from_wire_candidates(source, mode)?)
    }

    pub fn port_connection_facts(
        &self,
        port: PortId,
    ) -> Result<Option<PortConnectionFacts>, ViewSessionError> {
        let (_, runtime) = self.open_capability()?;
        runtime.port_connection_facts(port).map_err(Into::into)
    }

    pub fn port_connection_drag_intent(
        &self,
        port: PortId,
    ) -> Result<Option<PortConnectionDragIntent>, ViewSessionError> {
        let (_, runtime) = self.open_capability()?;
        runtime
            .port_connection_drag_intent(port)
            .map_err(Into::into)
    }

    pub fn subscribe(
        &self,
        callback: impl FnMut(ViewSessionEvent) + 'static,
    ) -> Result<ViewSubscriptionToken, ViewSessionError> {
        let (view, _) = self.open_capability()?;
        view.subscriptions
            .subscribe_event(callback)
            .ok_or(ViewSessionError::IdentityExhausted)
    }

    pub fn subscribe_node(
        &self,
        node: NodeId,
        callback: impl FnMut(NodeBindingEvent) + 'static,
    ) -> Result<ViewSubscriptionToken, ViewSessionError> {
        let (view, runtime) = self.open_capability()?;
        let incarnation = runtime
            .node_incarnation(node)?
            .ok_or(ViewSessionError::NodeUnavailable { node })?;
        view.subscriptions
            .subscribe_node(NodeBindingKey { node, incarnation }, callback)
            .ok_or(ViewSessionError::IdentityExhausted)
    }

    pub fn unsubscribe(&self, token: ViewSubscriptionToken) -> Result<bool, ViewSessionError> {
        let (view, _) = self.open_capability()?;
        Ok(view.subscriptions.unsubscribe(token))
    }

    fn open_capability(&self) -> Result<(Rc<ViewShared>, Rc<RuntimeShared>), ViewSessionError> {
        let Some(view) = self.view.upgrade() else {
            return Err(ViewSessionError::Closed {
                reason: ViewCloseReason::HostClosed,
            });
        };
        {
            let state = view
                .state
                .try_borrow()
                .map_err(|_| ViewSessionError::Busy)?;
            ensure_open(state.status)?;
        }
        let Some(runtime) = self.runtime.upgrade() else {
            let (_, callback_panic) = view.terminalize(ViewCloseReason::RuntimeDropped);
            resume(callback_panic);
            return Err(ViewSessionError::Closed {
                reason: ViewCloseReason::RuntimeDropped,
            });
        };
        if !runtime.alive.get() {
            let (_, callback_panic) = view.terminalize(ViewCloseReason::RuntimeDropped);
            resume(callback_panic);
            return Err(ViewSessionError::Closed {
                reason: ViewCloseReason::RuntimeDropped,
            });
        }
        Ok((view, runtime))
    }
}

impl ViewShared {
    pub(crate) fn new(
        id: ViewId,
        generation: ViewGeneration,
        seen_revision: GraphRevision,
        policy: GraphRuntimePolicy,
        seed: ViewSessionSeed,
    ) -> Rc<Self> {
        let facts = ViewFacts::from_seed(seed);
        let explicit_draw_order_nodes = facts.draw_order.iter().copied().collect();
        let explicit_draw_order_edges = facts.edge_draw_order.iter().copied().collect();
        let explicit_draw_order_groups = facts.group_draw_order.iter().copied().collect();
        let node_draw_order_ranks = index_draw_order(&facts.draw_order);
        let edge_draw_order_ranks = index_draw_order(&facts.edge_draw_order);
        let selected_node_membership = facts.selected_nodes.iter().copied().collect();
        let selected_edge_membership = facts.selected_edges.iter().copied().collect();
        let selected_group_membership = facts.selected_groups.iter().copied().collect();
        Rc::new(Self {
            state: RefCell::new(ViewState {
                id,
                generation,
                status: ViewStatus::Open,
                seen_revision,
                policy,
                facts,
                node_draw_order_ranks,
                edge_draw_order_ranks,
                selected_node_membership,
                selected_edge_membership,
                selected_group_membership,
                explicit_draw_order_nodes,
                explicit_draw_order_edges,
                explicit_draw_order_groups,
                retired_draw_order_nodes: BTreeSet::new(),
                retired_draw_order_edges: BTreeSet::new(),
                retired_draw_order_groups: BTreeSet::new(),
                layout_revision: 0,
                transform_revision: 0,
                interaction_order_revision: 0,
                environment: AdapterEnvironment::default(),
                layout_generations: BTreeMap::new(),
                binding_revisions: BTreeMap::new(),
                renderer_bindings: BTreeMap::new(),
                geometry: SharedGeometryMap::new_sync(),
                geometry_ports: SharedGeometryMap::new_sync(),
                geometry_port_outcomes: SharedGeometryMap::new_sync(),
                port_geometry: PortGeometryIndex::default(),
                spatial: ViewSpatialIndex::default(),
                admission: AdmissionIndex::default(),
                active_frame_nodes: BTreeSet::new(),
                routes: SharedGeometryMap::new_sync(),
                route_geometry: RouteGeometryIndex::default(),
                unresolved_routes: BTreeSet::new(),
                geometry_snapshot_cache: None,
                pending_frame: None,
                last_issued_frame: 0,
                last_superseded_frame: 0,
                stats: ViewSessionStats::default(),
            }),
            subscriptions: subscriptions::ViewEventHub::new(id),
        })
    }

    fn viewport_transform(&self) -> Result<ViewportTransform, ViewSessionError> {
        let state = self
            .state
            .try_borrow()
            .map_err(|_| ViewSessionError::Busy)?;
        ensure_open(state.status)?;
        ViewportTransform::new(state.facts.pan, state.facts.zoom)
            .ok_or(ViewSessionError::InvalidViewport)
    }

    fn set_viewport(&self, pan: CanvasPoint, zoom: f32) -> Result<(), ViewSessionError> {
        let changed = {
            let mut state = self
                .state
                .try_borrow_mut()
                .map_err(|_| ViewSessionError::Busy)?;
            ensure_open(state.status)?;
            let before = (state.facts.pan, state.facts.zoom);
            state.facts.set_viewport(pan, zoom);
            if before == (state.facts.pan, state.facts.zoom) {
                None
            } else {
                state.transform_revision = state.transform_revision.saturating_add(1);
                Some(
                    ViewportTransform::new(state.facts.pan, state.facts.zoom)
                        .ok_or(ViewSessionError::InvalidViewport)?,
                )
            }
        };
        if let Some(transform) = changed {
            resume(
                self.subscriptions
                    .publish_event(ViewSessionEvent::ViewportChanged { transform }),
            );
        }
        Ok(())
    }

    fn selection(&self) -> Result<ViewSelectionSeed, ViewSessionError> {
        let state = self
            .state
            .try_borrow()
            .map_err(|_| ViewSessionError::Busy)?;
        ensure_open(state.status)?;
        Ok(state.facts.selection())
    }

    fn selected_edge_ids(&self, limit: usize) -> Result<SelectedEdgeIds, ViewSessionError> {
        let state = self
            .state
            .try_borrow()
            .map_err(|_| ViewSessionError::Busy)?;
        ensure_open(state.status)?;
        Ok(SelectedEdgeIds::from_selection(
            &state.facts.selected_edges,
            limit,
        ))
    }

    fn set_selection(
        &self,
        graph: &Graph,
        nodes: Vec<NodeId>,
        edges: Vec<EdgeId>,
        groups: Vec<GroupId>,
    ) -> Result<(), ViewSessionError> {
        let changed = {
            let mut state = self
                .state
                .try_borrow_mut()
                .map_err(|_| ViewSessionError::Busy)?;
            ensure_open(state.status)?;
            let before = state.facts.selection();
            state.facts.set_selection(nodes, edges, groups);
            state.facts.sanitize_selection_for_graph(graph);
            let selection = state.facts.selection();
            if selection == before {
                None
            } else {
                let previous_edges = state.selected_edge_membership.clone();
                state.rebuild_selection_membership();
                state.interaction_order_revision =
                    state.interaction_order_revision.saturating_add(1);
                let changed_edges = previous_edges
                    .symmetric_difference(&state.selected_edge_membership)
                    .copied()
                    .collect::<BTreeSet<_>>();
                state.update_route_selection(&changed_edges);
                Some(selection)
            }
        };
        if let Some(selection) = changed {
            resume(
                self.subscriptions
                    .publish_event(ViewSessionEvent::SelectionChanged { selection }),
            );
        }
        Ok(())
    }

    fn export_seed(&self, graph: &Graph) -> Result<ViewSessionSeed, ViewSessionError> {
        let state = self
            .state
            .try_borrow()
            .map_err(|_| ViewSessionError::Busy)?;
        ensure_open(state.status)?;
        Ok(state.export_seed(graph))
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open_generation().is_some()
    }

    pub(crate) fn id(&self) -> ViewId {
        self.state
            .try_borrow()
            .map(|state| state.id)
            .unwrap_or_else(|_| ViewId::unavailable())
    }

    pub(crate) fn open_generation(&self) -> Option<ViewGeneration> {
        self.state
            .try_borrow()
            .ok()
            .and_then(|state| match state.status {
                ViewStatus::Open => Some(state.generation),
                ViewStatus::Closing | ViewStatus::Closed(_) => None,
            })
    }

    pub(crate) fn begin_close(&self) -> Result<(), ViewSessionError> {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| ViewSessionError::Busy)?;
        match state.status {
            ViewStatus::Open => {
                state.status = ViewStatus::Closing;
                Ok(())
            }
            ViewStatus::Closing => Err(ViewSessionError::Closing),
            ViewStatus::Closed(reason) => Err(ViewSessionError::Closed { reason }),
        }
    }

    pub(crate) fn apply_runtime_patch(&self, patch: &RuntimePatch) -> Option<ViewSelectionSeed> {
        for node in patch.affected_nodes() {
            if patch.node_incarnation_change(*node).is_some() {
                self.subscriptions.retire_node(*node);
            }
        }
        let mut state = self.state.borrow_mut();
        if !matches!(state.status, ViewStatus::Open) {
            return None;
        }
        let sanitize_selection = patch.kind() == RuntimePatchKind::DocumentReset
            || (patch.impact().sanitize_view_state && state.patch_may_change_selection(patch));
        let selection_before = sanitize_selection.then(|| state.facts.selection());
        if let Some(pending) = state.pending_frame.as_mut() {
            let fallback = pending.observe_patch(patch);
            state.stats.geometry_impact_patches_merged =
                state.stats.geometry_impact_patches_merged.saturating_add(1);
            state.stats.geometry_impact_global_fallbacks = state
                .stats
                .geometry_impact_global_fallbacks
                .saturating_add(u64::from(fallback));
        }
        state.seen_revision = patch.to_revision();
        state.stats.runtime_patches = state.stats.runtime_patches.saturating_add(1);
        state.stats.fact_records_visited = state
            .stats
            .fact_records_visited
            .saturating_add(patch.facts().len() as u64);

        if patch.kind() == RuntimePatchKind::DocumentReset {
            state.clear_node_geometry();
            state.spatial.clear();
            state.clear_routes();
            state.unresolved_routes.clear();
            state.geometry_snapshot_cache = None;
            state.admission.invalidate();
            state.active_frame_nodes.clear();
            state.layout_generations.clear();
            state.binding_revisions.clear();
            state.renderer_bindings.clear();
            state.retired_draw_order_nodes.clear();
            state.retired_draw_order_edges.clear();
            state.retired_draw_order_groups.clear();
        }

        let admission_nodes_visited = state.admission.update_nodes(
            patch.snapshot().graph(),
            patch.affected_nodes().iter().copied(),
        );
        state.stats.admission_nodes_visited = state
            .stats
            .admission_nodes_visited
            .saturating_add(admission_nodes_visited);
        let impact = patch.impact();
        let mut spatially_updated = BTreeSet::new();
        for node in &impact.retired_nodes {
            if state.remove_node_geometry(*node) {
                state.spatial.remove(*node);
                spatially_updated.insert(*node);
            }
            state.layout_generations.remove(node);
            state.binding_revisions.remove(node);
            state.renderer_bindings.remove(node);
        }
        for node in patch.affected_nodes() {
            if patch.snapshot().graph().node(node).is_some() {
                state.binding_revisions.insert(*node, patch.to_revision());
            } else {
                state.binding_revisions.remove(node);
            }
        }
        for node in &impact.geometry_withdrawals {
            if state.remove_node_geometry(*node) {
                state.spatial.remove(*node);
                spatially_updated.insert(*node);
            }
        }
        for node in patch.affected_nodes() {
            let Some(Some(incarnation)) = patch.node_incarnation_change(*node) else {
                continue;
            };
            if state
                .geometry
                .get(node)
                .is_some_and(|geometry| geometry.incarnation() != incarnation)
            {
                state.remove_node_geometry(*node);
                state.spatial.remove(*node);
                spatially_updated.insert(*node);
            }
        }
        for translation in &impact.translated_nodes {
            if let Some(bounds) = state.translate_node_geometry(
                translation.node,
                translation.delta.x,
                translation.delta.y,
            ) && state.spatial.upsert(translation.node, bounds)
            {
                spatially_updated.insert(translation.node);
            }
        }
        for node in &impact.layout_dirty_nodes {
            let generation = state.layout_generations.entry(*node).or_default();
            *generation = generation.saturating_add(1);
            let ports = patch
                .snapshot()
                .graph()
                .node(node)
                .map(|node| node.ports.iter().copied())
                .into_iter()
                .flatten();
            state.mark_node_geometry_stale(*node, ports);
        }
        let route_edges = impact.route_edges.iter().copied().collect::<BTreeSet<_>>();
        let route_recomputation = recompute_routes(&mut state, patch.snapshot(), &route_edges);
        state.stats.node_invalidations = state
            .stats
            .node_invalidations
            .saturating_add(impact.layout_dirty_nodes.len() as u64);
        state.stats.spatial_updates = state
            .stats
            .spatial_updates
            .saturating_add(spatially_updated.len() as u64);
        state.stats.route_invalidations = state
            .stats
            .route_invalidations
            .saturating_add(impact.route_edges.len() as u64);
        state.stats.route_recomputations = state
            .stats
            .route_recomputations
            .saturating_add(route_recomputation.attempts);
        if patch.kind() == RuntimePatchKind::DocumentReset {
            state.facts.sanitize_for_graph(patch.snapshot().graph());
            state.rebuild_explicit_draw_order_membership();
            state.rebuild_selection_membership();
            state.stats.view_state_sanitizations =
                state.stats.view_state_sanitizations.saturating_add(1);
        } else {
            if sanitize_selection {
                state
                    .facts
                    .sanitize_selection_for_graph(patch.snapshot().graph());
                state.rebuild_selection_membership();
                state.stats.view_state_sanitizations =
                    state.stats.view_state_sanitizations.saturating_add(1);
            }
            state.record_draw_order_retirements(patch);
        }
        if !spatially_updated.is_empty()
            || !impact.layout_dirty_nodes.is_empty()
            || route_recomputation.changed
        {
            state.advance_layout_revision();
        }
        selection_before.and_then(|before| {
            let selection = state.facts.selection();
            (selection != before).then_some(selection)
        })
    }

    pub(crate) fn publish_runtime_patch(
        &self,
        generation: ViewGeneration,
        patch: RuntimePatch,
        selection_changed: Option<ViewSelectionSeed>,
    ) -> Option<PanicPayload> {
        if self.open_generation() != Some(generation) {
            return None;
        }
        let mut panics = PanicCollector::default();
        panics.record(
            self.subscriptions
                .publish_event(ViewSessionEvent::RuntimePatched {
                    patch: patch.clone(),
                }),
        );
        if self.open_generation() != Some(generation) {
            return panics.into_payload();
        }
        if let Some(selection) = selection_changed {
            panics.record(
                self.subscriptions
                    .publish_event(ViewSessionEvent::SelectionChanged { selection }),
            );
        }
        for node in patch.affected_nodes() {
            if self.open_generation() != Some(generation) {
                return panics.into_payload();
            }
            let (notified, callback_panic) = self.subscriptions.publish_node(*node, &patch);
            panics.record(callback_panic);
            if notified > 0
                && let Ok(mut state) = self.state.try_borrow_mut()
            {
                state.stats.node_binding_notifications = state
                    .stats
                    .node_binding_notifications
                    .saturating_add(notified as u64);
            }
        }
        panics.into_payload()
    }

    pub(crate) fn terminalize(&self, reason: ViewCloseReason) -> (bool, Option<PanicPayload>) {
        let Ok(mut state) = self.state.try_borrow_mut() else {
            return (false, None);
        };
        if matches!(state.status, ViewStatus::Closed(_)) {
            return (false, None);
        }
        state.status = ViewStatus::Closed(reason);
        state.generation = state.generation.terminal();
        state.clear_node_geometry();
        state.spatial.clear();
        state.admission.invalidate();
        state.active_frame_nodes.clear();
        state.pending_frame = None;
        state.layout_generations.clear();
        state.binding_revisions.clear();
        state.renderer_bindings.clear();
        state.clear_routes();
        state.unresolved_routes.clear();
        state.geometry_snapshot_cache = None;
        drop(state);
        (true, self.subscriptions.publish_terminal(reason))
    }
}

fn ensure_open(status: ViewStatus) -> Result<(), ViewSessionError> {
    match status {
        ViewStatus::Open => Ok(()),
        ViewStatus::Closing => Err(ViewSessionError::Closing),
        ViewStatus::Closed(reason) => Err(ViewSessionError::Closed { reason }),
    }
}
