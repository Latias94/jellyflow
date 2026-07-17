//! Authoritative graph document runtime shared by independently disposable views.

mod create_from_wire;
mod execution;
mod index;
mod patch;
mod port_connection_facts;
mod subscriptions;

pub use create_from_wire::{CreateFromWireCandidates, CreateFromWireQueryError};
pub use port_connection_facts::{
    PortConnectionDragFacts, PortConnectionDragIntent, PortConnectionFacts,
    PortIncidentConnectionFacts,
};

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::graph_kernel::GraphHistory;
use jellyflow_core::core::{EdgeId, Graph, GraphValidationError, NodeId};

use crate::io::ViewSelectionSeed;
use crate::profile::{ApplyPipelineError, GraphProfile, validate_profile_document};
use crate::runtime::commands::{
    ConnectionEdit, ConnectionEvaluation, GraphCommand, GraphCommandOutcome, GraphCommandReceipt,
    plan_graph_command,
};
use crate::runtime::middleware::GraphRuntimeMiddleware;
use crate::runtime::unwind::{PanicCollector, PanicPayload, resume, suppress};
use crate::schema::{CanonicalGraph, NodeCatalog, NodeCatalogMigrationError};

pub use crate::runtime::policy::GraphRuntimePolicy;

use crate::runtime::view_session::{
    AttachViewError, ViewCloseOutcome, ViewCloseReason, ViewCloseReceipt, ViewCloseRequest,
    ViewGeneration, ViewId, ViewSession, ViewSessionError, ViewSessionOptions, ViewShared,
};

use self::execution::collect_live_sessions;
use self::execution::{
    CommandExecution, MutationGuard, NotificationDrainGuard, allocate_incarnation,
    command_rejection_from_planning, execute_command_locked, middleware_snapshot, validate_graph,
};
use self::index::IncidentNodeQuery;

static LAST_RUNTIME_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct RuntimeId(u64);

impl RuntimeId {
    pub(crate) const UNAVAILABLE: Self = Self(0);

    fn allocate() -> Result<Self, GraphRuntimeError> {
        let previous = LAST_RUNTIME_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                current.checked_add(1)
            })
            .map_err(|_| GraphRuntimeError::IdentityExhausted)?;
        previous
            .checked_add(1)
            .map(Self)
            .ok_or(GraphRuntimeError::IdentityExhausted)
    }
}

pub(crate) use self::index::GraphIndex;
pub use self::patch::{
    FactChange, FactChangeMask, RuntimeFactDelta, RuntimeFactKey, RuntimePatch, RuntimePatchKind,
};
pub use self::subscriptions::GraphRuntimeSubscriptionToken;

pub type GraphRevision = u64;
pub type DocumentEpoch = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeIncarnation(u64);

impl NodeIncarnation {
    pub fn get(self) -> u64 {
        self.0
    }
}

pub(crate) type IncidentEdgeSnapshot = Arc<BTreeSet<EdgeId>>;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct IncidentSnapshotWork {
    entries_read: u64,
    materializations: u64,
}

impl IncidentSnapshotWork {
    pub(crate) fn materialize(&mut self, snapshot: &BTreeSet<EdgeId>) {
        self.entries_read = self.entries_read.saturating_add(snapshot.len() as u64);
        self.materializations = self.materializations.saturating_add(1);
    }
}

#[derive(Debug, Clone)]
pub struct GraphSnapshot {
    graph: Arc<Graph>,
    revision: GraphRevision,
    document_epoch: DocumentEpoch,
}

impl GraphSnapshot {
    fn new(graph: Arc<Graph>, revision: GraphRevision, document_epoch: DocumentEpoch) -> Self {
        Self {
            graph,
            revision,
            document_epoch,
        }
    }

    pub fn graph(&self) -> &Graph {
        &self.graph
    }

    pub fn revision(&self) -> GraphRevision {
        self.revision
    }

    pub fn document_epoch(&self) -> DocumentEpoch {
        self.document_epoch
    }

    pub fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.graph, &other.graph)
            && self.revision == other.revision
            && self.document_epoch == other.document_epoch
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HistoryState {
    undo_len: usize,
    redo_len: usize,
}

impl HistoryState {
    pub fn undo_len(self) -> usize {
        self.undo_len
    }

    pub fn redo_len(self) -> usize {
        self.redo_len
    }

    pub fn can_undo(self) -> bool {
        self.undo_len > 0
    }

