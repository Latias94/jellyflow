//! Authoritative graph document runtime shared by independently disposable views.

mod connection_admission;
mod create_from_wire;
mod execution;
mod impact;
mod index;
mod patch;
mod port_connection_facts;
mod subscriptions;

pub use create_from_wire::{CreateFromWireCandidates, CreateFromWireQueryError};
pub use port_connection_facts::{
    PortConnectionDragFacts, PortConnectionDragIntent, PortConnectionDragQueryOutcome,
    PortConnectionFacts, PortConnectionQueryResult, PortConnectionQueryWork, PortConnectionSummary,
    PortConnectionSummaryBatch, PortIncidentConnectionFacts,
};

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::graph_kernel::GraphHistory;
use jellyflow_core::core::{EdgeId, Graph, GraphValidationError, NodeId, PortId};
use rpds::RedBlackTreeMapSync;

use crate::io::ViewSelectionSeed;
use crate::profile::{ApplyPipelineError, GraphProfile, validate_profile_document};
use crate::runtime::commands::{
    CommandExecutionCompletion, ConnectionEdit, ConnectionEvaluation, GraphCommand,
    GraphCommandOutcome, GraphCommandReceipt, plan_graph_command,
};
use crate::runtime::middleware::GraphRuntimeMiddleware;
use crate::runtime::unwind::{PanicCollector, PanicPayload, resume, suppress};
use crate::schema::{CanonicalGraph, NodeCatalog, NodeCatalogMigrationError};

pub use crate::runtime::policy::GraphRuntimePolicy;

use crate::runtime::view_session::{
    AttachViewError, RuntimePatchReconciliationReceipt, ViewCloseOutcome, ViewCloseReason,
    ViewCloseRequest, ViewGeneration, ViewId, ViewSession, ViewSessionError, ViewSessionOptions,
    ViewShared,
};

pub(crate) use self::connection_admission::{
    ConnectionAdmissionCursor, ConnectionAdmissionOutcome, ConnectionAdmissionWork,
};
use self::execution::collect_live_sessions;
use self::execution::{
    CommandExecution, CommandExecutionFailure, MutationGuard, NotificationDrainGuard,
    allocate_edge_incarnation, allocate_incarnation, allocate_port_incarnation,
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

/// Opaque identity of the runtime that issued semantic graph facts.
///
/// Values can only be obtained from a live runtime or facts issued by one; callers cannot
/// manufacture provenance from graph identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RuntimeProvenance(RuntimeId);

impl RuntimeProvenance {
    pub(crate) const fn new(runtime: RuntimeId) -> Self {
        Self(runtime)
    }

    #[cfg(test)]
    pub(crate) const fn runtime_id(self) -> RuntimeId {
        self.0
    }
}

pub(crate) use self::index::GraphIndex;
pub use self::patch::{
    FactChange, FactChangeMask, GeometryImpact, RuntimeFactDelta, RuntimeFactKey, RuntimePatch,
    RuntimePatchKind,
};
pub use self::subscriptions::GraphRuntimeSubscriptionToken;

pub type GraphRevision = u64;
pub type DocumentEpoch = u64;

/// Exact version of one authoritative graph runtime.
///
/// A revision is only meaningful inside the runtime that issued it. Keeping both values in one
/// opaque capability prevents a command planned from one runtime from being accepted by another
/// runtime that happens to have the same numeric revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GraphVersion {
    provenance: RuntimeProvenance,
    revision: GraphRevision,
}

impl GraphVersion {
    const fn new(provenance: RuntimeProvenance, revision: GraphRevision) -> Self {
        Self {
            provenance,
            revision,
        }
    }

    pub const fn provenance(self) -> RuntimeProvenance {
        self.provenance
    }

    pub const fn revision(self) -> GraphRevision {
        self.revision
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct NodeIncarnation(u64);

type NodeIncarnationMap = RedBlackTreeMapSync<NodeId, NodeIncarnation>;

/// Full runtime identity of one continuous node lifetime.
///
/// Reusing a [`NodeId`] after removal allocates a new incarnation, while provenance prevents a
/// key issued by another runtime with matching IDs and counters from gaining authority here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeLifetimeKey {
    provenance: RuntimeProvenance,
    node: NodeId,
    incarnation: NodeIncarnation,
}

impl NodeLifetimeKey {
    pub(crate) const fn new(
        provenance: RuntimeProvenance,
        node: NodeId,
        incarnation: NodeIncarnation,
    ) -> Self {
        Self {
            provenance,
            node,
            incarnation,
        }
    }

