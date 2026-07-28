use std::cell::Cell;

use jellyflow_core::core::{Edge, EdgeId, Node, NodeId, Port, PortCapacity, PortDirection, PortId};

use crate::rules::EdgeEndpoint;
use crate::runtime::policy::{GraphRuntimePolicy, resolve_edge_policy, resolve_port_policy};

use super::{
    EdgeIncarnationEntry, EdgeInstanceKey, GraphRevision, GraphRuntimeError, GraphVersion,
    PortIncarnationEntry, PortInstanceKey, RuntimeId, RuntimeProvenance, RuntimeShared,
    RuntimeState,
};

/// Exact authoritative work performed by one port-connection query.
///
/// The receipt is operation-scoped: its counters describe only the query that returned it, so
/// callers never need to sample mutable runtime-wide diagnostics before and after a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PortConnectionQueryWork {
    fact_reads: u64,
    incident_bucket_reads: u64,
    incident_entries_read: u64,
}

impl PortConnectionQueryWork {
    const fn new(fact_reads: u64, incident_bucket_reads: u64, incident_entries_read: u64) -> Self {
        Self {
            fact_reads,
            incident_bucket_reads,
            incident_entries_read,
        }
    }

    const fn empty() -> Self {
        Self::new(0, 0, 0)
    }

    /// Returns the number of authoritative graph records inspected by this query.
    #[must_use]
    pub const fn fact_reads(self) -> u64 {
        self.fact_reads
    }

    /// Returns the number of incident-edge index buckets opened by this query.
    #[must_use]
    pub const fn incident_bucket_reads(self) -> u64 {
        self.incident_bucket_reads
    }

    /// Returns the number of entries examined within incident-edge index buckets.
    #[must_use]
    pub const fn incident_entries_read(self) -> u64 {
        self.incident_entries_read
    }
}

struct PortConnectionQueryContext<'state> {
    runtime: RuntimeId,
    state: &'state RuntimeState,
    work: Cell<PortConnectionQueryWork>,
}

impl<'state> PortConnectionQueryContext<'state> {
    fn new(runtime: RuntimeId, state: &'state RuntimeState) -> Self {
        Self {
            runtime,
            state,
            work: Cell::new(PortConnectionQueryWork::empty()),
        }
    }

    fn port(&self, port: PortId) -> Option<&'state Port> {
        let mut work = self.work.get();
        work.fact_reads = work.fact_reads.saturating_add(1);
        self.work.set(work);
        self.state.graph.port(&port)
    }

    fn node(&self, node: NodeId) -> Option<&'state Node> {
        let mut work = self.work.get();
        work.fact_reads = work.fact_reads.saturating_add(1);
        self.work.set(work);
        self.state.graph.node(&node)
    }

    fn edge(&self, edge: EdgeId) -> Option<&'state Edge> {
        let mut work = self.work.get();
        work.fact_reads = work.fact_reads.saturating_add(1);
        self.work.set(work);
        self.state.graph.edge(&edge)
    }

    fn port_instance(&self, port: PortId, record: &Port) -> Option<PortInstanceKey> {
        let node_incarnation = self.state.incarnations.get(&record.node).copied()?;
        let port_incarnation = self
            .state
            .port_incarnations
            .get(&port)
            .map(PortIncarnationEntry::incarnation)?;
        Some(PortInstanceKey::new(
            RuntimeProvenance::new(self.runtime),
            record.node,
            node_incarnation,
            port,
            port_incarnation,
        ))
    }

    fn incident_edge_count(&self, port: PortId) -> usize {
        let mut work = self.work.get();
        work.incident_bucket_reads = work.incident_bucket_reads.saturating_add(1);
        self.work.set(work);
        self.state.index.incident_edge_count(port)
    }

    fn incident_edges(&self, port: PortId) -> impl Iterator<Item = EdgeId> + '_ {
        let mut work = self.work.get();
        work.incident_bucket_reads = work.incident_bucket_reads.saturating_add(1);
        self.work.set(work);
        self.state
            .index
            .incident_edges(port)
            .inspect(|_| self.inspect_incident_entry())
    }

    fn inspect_incident_entry(&self) {
        let mut work = self.work.get();
        work.incident_entries_read = work.incident_entries_read.saturating_add(1);
        self.work.set(work);
    }

    fn into_work(self) -> PortConnectionQueryWork {
        self.work.get()
    }
}