    pub fn can_redo(self) -> bool {
        self.redo_len > 0
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GraphRuntimeStats {
    pub patch_builds: u64,
    pub patch_deliveries: u64,
    pub command_planning_fact_reads: u64,
    pub command_planning_index_bucket_reads: u64,
    pub command_planning_index_entries_read: u64,
    pub command_planning_operations_emitted: u64,
    pub command_planning_full_type_revalidations: u64,
    pub connection_query_fact_reads: u64,
    pub connection_query_index_bucket_reads: u64,
    pub connection_query_index_entries_read: u64,
    pub frame_incident_routing_index_bucket_reads: u64,
    pub frame_incident_routing_index_entries_read: u64,
    pub frame_incident_snapshot_entries_read: u64,
    pub frame_incident_snapshot_materializations: u64,
    pub feedback_incident_snapshot_entries_read: u64,
    pub feedback_incident_snapshot_materializations: u64,
}

#[derive(Debug, Clone)]
pub enum GraphRuntimeEvent {
    Patched { patch: RuntimePatch },
    HistoryChanged { state: HistoryState },
}

#[derive(Debug, thiserror::Error)]
pub enum GraphRuntimeError {
    #[error("graph runtime state is currently borrowed")]
    Busy,
    #[error("a graph mutation cannot be started reentrantly")]
    ReentrantMutation,
    #[error("graph revision is exhausted")]
    RevisionExhausted,
    #[error("document epoch is exhausted")]
    DocumentEpochExhausted,
    #[error("canonical graph was produced by a different semantic catalog provenance")]
    CatalogProvenanceMismatch,
    #[error("runtime identity allocator is exhausted")]
    IdentityExhausted,
    #[error("graph structure is invalid")]
    InvalidGraph { errors: Vec<GraphValidationError> },
    #[error("graph canonicalization failed: {0}")]
    Canonicalization(#[from] NodeCatalogMigrationError),
    #[error(transparent)]
    Apply(#[from] ApplyPipelineError),
}

struct RuntimeState {
    graph: Arc<Graph>,
    revision: GraphRevision,
    document_epoch: DocumentEpoch,
    policy: GraphRuntimePolicy,
    history: GraphHistory,
    profile: Option<Box<dyn GraphProfile>>,
    middleware: Option<Box<dyn GraphRuntimeMiddleware>>,
    index: GraphIndex,
    incarnations: BTreeMap<NodeId, NodeIncarnation>,
    next_incarnation: u64,
    sessions: BTreeMap<ViewId, Weak<ViewShared>>,
    next_view_id: u64,
    stats: GraphRuntimeStats,
}

enum CommandFinalization {
    Uncommitted(GraphCommandOutcome),
    Committed {
        outcome: GraphCommandOutcome,
        middleware_panic: Option<PanicPayload>,
    },
}

enum QueuedNotification {
    RuntimeEvent(GraphRuntimeEvent),
    ViewPatch {
        view: Weak<ViewShared>,
        generation: ViewGeneration,
        patch: RuntimePatch,
        selection_changed: Option<ViewSelectionSeed>,
    },
}

#[derive(Default)]
struct NotificationPump {
    draining: Cell<bool>,
    deferred: Cell<u32>,
    queue: RefCell<VecDeque<QueuedNotification>>,
}

pub(crate) struct NotificationBatchGuard<'a> {
    runtime: &'a RuntimeShared,
    active: bool,
}

impl NotificationBatchGuard<'_> {
    pub(crate) fn finish(mut self) -> Option<PanicPayload> {
        self.active = false;
        self.runtime.finish_notification_batch()
    }
}

impl Drop for NotificationBatchGuard<'_> {
    fn drop(&mut self) {
        if self.active {
            self.runtime.release_notification_batch();
        }
    }
}

pub(crate) struct RuntimeShared {
    id: RuntimeId,
    catalog: Arc<NodeCatalog>,
    pub(crate) alive: Cell<bool>,
    mutating: Cell<bool>,
    state: RefCell<RuntimeState>,
    notifications: NotificationPump,
    subscriptions: subscriptions::RuntimeEventHub,
}

/// Authoritative graph document runtime.
///
/// Runtime-external incremental writes are accepted only as revisioned [`GraphCommand`] values.
/// Whole-document replacement requires a [`CanonicalGraph`] produced by this runtime's semantic
/// catalog provenance. Raw transactions and history maintenance remain crate-internal operations.
///
/// ```compile_fail
/// use jellyflow_runtime::graph_kernel::GraphTransaction;
/// ```
///
/// ```
/// use jellyflow_core::{Graph, GraphId};
/// use jellyflow_runtime::GraphRuntime;
/// use jellyflow_runtime::schema::NodeCatalog;
///
/// let runtime = GraphRuntime::builder(NodeCatalog::new())
///     .build(Graph::new(GraphId::new()))
///     .unwrap();
/// let document = runtime
///     .catalog()
///     .canonicalize_graph(&Graph::new(GraphId::new()))
///     .unwrap();
/// runtime.replace_document(document).unwrap();
/// ```
///
/// ```compile_fail
/// use jellyflow_core::{Graph, GraphId};
/// use jellyflow_runtime::GraphRuntime;
/// use jellyflow_runtime::schema::NodeCatalog;
///
/// let runtime = GraphRuntime::builder(NodeCatalog::new())
///     .build(Graph::new(GraphId::new()))
///     .unwrap();
/// runtime.clear_history().unwrap();
/// ```
pub struct GraphRuntime {
    shared: Rc<RuntimeShared>,
}

/// Configures one authoritative runtime before its source graph is canonicalized and published.
pub struct GraphRuntimeBuilder {
    catalog: NodeCatalog,
    profile: Option<Box<dyn GraphProfile>>,
    policy: GraphRuntimePolicy,
}

impl GraphRuntimeBuilder {
    /// Replaces the authoritative graph mutation policy.
    #[must_use]
    pub fn with_policy(mut self, policy: GraphRuntimePolicy) -> Self {
        self.policy = policy;
        self
    }