    pub const fn provenance(self) -> RuntimeProvenance {
        self.provenance
    }

    pub const fn node(self) -> NodeId {
        self.node
    }

    #[cfg(test)]
    pub(crate) const fn unavailable(node: NodeId) -> Self {
        Self::new(
            RuntimeProvenance::new(RuntimeId::UNAVAILABLE),
            node,
            NodeIncarnation(0),
        )
    }
}

/// Opaque lifetime identity for one materialized port.
///
/// A port's identity is retired when the port is removed or when its semantic identity is
/// replaced. Reusing the same [`PortId`] always allocates a new value, including through undo,
/// redo, and document replacement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct PortIncarnation(u64);

/// Full runtime identity of one continuous real-port lifetime.
///
/// The owning node incarnation participates in the key so neither a recycled node nor a recycled
/// port can regain authority held by an old adapter capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PortInstanceKey {
    provenance: RuntimeProvenance,
    node: NodeId,
    node_incarnation: NodeIncarnation,
    port: PortId,
    port_incarnation: PortIncarnation,
}

impl PortInstanceKey {
    pub(crate) const fn new(
        provenance: RuntimeProvenance,
        node: NodeId,
        node_incarnation: NodeIncarnation,
        port: PortId,
        port_incarnation: PortIncarnation,
    ) -> Self {
        Self {
            provenance,
            node,
            node_incarnation,
            port,
            port_incarnation,
        }
    }

    pub const fn provenance(self) -> RuntimeProvenance {
        self.provenance
    }

    pub const fn node(self) -> NodeId {
        self.node
    }

    pub const fn owner(self) -> NodeLifetimeKey {
        NodeLifetimeKey::new(self.provenance, self.node, self.node_incarnation)
    }

    pub const fn port(self) -> PortId {
        self.port
    }

    #[cfg(test)]
    pub(crate) const fn with_port_for_test(self, port: PortId) -> Self {
        Self { port, ..self }
    }

    #[cfg(test)]
    pub(crate) const fn unavailable(node: NodeId, port: PortId) -> Self {
        Self::new(
            RuntimeProvenance::new(RuntimeId::UNAVAILABLE),
            node,
            NodeIncarnation(0),
            port,
            PortIncarnation(0),
        )
    }
}

/// Opaque lifetime identity for one materialized edge.
///
/// Reusing an [`EdgeId`] after removal always allocates a new value, including through undo,
/// redo, and document replacement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct EdgeIncarnation(u64);

impl EdgeIncarnation {
    #[cfg(test)]
    pub(crate) const fn from_u64_for_test(value: u64) -> Self {
        Self(value)
    }
}

/// Full runtime identity of one continuous edge lifetime.
///
/// The constructor is runtime-private, so an adapter can transport this key but cannot upgrade a
/// recycled [`EdgeId`] into authority over its replacement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EdgeInstanceKey {
    provenance: RuntimeProvenance,
    edge: EdgeId,
    incarnation: EdgeIncarnation,
}

impl EdgeInstanceKey {
    pub(crate) const fn new(
        provenance: RuntimeProvenance,
        edge: EdgeId,
        incarnation: EdgeIncarnation,
    ) -> Self {
        Self {
            provenance,
            edge,
            incarnation,
        }
    }

    pub const fn provenance(self) -> RuntimeProvenance {
        self.provenance
    }

    pub const fn edge(self) -> EdgeId {
        self.edge
    }

    #[cfg(test)]
    pub(crate) const fn unavailable(edge: EdgeId) -> Self {
        Self::new(
            RuntimeProvenance::new(RuntimeId::UNAVAILABLE),
            edge,
            EdgeIncarnation::unavailable(),
        )
    }
}

#[cfg(test)]
std::thread_local! {
    static EDGE_INCARNATION_ENTRY_CLONES: Cell<u64> = const { Cell::new(0) };
}