/// Semantic outcome and bounded work evidence for one port-connection query.
///
/// Missing ports remain an explicit `None` outcome inside this value, preserving the work receipt
/// on negative queries. Likewise, drag decisions such as
/// [`PortConnectionDragIntent::BlockedOccupied`] retain the exact scan that produced them.
#[must_use = "port-connection query outcomes and their work receipts must be handled"]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PortConnectionQueryResult<T> {
    outcome: T,
    work: PortConnectionQueryWork,
}

impl<T> PortConnectionQueryResult<T> {
    const fn new(outcome: T, work: PortConnectionQueryWork) -> Self {
        Self { outcome, work }
    }

    /// Borrows the semantic outcome produced by this query.
    #[must_use]
    pub const fn outcome(&self) -> &T {
        &self.outcome
    }

    /// Consumes the query and returns its semantic outcome.
    #[must_use]
    pub fn into_outcome(self) -> T {
        self.outcome
    }

    /// Returns exact bounded work for this query.
    #[must_use]
    pub const fn work(&self) -> PortConnectionQueryWork {
        self.work
    }

    /// Consumes the query into its semantic outcome and work receipt.
    #[must_use]
    pub fn into_parts(self) -> (T, PortConnectionQueryWork) {
        (self.outcome, self.work)
    }
}

/// Revision-bound connection facts for one port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortConnectionFacts {
    version: GraphVersion,
    instance: PortInstanceKey,
    direction: PortDirection,
    capacity: PortCapacity,
    can_start_connection: bool,
    can_accept_connection: bool,
    incident: Vec<PortIncidentConnectionFacts>,
}

impl PortConnectionFacts {
    pub fn revision(&self) -> GraphRevision {
        self.version.revision()
    }

    pub fn version(&self) -> GraphVersion {
        self.version
    }

    pub fn port(&self) -> PortId {
        self.instance.port()
    }

    pub fn node(&self) -> NodeId {
        self.instance.node()
    }

    pub fn instance(&self) -> PortInstanceKey {
        self.instance
    }

    pub fn direction(&self) -> PortDirection {
        self.direction
    }

    pub fn capacity(&self) -> PortCapacity {
        self.capacity
    }

    pub fn can_begin_connection(&self) -> bool {
        match self.direction {
            PortDirection::Out => self.can_start_connection,
            PortDirection::In => self.can_accept_connection,
        }
    }

    pub fn can_start_connection(&self) -> bool {
        self.can_start_connection
    }

    pub fn can_accept_connection(&self) -> bool {
        self.can_accept_connection
    }

    pub fn incident(&self) -> &[PortIncidentConnectionFacts] {
        &self.incident
    }
}

/// Revision-bound port policy and connection-count summary.
///
/// This is the constant-work query for presentation and capability checks. It deliberately does
/// not materialize incident edge identities; callers that need per-edge policy facts must request
/// [`PortConnectionFacts`] explicitly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PortConnectionSummary {
    version: GraphVersion,
    instance: PortInstanceKey,
    direction: PortDirection,
    capacity: PortCapacity,
    can_start_connection: bool,
    can_accept_connection: bool,
    incident_count: usize,
}

/// One coherent ordered result for a batch of port-summary requests.
///
/// Entries preserve the caller's order, including duplicate and missing port identities. Every
/// present summary is derived under the same authoritative runtime-state borrow and therefore has
/// [`Self::revision`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortConnectionSummaryBatch {
    version: GraphVersion,
    entries: Vec<(PortId, Option<PortConnectionSummary>)>,
}