    /// Adds application-specific validation after node-kind canonicalization.
    #[must_use]
    pub fn with_profile(mut self, profile: Box<dyn GraphProfile>) -> Self {
        self.profile = Some(profile);
        self
    }

    /// Canonicalizes the entire source graph before publishing an authoritative runtime.
    pub fn build(self, graph: Graph) -> Result<GraphRuntime, GraphRuntimeError> {
        let document = self.catalog.canonicalize_graph(&graph)?;
        GraphRuntime::publish_canonical(document, self.profile, self.policy)
    }
}

impl std::fmt::Debug for GraphRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = self
            .shared
            .state
            .try_borrow()
            .map_err(|_| std::fmt::Error)?;
        f.debug_struct("GraphRuntime")
            .field("graph_id", &state.graph.graph_id())
            .field("revision", &state.revision)
            .field("document_epoch", &state.document_epoch)
            .field("live_views", &state.sessions.len())
            .finish()
    }
}

impl GraphRuntime {
    /// Starts a runtime build with the exact catalog that will canonicalize and interpret the graph.
    pub fn builder(catalog: NodeCatalog) -> GraphRuntimeBuilder {
        GraphRuntimeBuilder {
            catalog,
            profile: None,
            policy: GraphRuntimePolicy::default(),
        }
    }

    /// Publishes a graph only after document and known-kind canonicalization succeeded.
    pub fn from_canonical(document: CanonicalGraph) -> Result<Self, GraphRuntimeError> {
        Self::publish_canonical(document, None, GraphRuntimePolicy::default())
    }

    /// Publishes a canonical graph with an authoritative mutation policy.
    pub fn from_canonical_with_policy(
        document: CanonicalGraph,
        policy: GraphRuntimePolicy,
    ) -> Result<Self, GraphRuntimeError> {
        Self::publish_canonical(document, None, policy)
    }

    /// Publishes a canonical graph after applying the profile's final document validation.
    pub fn from_canonical_with_profile(
        document: CanonicalGraph,
        profile: Box<dyn GraphProfile>,
    ) -> Result<Self, GraphRuntimeError> {
        Self::publish_canonical(document, Some(profile), GraphRuntimePolicy::default())
    }

    /// Publishes a canonical graph with both semantic validation and mutation policy.
    pub fn from_canonical_with_profile_and_policy(
        document: CanonicalGraph,
        profile: Box<dyn GraphProfile>,
        policy: GraphRuntimePolicy,
    ) -> Result<Self, GraphRuntimeError> {
        Self::publish_canonical(document, Some(profile), policy)
    }

    #[cfg(test)]
    pub(crate) fn retain_shared_for_test(&self) -> Rc<RuntimeShared> {
        Rc::clone(&self.shared)
    }