#[derive(Debug)]
struct EdgeIncarnationEntry(EdgeIncarnation);

impl EdgeIncarnationEntry {
    fn incarnation(&self) -> EdgeIncarnation {
        self.0
    }
}

impl Clone for EdgeIncarnationEntry {
    fn clone(&self) -> Self {
        #[cfg(test)]
        EDGE_INCARNATION_ENTRY_CLONES.with(|clones| clones.set(clones.get().saturating_add(1)));
        Self(self.0)
    }
}

impl EdgeIncarnation {
    #[cfg(test)]
    pub(crate) const fn unavailable() -> Self {
        Self(0)
    }
}

#[cfg(test)]
pub(crate) fn reset_edge_incarnation_entry_clone_count_for_test() {
    EDGE_INCARNATION_ENTRY_CLONES.with(|clones| clones.set(0));
}

#[cfg(test)]
pub(crate) fn edge_incarnation_entry_clone_count_for_test() -> u64 {
    EDGE_INCARNATION_ENTRY_CLONES.with(Cell::get)
}

type EdgeIncarnationMap = RedBlackTreeMapSync<EdgeId, EdgeIncarnationEntry>;

#[cfg(test)]
std::thread_local! {
    static PORT_INCARNATION_ENTRY_CLONES: Cell<u64> = const { Cell::new(0) };
}

#[derive(Debug)]
struct PortIncarnationEntry(PortIncarnation);

impl PortIncarnationEntry {
    fn incarnation(&self) -> PortIncarnation {
        self.0
    }
}

impl Clone for PortIncarnationEntry {
    fn clone(&self) -> Self {
        #[cfg(test)]
        PORT_INCARNATION_ENTRY_CLONES.with(|clones| clones.set(clones.get().saturating_add(1)));
        Self(self.0)
    }
}

type PortIncarnationMap = RedBlackTreeMapSync<PortId, PortIncarnationEntry>;

#[cfg(test)]
pub(crate) fn reset_port_incarnation_entry_clone_count_for_test() {
    PORT_INCARNATION_ENTRY_CLONES.with(|clones| clones.set(0));
}

#[cfg(test)]
pub(crate) fn port_incarnation_entry_clone_count_for_test() -> u64 {
    PORT_INCARNATION_ENTRY_CLONES.with(Cell::get)
}

pub(crate) type IncidentEdgeSnapshot = Arc<BTreeSet<EdgeId>>;

#[derive(Debug, Clone)]
pub(crate) struct AdapterNodeRuntimeFacts {
    pub(crate) lifetime: NodeLifetimeKey,
    pub(crate) incident_edges: IncidentEdgeSnapshot,
    pub(crate) port_order: Vec<PortId>,
    pub(crate) port_instances: BTreeMap<PortId, PortInstanceKey>,
}

#[derive(Debug, Clone)]
pub(crate) struct AdapterFrameNodeFacts {
    pub(crate) nodes: BTreeMap<NodeId, AdapterNodeRuntimeFacts>,
    pub(crate) port_identities_examined: u64,
    pub(crate) port_budget_omitted_nodes: BTreeSet<NodeId>,
}

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

    pub(crate) const fn entries_read(self) -> u64 {
        self.entries_read
    }

    pub(crate) const fn materializations(self) -> u64 {
        self.materializations
    }
}

#[derive(Debug, Clone)]
pub struct GraphSnapshot {
    provenance: RuntimeProvenance,
    graph: Arc<Graph>,
    revision: GraphRevision,
    document_epoch: DocumentEpoch,
    node_incarnations: NodeIncarnationMap,
    edge_incarnations: EdgeIncarnationMap,
    port_incarnations: PortIncarnationMap,
}

impl GraphSnapshot {
    fn new(
        provenance: RuntimeProvenance,
        graph: Arc<Graph>,
        revision: GraphRevision,
        document_epoch: DocumentEpoch,
        node_incarnations: NodeIncarnationMap,
        edge_incarnations: EdgeIncarnationMap,
        port_incarnations: PortIncarnationMap,
    ) -> Self {
        Self {
            provenance,
            graph,
            revision,
            document_epoch,
            node_incarnations,
            edge_incarnations,
            port_incarnations,
        }
    }

