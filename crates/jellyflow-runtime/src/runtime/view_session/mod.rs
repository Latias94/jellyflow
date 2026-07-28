//! View-local capability issued by an authoritative [`GraphRuntime`](super::graph_runtime::GraphRuntime).

mod connection_summaries;
mod derived_geometry;
mod edge_pick;
mod facts;
mod geometry;
mod port_geometry;
mod protocol;
mod route_geometry;
mod selection_membership;
mod spatial_grid;
mod subscriptions;

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::rc::{Rc, Weak};
use std::sync::Arc;

use jellyflow_core::core::{EdgeId, Graph, GroupId, NodeId, PortId};

use crate::io::{
    ViewSelectionSeed, ViewSessionSeed, ViewSessionSeedGraphMismatch, edge_is_visible,
};
use crate::runtime::adapter_protocol::{AdapterEnvironment, GeometryImpactMergeWork, ViewPoint};
use crate::runtime::commands::{
    CommandCompletionCorrelationError, CommandExecutionCompletion, CommandExecutionCompletions,
    ConnectionEdit, ConnectionEvaluation, GraphCommand, GraphCommandIdentity,
};
use crate::runtime::graph_runtime::{
    CreateFromWireCandidates, CreateFromWireQueryError, EdgeInstanceKey, FactChangeMask,
    GraphRevision, GraphRuntimeError, GraphRuntimePolicy, GraphSnapshot, NodeLifetimeKey,
    PortConnectionDragIntent, PortConnectionFacts, PortConnectionQueryResult,
    PortConnectionSummary, PortInstanceKey, RuntimeFactKey, RuntimeId, RuntimePatch,
    RuntimePatchKind, RuntimeProvenance, RuntimeShared,
};
use crate::runtime::interaction::{
    AlignmentSnapWork, ConnectionGestureBinding, InteractionQueryId, NodeDragStart,
    SelectionBaseline, SelectionMode, SelectionMutation,
};
use crate::runtime::unwind::{PanicCollector, PanicPayload, resume, suppress};
use crate::runtime::viewport::{
    ViewportIntent, ViewportPointerSample, ViewportRevision, ViewportSnapshot, ViewportTransform,
};
use crate::schema::NodeCatalog;

use self::derived_geometry::DerivedGeometry;
pub use self::edge_pick::{
    EdgePick, EdgePickCursor, EdgePickCursorStaleReason, EdgePickGeometryFailure, EdgePickQuery,
    EdgePickQueryError, EdgePickQueryOutcome, EdgePickWork,
};
pub use self::geometry::{
    NodeIntersectionCursor, NodeIntersectionCursorStaleReason, NodeIntersectionQuery,
    NodeIntersectionQueryError, NodeIntersectionQueryOutcome, NodeIntersectionWork,
};
use self::order::index_draw_order;
pub use self::port_geometry::{
    ConnectionProbeCursor, ConnectionProbeCursorStaleReason, ConnectionProbeQuery,
    ConnectionProbeQueryError, ConnectionProbeQueryOutcome, ConnectionProbeTargeting,
    ConnectionProbeWork,
};
use self::protocol::PendingFrame;
pub use self::route_geometry::{
    RouteGeometryRevision, VisibleRoute, VisibleRouteCursor, VisibleRouteCursorStaleReason,
    VisibleRouteGeometryOverflow, VisibleRouteQuery, VisibleRouteQueryError,
    VisibleRouteQueryOutcome, VisibleRouteQueryRevision, VisibleRouteQueryWork, VisibleRoutes,
};
use self::selection_membership::SelectedEdgeMembership;
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

    pub(crate) const fn runtime_provenance(self) -> RuntimeProvenance {
        RuntimeProvenance::new(self.runtime)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ViewGeneration(u64);

impl ViewGeneration {
    pub(crate) const fn initial() -> Self {
        Self(1)
    }

    fn terminal(self) -> Self {
        Self(self.0.saturating_add(1))
    }

    pub fn get(self) -> u64 {
        self.0
    }
}

/// Opaque identity of the exact view generation that issued native geometry facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ViewProvenance {
    view: ViewId,
    generation: ViewGeneration,
}

impl ViewProvenance {
    pub(crate) const fn new(view: ViewId, generation: ViewGeneration) -> Self {
        Self { view, generation }
    }

    #[cfg(test)]
    pub(crate) const fn unavailable() -> Self {
        Self::new(ViewId::unavailable(), ViewGeneration::initial())
    }

    pub fn view(self) -> ViewId {
        self.view
    }

    pub fn generation(self) -> ViewGeneration {
        self.generation
    }