    fn publish_canonical(
        document: CanonicalGraph,
        profile: Option<Box<dyn GraphProfile>>,
        policy: GraphRuntimePolicy,
    ) -> Result<Self, GraphRuntimeError> {
        let (graph, catalog) = document.into_parts();
        validate_graph(&graph)?;
        if let Some(profile) = profile.as_deref() {
            validate_profile_document(profile, &graph)?;
        }
        let mut next_incarnation = 1;
        let mut incarnations = BTreeMap::new();
        for node in graph.nodes().keys().copied() {
            incarnations.insert(node, allocate_incarnation(&mut next_incarnation)?);
        }
        let index = GraphIndex::from_graph(&graph);
        let runtime_id = RuntimeId::allocate()?;
        Ok(Self {
            shared: Rc::new(RuntimeShared {
                id: runtime_id,
                catalog,
                alive: Cell::new(true),
                mutating: Cell::new(false),
                state: RefCell::new(RuntimeState {
                    graph: Arc::new(graph),
                    revision: 0,
                    document_epoch: 0,
                    policy,
                    history: GraphHistory::default(),
                    profile,
                    middleware: None,
                    index,
                    incarnations,
                    next_incarnation,
                    sessions: BTreeMap::new(),
                    next_view_id: 1,
                    stats: GraphRuntimeStats::default(),
                }),
                notifications: NotificationPump::default(),
                subscriptions: subscriptions::RuntimeEventHub::new(runtime_id),
            }),
        })
    }

    pub fn with_middleware(self, middleware: impl GraphRuntimeMiddleware) -> Self {
        self.shared.state.borrow_mut().middleware = Some(Box::new(middleware));
        self
    }

    pub fn snapshot(&self) -> Result<GraphSnapshot, GraphRuntimeError> {
        self.shared.snapshot()
    }

    /// Returns the exact semantic catalog snapshot used to canonicalize this runtime.
    pub fn catalog(&self) -> &NodeCatalog {
        &self.shared.catalog
    }

    /// Clones the read-only semantic catalog provenance for a long-lived consumer.
    pub fn catalog_owned(&self) -> Arc<NodeCatalog> {
        Arc::clone(&self.shared.catalog)
    }

    pub fn policy(&self) -> Result<GraphRuntimePolicy, GraphRuntimeError> {
        self.shared
            .state
            .try_borrow()
            .map(|state| state.policy)
            .map_err(|_| GraphRuntimeError::Busy)
    }

    pub fn node_incarnation(
        &self,
        node: NodeId,
    ) -> Result<Option<NodeIncarnation>, GraphRuntimeError> {
        self.shared.node_incarnation(node)
    }

    pub fn history_state(&self) -> Result<HistoryState, GraphRuntimeError> {
        self.shared.history_state()
    }

    #[cfg(test)]
    pub(crate) fn clear_history(&self) -> Result<(), GraphRuntimeError> {
        self.shared.clear_history()
    }

    pub fn stats(&self) -> Result<GraphRuntimeStats, GraphRuntimeError> {
        let state = self
            .shared
            .state
            .try_borrow()
            .map_err(|_| GraphRuntimeError::Busy)?;
        Ok(state.stats)
    }

    pub fn attach_view(&self, options: ViewSessionOptions) -> Result<ViewSession, AttachViewError> {
        self.shared.attach_view(options)
    }

    pub fn subscribe(
        &self,
        callback: impl FnMut(GraphRuntimeEvent) + 'static,
    ) -> Result<GraphRuntimeSubscriptionToken, GraphRuntimeError> {
        self.shared
            .subscriptions
            .subscribe(callback)
            .ok_or(GraphRuntimeError::IdentityExhausted)
    }

    pub fn unsubscribe(&self, token: GraphRuntimeSubscriptionToken) -> bool {
        self.shared.subscriptions.unsubscribe(token)
    }

    /// Executes one revision-checked graph command through the authoritative runtime.
    pub fn execute(&self, command: GraphCommand) -> Result<GraphCommandOutcome, GraphRuntimeError> {
        self.shared.execute_command(command)
    }

    /// Evaluates a connection intent against the current revision without exposing planned ops.
    pub fn evaluate_connection(
        &self,
        edit: ConnectionEdit,
    ) -> Result<ConnectionEvaluation, GraphRuntimeError> {
        self.shared.evaluate_connection(edit)
    }

    #[cfg(test)]
    pub(crate) fn set_next_incarnation_for_test(&self, next: u64) {
        self.shared.state.borrow_mut().next_incarnation = next;
    }

    #[cfg(test)]
    pub(crate) fn incident_snapshot_for_test(&self, node: NodeId) -> Option<IncidentEdgeSnapshot> {
        self.shared
            .state
            .borrow()
            .index
            .incident_edge_snapshot(node)
    }