impl PortConnectionSummaryBatch {
    /// Returns the authoritative graph revision shared by every present entry.
    pub fn revision(&self) -> GraphRevision {
        self.version.revision()
    }

    pub fn version(&self) -> GraphVersion {
        self.version
    }

    /// Returns the number of requested positions, including missing and duplicate ports.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns whether no port identities were requested.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterates requested identities and summaries in caller order.
    pub fn iter(
        &self,
    ) -> impl ExactSizeIterator<Item = (PortId, Option<PortConnectionSummary>)> + '_ {
        self.entries.iter().copied()
    }

    /// Consumes the batch into requested identities and summaries in caller order.
    pub fn into_entries(self) -> Vec<(PortId, Option<PortConnectionSummary>)> {
        self.entries
    }
}

impl PortConnectionSummary {
    pub fn revision(self) -> GraphRevision {
        self.version.revision()
    }

    pub fn version(self) -> GraphVersion {
        self.version
    }

    pub fn port(self) -> PortId {
        self.instance.port()
    }

    pub fn node(self) -> NodeId {
        self.instance.node()
    }

    pub fn instance(self) -> PortInstanceKey {
        self.instance
    }

    pub fn direction(self) -> PortDirection {
        self.direction
    }

    pub fn capacity(self) -> PortCapacity {
        self.capacity
    }

    pub fn can_begin_connection(self) -> bool {
        match self.direction {
            PortDirection::Out => self.can_start_connection,
            PortDirection::In => self.can_accept_connection,
        }
    }

    pub fn can_start_connection(self) -> bool {
        self.can_start_connection
    }

    pub fn can_accept_connection(self) -> bool {
        self.can_accept_connection
    }

    pub fn incident_count(self) -> usize {
        self.incident_count
    }

    pub fn is_connected(self) -> bool {
        self.incident_count > 0
    }
}

/// Revision-bound port and policy facts needed to start a connection drag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PortConnectionDragFacts {
    version: GraphVersion,
    instance: PortInstanceKey,
    direction: PortDirection,
    capacity: PortCapacity,
    can_start_connection: bool,
    can_accept_connection: bool,
}

impl PortConnectionDragFacts {
    pub fn provenance(self) -> RuntimeProvenance {
        self.instance.provenance()
    }

    pub fn revision(self) -> GraphRevision {
        self.version.revision()
    }

    pub fn version(self) -> GraphVersion {
        self.version
    }

    pub fn port(self) -> PortId {
        self.instance.port()
    }

    pub fn node(self) -> NodeId {
        self.instance.node()
    }

    pub fn instance(self) -> PortInstanceKey {
        self.instance
    }

    pub fn direction(self) -> PortDirection {
        self.direction
    }

    pub fn capacity(self) -> PortCapacity {
        self.capacity
    }

    pub fn can_begin_connection(self) -> bool {
        match self.direction {
            PortDirection::Out => self.can_start_connection,
            PortDirection::In => self.can_accept_connection,
        }
    }

    pub fn can_start_connection(self) -> bool {
        self.can_start_connection
    }

    pub fn can_accept_connection(self) -> bool {
        self.can_accept_connection
    }
}

/// Host-facing connection drag decision that avoids materializing the full incident edge list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortConnectionDragIntent {
    /// Start a new connection from an output or a vacant input.
    Connect { facts: PortConnectionDragFacts },
    /// Reconnect the first eligible incident edge in stable [`EdgeId`] order.
    Reconnect {
        facts: PortConnectionDragFacts,
        candidate: PortIncidentConnectionFacts,
    },
    /// The input is occupied, but none of its incident edges can be reconnected.
    BlockedOccupied { facts: PortConnectionDragFacts },
}

