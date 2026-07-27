use jellyflow_core::core::{Edge, EdgeId, NodeId, PortCapacity, PortDirection, PortId};

use crate::rules::EdgeEndpoint;
use crate::runtime::policy::{GraphRuntimePolicy, resolve_edge_policy, resolve_port_policy};

use super::{
    EdgeIncarnationEntry, EdgeInstanceKey, GraphRevision, GraphRuntimeError, GraphVersion,
    PortInstanceKey, RuntimeProvenance, RuntimeShared, port_instance_key_from_state,
};

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
    ) -> Result<Option<PortConnectionSummary>, GraphRuntimeError> {
        Ok(self
            .port_connection_summaries(&[port])?
            .into_entries()
            .into_iter()
            .next()
            .and_then(|(_, summary)| summary))
    }

    pub(crate) fn port_connection_summaries(
        &self,
        ports: &[PortId],
    ) -> Result<PortConnectionSummaryBatch, GraphRuntimeError> {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| GraphRuntimeError::Busy)?;
        let version = GraphVersion::new(state.provenance, state.revision);
        let mut fact_reads = 0_u64;
        let entries = ports
            .iter()
            .copied()
            .map(|port| {
                let Some(current_port) = state.graph.port(&port) else {
                    fact_reads = fact_reads.saturating_add(1);
                    return (port, None);
                };
                let node = current_port.node;
                let Some(owner) = state.graph.node(&node) else {
                    fact_reads = fact_reads.saturating_add(2);
                    return (port, None);
                };
                let port_policy = resolve_port_policy(owner, current_port, &state.policy);
                let Some(instance) = port_instance_key_from_state(self.id, &state, port) else {
                    fact_reads = fact_reads.saturating_add(2);
                    return (port, None);
                };
                fact_reads = fact_reads.saturating_add(2);
                (
                    port,
                    Some(PortConnectionSummary {
                        version,
                        instance,
                        direction: current_port.dir,
                        capacity: current_port.capacity,
                        can_start_connection: port_policy.can_start_connection(),
                        can_accept_connection: port_policy.can_accept_connection(),
                        incident_count: state.index.incident_edge_count(port),
                    }),
                )
            })
            .collect();
        state.stats.connection_query_fact_reads = state
            .stats
            .connection_query_fact_reads
            .saturating_add(fact_reads);
        Ok(PortConnectionSummaryBatch { version, entries })
    }

    pub(crate) fn port_connection_facts(
        &self,
        port: PortId,
    ) -> Result<Option<PortConnectionFacts>, GraphRuntimeError> {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| GraphRuntimeError::Busy)?;
        let Some(current_port) = state.graph.port(&port) else {
            state.stats.connection_query_fact_reads =
                state.stats.connection_query_fact_reads.saturating_add(1);
            return Ok(None);
        };
        let node = current_port.node;
        let direction = current_port.dir;
        let capacity = current_port.capacity;
        let Some(owner) = state.graph.node(&node) else {
            state.stats.connection_query_fact_reads =
                state.stats.connection_query_fact_reads.saturating_add(2);
            return Ok(None);
        };
        let port_policy = resolve_port_policy(owner, current_port, &state.policy);
        let Some(instance) = port_instance_key_from_state(self.id, &state, port) else {
            state.stats.connection_query_fact_reads =
                state.stats.connection_query_fact_reads.saturating_add(2);
            return Ok(None);
        };
        let can_start_connection = port_policy.can_start_connection();
        let can_accept_connection = port_policy.can_accept_connection();

        let incident_edges = state.index.incident_edges(port).collect::<Vec<_>>();
        state.stats.connection_query_index_bucket_reads = state
            .stats
            .connection_query_index_bucket_reads
            .saturating_add(1);
        state.stats.connection_query_index_entries_read = state
            .stats
            .connection_query_index_entries_read
            .saturating_add(incident_edges.len() as u64);
        state.stats.connection_query_fact_reads =
            state.stats.connection_query_fact_reads.saturating_add(2);
        let mut incident = Vec::with_capacity(incident_edges.len());
        for edge_id in incident_edges {
            state.stats.connection_query_fact_reads =
                state.stats.connection_query_fact_reads.saturating_add(1);
            let Some(current) = state.graph.edge(&edge_id) else {
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
            let Some(opposite_port) = port_instance_key_from_state(self.id, &state, opposite_port)
            else {
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

        Ok(Some(PortConnectionFacts {
            version: GraphVersion::new(state.provenance, state.revision),
            instance,
            direction,
            capacity,
            can_start_connection,
            can_accept_connection,
            incident,
        }))
    }

    pub(crate) fn port_connection_drag_intent(
        &self,
        instance: PortInstanceKey,
    ) -> Result<Option<PortConnectionDragIntent>, GraphRuntimeError> {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| GraphRuntimeError::Busy)?;
        let port = instance.port();
        if instance.provenance() != state.provenance
            || port_instance_key_from_state(self.id, &state, port) != Some(instance)
        {
            state.stats.connection_query_fact_reads =
                state.stats.connection_query_fact_reads.saturating_add(1);
            return Ok(None);
        }
        let Some(current_port) = state.graph.port(&port) else {
            state.stats.connection_query_fact_reads =
                state.stats.connection_query_fact_reads.saturating_add(1);
            return Ok(None);
        };
        let node = current_port.node;
        let Some(owner) = state.graph.node(&node) else {
            state.stats.connection_query_fact_reads =
                state.stats.connection_query_fact_reads.saturating_add(2);
            return Ok(None);
        };
        let port_policy = resolve_port_policy(owner, current_port, &state.policy);
        let Some(instance) = port_instance_key_from_state(self.id, &state, port) else {
            state.stats.connection_query_fact_reads =
                state.stats.connection_query_fact_reads.saturating_add(2);
            return Ok(None);
        };
        let facts = PortConnectionDragFacts {
            version: GraphVersion::new(state.provenance, state.revision),
            instance,
            direction: current_port.dir,
            capacity: current_port.capacity,
            can_start_connection: port_policy.can_start_connection(),
            can_accept_connection: port_policy.can_accept_connection(),
        };
        state.stats.connection_query_fact_reads =
            state.stats.connection_query_fact_reads.saturating_add(2);

        if facts.direction == PortDirection::Out {
            return Ok(Some(PortConnectionDragIntent::Connect { facts }));
        }

        let (candidate, occupied, entries_read) = {
            let mut candidate = None;
            let mut occupied = false;
            let mut entries_read = 0_u64;
            for edge_id in state.index.incident_edges(port) {
                entries_read = entries_read.saturating_add(1);
                let Some(edge) = state.graph.edge(&edge_id) else {
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
                let Some(opposite_port) =
                    port_instance_key_from_state(self.id, &state, opposite_port)
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
            (candidate, occupied, entries_read)
        };
        state.stats.connection_query_index_bucket_reads = state
            .stats
            .connection_query_index_bucket_reads
            .saturating_add(1);
        state.stats.connection_query_index_entries_read = state
            .stats
            .connection_query_index_entries_read
            .saturating_add(entries_read);
        state.stats.connection_query_fact_reads = state
            .stats
            .connection_query_fact_reads
            .saturating_add(entries_read);

        Ok(Some(match candidate {
            Some(candidate) => PortConnectionDragIntent::Reconnect { facts, candidate },
            None if occupied => PortConnectionDragIntent::BlockedOccupied { facts },
            None => PortConnectionDragIntent::Connect { facts },
        }))
    }
}