    /// Atomically replaces the authoritative document with a canonical graph from this runtime's
    /// semantic catalog provenance.
    ///
    /// A successful replacement advances both graph revision and document epoch, clears history,
    /// hard-retires every prior node incarnation, and fans out one document-reset patch. Documents
    /// produced by another or subsequently modified catalog are rejected without side effects.
    pub fn replace_document(
        &self,
        document: CanonicalGraph,
    ) -> Result<RuntimePatch, GraphRuntimeError> {
        if !document.shares_catalog_provenance_with(&self.shared.catalog) {
            return Err(GraphRuntimeError::CatalogProvenanceMismatch);
        }
        self.shared.replace_document(document.into_graph())
    }

    pub fn live_view_count(&self) -> usize {
        self.shared
            .state
            .try_borrow()
            .map(|state| {
                state
                    .sessions
                    .values()
                    .filter_map(Weak::upgrade)
                    .filter(|view| view.is_open())
                    .count()
            })
            .unwrap_or_default()
    }
}

impl Drop for GraphRuntime {
    fn drop(&mut self) {
        self.shared.alive.set(false);
        self.shared.notifications.queue.borrow_mut().clear();
        self.shared.subscriptions.clear();
        let sessions = self
            .shared
            .state
            .try_borrow_mut()
            .map(|mut state| std::mem::take(&mut state.sessions))
            .unwrap_or_default();
        for session in sessions
            .into_values()
            .filter_map(|session| session.upgrade())
        {
            let (_, callback_panic) = session.terminalize(ViewCloseReason::RuntimeDropped);
            suppress(callback_panic);
        }
    }
}