impl PortConnectionDragIntent {
    pub fn provenance(self) -> RuntimeProvenance {
        self.facts().provenance()
    }

    pub fn facts(self) -> PortConnectionDragFacts {
        match self {
            Self::Connect { facts }
            | Self::Reconnect { facts, .. }
            | Self::BlockedOccupied { facts } => facts,
        }
    }

    pub fn revision(self) -> GraphRevision {
        self.facts().revision()
    }

    pub fn version(self) -> GraphVersion {
        self.facts().version()
    }

    pub fn port(self) -> PortId {
        self.facts().port()
    }

    pub fn instance(self) -> PortInstanceKey {
        self.facts().instance()
    }

    pub fn node(self) -> NodeId {
        self.facts().node()
    }

    pub fn direction(self) -> PortDirection {
        self.facts().direction()
    }

    pub fn capacity(self) -> PortCapacity {
        self.facts().capacity()
    }

    pub fn can_begin_connection(self) -> bool {
        self.facts().can_begin_connection()
    }

    pub fn can_start_connection(self) -> bool {
        self.facts().can_start_connection()
    }

    pub fn can_accept_connection(self) -> bool {
        self.facts().can_accept_connection()
    }
}

/// Authority classification and semantic decision for one port-drag query.
///
/// Foreign and stale capabilities remain distinct even though native adapters commonly project
/// both into a rejected gesture start. Keeping the classification inside the query outcome lets
/// every branch retain its operation-scoped work receipt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortConnectionDragQueryOutcome {
    /// The exact live port capability resolved to a drag decision.
    Ready(PortConnectionDragIntent),
    /// The capability was issued by another graph runtime.
    Foreign,
    /// The capability belonged to this runtime but its port lifetime has ended.
    Stale,
}

impl PortConnectionDragQueryOutcome {
    /// Returns the live drag decision, or `None` for foreign and stale capabilities.
    #[must_use]
    pub const fn ready(self) -> Option<PortConnectionDragIntent> {
        match self {
            Self::Ready(intent) => Some(intent),
            Self::Foreign | Self::Stale => None,
        }
    }
}

/// Connection facts for one edge incident to a queried port.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PortIncidentConnectionFacts {
    edge: EdgeInstanceKey,
    endpoint: EdgeEndpoint,
    opposite_port: PortInstanceKey,
    can_reconnect: bool,
}

impl PortIncidentConnectionFacts {
    pub fn edge(self) -> EdgeId {
        self.edge.edge()
    }

    pub fn edge_instance(self) -> EdgeInstanceKey {
        self.edge
    }

    pub fn endpoint(self) -> EdgeEndpoint {
        self.endpoint
    }

    pub fn opposite_port(self) -> PortId {
        self.opposite_port.port()
    }

    pub fn opposite_port_instance(self) -> PortInstanceKey {
        self.opposite_port
    }

    pub fn can_reconnect(self) -> bool {
        self.can_reconnect
    }
}

fn resolve_incident_connection_facts(
    edge: EdgeInstanceKey,
    port: PortId,
    model: &Edge,
    opposite_port: PortInstanceKey,
    policy: &GraphRuntimePolicy,
) -> Option<PortIncidentConnectionFacts> {
    let endpoint = if model.from == port && opposite_port.port() == model.to {
        EdgeEndpoint::From
    } else if model.to == port && opposite_port.port() == model.from {
        EdgeEndpoint::To
    } else {
        return None;
    };
    let edge_policy = resolve_edge_policy(model, policy);
    let can_reconnect = match endpoint {
        EdgeEndpoint::From => edge_policy.can_reconnect_source(),
        EdgeEndpoint::To => edge_policy.can_reconnect_target(),
    };
    Some(PortIncidentConnectionFacts {
        edge,
        endpoint,
        opposite_port,
        can_reconnect,
    })
}