    pub fn graph(&self) -> &Graph {
        &self.graph
    }

    pub fn provenance(&self) -> RuntimeProvenance {
        self.provenance
    }

    pub fn revision(&self) -> GraphRevision {
        self.revision
    }

    pub fn version(&self) -> GraphVersion {
        GraphVersion::new(self.provenance, self.revision)
    }

    pub fn document_epoch(&self) -> DocumentEpoch {
        self.document_epoch
    }

    /// Returns the lifetime identity of a node present in this exact graph snapshot.
    pub(crate) fn node_incarnation(&self, node: NodeId) -> Option<NodeIncarnation> {
        self.node_incarnations.get(&node).copied()
    }

    /// Returns the complete identity of a node present in this exact graph snapshot.
    pub fn node_lifetime_key(&self, node: NodeId) -> Option<NodeLifetimeKey> {
        Some(NodeLifetimeKey::new(
            self.provenance,
            node,
            self.node_incarnation(node)?,
        ))
    }

    /// Returns whether the key still names the same live node in this runtime.
    pub fn node_lifetime_is_current(&self, key: NodeLifetimeKey) -> bool {
        self.node_lifetime_key(key.node()) == Some(key)
    }

    /// Returns the lifetime identity of an edge present in this exact graph snapshot.
    pub(crate) fn edge_incarnation(&self, edge: EdgeId) -> Option<EdgeIncarnation> {
        self.edge_incarnations
            .get(&edge)
            .map(EdgeIncarnationEntry::incarnation)
    }

    /// Returns the complete identity of an edge present in this exact graph snapshot.
    pub fn edge_instance_key(&self, edge: EdgeId) -> Option<EdgeInstanceKey> {
        Some(EdgeInstanceKey::new(
            self.provenance,
            edge,
            self.edge_incarnation(edge)?,
        ))
    }

    /// Returns whether the key still names the same live edge in this runtime.
    pub fn edge_instance_is_current(&self, key: EdgeInstanceKey) -> bool {
        self.edge_instance_key(key.edge()) == Some(key)
    }

    /// Returns the lifetime identity of a port present in this exact graph snapshot.
    pub(crate) fn port_incarnation(&self, port: PortId) -> Option<PortIncarnation> {
        self.port_incarnations
            .get(&port)
            .map(PortIncarnationEntry::incarnation)
    }

    /// Returns the complete identity of a port present in this exact graph snapshot.
    pub fn port_instance_key(&self, port: PortId) -> Option<PortInstanceKey> {
        let record = self.graph.port(&port)?;
        Some(PortInstanceKey::new(
            self.provenance,
            record.node,
            self.node_incarnation(record.node)?,
            port,
            self.port_incarnation(port)?,
        ))
    }

    /// Returns whether the key still names the same live port and owning node in this runtime.
    pub fn port_instance_is_current(&self, key: PortInstanceKey) -> bool {
        key.provenance() == self.provenance && self.port_instance_key(key.port()) == Some(key)
    }