impl RuntimeShared {
    pub(crate) fn defer_notifications(&self) -> NotificationBatchGuard<'_> {
        let depth = self.notifications.deferred.get();
        self.notifications.deferred.set(depth.saturating_add(1));
        NotificationBatchGuard {
            runtime: self,
            active: true,
        }
    }

    fn finish_notification_batch(&self) -> Option<PanicPayload> {
        self.release_notification_batch();
        self.drain_notifications()
    }

    fn release_notification_batch(&self) {
        let depth = self.notifications.deferred.get();
        debug_assert!(depth > 0);
        self.notifications.deferred.set(depth.saturating_sub(1));
    }

    pub(crate) fn snapshot(&self) -> Result<GraphSnapshot, GraphRuntimeError> {
        let state = self
            .state
            .try_borrow()
            .map_err(|_| GraphRuntimeError::Busy)?;
        Ok(GraphSnapshot::new(
            state.graph.clone(),
            state.revision,
            state.document_epoch,
        ))
    }

    pub(crate) fn node_incarnation(
        &self,
        node: NodeId,
    ) -> Result<Option<NodeIncarnation>, GraphRuntimeError> {
        let state = self
            .state
            .try_borrow()
            .map_err(|_| GraphRuntimeError::Busy)?;
        Ok(state.incarnations.get(&node).copied())
    }

    pub(crate) fn adapter_frame_node_facts(
        &self,
        nodes: &BTreeSet<NodeId>,
    ) -> Result<BTreeMap<NodeId, (NodeIncarnation, IncidentEdgeSnapshot)>, GraphRuntimeError> {
        let state = self
            .state
            .try_borrow()
            .map_err(|_| GraphRuntimeError::Busy)?;
        Ok(nodes
            .iter()
            .filter_map(|node| {
                let incarnation = state.incarnations.get(node).copied()?;
                let incident_edges = state.index.incident_edge_snapshot(*node)?;
                Some((*node, (incarnation, incident_edges)))
            })
            .collect())
    }

    pub(crate) fn adapter_node_incarnations(
        &self,
        nodes: &BTreeSet<NodeId>,
    ) -> Result<BTreeMap<NodeId, NodeIncarnation>, GraphRuntimeError> {
        let state = self
            .state
            .try_borrow()
            .map_err(|_| GraphRuntimeError::Busy)?;
        Ok(nodes
            .iter()
            .filter_map(|node| {
                state
                    .incarnations
                    .get(node)
                    .copied()
                    .map(|incarnation| (*node, incarnation))
            })
            .collect())
    }

    pub(crate) fn record_frame_incident_snapshot_work(
        &self,
        work: IncidentSnapshotWork,
    ) -> Result<(), GraphRuntimeError> {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| GraphRuntimeError::Busy)?;
        state.stats.frame_incident_snapshot_entries_read = state
            .stats
            .frame_incident_snapshot_entries_read
            .saturating_add(work.entries_read);
        state.stats.frame_incident_snapshot_materializations = state
            .stats
            .frame_incident_snapshot_materializations
            .saturating_add(work.materializations);
        Ok(())
    }

    pub(crate) fn record_feedback_incident_snapshot_work(
        &self,
        work: IncidentSnapshotWork,
    ) -> Result<(), GraphRuntimeError> {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| GraphRuntimeError::Busy)?;
        state.stats.feedback_incident_snapshot_entries_read = state
            .stats
            .feedback_incident_snapshot_entries_read
            .saturating_add(work.entries_read);
        state.stats.feedback_incident_snapshot_materializations = state
            .stats
            .feedback_incident_snapshot_materializations
            .saturating_add(work.materializations);
        Ok(())
    }

    pub(crate) fn adapter_frame_incident_routing_dependencies(
        &self,
        nodes: &BTreeSet<NodeId>,
        excluded: &BTreeSet<NodeId>,
        node_limit: usize,
        scan_limit: usize,
    ) -> Result<IncidentNodeQuery, GraphRuntimeError> {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| GraphRuntimeError::Busy)?;
        let query = state.index.incident_nodes_for_nodes(
            &state.graph,
            nodes,
            excluded,
            node_limit,
            scan_limit,
        );
        state.stats.frame_incident_routing_index_bucket_reads = state
            .stats
            .frame_incident_routing_index_bucket_reads
            .saturating_add(query.index_bucket_reads);
        state.stats.frame_incident_routing_index_entries_read = state
            .stats
            .frame_incident_routing_index_entries_read
            .saturating_add(query.index_entries_read);
        Ok(query)
    }

    pub(crate) fn history_state(&self) -> Result<HistoryState, GraphRuntimeError> {
        let state = self
            .state
            .try_borrow()
            .map_err(|_| GraphRuntimeError::Busy)?;
        Ok(HistoryState {
            undo_len: state.history.undo_len(),
            redo_len: state.history.redo_len(),
        })
    }

    #[cfg(test)]
    fn clear_history(self: &Rc<Self>) -> Result<(), GraphRuntimeError> {
        let guard = MutationGuard::acquire(self)?;
        let history_state = {
            let mut state = self
                .state
                .try_borrow_mut()
                .map_err(|_| GraphRuntimeError::Busy)?;
            if !state.history.can_undo() && !state.history.can_redo() {
                return Ok(());
            }
            state.history.clear();
            HistoryState::default()
        };
        self.notifications
            .queue
            .borrow_mut()
            .push_back(QueuedNotification::RuntimeEvent(
                GraphRuntimeEvent::HistoryChanged {
                    state: history_state,
                },
            ));
        drop(guard);
        resume(self.drain_notifications());
        Ok(())
    }

    fn attach_view(
        self: &Rc<Self>,
        options: ViewSessionOptions,
    ) -> Result<ViewSession, AttachViewError> {
        if !self.alive.get() {
            return Err(AttachViewError::RuntimeUnavailable);
        }
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| AttachViewError::RuntimeBusy)?;
        let seed = options.sanitized_seed(&state.graph)?;
        let view_id = ViewId::new(self.id, state.next_view_id);
        state.next_view_id = state
            .next_view_id
            .checked_add(1)
            .ok_or(AttachViewError::IdentityExhausted)?;
        let view = ViewShared::new(view_id, ViewGeneration::initial(), state.revision, seed);
        state.sessions.insert(view_id, Rc::downgrade(&view));
        Ok(ViewSession::new(
            Rc::downgrade(self),
            view,
            Arc::clone(&self.catalog),
        ))
    }

    pub(crate) fn execute_command(
        self: &Rc<Self>,
        command: GraphCommand,
    ) -> Result<GraphCommandOutcome, GraphRuntimeError> {
        let guard = MutationGuard::acquire(self)?;
        let execution = {
            let mut state = self
                .state
                .try_borrow_mut()
                .map_err(|_| GraphRuntimeError::Busy)?;
            execute_command_locked(&mut state, &self.catalog, &command)?
        };
        let finalization = self.finalize_command_execution(execution, |patch, sessions| {
            self.apply_and_queue_patch(patch, &sessions);
        });
        let (outcome, middleware_panic) = match finalization {
            CommandFinalization::Uncommitted(outcome) => return Ok(outcome),
            CommandFinalization::Committed {
                outcome,
                middleware_panic,
            } => (outcome, middleware_panic),
        };
        drop(guard);
        let mut panics = PanicCollector::default();
        panics.record(middleware_panic);
        panics.record(self.drain_notifications());
        panics.resume();
        Ok(outcome)
    }

    pub(crate) fn evaluate_connection(
        self: &Rc<Self>,
        edit: ConnectionEdit,
    ) -> Result<ConnectionEvaluation, GraphRuntimeError> {
        let guard = MutationGuard::acquire(self)?;
        let evaluation = {
            let state = self
                .state
                .try_borrow()
                .map_err(|_| GraphRuntimeError::Busy)?;
            let revision = state.revision;
            let command = GraphCommand::connection(revision, edit);
            let planned = plan_graph_command(
                &state.graph,
                &state.index,
                &self.catalog,
                &command,
                state.profile.as_deref(),
                &state.policy,
            );
            match planned {
                Ok(_) => ConnectionEvaluation::accepted(revision),
                Err(error) => {
                    let rejection = command_rejection_from_planning(error);
                    ConnectionEvaluation::rejected(revision, rejection.diagnostics().to_vec())
                }
            }
        };
        drop(guard);
        Ok(evaluation)
    }

    fn replace_document(self: &Rc<Self>, graph: Graph) -> Result<RuntimePatch, GraphRuntimeError> {
        let guard = MutationGuard::acquire(self)?;
        validate_graph(&graph)?;
        let (patch, sessions) = {
            let mut state = self
                .state
                .try_borrow_mut()
                .map_err(|_| GraphRuntimeError::Busy)?;
            if let Some(profile) = state.profile.as_deref() {
                validate_profile_document(profile, &graph)?;
            }
            let to_revision = state
                .revision
                .checked_add(1)
                .ok_or(GraphRuntimeError::RevisionExhausted)?;
            let document_epoch = state
                .document_epoch
                .checked_add(1)
                .ok_or(GraphRuntimeError::DocumentEpochExhausted)?;
            let before = state.graph.clone();
            let mut next_incarnation = state.next_incarnation;
            let mut next_incarnations = BTreeMap::new();
            let mut incarnation_delta = before
                .nodes()
                .keys()
                .copied()
                .map(|node| (node, None))
                .collect::<BTreeMap<_, _>>();
            for node in graph.nodes().keys().copied() {
                let incarnation = allocate_incarnation(&mut next_incarnation)?;
                next_incarnations.insert(node, incarnation);
                incarnation_delta.insert(node, Some(incarnation));
            }
            let graph = Arc::new(graph);
            let snapshot = GraphSnapshot::new(graph.clone(), to_revision, document_epoch);
            let patch = RuntimePatch::document_reset(
                &before,
                state.revision,
                to_revision,
                document_epoch,
                snapshot,
                incarnation_delta,
            );
            state.graph = graph.clone();
            state.revision = to_revision;
            state.document_epoch = document_epoch;
            state.history.clear();
            state.index = GraphIndex::from_graph(&graph);
            state.incarnations = next_incarnations;
            state.next_incarnation = next_incarnation;
            state.stats.patch_builds = state.stats.patch_builds.saturating_add(1);
            let sessions = collect_live_sessions(&mut state);
            state.stats.patch_deliveries = state
                .stats
                .patch_deliveries
                .saturating_add(sessions.len() as u64);
            (patch, sessions)
        };
        self.apply_and_queue_patch(&patch, &sessions);
        drop(guard);
        resume(self.drain_notifications());
        Ok(patch)
    }

    pub(crate) fn close_view(
        self: &Rc<Self>,
        view: &Rc<ViewShared>,
        request: ViewCloseRequest,
    ) -> Result<ViewCloseOutcome, ViewSessionError> {
        let guard = MutationGuard::acquire(self)?;
        view.begin_close()?;
        let mut receipts = Vec::with_capacity(request.commands().len());
        let mut notifications = Vec::new();
        let mut panics = PanicCollector::default();
        for command in request.commands() {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                self.execute_command_deferred(command, &mut notifications)
            }));
            let result = match result {
                Ok(result) => result,
                Err(payload) => {
                    panics.record(Some(payload));
                    break;
                }
            };
            match result {
                Ok(CommandFinalization::Uncommitted(outcome)) => {
                    receipts.push(ViewCloseReceipt::Command { outcome });
                }
                Ok(CommandFinalization::Committed {
                    outcome,
                    middleware_panic: command_panic,
                }) => {
                    receipts.push(ViewCloseReceipt::Command { outcome });
                    if command_panic.is_some() {
                        panics.record(command_panic);
                        break;
                    }
                }
                Err(error) => receipts.push(ViewCloseReceipt::CommandFailed { error }),
            }
        }
        let (_, terminal_panic) = view.terminalize(ViewCloseReason::HostClosed);
        panics.record(terminal_panic);
        self.unregister_view(view.id());
        for (patch, sessions) in notifications {
            self.apply_and_queue_patch(&patch, &sessions);
        }
        drop(guard);
        panics.record(self.drain_notifications());
        panics.resume();
        Ok(ViewCloseOutcome::new(ViewCloseReason::HostClosed, receipts))
    }

    fn execute_command_deferred(
        self: &Rc<Self>,
        command: &GraphCommand,
        notifications: &mut Vec<(RuntimePatch, Vec<(Rc<ViewShared>, ViewGeneration)>)>,
    ) -> Result<CommandFinalization, GraphRuntimeError> {
        let execution = {
            let mut state = self
                .state
                .try_borrow_mut()
                .map_err(|_| GraphRuntimeError::Busy)?;
            execute_command_locked(&mut state, &self.catalog, command)?
        };
        Ok(
            self.finalize_command_execution(execution, |patch, sessions| {
                notifications.push((patch.clone(), sessions));
            }),
        )
    }

    fn finalize_command_execution(
        &self,
        execution: CommandExecution,
        deliver: impl FnOnce(&RuntimePatch, Vec<(Rc<ViewShared>, ViewGeneration)>),
    ) -> CommandFinalization {
        match execution {
            CommandExecution::Outcome(outcome) => CommandFinalization::Uncommitted(outcome),
            CommandExecution::Committed {
                patch,
                sessions,
                footprint,
            } => {
                let middleware_panic =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        self.run_after_command_middleware(&patch);
                    }))
                    .err();
                deliver(&patch, sessions);
                CommandFinalization::Committed {
                    outcome: GraphCommandOutcome::Applied(GraphCommandReceipt::new(
                        patch, footprint,
                    )),
                    middleware_panic,
                }
            }
        }
    }

    fn apply_and_queue_patch(
        &self,
        patch: &RuntimePatch,
        sessions: &[(Rc<ViewShared>, ViewGeneration)],
    ) {
        let view_patches = sessions
            .iter()
            .map(|(session, generation)| {
                (
                    Rc::downgrade(session),
                    *generation,
                    session.apply_runtime_patch(patch),
                )
            })
            .collect::<Vec<_>>();
        let mut queue = self.notifications.queue.borrow_mut();
        queue.push_back(QueuedNotification::RuntimeEvent(
            GraphRuntimeEvent::Patched {
                patch: patch.clone(),
            },
        ));
        queue.extend(
            view_patches
                .into_iter()
                .map(
                    |(view, generation, selection_changed)| QueuedNotification::ViewPatch {
                        view,
                        generation,
                        patch: patch.clone(),
                        selection_changed,
                    },
                ),
        );
    }

    fn run_after_command_middleware(&self, patch: &RuntimePatch) {
        let Ok(mut state) = self.state.try_borrow_mut() else {
            return;
        };
        if state.middleware.is_none() {
            return;
        }
        let snapshot = middleware_snapshot(&state);
        state
            .middleware
            .as_deref_mut()
            .expect("middleware presence was checked before snapshot creation")
            .after_command(snapshot, patch);
    }

    fn drain_notifications(&self) -> Option<PanicPayload> {
        if self.notifications.deferred.get() > 0 {
            return None;
        }
        if self.notifications.draining.replace(true) {
            return None;
        }
        let guard = NotificationDrainGuard(&self.notifications.draining);
        let mut panics = PanicCollector::default();
        loop {
            let notification = self.notifications.queue.borrow_mut().pop_front();
            let Some(notification) = notification else {
                break;
            };
            match notification {
                QueuedNotification::RuntimeEvent(event) => {
                    panics.record(self.subscriptions.publish(event));
                }
                QueuedNotification::ViewPatch {
                    view,
                    generation,
                    patch,
                    selection_changed,
                } => {
                    let Some(view) = view.upgrade() else {
                        continue;
                    };
                    panics.record(view.publish_runtime_patch(generation, patch, selection_changed));
                }
            }
        }
        drop(guard);
        panics.into_payload()
    }

    pub(crate) fn unregister_view(&self, view: ViewId) {
        if let Ok(mut state) = self.state.try_borrow_mut() {
            state.sessions.remove(&view);
        }
    }

    #[cfg(test)]
    pub(crate) fn snapshot_with_incarnations(
        &self,
    ) -> Result<(GraphSnapshot, BTreeMap<NodeId, NodeIncarnation>), GraphRuntimeError> {
        let state = self
            .state
            .try_borrow()
            .map_err(|_| GraphRuntimeError::Busy)?;
        Ok((
            GraphSnapshot::new(state.graph.clone(), state.revision, state.document_epoch),
            state.incarnations.clone(),
        ))
    }
}
