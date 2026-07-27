use jellyflow_core::core::{PortDirection, PortId};
use jellyflow_core::interaction::NodeGraphConnectionMode;

use super::{
    GraphRevision, GraphRuntime, GraphVersion, PortInstanceKey, RuntimeProvenance, RuntimeShared,
    port_instance_key_from_state,
};
use crate::profile::{resolved_port_type, type_compatibility_or_default};
use crate::rules::ConnectionPortFacts;
use crate::runtime::policy::resolve_port_policy;
use crate::schema::CreateFromWireCandidate;

/// Revision-bound semantic candidates for completing one dropped wire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateFromWireCandidates {
    version: GraphVersion,
    source: PortInstanceKey,
    candidates: Vec<CreateFromWireCandidate>,
}

impl CreateFromWireCandidates {
    pub const fn revision(&self) -> GraphRevision {
        self.version.revision()
    }

    pub const fn version(&self) -> GraphVersion {
        self.version
    }

    pub const fn source_instance(&self) -> PortInstanceKey {
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
    #[error("source port belongs to another graph runtime: {port:?}")]
    ForeignSourcePort { port: PortId },
    #[error("source port was retired or replaced: {port:?}")]
    SourcePortRetired { port: PortId },
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
        source: PortInstanceKey,
        mode: NodeGraphConnectionMode,
    ) -> Result<CreateFromWireCandidates, CreateFromWireQueryError> {
        self.shared.create_from_wire_candidates(source, mode)
    }
}

impl RuntimeShared {
    pub(crate) fn create_from_wire_candidates(
        &self,
        source: PortInstanceKey,
        mode: NodeGraphConnectionMode,
    ) -> Result<CreateFromWireCandidates, CreateFromWireQueryError> {
        let state = self
            .state
            .try_borrow()
            .map_err(|_| CreateFromWireQueryError::Busy)?;
        if source.provenance() != RuntimeProvenance::new(self.id) {
            return Err(CreateFromWireQueryError::ForeignSourcePort {
                port: source.port(),
            });
        }
        if port_instance_key_from_state(self.id, &state, source.port()) != Some(source) {
            return Err(CreateFromWireQueryError::SourcePortRetired {
                port: source.port(),
            });
        }
        let source_port = state.graph.port(&source.port()).ok_or(
            CreateFromWireQueryError::MissingSourcePort {
                port: source.port(),
            },
        )?;
        let source_node = state.graph.node(&source_port.node).ok_or(
            CreateFromWireQueryError::MissingSourceNode {
                port: source.port(),
            },
        )?;
        let source_policy = resolve_port_policy(source_node, source_port, &state.policy);
        let source_is_connectable = match source_port.dir {
            PortDirection::Out => source_policy.can_start_connection(),
            PortDirection::In => source_policy.can_accept_connection(),
        };
        if !source_is_connectable {
            return Err(CreateFromWireQueryError::SourceNotConnectable {
                port: source.port(),
            });
        }

        let profile = state.profile.as_deref();
        let resolved_type = resolved_port_type(profile, &state.graph, source.port());
        let candidates = self.catalog.create_from_wire_candidates_from_facts(
            ConnectionPortFacts::new(source_port.dir, source_port.kind, resolved_type.as_ref()),
            mode,
            type_compatibility_or_default(profile),
        );

        Ok(CreateFromWireCandidates {
            version: GraphVersion::new(state.provenance, state.revision),
            source,
            candidates,
        })
    }
}
