use jellyflow_core::core::{PortDirection, PortId};
use jellyflow_core::interaction::NodeGraphConnectionMode;

use super::{GraphRevision, GraphRuntime, RuntimeShared};
use crate::profile::{resolved_port_type, type_compatibility_or_default};
use crate::rules::ConnectionPortFacts;
use crate::runtime::policy::resolve_port_policy;
use crate::schema::CreateFromWireCandidate;

/// Revision-bound semantic candidates for completing one dropped wire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateFromWireCandidates {
    revision: GraphRevision,
    source: PortId,
    candidates: Vec<CreateFromWireCandidate>,
}

impl CreateFromWireCandidates {
    pub const fn revision(&self) -> GraphRevision {
        self.revision
    }

    pub const fn source(&self) -> PortId {
        self.source
    }

    pub fn candidates(&self) -> &[CreateFromWireCandidate] {
        &self.candidates
    }

    pub fn into_candidates(self) -> Vec<CreateFromWireCandidate> {
        self.candidates
    }

    pub fn is_empty(&self) -> bool {
        self.candidates.is_empty()
    }
}

/// Failure to query create-from-wire candidates from the current authoritative graph.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CreateFromWireQueryError {
    #[error("graph runtime state is currently borrowed")]
    Busy,
    #[error("source port does not exist in the current graph: {port:?}")]
    MissingSourcePort { port: PortId },
    #[error("source port owner does not exist in the current graph: port={port:?}")]
    MissingSourceNode { port: PortId },
    #[error("source port cannot participate in a connection: {port:?}")]
    SourceNotConnectable { port: PortId },
}

impl GraphRuntime {
    /// Queries stable create-node candidates from the current graph revision.
    ///
    /// The query is authoritative for source existence, effective runtime policy, resolved
    /// source type, and profile compatibility. Capacity, cycle, and other graph-wide facts remain
    /// final-command checks after the selected node has concrete IDs.
    pub fn create_from_wire_candidates(
        &self,
        source: PortId,
        mode: NodeGraphConnectionMode,
    ) -> Result<CreateFromWireCandidates, CreateFromWireQueryError> {
        self.shared.create_from_wire_candidates(source, mode)
    }
}

impl RuntimeShared {
    pub(crate) fn create_from_wire_candidates(
        &self,
        source: PortId,
        mode: NodeGraphConnectionMode,
    ) -> Result<CreateFromWireCandidates, CreateFromWireQueryError> {
        let state = self
            .state
            .try_borrow()
            .map_err(|_| CreateFromWireQueryError::Busy)?;
        let source_port = state
            .graph
            .port(&source)
            .ok_or(CreateFromWireQueryError::MissingSourcePort { port: source })?;
        let source_node = state
            .graph
            .node(&source_port.node)
            .ok_or(CreateFromWireQueryError::MissingSourceNode { port: source })?;
        let source_policy = resolve_port_policy(source_node, source_port, &state.policy);
        let source_is_connectable = match source_port.dir {
            PortDirection::Out => source_policy.can_start_connection(),
            PortDirection::In => source_policy.can_accept_connection(),
        };
        if !source_is_connectable {
            return Err(CreateFromWireQueryError::SourceNotConnectable { port: source });
        }

        let profile = state.profile.as_deref();
        let resolved_type = resolved_port_type(profile, &state.graph, source);
        let candidates = self.catalog.create_from_wire_candidates_from_facts(
            ConnectionPortFacts::new(source_port.dir, source_port.kind, resolved_type.as_ref()),
            mode,
            type_compatibility_or_default(profile),
        );

        Ok(CreateFromWireCandidates {
            revision: state.revision,
            source,
            candidates,
        })
    }
}