    pub fn runtime(self) -> RuntimeProvenance {
        self.view.runtime_provenance()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ViewQueryIdentity {
    view: ViewId,
    generation: ViewGeneration,
    viewport_revision: ViewportRevision,
}

impl ViewQueryIdentity {
    fn new(view: ViewId, generation: ViewGeneration, viewport_revision: ViewportRevision) -> Self {
        Self {
            view,
            generation,
            viewport_revision,
        }
    }

    fn stale_reason(self, current: Self) -> Option<ViewQueryIdentityStaleReason> {
        if self.view != current.view {
            Some(ViewQueryIdentityStaleReason::ForeignView)
        } else if self.generation != current.generation {
            Some(ViewQueryIdentityStaleReason::ViewGeneration)
        } else if self.viewport_revision != current.viewport_revision {
            Some(ViewQueryIdentityStaleReason::ViewportTransform)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ViewQueryIdentityStaleReason {
    ForeignView,
    ViewGeneration,
    ViewportTransform,
}

/// Private binding carried from one semantic interaction query into view execution.
///
/// The binding is deliberately crate-private: adapters can transport a query but cannot mint a
/// receipt for a different view or nonce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InteractionQueryBinding {
    provenance: ViewProvenance,
    query: InteractionQueryId,
}

impl InteractionQueryBinding {
    pub(crate) const fn new(provenance: ViewProvenance, query: InteractionQueryId) -> Self {
        Self { provenance, query }
    }

    fn validate(self, identity: ViewQueryIdentity) -> Result<(), ViewSessionError> {
        (self.provenance == ViewProvenance::new(identity.view, identity.generation))
            .then_some(())
            .ok_or(ViewSessionError::ForeignInteractionFacts)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InteractionQueryProof {
    provenance: ViewProvenance,
    query: InteractionQueryId,
    viewport_revision: ViewportRevision,
    fact_generations: [u128; 4],
}

impl InteractionQueryProof {
    fn issue(
        binding: Option<InteractionQueryBinding>,
        identity: ViewQueryIdentity,
        fact_generations: [u128; 4],
    ) -> Result<Option<Self>, ViewSessionError> {
        let Some(binding) = binding else {
            return Ok(None);
        };
        binding.validate(identity)?;
        Ok(Some(Self {
            provenance: ViewProvenance::new(identity.view, identity.generation),
            query: binding.query,
            viewport_revision: identity.viewport_revision,
            fact_generations,
        }))
    }

    pub(crate) fn authenticates(self, binding: InteractionQueryBinding) -> bool {
        self.provenance == binding.provenance && self.query == binding.query
    }

    pub(crate) const fn query_id(self) -> InteractionQueryId {
        self.query
    }

    fn is_current(
        self,
        identity: ViewQueryIdentity,
        fact_generations: [u128; 4],
    ) -> Result<bool, ViewSessionError> {
        if self.provenance != ViewProvenance::new(identity.view, identity.generation) {
            return Err(ViewSessionError::ForeignInteractionFacts);
        }
        Ok(self.viewport_revision == identity.viewport_revision
            && self.fact_generations == fact_generations)
    }
}

/// Bounded work evidence for one successfully executed query page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueryPageReceipt<W> {
    work: W,
    interaction: Option<InteractionQueryProof>,
}

impl<W> QueryPageReceipt<W> {
    pub(crate) const fn new(work: W, interaction: Option<InteractionQueryProof>) -> Self {
        Self { work, interaction }
    }

    pub fn work(&self) -> &W {
        &self.work
    }

    pub fn into_work(self) -> W {
        self.work
    }

    pub(crate) const fn interaction_proof(&self) -> Option<InteractionQueryProof> {
        self.interaction
    }
}

pub type EdgePickQueryReceipt = QueryPageReceipt<EdgePickWork>;
pub type ConnectionProbeQueryReceipt = QueryPageReceipt<ConnectionProbeWork>;
pub type NodeIntersectionQueryReceipt = QueryPageReceipt<NodeIntersectionWork>;
pub type AlignmentSnapQueryReceipt = QueryPageReceipt<AlignmentSnapWork>;

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

    pub(crate) fn validate_unique_identities(&self) -> Result<(), ViewSessionError> {
        let mut identities = HashSet::with_capacity(self.commands.len());
        for command in &self.commands {
            let identity = command.identity().clone();
            if !identities.insert(identity.clone()) {
                return Err(ViewSessionError::DuplicateCommandIdentity { identity });
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct ViewCloseOutcome {
    reason: ViewCloseReason,
    command_completions: CommandExecutionCompletions,
}

impl ViewCloseOutcome {
    pub(crate) fn new(
        reason: ViewCloseReason,
        completions: Vec<CommandExecutionCompletion>,
    ) -> Self {
        Self {
            reason,
            command_completions: CommandExecutionCompletions::from_prevalidated_ordered(
                completions,
            ),
        }
    }

    pub fn reason(&self) -> ViewCloseReason {
        self.reason
    }

    pub fn command_completions(&self) -> impl Iterator<Item = &CommandExecutionCompletion> {
        self.command_completions.iter()
    }

    pub fn command_completion(
        &self,
        identity: &GraphCommandIdentity,
    ) -> Result<&CommandExecutionCompletion, CommandCompletionCorrelationError> {
        self.command_completions.get(identity)
    }

    pub fn take_command_completion(
        &mut self,
        identity: &GraphCommandIdentity,
    ) -> Result<CommandExecutionCompletion, CommandCompletionCorrelationError> {
        self.command_completions.take(identity)
    }

    pub fn take_command_completions(
        &mut self,
        expected: &[GraphCommandIdentity],
    ) -> Result<Vec<CommandExecutionCompletion>, CommandCompletionCorrelationError> {
        self.command_completions.take_all(expected)
    }

    pub fn into_parts(self) -> (ViewCloseReason, CommandExecutionCompletions) {
        (self.reason, self.command_completions)
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
    #[error("a close request contains the same command identity more than once: {identity:?}")]
    DuplicateCommandIdentity { identity: GraphCommandIdentity },
    #[error(transparent)]
    CommandCorrelation(#[from] CommandCompletionCorrelationError),
    #[error("view session viewport is invalid")]
    InvalidViewport,
    #[error("viewport intent does not resolve to a valid transform")]
    InvalidViewportIntent,
    #[error("viewport pointer sample is not finite")]
    InvalidViewportPointer,
    #[error("viewport revision allocator is exhausted")]
    ViewportRevisionExhausted,
    #[error("node is not present in the current graph: {node:?}")]
    NodeUnavailable { node: NodeId },
    #[error("interaction facts were issued by another runtime or view generation")]
    ForeignInteractionFacts,
    #[error("interaction facts have been superseded in this view generation")]
    StaleInteractionFacts,
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
        snapshot: ViewportSnapshot,
    },
    SelectionChanged {
        selection: ViewSelectionSeed,
    },
    RuntimePatched {
        patch: RuntimePatch,
        reconciliation: RuntimePatchReconciliationReceipt,
    },
    LayoutPublished {
        layout_revision: u64,
        nodes: Vec<NodeId>,
    },
    Closed {
        reason: ViewCloseReason,
    },
}

/// Operation-scoped work evidence for reconciling one runtime patch into one view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimePatchReconciliationReceipt {
    pending_frame_impact: Option<GeometryImpactMergeWork>,
    facts: RuntimePatchFactReceipt,
    geometry: RuntimePatchGeometryReceipt,
    selection: RuntimePatchSelectionReceipt,
}

/// Runtime-fact classification work performed for one view patch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimePatchFactReceipt {
    fact_records_visited: u64,
    route_impact_entries_visited: u64,
}

/// Incremental derived-geometry work performed for one view patch.
///
/// A document reset releases owned geometry and indexes in bulk. That bulk release is identified
/// by the accompanying [`RuntimePatch`] kind and is intentionally not expanded into per-entry
/// counters here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimePatchGeometryReceipt {
    admission_nodes_visited: u64,
    node_invalidations: u64,
    spatial_updates: u64,
    route_invalidations: u64,
    route_recomputations: u64,
}

/// Selection work performed while reconciling one view patch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimePatchSelectionReceipt {
    reconciled: bool,
    membership_rebuilt: bool,
    node_membership_entries_rebuilt: u64,
    edge_membership_entries_rebuilt: u64,
    group_membership_entries_rebuilt: u64,
    route_edges_examined: u64,
}

impl RuntimePatchReconciliationReceipt {
    const fn new(
        pending_frame_impact: Option<GeometryImpactMergeWork>,
        facts: RuntimePatchFactReceipt,
        geometry: RuntimePatchGeometryReceipt,
        selection: RuntimePatchSelectionReceipt,
    ) -> Self {
        Self {
            pending_frame_impact,
            facts,
            geometry,
            selection,
        }
    }

    /// Work performed while merging this patch into an outstanding frame, when one existed.
    ///
    /// `None` means the view had no pending frame. `Some` is retained even when every counter is
    /// zero so callers can distinguish an observed patch from a skipped merge.
    #[must_use]
    pub const fn pending_frame_impact(&self) -> Option<GeometryImpactMergeWork> {
        self.pending_frame_impact
    }

    #[must_use]
    pub const fn facts(&self) -> &RuntimePatchFactReceipt {
        &self.facts
    }

    #[must_use]
    pub const fn geometry(&self) -> &RuntimePatchGeometryReceipt {
        &self.geometry
    }

    #[must_use]
    pub const fn selection(&self) -> &RuntimePatchSelectionReceipt {
        &self.selection
    }
}

impl RuntimePatchFactReceipt {
    const fn new(fact_records_visited: u64, route_impact_entries_visited: u64) -> Self {
        Self {
            fact_records_visited,
            route_impact_entries_visited,
        }
    }

    /// Returns the number of patch fact records classified for this view.
    #[must_use]
    pub const fn fact_records_visited(self) -> u64 {
        self.fact_records_visited
    }

    /// Returns the number of route-impact entries classified outside the fact record stream.
    #[must_use]
    pub const fn route_impact_entries_visited(self) -> u64 {
        self.route_impact_entries_visited
    }
}

impl RuntimePatchGeometryReceipt {
    const fn new(
        admission_nodes_visited: u64,
        node_invalidations: u64,
        spatial_updates: u64,
        route_invalidations: u64,
        route_recomputations: u64,
    ) -> Self {
        Self {
            admission_nodes_visited,
            node_invalidations,
            spatial_updates,
            route_invalidations,
            route_recomputations,
        }
    }

    /// Returns the nodes examined by the admission index's incremental patch update.
    #[must_use]
    pub const fn admission_nodes_visited(self) -> u64 {
        self.admission_nodes_visited
    }

    /// Returns layout-dirty node invalidation requests carried by the patch.
    #[must_use]
    pub const fn node_invalidations(self) -> u64 {
        self.node_invalidations
    }

    /// Returns unique node geometry entries actually removed or translated.
    #[must_use]
    pub const fn spatial_updates(self) -> u64 {
        self.spatial_updates
    }

    /// Returns route identities submitted for invalidation.
    #[must_use]
    pub const fn route_invalidations(self) -> u64 {
        self.route_invalidations
    }

    /// Returns route recomputation attempts, including unresolved routes.
    #[must_use]
    pub const fn route_recomputations(self) -> u64 {
        self.route_recomputations
    }
}

impl RuntimePatchSelectionReceipt {
    #[allow(clippy::too_many_arguments)]
    const fn new(
        reconciled: bool,
        membership_rebuilt: bool,
        node_membership_entries_rebuilt: u64,
        edge_membership_entries_rebuilt: u64,
        group_membership_entries_rebuilt: u64,
        route_edges_examined: u64,
    ) -> Self {
        Self {
            reconciled,
            membership_rebuilt,
            node_membership_entries_rebuilt,
            edge_membership_entries_rebuilt,
            group_membership_entries_rebuilt,
            route_edges_examined,
        }
    }

    /// Returns whether selection facts required authoritative reconciliation.
    #[must_use]
    pub const fn reconciled(self) -> bool {
        self.reconciled
    }

    /// Returns whether changed selection rebuilt the view's membership indexes.
    #[must_use]
    pub const fn membership_rebuilt(self) -> bool {
        self.membership_rebuilt
    }

    /// Returns new selected-node entries consumed by a membership rebuild.
    #[must_use]
    pub const fn node_membership_entries_rebuilt(self) -> u64 {
        self.node_membership_entries_rebuilt
    }

    /// Returns new selected-edge entries consumed by a membership rebuild.
    #[must_use]
    pub const fn edge_membership_entries_rebuilt(self) -> u64 {
        self.edge_membership_entries_rebuilt
    }

    /// Returns new selected-group entries consumed by a membership rebuild.
    #[must_use]
    pub const fn group_membership_entries_rebuilt(self) -> u64 {
        self.group_membership_entries_rebuilt
    }

    /// Returns unique route identities examined while updating selection paint facts.
    #[must_use]
    pub const fn route_edges_examined(self) -> u64 {
        self.route_edges_examined
    }
}

pub(crate) struct RuntimePatchApplication {
    selection_changed: Option<ViewSelectionSeed>,
    reconciliation: RuntimePatchReconciliationReceipt,
}

impl RuntimePatchApplication {
    pub(crate) fn into_parts(
        self,
    ) -> (Option<ViewSelectionSeed>, RuntimePatchReconciliationReceipt) {
        (self.selection_changed, self.reconciliation)
    }
}

#[derive(Debug, Clone)]
pub struct NodeBindingEvent {
    key: NodeBindingKey,
    patch: RuntimePatch,
}

impl NodeBindingEvent {
    pub fn node(&self) -> NodeId {
        self.key.node()
    }

    pub fn lifetime(&self) -> NodeLifetimeKey {
        self.key.lifetime()
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
    lifetime: NodeLifetimeKey,
}

impl NodeBindingKey {
    pub fn node(self) -> NodeId {
        self.lifetime.node()
    }

    pub fn lifetime(self) -> NodeLifetimeKey {
        self.lifetime
    }
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
    selected_edge_membership: SelectedEdgeMembership,
    selected_group_membership: HashSet<GroupId>,
    recent_interaction_node: Option<NodeLifetimeKey>,
    explicit_draw_order_nodes: BTreeSet<NodeId>,
    explicit_draw_order_edges: BTreeSet<EdgeId>,
    explicit_draw_order_groups: BTreeSet<GroupId>,
    retired_draw_order_nodes: BTreeSet<NodeId>,
    retired_draw_order_edges: BTreeSet<EdgeId>,
    retired_draw_order_groups: BTreeSet<GroupId>,
    viewport_revision: ViewportRevision,
    initial_viewport_claim_available: bool,
    interaction_order_revision: u128,
    environment: AdapterEnvironment,
    derived: DerivedGeometry,
    pending_frame: Option<PendingFrame>,
    last_issued_frame: u64,
    last_superseded_frame: u64,
}

struct RuntimePatchFactClassification {
    fact_records_visited: u64,
    route_impact_entries_visited: u64,
    retired_groups: BTreeSet<GroupId>,
    selection_may_change: bool,
}

impl ViewState {
    fn query_identity(&self) -> ViewQueryIdentity {
        ViewQueryIdentity::new(self.id, self.generation, self.viewport_revision)
    }

    fn recent_interaction_node_id(&self) -> Option<NodeId> {
        self.recent_interaction_node.map(NodeLifetimeKey::node)
    }

    fn reconcile_recent_interaction(&mut self, snapshot: &GraphSnapshot) -> bool {
        let Some(recent) = self.recent_interaction_node else {
            return false;
        };
        if snapshot.node_lifetime_is_current(recent)
            && self.selected_node_membership.contains(&recent.node())
        {
            return false;
        }
        self.recent_interaction_node = None;
        true
    }

    fn classify_runtime_patch_facts(
        &mut self,
        patch: &RuntimePatch,
        document_reset: bool,
    ) -> RuntimePatchFactClassification {
        let graph = patch.snapshot().graph();
        let sanitize_view_state = patch.impact().sanitize_view_state;
        let mut retired_groups = BTreeSet::new();
        let mut selection_may_change = false;
        for fact in patch.facts() {
            match fact.key() {
                RuntimeFactKey::Node(node) => {
                    selection_may_change |= self.selected_node_membership.contains(&node)
                        && (patch.node_lifetime_change(node).is_some()
                            || (sanitize_view_state
                                && fact.change_mask().contains(FactChangeMask::VISIBILITY)));
                    if !document_reset {
                        update_draw_order_retirement(
                            &mut self.explicit_draw_order_nodes,
                            &mut self.retired_draw_order_nodes,
                            node,
                            graph.node(&node).is_some_and(|node| !node.hidden),
                        );
                    }
                }
                RuntimeFactKey::Edge(edge) => {
                    selection_may_change |= self.selected_edge_membership.contains_id(edge)
                        && sanitize_view_state
                        && (fact.change_mask().contains(FactChangeMask::IDENTITY)
                            || fact.change_mask().contains(FactChangeMask::VISIBILITY));
                    if !document_reset {
                        update_draw_order_retirement(
                            &mut self.explicit_draw_order_edges,
                            &mut self.retired_draw_order_edges,
                            edge,
                            edge_is_visible(graph, edge),
                        );
                    }
                }
                RuntimeFactKey::Group(group) => {
                    let identity_changed = fact.change_mask().contains(FactChangeMask::IDENTITY);
                    selection_may_change |= self.selected_group_membership.contains(&group)
                        && sanitize_view_state
                        && identity_changed;
                    if identity_changed {
                        retired_groups.insert(group);
                    }
                    if !document_reset {
                        update_draw_order_retirement(
                            &mut self.explicit_draw_order_groups,
                            &mut self.retired_draw_order_groups,
                            group,
                            graph.group(&group).is_some(),
                        );
                    }
                }
                _ => {}
            }
        }
        for edge in &patch.impact().route_edges {
            let visible = edge_is_visible(graph, *edge);
            selection_may_change |=
                sanitize_view_state && self.selected_edge_membership.contains_id(*edge) && !visible;
            if !document_reset {
                update_draw_order_retirement(
                    &mut self.explicit_draw_order_edges,
                    &mut self.retired_draw_order_edges,
                    *edge,
                    visible,
                );
            }
        }
        RuntimePatchFactClassification {
            fact_records_visited: patch.facts().len() as u64,
            route_impact_entries_visited: patch.impact().route_edges.len() as u64,
            retired_groups,
            selection_may_change,
        }
    }

    fn rebuild_explicit_draw_order_membership(&mut self) {
        self.node_draw_order_ranks = index_draw_order(&self.facts.draw_order);
        self.edge_draw_order_ranks = index_draw_order(&self.facts.edge_draw_order);
        self.explicit_draw_order_nodes = self.facts.draw_order.iter().copied().collect();
        self.explicit_draw_order_edges = self.facts.edge_draw_order.iter().copied().collect();
        self.explicit_draw_order_groups = self.facts.group_draw_order.iter().copied().collect();
    }

    fn rebuild_selection_membership(&mut self) -> SelectedEdgeMembership {
        self.selected_node_membership = self
            .facts
            .selected_nodes
            .iter()
            .map(|node| node.node())
            .collect();
        let previous_edges = self
            .selected_edge_membership
            .replace(self.facts.selected_edges.iter().copied());
        self.selected_group_membership = self.facts.selected_groups.iter().copied().collect();
        previous_edges
    }

    fn export_seed(&self, snapshot: &GraphSnapshot) -> ViewSessionSeed {
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
        facts.reconcile_for_snapshot(snapshot, &BTreeSet::new(), false);
        facts.into_seed(snapshot.graph().graph_id())
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

impl std::fmt::Debug for WeakViewSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WeakViewSession")
            .field("provenance", &self.provenance().ok())
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

    pub fn provenance(&self) -> Result<ViewProvenance, ViewSessionError> {
        let state = self
            .shared
            .state
            .try_borrow()
            .map_err(|_| ViewSessionError::Busy)?;
        ensure_open(state.status)?;
        Ok(ViewProvenance::new(state.id, state.generation))
    }

    pub fn graph_snapshot(&self) -> Result<GraphSnapshot, ViewSessionError> {
        self.runtime()?.snapshot().map_err(Into::into)
    }

    /// Returns the full current lifetime identity for an edge in this runtime.
    pub fn edge_instance_key(
        &self,
        edge: EdgeId,
    ) -> Result<Option<EdgeInstanceKey>, ViewSessionError> {
        self.runtime()?.edge_instance_key(edge).map_err(Into::into)
    }

    /// Returns the effective runtime fallback policy used to admit graph mutations.
    pub fn graph_policy(&self) -> Result<GraphRuntimePolicy, ViewSessionError> {
        self.runtime()?.policy().map_err(Into::into)
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
        Ok(self.viewport_snapshot()?.transform())
    }

    /// Captures the exact live viewport state owned by this view generation.
    pub fn viewport_snapshot(&self) -> Result<ViewportSnapshot, ViewSessionError> {
        self.runtime()?;
        self.shared.viewport_snapshot()
    }

    /// Binds one finite native pointer point to the exact current viewport.
    pub fn viewport_pointer_sample(
        &self,
        raw: ViewPoint,
    ) -> Result<ViewportPointerSample, ViewSessionError> {
        ViewportPointerSample::new(self.viewport_snapshot()?, raw)
            .ok_or(ViewSessionError::InvalidViewportPointer)
    }

    /// Returns the full current node identity visible to this session capability.
    pub fn node_lifetime_key(
        &self,
        node: NodeId,
    ) -> Result<Option<NodeLifetimeKey>, ViewSessionError> {
        self.runtime()?.node_lifetime_key(node).map_err(Into::into)
    }

    /// Returns the full current lifetime identity for a real port in this runtime.
    pub fn port_instance_key(
        &self,
        port: PortId,
    ) -> Result<Option<PortInstanceKey>, ViewSessionError> {
        self.runtime()?.port_instance_key(port).map_err(Into::into)
    }

    /// Commits one already constructed transform without seed-style normalization.
    pub fn set_viewport(
        &self,
        transform: ViewportTransform,
    ) -> Result<ViewportSnapshot, ViewSessionError> {
        self.runtime()?;
        self.shared.set_viewport(transform)
    }

    /// Atomically claims the one renderer-provided initial viewport for this view generation.
    ///
    /// The claim is available only for an unseeded view that has not received any ordinary
    /// viewport operation. `expected` prevents a measurement computed from an older viewport from
    /// winning a race with user input. A successful no-op still consumes the one-shot claim.
    pub fn claim_initial_viewport(
        &self,
        expected: ViewportSnapshot,
        transform: ViewportTransform,
    ) -> Result<Option<ViewportSnapshot>, ViewSessionError> {
        self.runtime()?;
        self.shared.claim_initial_viewport(expected, transform)
    }

    /// Resolves and commits one viewport intent against the current live transform atomically.
    pub fn apply_viewport_intent(
        &self,
        intent: ViewportIntent,
    ) -> Result<ViewportSnapshot, ViewSessionError> {
        self.runtime()?;
        self.shared.apply_viewport_intent(intent)
    }

    /// Returns the current normalized selection in stable selection order.
    pub fn selection(&self) -> Result<ViewSelectionSeed, ViewSessionError> {
        self.runtime()?;
        self.shared.selection()
    }

    /// Captures the exact live selection owned by this view generation.
    ///
    /// Unlike persisted selection seeds, this baseline retains runtime incarnations and is safe to
    /// carry across one native interaction without selecting same-ID replacement entities.
    pub fn selection_baseline(&self) -> Result<SelectionBaseline, ViewSessionError> {
        let snapshot = self.graph_snapshot()?;
        self.shared.selection_baseline(snapshot.revision())
    }

    /// Admits a node drag from authoritative graph and view-selection facts.
    ///
    /// Hidden, non-selectable, and non-draggable nodes never enter the optimistic moved set. The
    /// returned set preserves selected peers when the clicked node is already selected or the
    /// adapter requested an additive/toggle gesture that becomes additive once dragging starts.
    ///
    /// A bare node ID retained from a frame is not an interaction capability:
    ///
    /// ```compile_fail
    /// use jellyflow_core::core::NodeId;
    /// use jellyflow_runtime::interaction::SelectionMode;
    /// use jellyflow_runtime::view::ViewSession;
    ///
    /// fn admit_stale_frame_node(view: &ViewSession, node: NodeId) {
    ///     let _ = view.node_drag_start(node, SelectionMode::Replace);
    /// }
    /// ```
    pub fn node_drag_start(
        &self,
        clicked: NodeLifetimeKey,
        selection_mode: SelectionMode,
    ) -> Result<Option<NodeDragStart>, ViewSessionError> {
        let provenance = self.provenance()?;
        let snapshot = self.graph_snapshot()?;
        let (selected_nodes, _, _) = self.shared.live_selection()?;
        admit_node_drag_start(
            provenance,
            &snapshot,
            &selected_nodes,
            clicked,
            selection_mode,
        )
    }

    /// Replaces the current selection with exact graph-entity capabilities.
    ///
    /// Persisted IDs and IDs retained from an older frame cannot be promoted through this API:
    ///
    /// ```compile_fail
    /// use jellyflow_core::core::NodeId;
    /// use jellyflow_runtime::view::ViewSession;
    ///
    /// fn select_stale_frame_node(view: &ViewSession, node: NodeId) {
    ///     let _ = view.set_selection(vec![node], Vec::new(), Vec::new());
    /// }
    /// ```
    pub fn set_selection(
        &self,
        nodes: Vec<NodeLifetimeKey>,
        edges: Vec<EdgeInstanceKey>,
        groups: Vec<GroupId>,
    ) -> Result<(), ViewSessionError> {
        let snapshot = self.graph_snapshot()?;
        self.shared.set_selection(&snapshot, nodes, edges, groups)
    }

    pub fn export_seed(&self) -> Result<ViewSessionSeed, ViewSessionError> {
        let snapshot = self.graph_snapshot()?;
        self.shared.export_seed(&snapshot)
    }

    /// Executes one revision-checked graph command through this session's runtime capability.
    pub fn execute(
        &self,
        command: GraphCommand,
    ) -> Result<CommandExecutionCompletion, ViewSessionError> {
        Ok(self.runtime()?.execute_command(command))
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
        binding: &ConnectionGestureBinding,
    ) -> Result<CreateFromWireCandidates, ViewCreateFromWireQueryError> {
        self.validate_connection_binding(binding)?;
        Ok(self
            .runtime()?
            .create_from_wire_candidates(binding.fixed_port_instance(), binding.mode())?)
    }

    /// Resolves full incident connection facts and exact work for one port query.
    pub fn port_connection_facts(
        &self,
        port: PortId,
    ) -> Result<PortConnectionQueryResult<Option<PortConnectionFacts>>, ViewSessionError> {
        self.runtime()?
            .port_connection_facts(port)
            .map_err(Into::into)
    }

    /// Resolves a constant-work connection summary and exact work for one port query.
    pub fn port_connection_summary(
        &self,
        port: PortId,
    ) -> Result<PortConnectionQueryResult<Option<PortConnectionSummary>>, ViewSessionError> {
        self.runtime()?
            .port_connection_summary(port)
            .map_err(Into::into)
    }

    /// Resolves pointer-connection facts and exact work for one port instance.
    ///
    /// A retired or foreign instance produces a successful query with a `None` outcome, preserving
    /// the negative lookup's work receipt for diagnostics and bounded-work assertions.
    ///
    /// ```compile_fail
    /// use jellyflow_core::core::PortId;
    /// use jellyflow_runtime::view::ViewSession;
    ///
    /// fn start_from_stale_frame_id(view: &ViewSession, port: PortId) {
    ///     let _ = view.port_connection_drag_intent(port);
    /// }
    /// ```
    pub fn port_connection_drag_intent(
        &self,
        port: PortInstanceKey,
    ) -> Result<PortConnectionQueryResult<Option<PortConnectionDragIntent>>, ViewSessionError> {
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
        let lifetime = runtime
            .node_lifetime_key(node)?
            .ok_or(ViewSessionError::NodeUnavailable { node })?;
        self.shared
            .subscriptions
            .subscribe_node(NodeBindingKey { lifetime }, callback)
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
        request.validate_unique_identities()?;
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

    #[cfg(test)]
    pub(crate) fn layout_revision_for_test(&self) -> u64 {
        self.shared.state.borrow().derived.layout_revision()
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

    #[cfg(test)]
    pub(crate) fn with_closing_for_test<R>(&self, callback: impl FnOnce() -> R) -> R {
        struct RestoreOpen<'a>(&'a ViewShared);

        impl Drop for RestoreOpen<'_> {
            fn drop(&mut self) {
                let mut state = self.0.state.borrow_mut();
                if matches!(state.status, ViewStatus::Closing) {
                    state.status = ViewStatus::Open;
                }
            }
        }

        self.shared
            .begin_close()
            .expect("test view must be open before entering Closing");
        let _restore = RestoreOpen(&self.shared);
        callback()
    }

    #[cfg(test)]
    pub(crate) fn exhaust_viewport_revision_for_test(&self) {
        self.shared.state.borrow_mut().viewport_revision = ViewportRevision::exhausted_for_test();
    }

    #[cfg(test)]
    pub(crate) fn advance_generation_for_test(&self) {
        let mut state = self.shared.state.borrow_mut();
        assert!(matches!(state.status, ViewStatus::Open));
        state.generation = state.generation.terminal();
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
    #[cfg(test)]
    pub(crate) fn unavailable_for_test() -> Self {
        Self {
            runtime: Weak::new(),
            view: Weak::new(),
            catalog: Arc::new(NodeCatalog::new()),
        }
    }

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

    pub(crate) fn graph_policy(&self) -> Result<GraphRuntimePolicy, ViewSessionError> {
        let (_, runtime) = self.open_capability()?;
        runtime.policy().map_err(Into::into)
    }

    pub(crate) fn live_selection(
        &self,
    ) -> Result<(Vec<NodeLifetimeKey>, Vec<EdgeInstanceKey>, Vec<GroupId>), ViewSessionError> {
        let (view, _) = self.open_capability()?;
        view.live_selection()
    }

    pub(crate) fn apply_selection_mutation(
        &self,
        mutation: SelectionMutation,
    ) -> Result<(), ViewSessionError> {
        let (view, runtime) = self.open_capability()?;
        let snapshot = runtime.snapshot().map_err(ViewSessionError::from)?;
        let provenance = view.provenance()?;
        apply_selection_mutation(&view, &snapshot, provenance, mutation)
    }

    pub(crate) fn edge_pick_receipt_is_current(
        &self,
        receipt: EdgePickQueryReceipt,
    ) -> Result<bool, ViewSessionError> {
        let (view, _) = self.open_capability()?;
        let state = view
            .state
            .try_borrow()
            .map_err(|_| ViewSessionError::Busy)?;
        ensure_open(state.status)?;
        let (path_epoch, pick_epoch) = state.derived.edge_pick_epochs();
        receipt
            .interaction_proof()
            .ok_or(ViewSessionError::ForeignInteractionFacts)?
            .is_current(
                state.query_identity(),
                [path_epoch, pick_epoch, state.interaction_order_revision, 0],
            )
    }

    pub(crate) fn connection_probe_receipt_stale_reason(
        &self,
        receipt: ConnectionProbeQueryReceipt,
    ) -> Result<Option<ConnectionProbeCursorStaleReason>, ViewSessionError> {
        let (view, runtime) = self.open_capability()?;
        let graph = runtime.snapshot().map_err(ViewSessionError::from)?;
        let state = view
            .state
            .try_borrow()
            .map_err(|_| ViewSessionError::Busy)?;
        ensure_open(state.status)?;
        let identity = state.query_identity();
        let proof = receipt
            .interaction_proof()
            .ok_or(ViewSessionError::ForeignInteractionFacts)?;
        let reason = if proof.provenance.view() != identity.view {
            Some(ConnectionProbeCursorStaleReason::ForeignView)
        } else if proof.provenance.generation() != identity.generation {
            Some(ConnectionProbeCursorStaleReason::ViewGeneration)
        } else if proof.viewport_revision != identity.viewport_revision {
            Some(ConnectionProbeCursorStaleReason::ViewportTransform)
        } else {
            let facts = proof.fact_generations;
            (facts[0] != u128::from(graph.revision()))
                .then_some(ConnectionProbeCursorStaleReason::GraphRevision)
                .or_else(|| {
                    (facts[1] != state.derived.connection_probe_index_epoch())
                        .then_some(ConnectionProbeCursorStaleReason::PortGeometry)
                })
                .or_else(|| {
                    (facts[2] != state.interaction_order_revision)
                        .then_some(ConnectionProbeCursorStaleReason::PaintOrder)
                })
        };
        Ok(reason)
    }

    pub(crate) fn node_intersection_receipt_is_current(
        &self,
        receipt: NodeIntersectionQueryReceipt,
    ) -> Result<bool, ViewSessionError> {
        let (view, _) = self.open_capability()?;
        let state = view
            .state
            .try_borrow()
            .map_err(|_| ViewSessionError::Busy)?;
        ensure_open(state.status)?;
        receipt
            .interaction_proof()
            .ok_or(ViewSessionError::ForeignInteractionFacts)?
            .is_current(
                state.query_identity(),
                [state.derived.node_spatial_index_epoch(), 0, 0, 0],
            )
    }

    pub(crate) fn alignment_snap_receipt_is_current(
        &self,
        receipt: AlignmentSnapQueryReceipt,
    ) -> Result<bool, ViewSessionError> {
        let (view, runtime) = self.open_capability()?;
        let graph = runtime.snapshot().map_err(ViewSessionError::from)?;
        let state = view
            .state
            .try_borrow()
            .map_err(|_| ViewSessionError::Busy)?;
        ensure_open(state.status)?;
        receipt
            .interaction_proof()
            .ok_or(ViewSessionError::ForeignInteractionFacts)?
            .is_current(
                state.query_identity(),
                [
                    state.derived.node_spatial_index_epoch(),
                    state.interaction_order_revision,
                    u128::from(graph.revision()),
                    0,
                ],
            )
    }

    /// Returns the exact live view generation represented by this weak capability.
    pub fn provenance(&self) -> Result<ViewProvenance, ViewSessionError> {
        let (view, _) = self.open_capability()?;
        view.provenance()
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
        Ok(self.viewport_snapshot()?.transform())
    }

    /// Captures the exact live viewport without owning or reviving the view session.
    pub fn viewport_snapshot(&self) -> Result<ViewportSnapshot, ViewSessionError> {
        let (view, _) = self.open_capability()?;
        view.viewport_snapshot()
    }

    /// Binds one finite native pointer point to the exact current viewport.
    pub fn viewport_pointer_sample(
        &self,
        raw: ViewPoint,
    ) -> Result<ViewportPointerSample, ViewSessionError> {
        let (view, _) = self.open_capability()?;
        ViewportPointerSample::new(view.viewport_snapshot()?, raw)
            .ok_or(ViewSessionError::InvalidViewportPointer)
    }

    /// Updates the viewport without owning or reviving the view session.
    pub fn set_viewport(
        &self,
        transform: ViewportTransform,
    ) -> Result<ViewportSnapshot, ViewSessionError> {
        let (view, _) = self.open_capability()?;
        view.set_viewport(transform)
    }

    /// Claims the one renderer-provided initial viewport without reviving the owning session.
    pub fn claim_initial_viewport(
        &self,
        expected: ViewportSnapshot,
        transform: ViewportTransform,
    ) -> Result<Option<ViewportSnapshot>, ViewSessionError> {
        let (view, _) = self.open_capability()?;
        view.claim_initial_viewport(expected, transform)
    }

    /// Resolves one viewport intent against the current transform without reviving the view.
    pub fn apply_viewport_intent(
        &self,
        intent: ViewportIntent,
    ) -> Result<ViewportSnapshot, ViewSessionError> {
        let (view, _) = self.open_capability()?;
        view.apply_viewport_intent(intent)
    }

    /// Returns the current normalized selection in stable selection order.
    pub fn selection(&self) -> Result<ViewSelectionSeed, ViewSessionError> {
        let (view, _) = self.open_capability()?;
        view.selection()
    }

    /// Captures the exact live selection without reviving the owning view session.
    pub fn selection_baseline(&self) -> Result<SelectionBaseline, ViewSessionError> {
        let (view, runtime) = self.open_capability()?;
        let snapshot = runtime.snapshot().map_err(ViewSessionError::from)?;
        view.selection_baseline(snapshot.revision())
    }

    /// Admits a node drag without reviving or owning the view session.
    pub fn node_drag_start(
        &self,
        clicked: NodeLifetimeKey,
        selection_mode: SelectionMode,
    ) -> Result<Option<NodeDragStart>, ViewSessionError> {
        let (view, runtime) = self.open_capability()?;
        let provenance = view.provenance()?;
        let snapshot = runtime.snapshot().map_err(ViewSessionError::from)?;
        let (selected_nodes, _, _) = view.live_selection()?;
        admit_node_drag_start(
            provenance,
            &snapshot,
            &selected_nodes,
            clicked,
            selection_mode,
        )
    }

    /// Updates the selection without owning or reviving the view session.
    pub fn set_selection(
        &self,
        nodes: Vec<NodeLifetimeKey>,
        edges: Vec<EdgeInstanceKey>,
        groups: Vec<GroupId>,
    ) -> Result<(), ViewSessionError> {
        let (view, runtime) = self.open_capability()?;
        let snapshot = runtime.snapshot()?;
        view.set_selection(&snapshot, nodes, edges, groups)
    }

    /// Exports the current persisted view facts without owning the view session.
    pub fn export_seed(&self) -> Result<ViewSessionSeed, ViewSessionError> {
        let (view, runtime) = self.open_capability()?;
        let snapshot = runtime.snapshot()?;
        view.export_seed(&snapshot)
    }

    /// Returns the exact current node lifetime without reviving a closed view capability.
    pub fn node_lifetime_key(
        &self,
        node: NodeId,
    ) -> Result<Option<NodeLifetimeKey>, ViewSessionError> {
        let (_, runtime) = self.open_capability()?;
        runtime.node_lifetime_key(node).map_err(Into::into)
    }

    /// Returns the full current port identity without reviving a closed view capability.
    pub fn port_instance_key(
        &self,
        port: PortId,
    ) -> Result<Option<PortInstanceKey>, ViewSessionError> {
        let (_, runtime) = self.open_capability()?;
        runtime.port_instance_key(port).map_err(Into::into)
    }

    /// Executes one revision-checked graph command without reviving a closed view capability.
    pub fn execute(
        &self,
        command: GraphCommand,
    ) -> Result<CommandExecutionCompletion, ViewSessionError> {
        let (_, runtime) = self.open_capability()?;
        Ok(runtime.execute_command(command))
    }

    pub fn evaluate_connection(
        &self,
        edit: ConnectionEdit,
    ) -> Result<ConnectionEvaluation, ViewSessionError> {
        let (_, runtime) = self.open_capability()?;
        runtime.evaluate_connection(edit).map_err(Into::into)
    }

    pub(crate) fn promote_node_interaction(
        &self,
        node: NodeLifetimeKey,
    ) -> Result<bool, ViewSessionError> {
        let (view, runtime) = self.open_capability()?;
        let snapshot = runtime.snapshot().map_err(ViewSessionError::from)?;
        view.promote_node_interaction(&snapshot, node)
    }

    pub fn create_from_wire_candidates(
        &self,
        binding: &ConnectionGestureBinding,
    ) -> Result<CreateFromWireCandidates, ViewCreateFromWireQueryError> {
        let (_, runtime) = self.open_capability()?;
        let snapshot = runtime.snapshot().map_err(ViewSessionError::from)?;
        if binding.provenance() != snapshot.provenance() {
            return Err(ViewSessionError::ForeignInteractionFacts.into());
        }
        let policy = runtime.policy().map_err(ViewSessionError::from)?;
        if !binding.is_interactive(&snapshot, policy) {
            return Err(ViewSessionError::StaleInteractionFacts.into());
        }
        Ok(runtime.create_from_wire_candidates(binding.fixed_port_instance(), binding.mode())?)
    }

    /// Resolves full incident connection facts without reviving either capability.
    pub fn port_connection_facts(
        &self,
        port: PortId,
    ) -> Result<PortConnectionQueryResult<Option<PortConnectionFacts>>, ViewSessionError> {
        let (_, runtime) = self.open_capability()?;
        runtime.port_connection_facts(port).map_err(Into::into)
    }

    /// Resolves a constant-work connection summary without reviving either capability.
    pub fn port_connection_summary(
        &self,
        port: PortId,
    ) -> Result<PortConnectionQueryResult<Option<PortConnectionSummary>>, ViewSessionError> {
        let (_, runtime) = self.open_capability()?;
        runtime.port_connection_summary(port).map_err(Into::into)
    }

    /// Resolves a drag decision without reviving either capability.
    pub fn port_connection_drag_intent(
        &self,
        port: PortInstanceKey,
    ) -> Result<PortConnectionQueryResult<Option<PortConnectionDragIntent>>, ViewSessionError> {
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
        let lifetime = runtime
            .node_lifetime_key(node)?
            .ok_or(ViewSessionError::NodeUnavailable { node })?;
        view.subscriptions
            .subscribe_node(NodeBindingKey { lifetime }, callback)
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
        policy: GraphRuntimePolicy,
        snapshot: &GraphSnapshot,
        seed: ViewSessionSeed,
        initial_viewport_claim_available: bool,
    ) -> Rc<Self> {
        let facts = ViewFacts::from_seed(seed, snapshot);
        let explicit_draw_order_nodes = facts.draw_order.iter().copied().collect();
        let explicit_draw_order_edges = facts.edge_draw_order.iter().copied().collect();
        let explicit_draw_order_groups = facts.group_draw_order.iter().copied().collect();
        let node_draw_order_ranks = index_draw_order(&facts.draw_order);
        let edge_draw_order_ranks = index_draw_order(&facts.edge_draw_order);
        let selected_node_membership = facts
            .selected_nodes
            .iter()
            .map(|node| node.node())
            .collect();
        let selected_edge_membership =
            SelectedEdgeMembership::new(facts.selected_edges.iter().copied());
        let selected_group_membership = facts.selected_groups.iter().copied().collect();
        Rc::new(Self {
            state: RefCell::new(ViewState {
                id,
                generation,
                status: ViewStatus::Open,
                seen_revision: snapshot.revision(),
                policy,
                facts,
                node_draw_order_ranks,
                edge_draw_order_ranks,
                selected_node_membership,
                selected_edge_membership,
                selected_group_membership,
                recent_interaction_node: None,
                explicit_draw_order_nodes,
                explicit_draw_order_edges,
                explicit_draw_order_groups,
                retired_draw_order_nodes: BTreeSet::new(),
                retired_draw_order_edges: BTreeSet::new(),
                retired_draw_order_groups: BTreeSet::new(),
                viewport_revision: ViewportRevision::initial(),
                initial_viewport_claim_available,
                interaction_order_revision: 0,
                environment: AdapterEnvironment::default(),
                derived: DerivedGeometry::new(ViewProvenance::new(id, generation)),
                pending_frame: None,
                last_issued_frame: 0,
                last_superseded_frame: 0,
            }),
            subscriptions: subscriptions::ViewEventHub::new(id),
        })
    }

    fn viewport_snapshot(&self) -> Result<ViewportSnapshot, ViewSessionError> {
        let state = self
            .state
            .try_borrow()
            .map_err(|_| ViewSessionError::Busy)?;
        ensure_open(state.status)?;
        let transform = ViewportTransform::new(state.facts.pan, state.facts.zoom)
            .ok_or(ViewSessionError::InvalidViewport)?;
        Ok(ViewportSnapshot::new(
            ViewProvenance::new(state.id, state.generation),
            state.viewport_revision,
            transform,
        ))
    }

    fn set_viewport(
        &self,
        transform: ViewportTransform,
    ) -> Result<ViewportSnapshot, ViewSessionError> {
        self.update_viewport(|_| Some(transform), ViewSessionError::InvalidViewport)
    }

    fn claim_initial_viewport(
        &self,
        expected: ViewportSnapshot,
        transform: ViewportTransform,
    ) -> Result<Option<ViewportSnapshot>, ViewSessionError> {
        if !transform.is_valid() {
            return Err(ViewSessionError::InvalidViewport);
        }
        let (snapshot, changed) = {
            let mut state = self
                .state
                .try_borrow_mut()
                .map_err(|_| ViewSessionError::Busy)?;
            ensure_open(state.status)?;
            let provenance = ViewProvenance::new(state.id, state.generation);
            let current = ViewportTransform::new(state.facts.pan, state.facts.zoom)
                .ok_or(ViewSessionError::InvalidViewport)?;
            let current_snapshot =
                ViewportSnapshot::new(provenance, state.viewport_revision, current);
            if !state.initial_viewport_claim_available || expected != current_snapshot {
                return Ok(None);
            }
            let revision = if transform == current {
                state.viewport_revision
            } else {
                state
                    .viewport_revision
                    .checked_next()
                    .ok_or(ViewSessionError::ViewportRevisionExhausted)?
            };
            state.initial_viewport_claim_available = false;
            if transform != current {
                state.facts.set_viewport(transform);
                state.viewport_revision = revision;
            }
            (
                ViewportSnapshot::new(provenance, revision, transform),
                transform != current,
            )
        };
        if changed {
            resume(
                self.subscriptions
                    .publish_event(ViewSessionEvent::ViewportChanged { snapshot }),
            );
        }
        Ok(Some(snapshot))
    }

    fn apply_viewport_intent(
        &self,
        intent: ViewportIntent,
    ) -> Result<ViewportSnapshot, ViewSessionError> {
        self.update_viewport(
            |current| intent.resolve(current),
            ViewSessionError::InvalidViewportIntent,
        )
    }

    fn update_viewport(
        &self,
        resolve: impl FnOnce(ViewportTransform) -> Option<ViewportTransform>,
        invalid: ViewSessionError,
    ) -> Result<ViewportSnapshot, ViewSessionError> {
        let (snapshot, changed) = {
            let mut state = self
                .state
                .try_borrow_mut()
                .map_err(|_| ViewSessionError::Busy)?;
            ensure_open(state.status)?;
            let current = ViewportTransform::new(state.facts.pan, state.facts.zoom)
                .ok_or(ViewSessionError::InvalidViewport)?;
            let next = resolve(current).ok_or(invalid)?;
            if !next.is_valid() {
                return Err(ViewSessionError::InvalidViewport);
            }
            let provenance = ViewProvenance::new(state.id, state.generation);
            if next == current {
                state.initial_viewport_claim_available = false;
                (
                    ViewportSnapshot::new(provenance, state.viewport_revision, current),
                    false,
                )
            } else {
                let revision = state
                    .viewport_revision
                    .checked_next()
                    .ok_or(ViewSessionError::ViewportRevisionExhausted)?;
                state.initial_viewport_claim_available = false;
                state.facts.set_viewport(next);
                state.viewport_revision = revision;
                (ViewportSnapshot::new(provenance, revision, next), true)
            }
        };
        if changed {
            resume(
                self.subscriptions
                    .publish_event(ViewSessionEvent::ViewportChanged { snapshot }),
            );
        }
        Ok(snapshot)
    }

    fn selection(&self) -> Result<ViewSelectionSeed, ViewSessionError> {
        let state = self
            .state
            .try_borrow()
            .map_err(|_| ViewSessionError::Busy)?;
        ensure_open(state.status)?;
        Ok(state.facts.selection())
    }

    fn live_selection(
        &self,
    ) -> Result<(Vec<NodeLifetimeKey>, Vec<EdgeInstanceKey>, Vec<GroupId>), ViewSessionError> {
        let state = self
            .state
            .try_borrow()
            .map_err(|_| ViewSessionError::Busy)?;
        ensure_open(state.status)?;
        Ok((
            state.facts.selected_nodes.clone(),
            state.facts.selected_edges.clone(),
            state.facts.selected_groups.clone(),
        ))
    }

    fn selection_baseline(
        &self,
        graph_revision: GraphRevision,
    ) -> Result<SelectionBaseline, ViewSessionError> {
        let state = self
            .state
            .try_borrow()
            .map_err(|_| ViewSessionError::Busy)?;
        ensure_open(state.status)?;
        Ok(SelectionBaseline::new(
            ViewProvenance::new(state.id, state.generation),
            graph_revision,
            state.facts.selected_nodes.iter().copied(),
            state.facts.selected_edges.iter().copied(),
            state.facts.selected_groups.iter().copied(),
        ))
    }

    fn promote_node_interaction(
        &self,
        snapshot: &GraphSnapshot,
        node: NodeLifetimeKey,
    ) -> Result<bool, ViewSessionError> {
        if node.provenance() != snapshot.provenance() {
            return Err(ViewSessionError::ForeignInteractionFacts);
        }
        if !snapshot.node_lifetime_is_current(node)
            || snapshot
                .graph()
                .node(&node.node())
                .is_none_or(|record| record.hidden || !record.selectable.unwrap_or(true))
        {
            return Err(ViewSessionError::StaleInteractionFacts);
        }
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| ViewSessionError::Busy)?;
        ensure_open(state.status)?;
        if state.recent_interaction_node == Some(node) {
            return Ok(false);
        }
        state.recent_interaction_node = Some(node);
        state.interaction_order_revision = state.interaction_order_revision.saturating_add(1);
        Ok(true)
    }

    fn set_selection(
        &self,
        snapshot: &GraphSnapshot,
        nodes: Vec<NodeLifetimeKey>,
        edges: Vec<EdgeInstanceKey>,
        groups: Vec<GroupId>,
    ) -> Result<(), ViewSessionError> {
        ensure_selection_keys_are_current(snapshot, &nodes, &edges)?;
        let changed = {
            let mut state = self
                .state
                .try_borrow_mut()
                .map_err(|_| ViewSessionError::Busy)?;
            ensure_open(state.status)?;
            let before_nodes = state.facts.selected_nodes.clone();
            let before_edges = state.facts.selected_edges.clone();
            let before_groups = state.facts.selected_groups.clone();
            state
                .facts
                .set_live_selection(snapshot, nodes, edges, groups);
            let selection = state.facts.selection();
            if state.facts.selected_nodes == before_nodes
                && state.facts.selected_edges == before_edges
                && state.facts.selected_groups == before_groups
            {
                None
            } else {
                let previous_edges = state.rebuild_selection_membership();
                state.reconcile_recent_interaction(snapshot);
                state.interaction_order_revision =
                    state.interaction_order_revision.saturating_add(1);
                let changed_edges = previous_edges
                    .symmetric_difference(&state.selected_edge_membership)
                    .map(|edge| edge.edge())
                    .collect::<BTreeSet<_>>();
                let ViewState {
                    derived,
                    selected_edge_membership,
                    ..
                } = &mut *state;
                derived.update_route_selection(&changed_edges, selected_edge_membership);
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

    fn set_live_selection(
        &self,
        snapshot: &GraphSnapshot,
        nodes: Vec<NodeLifetimeKey>,
        edges: Vec<EdgeInstanceKey>,
        groups: Vec<GroupId>,
    ) -> Result<(), ViewSessionError> {
        let changed = {
            let mut state = self
                .state
                .try_borrow_mut()
                .map_err(|_| ViewSessionError::Busy)?;
            ensure_open(state.status)?;
            let before_nodes = state.facts.selected_nodes.clone();
            let before_edges = state.facts.selected_edges.clone();
            let before_groups = state.facts.selected_groups.clone();
            state
                .facts
                .set_live_selection(snapshot, nodes, edges, groups);
            let selection = state.facts.selection();
            if state.facts.selected_nodes == before_nodes
                && state.facts.selected_edges == before_edges
                && state.facts.selected_groups == before_groups
            {
                None
            } else {
                let previous_edges = state.rebuild_selection_membership();
                state.reconcile_recent_interaction(snapshot);
                state.interaction_order_revision =
                    state.interaction_order_revision.saturating_add(1);
                let changed_edges = previous_edges
                    .symmetric_difference(&state.selected_edge_membership)
                    .map(|edge| edge.edge())
                    .collect::<BTreeSet<_>>();
                let ViewState {
                    derived,
                    selected_edge_membership,
                    ..
                } = &mut *state;
                derived.update_route_selection(&changed_edges, selected_edge_membership);
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

    fn export_seed(&self, snapshot: &GraphSnapshot) -> Result<ViewSessionSeed, ViewSessionError> {
        let state = self
            .state
            .try_borrow()
            .map_err(|_| ViewSessionError::Busy)?;
        ensure_open(state.status)?;
        Ok(state.export_seed(snapshot))
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

    pub(crate) fn provenance(&self) -> Result<ViewProvenance, ViewSessionError> {
        let state = self
            .state
            .try_borrow()
            .map_err(|_| ViewSessionError::Busy)?;
        ensure_open(state.status)?;
        Ok(ViewProvenance::new(state.id, state.generation))
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

    pub(crate) fn apply_runtime_patch(
        &self,
        patch: &RuntimePatch,
    ) -> Option<RuntimePatchApplication> {
        for node in patch.affected_nodes() {
            if patch.node_lifetime_change(*node).is_some() {
                self.subscriptions.retire_node(*node);
            }
        }
        let mut state = self.state.borrow_mut();
        if !matches!(state.status, ViewStatus::Open) {
            return None;
        }
        let document_reset = patch.kind() == RuntimePatchKind::DocumentReset;
        let pending_frame_impact = state.pending_frame.as_mut().map(|pending| {
            let (fallback, records_merged) = pending.observe_patch(patch);
            GeometryImpactMergeWork::new(1, records_merged, u64::from(fallback))
        });
        state.seen_revision = patch.to_revision();

        if document_reset {
            state.retired_draw_order_nodes.clear();
            state.retired_draw_order_edges.clear();
            state.retired_draw_order_groups.clear();
        }
        let fact_classification = state.classify_runtime_patch_facts(patch, document_reset);
        let reconcile_selection = document_reset || fact_classification.selection_may_change;
        let mut changed_edges = BTreeSet::new();
        let selection_changed = reconcile_selection.then(|| {
            let before = state.facts.selection();
            if document_reset {
                state.facts.reconcile_for_snapshot(
                    patch.snapshot(),
                    &fact_classification.retired_groups,
                    true,
                );
                state.rebuild_explicit_draw_order_membership();
            } else {
                state.facts.reconcile_selection(
                    patch.snapshot(),
                    &fact_classification.retired_groups,
                    false,
                );
            }
            let after = state.facts.selection();
            if after != before {
                let previous_edges = state.rebuild_selection_membership();
                changed_edges.extend(
                    previous_edges
                        .symmetric_difference(&state.selected_edge_membership)
                        .map(|edge| edge.edge()),
                );
                state.interaction_order_revision =
                    state.interaction_order_revision.saturating_add(1);
                Some(after)
            } else {
                None
            }
        });
        let selection_order_changed = selection_changed
            .as_ref()
            .is_some_and(|selection| selection.is_some());
        let (
            node_membership_entries_rebuilt,
            edge_membership_entries_rebuilt,
            group_membership_entries_rebuilt,
        ) = if selection_order_changed {
            (
                state.facts.selected_nodes.len() as u64,
                state.facts.selected_edges.len() as u64,
                state.facts.selected_groups.len() as u64,
            )
        } else {
            (0, 0, 0)
        };
        if state.reconcile_recent_interaction(patch.snapshot()) && !selection_order_changed {
            state.interaction_order_revision = state.interaction_order_revision.saturating_add(1);
        }
        let policy = state.policy;
        let ViewState {
            derived,
            selected_edge_membership,
            ..
        } = &mut *state;
        let geometry_work = derived.apply_runtime_patch(patch, policy, selected_edge_membership);
        let route_edges_examined =
            derived.update_route_selection(&changed_edges, selected_edge_membership);
        Some(RuntimePatchApplication {
            selection_changed: selection_changed.flatten(),
            reconciliation: RuntimePatchReconciliationReceipt::new(
                pending_frame_impact,
                RuntimePatchFactReceipt::new(
                    fact_classification.fact_records_visited,
                    fact_classification.route_impact_entries_visited,
                ),
                RuntimePatchGeometryReceipt::new(
                    geometry_work.admission_nodes_visited,
                    geometry_work.node_invalidations,
                    geometry_work.spatial_updates,
                    geometry_work.route_invalidations,
                    geometry_work.route_recomputations,
                ),
                RuntimePatchSelectionReceipt::new(
                    reconcile_selection,
                    selection_order_changed,
                    node_membership_entries_rebuilt,
                    edge_membership_entries_rebuilt,
                    group_membership_entries_rebuilt,
                    route_edges_examined,
                ),
            ),
        })
    }

    pub(crate) fn publish_runtime_patch(
        &self,
        generation: ViewGeneration,
        patch: RuntimePatch,
        selection_changed: Option<ViewSelectionSeed>,
        reconciliation: RuntimePatchReconciliationReceipt,
    ) -> Option<PanicPayload> {
        if self.open_generation() != Some(generation) {
            return None;
        }
        let mut panics = PanicCollector::default();
        panics.record(
            self.subscriptions
                .publish_event(ViewSessionEvent::RuntimePatched {
                    patch: patch.clone(),
                    reconciliation,
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
            let (_, callback_panic) = self.subscriptions.publish_node(*node, &patch);
            panics.record(callback_panic);
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
        state.derived.reset();
        state.pending_frame = None;
        drop(state);
        (true, self.subscriptions.publish_terminal(reason))
    }
}

fn admit_node_drag_start(
    provenance: ViewProvenance,
    snapshot: &GraphSnapshot,
    selected_nodes: &[NodeLifetimeKey],
    clicked: NodeLifetimeKey,
    selection_mode: SelectionMode,
) -> Result<Option<NodeDragStart>, ViewSessionError> {
    if clicked.provenance() != snapshot.provenance() {
        return Err(ViewSessionError::ForeignInteractionFacts);
    }
    if !snapshot.node_lifetime_is_current(clicked) {
        return Err(ViewSessionError::StaleInteractionFacts);
    }
    let graph = snapshot.graph();
    let clicked_node = graph
        .node(&clicked.node())
        .ok_or(ViewSessionError::StaleInteractionFacts)?;
    if clicked_node.hidden
        || !clicked_node.selectable.unwrap_or(true)
        || !clicked_node.draggable.unwrap_or(true)
    {
        return Ok(None);
    }
    let clicked_was_selected = selected_nodes.contains(&clicked);
    let preserve_selected_peers = clicked_was_selected
        || matches!(selection_mode, SelectionMode::Add | SelectionMode::Toggle);
    let peers = preserve_selected_peers
        .then_some(selected_nodes)
        .into_iter()
        .flatten()
        .copied()
        .filter(|node| {
            snapshot.node_lifetime_is_current(*node)
                && graph.node(&node.node()).is_some_and(|record| {
                    !record.hidden
                        && record.selectable.unwrap_or(true)
                        && record.draggable.unwrap_or(true)
                })
        });
    Ok(NodeDragStart::new(
        provenance,
        clicked,
        peers,
        clicked_node.pos,
        selection_mode,
    ))
}

fn apply_selection_mutation(
    view: &ViewShared,
    snapshot: &GraphSnapshot,
    expected_provenance: ViewProvenance,
    mutation: SelectionMutation,
) -> Result<(), ViewSessionError> {
    match mutation {
        SelectionMutation::Node {
            provenance,
            node,
            mode,
        } => {
            ensure_selection_provenance(expected_provenance, provenance)?;
            if node.provenance() != snapshot.provenance() {
                return Err(ViewSessionError::ForeignInteractionFacts);
            }
            if !snapshot.node_lifetime_is_current(node)
                || snapshot
                    .graph()
                    .node(&node.node())
                    .is_none_or(|record| record.hidden || !record.selectable.unwrap_or(true))
            {
                return Err(ViewSessionError::StaleInteractionFacts);
            }
            let (mut nodes, mut edges, mut groups) = view.live_selection()?;
            if mode != SelectionMode::Replace || !nodes.contains(&node) {
                apply_item_selection(&mut nodes, node, mode);
            }
            if mode == SelectionMode::Replace {
                edges.clear();
                groups.clear();
            }
            view.set_live_selection(snapshot, nodes, edges, groups)
        }
        SelectionMutation::Edge {
            provenance,
            instance,
            mode,
        } => {
            ensure_selection_provenance(expected_provenance, provenance)?;
            if instance.provenance() != snapshot.provenance() {
                return Err(ViewSessionError::ForeignInteractionFacts);
            }
            if !snapshot.edge_instance_is_current(instance)
                || !edge_is_visible(snapshot.graph(), instance.edge())
            {
                return Err(ViewSessionError::StaleInteractionFacts);
            }
            let (mut nodes, mut edges, mut groups) = view.live_selection()?;
            apply_item_selection(&mut edges, instance, mode);
            if mode == SelectionMode::Replace {
                nodes.clear();
                groups.clear();
            }
            view.set_live_selection(snapshot, nodes, edges, groups)
        }
        SelectionMutation::Marquee {
            provenance,
            nodes: intersecting,
            mode,
            baseline,
        } => {
            ensure_selection_provenance(expected_provenance, provenance)?;
            ensure_selection_provenance(expected_provenance, baseline.provenance())?;
            ensure_selection_keys_belong_to_snapshot(
                snapshot,
                baseline.nodes().iter().copied(),
                baseline.edges().iter().copied(),
            )?;
            if intersecting
                .iter()
                .any(|node| node.provenance() != snapshot.provenance())
            {
                return Err(ViewSessionError::ForeignInteractionFacts);
            }
            let intersecting = intersecting.iter().copied().filter(|node| {
                snapshot.node_lifetime_is_current(*node)
                    && snapshot
                        .graph()
                        .node(&node.node())
                        .is_some_and(|record| !record.hidden && record.selectable.unwrap_or(true))
            });
            let (nodes, edges, groups) =
                marquee_selection_from_baseline(snapshot, &baseline, intersecting, mode);
            view.set_live_selection(snapshot, nodes, edges, groups)
        }
        SelectionMutation::Clear { provenance } => {
            ensure_selection_provenance(expected_provenance, provenance)?;
            view.set_live_selection(snapshot, Vec::new(), Vec::new(), Vec::new())
        }
        SelectionMutation::Restore(baseline) => {
            ensure_selection_provenance(expected_provenance, baseline.provenance())?;
            ensure_selection_keys_belong_to_snapshot(
                snapshot,
                baseline.nodes().iter().copied(),
                baseline.edges().iter().copied(),
            )?;
            let (nodes, edges, groups) = live_baseline_selection(snapshot, &baseline);
            view.set_live_selection(snapshot, nodes, edges, groups)
        }
    }
}

fn apply_item_selection<T: Copy + Eq>(items: &mut Vec<T>, item: T, mode: SelectionMode) {
    match mode {
        SelectionMode::Replace => {
            items.clear();
            items.push(item);
        }
        SelectionMode::Add => {
            if !items.contains(&item) {
                items.push(item);
            }
        }
        SelectionMode::Toggle => {
            if let Some(index) = items.iter().position(|current| *current == item) {
                items.remove(index);
            } else {
                items.push(item);
            }
        }
    }
}

fn apply_many_selection<T: Copy + Ord>(
    items: &mut Vec<T>,
    incoming: impl IntoIterator<Item = T>,
    mode: SelectionMode,
) {
    if mode == SelectionMode::Replace {
        items.clear();
    }
    let mut positions = items
        .iter()
        .copied()
        .enumerate()
        .map(|(index, item)| (item, index))
        .collect::<BTreeMap<_, _>>();
    let mut ordered = items.iter().copied().map(Some).collect::<Vec<_>>();
    for item in incoming {
        match mode {
            SelectionMode::Replace | SelectionMode::Add => {
                if let std::collections::btree_map::Entry::Vacant(entry) = positions.entry(item) {
                    entry.insert(ordered.len());
                    ordered.push(Some(item));
                }
            }
            SelectionMode::Toggle => {
                if let Some(index) = positions.remove(&item) {
                    ordered[index] = None;
                } else {
                    positions.insert(item, ordered.len());
                    ordered.push(Some(item));
                }
            }
        }
    }
    *items = ordered.into_iter().flatten().collect();
}

fn ensure_selection_provenance(
    expected: ViewProvenance,
    actual: ViewProvenance,
) -> Result<(), ViewSessionError> {
    if actual != expected {
        return Err(ViewSessionError::ForeignInteractionFacts);
    }
    Ok(())
}

fn ensure_selection_keys_belong_to_snapshot(
    snapshot: &GraphSnapshot,
    nodes: impl IntoIterator<Item = NodeLifetimeKey>,
    edges: impl IntoIterator<Item = EdgeInstanceKey>,
) -> Result<(), ViewSessionError> {
    if nodes
        .into_iter()
        .any(|node| node.provenance() != snapshot.provenance())
        || edges
            .into_iter()
            .any(|edge| edge.provenance() != snapshot.provenance())
    {
        return Err(ViewSessionError::ForeignInteractionFacts);
    }
    Ok(())
}

fn ensure_selection_keys_are_current(
    snapshot: &GraphSnapshot,
    nodes: &[NodeLifetimeKey],
    edges: &[EdgeInstanceKey],
) -> Result<(), ViewSessionError> {
    ensure_selection_keys_belong_to_snapshot(
        snapshot,
        nodes.iter().copied(),
        edges.iter().copied(),
    )?;
    if nodes
        .iter()
        .any(|node| !snapshot.node_lifetime_is_current(*node))
        || edges
            .iter()
            .any(|edge| !snapshot.edge_instance_is_current(*edge))
    {
        return Err(ViewSessionError::StaleInteractionFacts);
    }
    Ok(())
}

fn live_baseline_selection(
    snapshot: &GraphSnapshot,
    baseline: &SelectionBaseline,
) -> (Vec<NodeLifetimeKey>, Vec<EdgeInstanceKey>, Vec<GroupId>) {
    let graph = snapshot.graph();
    let nodes = baseline
        .nodes()
        .iter()
        .copied()
        .filter(|node| {
            snapshot.node_lifetime_is_current(*node)
                && graph
                    .node(&node.node())
                    .is_some_and(|record| !record.hidden)
        })
        .collect();
    let edges = baseline
        .edges()
        .iter()
        .copied()
        .filter(|edge| {
            snapshot.edge_instance_is_current(*edge) && edge_is_visible(graph, edge.edge())
        })
        .collect();
    let groups = if baseline.graph_revision() == snapshot.revision() {
        baseline
            .groups()
            .iter()
            .copied()
            .filter(|group| graph.group(group).is_some())
            .collect()
    } else {
        Vec::new()
    };
    (nodes, edges, groups)
}

fn marquee_selection_from_baseline(
    snapshot: &GraphSnapshot,
    baseline: &SelectionBaseline,
    intersecting: impl IntoIterator<Item = NodeLifetimeKey>,
    mode: SelectionMode,
) -> (Vec<NodeLifetimeKey>, Vec<EdgeInstanceKey>, Vec<GroupId>) {
    let (mut nodes, mut edges, mut groups) = live_baseline_selection(snapshot, baseline);
    apply_many_selection(&mut nodes, intersecting, mode);
    if mode == SelectionMode::Replace {
        edges.clear();
        groups.clear();
    }
    (nodes, edges, groups)
}

fn ensure_open(status: ViewStatus) -> Result<(), ViewSessionError> {
    match status {
        ViewStatus::Open => Ok(()),
        ViewStatus::Closing => Err(ViewSessionError::Closing),
        ViewStatus::Closed(reason) => Err(ViewSessionError::Closed { reason }),
    }
}