impl RuntimeShared {
    pub(crate) fn port_connection_summary(
        &self,
        port: PortId,
    ) -> Result<PortConnectionQueryResult<Option<PortConnectionSummary>>, GraphRuntimeError> {
        let query = self.port_connection_summaries(&[port])?;
        let (batch, work) = query.into_parts();
        let summary = batch
            .into_entries()
            .into_iter()
            .next()
            .and_then(|(_, summary)| summary);
        Ok(PortConnectionQueryResult::new(summary, work))
    }

    pub(crate) fn port_connection_summaries(
        &self,
        ports: &[PortId],
    ) -> Result<PortConnectionQueryResult<PortConnectionSummaryBatch>, GraphRuntimeError> {
        let state = self
            .state
            .try_borrow()
            .map_err(|_| GraphRuntimeError::Busy)?;
        let version = GraphVersion::new(state.provenance, state.revision);
        let query = PortConnectionQueryContext::new(self.id, &state);
        let mut entries = Vec::with_capacity(ports.len());
        for port in ports.iter().copied() {
            let Some(current_port) = query.port(port) else {
                entries.push((port, None));
                continue;
            };
            let Some(owner) = query.node(current_port.node) else {
                entries.push((port, None));
                continue;
            };
            let port_policy = resolve_port_policy(owner, current_port, &state.policy);
            let Some(instance) = query.port_instance(port, current_port) else {
                entries.push((port, None));
                continue;
            };
            let incident_count = query.incident_edge_count(port);
            entries.push((
                port,
                Some(PortConnectionSummary {
                    version,
                    instance,
                    direction: current_port.dir,
                    capacity: current_port.capacity,
                    can_start_connection: port_policy.can_start_connection(),
                    can_accept_connection: port_policy.can_accept_connection(),
                    incident_count,
                }),
            ));
        }
        let work = query.into_work();
        Ok(PortConnectionQueryResult::new(
            PortConnectionSummaryBatch { version, entries },
            work,
        ))
    }

    pub(crate) fn port_connection_facts(
        &self,
        port: PortId,
    ) -> Result<PortConnectionQueryResult<Option<PortConnectionFacts>>, GraphRuntimeError> {
        let state = self
            .state
            .try_borrow()
            .map_err(|_| GraphRuntimeError::Busy)?;
        let query = PortConnectionQueryContext::new(self.id, &state);
        let Some(current_port) = query.port(port) else {
            return Ok(PortConnectionQueryResult::new(None, query.into_work()));
        };
        let node = current_port.node;
        let direction = current_port.dir;
        let capacity = current_port.capacity;
        let Some(owner) = query.node(node) else {
            return Ok(PortConnectionQueryResult::new(None, query.into_work()));
        };
        let port_policy = resolve_port_policy(owner, current_port, &state.policy);
        let Some(instance) = query.port_instance(port, current_port) else {
            return Ok(PortConnectionQueryResult::new(None, query.into_work()));
        };
        let can_start_connection = port_policy.can_start_connection();
        let can_accept_connection = port_policy.can_accept_connection();

        let mut incident = Vec::new();
        for edge_id in query.incident_edges(port) {
            let Some(current) = query.edge(edge_id) else {
                continue;
            };
            let Some(edge_incarnation) = state
                .edge_incarnations
                .get(&edge_id)
                .map(EdgeIncarnationEntry::incarnation)
            else {
                continue;
            };
            let opposite_port = if current.from == port {
                current.to
            } else if current.to == port {
                current.from
            } else {
                continue;
            };
            let Some(opposite_record) = query.port(opposite_port) else {
                continue;
            };
            let Some(opposite_port) = query.port_instance(opposite_port, opposite_record) else {
                continue;
            };
            let Some(facts) = resolve_incident_connection_facts(
                EdgeInstanceKey::new(RuntimeProvenance::new(self.id), edge_id, edge_incarnation),
                port,
                current,
                opposite_port,
                &state.policy,
            ) else {
                continue;
            };
            incident.push(facts);
        }

        let work = query.into_work();
        Ok(PortConnectionQueryResult::new(
            Some(PortConnectionFacts {
                version: GraphVersion::new(state.provenance, state.revision),
                instance,
                direction,
                capacity,
                can_start_connection,
                can_accept_connection,
                incident,
            }),
            work,
        ))
    }

