use jellyflow_core::core::{Edge, EdgeId, NodeId, PortCapacity, PortDirection, PortId};

use crate::rules::EdgeEndpoint;
use crate::runtime::policy::{GraphRuntimePolicy, resolve_edge_policy, resolve_port_policy};

use super::{GraphRevision, GraphRuntimeError, RuntimeShared};

/// Revision-bound connection facts for one port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortConnectionFacts {
    revision: GraphRevision,
    port: PortId,
    node: NodeId,
    direction: PortDirection,
    capacity: PortCapacity,
    can_start_connection: bool,
    can_accept_connection: bool,
    incident: Vec<PortIncidentConnectionFacts>,
}

impl PortConnectionFacts {
    pub fn revision(&self) -> GraphRevision {
        self.revision
    }

    pub fn port(&self) -> PortId {
        self.port
    }

    pub fn node(&self) -> NodeId {
        self.node
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

/// Revision-bound port and policy facts needed to start a connection drag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PortConnectionDragFacts {
    revision: GraphRevision,
    port: PortId,
    node: NodeId,
    direction: PortDirection,
    capacity: PortCapacity,
    can_start_connection: bool,
    can_accept_connection: bool,
}

impl PortConnectionDragFacts {
    pub fn revision(self) -> GraphRevision {
        self.revision
    }

    pub fn port(self) -> PortId {
        self.port
    }

    pub fn node(self) -> NodeId {
        self.node
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

    pub fn port(self) -> PortId {
        self.facts().port()
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
    edge: EdgeId,
    endpoint: EdgeEndpoint,
    opposite_port: PortId,
    can_reconnect: bool,
}

impl PortIncidentConnectionFacts {
    pub fn edge(self) -> EdgeId {
        self.edge
    }

    pub fn endpoint(self) -> EdgeEndpoint {
        self.endpoint
    }

    pub fn opposite_port(self) -> PortId {
        self.opposite_port
    }

    pub fn can_reconnect(self) -> bool {
        self.can_reconnect
    }
}

fn resolve_incident_connection_facts(
    edge_id: EdgeId,
    port: PortId,
    edge: &Edge,
    policy: &GraphRuntimePolicy,
) -> Option<PortIncidentConnectionFacts> {
    let (endpoint, opposite_port) = if edge.from == port {
        (EdgeEndpoint::From, edge.to)
    } else if edge.to == port {
        (EdgeEndpoint::To, edge.from)
    } else {
        return None;
    };
    let edge_policy = resolve_edge_policy(edge, policy);
    let can_reconnect = match endpoint {
        EdgeEndpoint::From => edge_policy.can_reconnect_source(),
        EdgeEndpoint::To => edge_policy.can_reconnect_target(),
    };
    Some(PortIncidentConnectionFacts {
        edge: edge_id,
        endpoint,
        opposite_port,
        can_reconnect,
    })
}

impl RuntimeShared {
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
            let Some(facts) =
                resolve_incident_connection_facts(edge_id, port, current, &state.policy)
            else {
                continue;
            };
            incident.push(facts);
        }

        Ok(Some(PortConnectionFacts {
            revision: state.revision,
            port,
            node,
            direction,
            capacity,
            can_start_connection,
            can_accept_connection,
            incident,
        }))
    }

    pub(crate) fn port_connection_drag_intent(
        &self,
        port: PortId,
    ) -> Result<Option<PortConnectionDragIntent>, GraphRuntimeError> {
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
        let Some(owner) = state.graph.node(&node) else {
            state.stats.connection_query_fact_reads =
                state.stats.connection_query_fact_reads.saturating_add(2);
            return Ok(None);
        };
        let port_policy = resolve_port_policy(owner, current_port, &state.policy);
        let facts = PortConnectionDragFacts {
            revision: state.revision,
            port,
            node,
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
                let Some(incident) =
                    resolve_incident_connection_facts(edge_id, port, edge, &state.policy)
                else {
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