    pub fn ptr_eq(&self, other: &Self) -> bool {
        self.provenance == other.provenance
            && Arc::ptr_eq(&self.graph, &other.graph)
            && self.revision == other.revision
            && self.document_epoch == other.document_epoch
            && self.node_incarnations.ptr_eq(&other.node_incarnations)
            && self.edge_incarnations.ptr_eq(&other.edge_incarnations)
            && self.port_incarnations.ptr_eq(&other.port_incarnations)
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
    #[error("graph command was issued by another runtime")]
    ForeignCommandRuntime {
        command: RuntimeProvenance,
        runtime: RuntimeProvenance,
    },
    #[error("graph structure is invalid")]
    InvalidGraph { errors: Vec<GraphValidationError> },
    #[error("graph canonicalization failed: {0}")]
    Canonicalization(#[from] NodeCatalogMigrationError),
    #[error(transparent)]
    Apply(#[from] ApplyPipelineError),
}

struct RuntimeState {
    provenance: RuntimeProvenance,
    graph: Arc<Graph>,
    revision: GraphRevision,
    document_epoch: DocumentEpoch,
    policy: GraphRuntimePolicy,
    history: GraphHistory,
    profile: Option<Box<dyn GraphProfile>>,
    middleware: Option<Box<dyn GraphRuntimeMiddleware>>,
    index: GraphIndex,
    incarnations: NodeIncarnationMap,
    edge_incarnations: EdgeIncarnationMap,
    port_incarnations: PortIncarnationMap,
    next_incarnation: u64,
    sessions: BTreeMap<ViewId, Weak<ViewShared>>,
    next_view_id: u64,
}

fn node_lifetime_key_from_state(
    runtime: RuntimeId,
    state: &RuntimeState,
    node: NodeId,
) -> Option<NodeLifetimeKey> {
    Some(NodeLifetimeKey::new(
        RuntimeProvenance::new(runtime),
        node,
        state.incarnations.get(&node).copied()?,
    ))
}

fn port_instance_key_from_state(
    runtime: RuntimeId,
    state: &RuntimeState,
    port: PortId,
) -> Option<PortInstanceKey> {
    let record = state.graph.port(&port)?;
    let node_incarnation = state.incarnations.get(&record.node).copied()?;
    let port_incarnation = state
        .port_incarnations
        .get(&port)
        .map(PortIncarnationEntry::incarnation)?;
    Some(PortInstanceKey::new(
        RuntimeProvenance::new(runtime),
        record.node,
        node_incarnation,
        port,
        port_incarnation,
    ))
}

fn edge_instance_key_from_state(
    runtime: RuntimeId,
    state: &RuntimeState,
    edge: EdgeId,
) -> Option<EdgeInstanceKey> {
    let incarnation = state
        .edge_incarnations
        .get(&edge)
        .map(EdgeIncarnationEntry::incarnation)?;
    Some(EdgeInstanceKey::new(
        RuntimeProvenance::new(runtime),
        edge,
        incarnation,
    ))
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
        reconciliation: RuntimePatchReconciliationReceipt,
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
/// use jellyflow_runtime::graph::GraphRuntime;
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
/// use jellyflow_runtime::graph::GraphRuntime;
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
    pub fn provenance(&self) -> RuntimeProvenance {
        RuntimeProvenance::new(self.shared.id)
    }

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
        let mut incarnations = NodeIncarnationMap::new_sync();
        for node in graph.nodes().keys().copied() {
            incarnations.insert_mut(node, allocate_incarnation(&mut next_incarnation)?);
        }
        let mut edge_incarnations = EdgeIncarnationMap::new_sync();
        for edge in graph.edges().keys().copied() {
            edge_incarnations.insert_mut(
                edge,
                EdgeIncarnationEntry(allocate_edge_incarnation(&mut next_incarnation)?),
            );
        }
        let mut port_incarnations = PortIncarnationMap::new_sync();
        for port in graph.ports().keys().copied() {
            port_incarnations.insert_mut(
                port,
                PortIncarnationEntry(allocate_port_incarnation(&mut next_incarnation)?),
            );
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
                    provenance: RuntimeProvenance::new(runtime_id),
                    graph: Arc::new(graph),
                    revision: 0,
                    document_epoch: 0,
                    policy,
                    history: GraphHistory::default(),
                    profile,
                    middleware: None,
                    index,
                    incarnations,
                    edge_incarnations,
                    port_incarnations,
                    next_incarnation,
                    sessions: BTreeMap::new(),
                    next_view_id: 1,
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

    /// Returns the full current identity for a node owned by this runtime.
    pub fn node_lifetime_key(
        &self,
        node: NodeId,
    ) -> Result<Option<NodeLifetimeKey>, GraphRuntimeError> {
        self.shared.node_lifetime_key(node)
    }

    /// Returns the full current identity for an edge owned by this runtime.
    pub fn edge_instance_key(
        &self,
        edge: EdgeId,
    ) -> Result<Option<EdgeInstanceKey>, GraphRuntimeError> {
        self.shared.edge_instance_key(edge)
    }

    /// Returns the full current identity for a real port owned by this runtime.
    pub fn port_instance_key(
        &self,
        port: PortId,
    ) -> Result<Option<PortInstanceKey>, GraphRuntimeError> {
        self.shared.port_instance_key(port)
    }

    pub fn history_state(&self) -> Result<HistoryState, GraphRuntimeError> {
        self.shared.history_state()
    }

    #[cfg(test)]
    pub(crate) fn clear_history(&self) -> Result<(), GraphRuntimeError> {
        self.shared.clear_history()
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
    pub fn execute(&self, command: GraphCommand) -> CommandExecutionCompletion {
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
    pub(crate) fn set_revision_for_test(&self, revision: GraphRevision) {
        self.shared.state.borrow_mut().revision = revision;
    }

    #[cfg(test)]
    pub(crate) fn installed_impact_state_for_test(
        &self,
    ) -> (GraphIndex, BTreeMap<NodeId, NodeIncarnation>, u64) {
        let state = self.shared.state.borrow();
        (
            state.index.clone(),
            state
                .incarnations
                .iter()
                .map(|(node, incarnation)| (*node, *incarnation))
                .collect(),
            state.next_incarnation,
        )
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
            state.provenance,
            state.graph.clone(),
            state.revision,
            state.document_epoch,
            state.incarnations.clone(),
            state.edge_incarnations.clone(),
            state.port_incarnations.clone(),
        ))
    }

    pub(crate) fn policy(&self) -> Result<GraphRuntimePolicy, GraphRuntimeError> {
        self.state
            .try_borrow()
            .map(|state| state.policy)
            .map_err(|_| GraphRuntimeError::Busy)
    }

    pub(crate) fn node_lifetime_key(
        &self,
        node: NodeId,
    ) -> Result<Option<NodeLifetimeKey>, GraphRuntimeError> {
        let state = self
            .state
            .try_borrow()
            .map_err(|_| GraphRuntimeError::Busy)?;
        Ok(node_lifetime_key_from_state(self.id, &state, node))
    }

    pub(crate) fn edge_instance_key(
        &self,
        edge: EdgeId,
    ) -> Result<Option<EdgeInstanceKey>, GraphRuntimeError> {
        let state = self
            .state
            .try_borrow()
            .map_err(|_| GraphRuntimeError::Busy)?;
        Ok(edge_instance_key_from_state(self.id, &state, edge))
    }

    pub(crate) fn port_instance_key(
        &self,
        port: PortId,
    ) -> Result<Option<PortInstanceKey>, GraphRuntimeError> {
        let state = self
            .state
            .try_borrow()
            .map_err(|_| GraphRuntimeError::Busy)?;
        Ok(port_instance_key_from_state(self.id, &state, port))
    }

    pub(crate) fn adapter_node_facts(
        &self,
        nodes: &BTreeSet<NodeId>,
    ) -> Result<BTreeMap<NodeId, AdapterNodeRuntimeFacts>, GraphRuntimeError> {
        let port_nodes = nodes.iter().copied().collect::<Vec<_>>();
        self.adapter_frame_node_facts(nodes, &port_nodes, usize::MAX)
            .map(|facts| facts.nodes)
    }

    pub(crate) fn adapter_frame_node_facts(
        &self,
        nodes: &BTreeSet<NodeId>,
        port_nodes_in_render_order: &[NodeId],
        port_scan_limit: usize,
    ) -> Result<AdapterFrameNodeFacts, GraphRuntimeError> {
        let state = self
            .state
            .try_borrow()
            .map_err(|_| GraphRuntimeError::Busy)?;
        let mut remaining_port_scan = port_scan_limit;
        let mut port_identities_examined = 0_u64;
        let mut port_budget_omitted_nodes = BTreeSet::new();
        let mut admitted_port_instances = BTreeMap::new();
        for node in port_nodes_in_render_order.iter().rev().copied() {
            let Some(record) = state.graph.node(&node) else {
                continue;
            };
            if record.ports.len() > remaining_port_scan {
                port_budget_omitted_nodes.insert(node);
                continue;
            }
            remaining_port_scan = remaining_port_scan.saturating_sub(record.ports.len());
            port_identities_examined =
                port_identities_examined.saturating_add(record.ports.len() as u64);
            let mut port_order = Vec::with_capacity(record.ports.len());
            let mut port_instances = BTreeMap::new();
            for port in &record.ports {
                if let Some(instance) = port_instance_key_from_state(self.id, &state, *port) {
                    port_order.push(*port);
                    port_instances.insert(*port, instance);
                }
            }
            admitted_port_instances.insert(node, (port_order, port_instances));
        }
        let facts = nodes
            .iter()
            .filter_map(|node| {
                let lifetime = node_lifetime_key_from_state(self.id, &state, *node)?;
                let incident_edges = state.index.incident_edge_snapshot(*node)?;
                let (port_order, port_instances) =
                    admitted_port_instances.remove(node).unwrap_or_default();
                Some((
                    *node,
                    AdapterNodeRuntimeFacts {
                        lifetime,
                        incident_edges,
                        port_order,
                        port_instances,
                    },
                ))
            })
            .collect();
        Ok(AdapterFrameNodeFacts {
            nodes: facts,
            port_identities_examined,
            port_budget_omitted_nodes,
        })
    }

    pub(crate) fn adapter_frame_incident_routing_dependencies(
        &self,
        nodes: &BTreeSet<NodeId>,
        excluded: &BTreeSet<NodeId>,
        node_limit: usize,
        scan_limit: usize,
    ) -> Result<IncidentNodeQuery, GraphRuntimeError> {
        let state = self
            .state
            .try_borrow()
            .map_err(|_| GraphRuntimeError::Busy)?;
        Ok(state.index.incident_nodes_for_nodes(
            &state.graph,
            nodes,
            excluded,
            node_limit,
            scan_limit,
        ))
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
        let initial_viewport_claim_available = options.seed.is_none();
        let seed = options.sanitized_seed(&state.graph)?;
        let view_id = ViewId::new(self.id, state.next_view_id);
        state.next_view_id = state
            .next_view_id
            .checked_add(1)
            .ok_or(AttachViewError::IdentityExhausted)?;
        let snapshot = GraphSnapshot::new(
            state.provenance,
            state.graph.clone(),
            state.revision,
            state.document_epoch,
            state.incarnations.clone(),
            state.edge_incarnations.clone(),
            state.port_incarnations.clone(),
        );
        let view = ViewShared::new(
            view_id,
            ViewGeneration::initial(),
            state.policy,
            &snapshot,
            seed,
            initial_viewport_claim_available,
        );
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
    ) -> CommandExecutionCompletion {
        let identity = command.identity().clone();
        match self.try_execute_command(command) {
            Ok(outcome) => CommandExecutionCompletion::from_outcome(outcome),
            Err(failure) => {
                let (error, planning_work) = failure.into_parts();
                CommandExecutionCompletion::from_error(identity, error, planning_work)
            }
        }
    }

    fn try_execute_command(
        self: &Rc<Self>,
        command: GraphCommand,
    ) -> Result<GraphCommandOutcome, CommandExecutionFailure> {
        self.ensure_command_runtime(&command)?;
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
            let version = GraphVersion::new(state.provenance, state.revision);
            let command = GraphCommand::connection(version, edit);
            let planned = plan_graph_command(
                &state.graph,
                &state.index,
                &self.catalog,
                &command,
                state.profile.as_deref(),
                &state.policy,
            );
            match planned {
                Ok(planned) => ConnectionEvaluation::accepted(version, planned.planning_work),
                Err(failure) => {
                    let planning_work = failure.work();
                    let rejection = command_rejection_from_planning(
                        command.identity().clone(),
                        version,
                        failure.into_error(),
                        planning_work,
                    );
                    ConnectionEvaluation::rejected(
                        version,
                        rejection.diagnostics().to_vec(),
                        planning_work,
                    )
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
            let mut next_incarnations = NodeIncarnationMap::new_sync();
            let mut next_edge_incarnations = EdgeIncarnationMap::new_sync();
            let mut next_port_incarnations = PortIncarnationMap::new_sync();
            let mut incarnation_delta = before
                .nodes()
                .keys()
                .copied()
                .map(|node| (node, None))
                .collect::<BTreeMap<_, _>>();
            for node in graph.nodes().keys().copied() {
                let incarnation = allocate_incarnation(&mut next_incarnation)?;
                next_incarnations.insert_mut(node, incarnation);
                incarnation_delta.insert(node, Some(incarnation));
            }
            for edge in graph.edges().keys().copied() {
                let incarnation = allocate_edge_incarnation(&mut next_incarnation)?;
                next_edge_incarnations.insert_mut(edge, EdgeIncarnationEntry(incarnation));
            }
            for port in graph.ports().keys().copied() {
                let incarnation = allocate_port_incarnation(&mut next_incarnation)?;
                next_port_incarnations.insert_mut(port, PortIncarnationEntry(incarnation));
            }
            let graph = Arc::new(graph);
            let edge_incarnations = next_edge_incarnations;
            let snapshot = GraphSnapshot::new(
                state.provenance,
                graph.clone(),
                to_revision,
                document_epoch,
                next_incarnations.clone(),
                edge_incarnations.clone(),
                next_port_incarnations.clone(),
            );
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
            state.edge_incarnations = edge_incarnations;
            state.port_incarnations = next_port_incarnations;
            state.next_incarnation = next_incarnation;
            let sessions = collect_live_sessions(&mut state);
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
        let mut completions = Vec::with_capacity(request.commands().len());
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
                    completions.push(CommandExecutionCompletion::from_outcome(outcome));
                }
                Ok(CommandFinalization::Committed {
                    outcome,
                    middleware_panic: command_panic,
                }) => {
                    completions.push(CommandExecutionCompletion::from_outcome(outcome));
                    if command_panic.is_some() {
                        panics.record(command_panic);
                        break;
                    }
                }
                Err(error) => {
                    let (error, planning_work) = error.into_parts();
                    completions.push(CommandExecutionCompletion::from_error(
                        command.identity().clone(),
                        error,
                        planning_work,
                    ));
                }
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
        Ok(ViewCloseOutcome::new(
            ViewCloseReason::HostClosed,
            completions,
        ))
    }

    fn execute_command_deferred(
        self: &Rc<Self>,
        command: &GraphCommand,
        notifications: &mut Vec<(RuntimePatch, Vec<(Rc<ViewShared>, ViewGeneration)>)>,
    ) -> Result<CommandFinalization, CommandExecutionFailure> {
        self.ensure_command_runtime(command)?;
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

    fn ensure_command_runtime(&self, command: &GraphCommand) -> Result<(), GraphRuntimeError> {
        let runtime = RuntimeProvenance::new(self.id);
        let command = command.expected_version().provenance();
        if command != runtime {
            return Err(GraphRuntimeError::ForeignCommandRuntime { command, runtime });
        }
        Ok(())
    }

    fn finalize_command_execution(
        &self,
        execution: CommandExecution,
        deliver: impl FnOnce(&RuntimePatch, Vec<(Rc<ViewShared>, ViewGeneration)>),
    ) -> CommandFinalization {
        match execution {
            CommandExecution::Outcome(outcome) => CommandFinalization::Uncommitted(outcome),
            CommandExecution::Committed {
                identity,
                patch,
                sessions,
                footprint,
                planning_work,
            } => {
                let middleware_panic =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        self.run_after_command_middleware(&patch);
                    }))
                    .err();
                deliver(&patch, sessions);
                CommandFinalization::Committed {
                    outcome: GraphCommandOutcome::Applied(GraphCommandReceipt::new(
                        identity,
                        GraphVersion::new(RuntimeProvenance::new(self.id), patch.to_revision()),
                        patch,
                        footprint,
                        planning_work,
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
            .filter_map(|(session, generation)| {
                let (selection_changed, reconciliation) =
                    session.apply_runtime_patch(patch)?.into_parts();
                Some((
                    Rc::downgrade(session),
                    *generation,
                    selection_changed,
                    reconciliation,
                ))
            })
            .collect::<Vec<_>>();
        let mut queue = self.notifications.queue.borrow_mut();
        queue.push_back(QueuedNotification::RuntimeEvent(
            GraphRuntimeEvent::Patched {
                patch: patch.clone(),
            },
        ));
        queue.extend(view_patches.into_iter().map(
            |(view, generation, selection_changed, reconciliation)| QueuedNotification::ViewPatch {
                view,
                generation,
                patch: patch.clone(),
                selection_changed,
                reconciliation,
            },
        ));
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
                    reconciliation,
                } => {
                    let Some(view) = view.upgrade() else {
                        continue;
                    };
                    panics.record(view.publish_runtime_patch(
                        generation,
                        patch,
                        selection_changed,
                        reconciliation,
                    ));
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
}