    pub(crate) fn port_connection_drag_intent(
        &self,
        instance: PortInstanceKey,
    ) -> Result<PortConnectionQueryResult<PortConnectionDragQueryOutcome>, GraphRuntimeError> {
        let state = self
            .state
            .try_borrow()
            .map_err(|_| GraphRuntimeError::Busy)?;
        let port = instance.port();
        if instance.provenance() != state.provenance {
            return Ok(PortConnectionQueryResult::new(
                PortConnectionDragQueryOutcome::Foreign,
                PortConnectionQueryWork::empty(),
            ));
        }
        let query = PortConnectionQueryContext::new(self.id, &state);
        let Some(current_port) = query.port(port) else {
            return Ok(PortConnectionQueryResult::new(
                PortConnectionDragQueryOutcome::Stale,
                query.into_work(),
            ));
        };
        if query.port_instance(port, current_port) != Some(instance) {
            return Ok(PortConnectionQueryResult::new(
                PortConnectionDragQueryOutcome::Stale,
                query.into_work(),
            ));
        }
        let node = current_port.node;
        let Some(owner) = query.node(node) else {
            return Ok(PortConnectionQueryResult::new(
                PortConnectionDragQueryOutcome::Stale,
                query.into_work(),
            ));
        };
        let port_policy = resolve_port_policy(owner, current_port, &state.policy);
        let facts = PortConnectionDragFacts {
            version: GraphVersion::new(state.provenance, state.revision),
            instance,
            direction: current_port.dir,
            capacity: current_port.capacity,
            can_start_connection: port_policy.can_start_connection(),
            can_accept_connection: port_policy.can_accept_connection(),
        };

        if facts.direction == PortDirection::Out {
            let work = query.into_work();
            return Ok(PortConnectionQueryResult::new(
                PortConnectionDragQueryOutcome::Ready(PortConnectionDragIntent::Connect { facts }),
                work,
            ));
        }

        let (candidate, occupied) = {
            let mut candidate = None;
            let mut occupied = false;
            for edge_id in query.incident_edges(port) {
                let Some(edge) = query.edge(edge_id) else {
                    continue;
                };
                let Some(edge_incarnation) = state
                    .edge_incarnations
                    .get(&edge_id)
                    .map(EdgeIncarnationEntry::incarnation)
                else {
                    continue;
                };
                let opposite_port = if edge.from == port {
                    edge.to
                } else if edge.to == port {
                    edge.from
                } else {
                    continue;
                };
                let Some(opposite_record) = query.port(opposite_port) else {
                    continue;
                };
                let Some(opposite_port) = query.port_instance(opposite_port, opposite_record)
                else {
                    continue;
                };
                let Some(incident) = resolve_incident_connection_facts(
                    EdgeInstanceKey::new(
                        RuntimeProvenance::new(self.id),
                        edge_id,
                        edge_incarnation,
                    ),
                    port,
                    edge,
                    opposite_port,
                    &state.policy,
                ) else {
                    continue;
                };
                occupied = true;
                if incident.can_reconnect() {
                    candidate = Some(incident);
                    break;
                }
            }
            (candidate, occupied)
        };
        let work = query.into_work();
        Ok(PortConnectionQueryResult::new(
            PortConnectionDragQueryOutcome::Ready(match candidate {
                Some(candidate) => PortConnectionDragIntent::Reconnect { facts, candidate },
                None if occupied => PortConnectionDragIntent::BlockedOccupied { facts },
                None => PortConnectionDragIntent::Connect { facts },
            }),
            work,
        ))
    }
}
