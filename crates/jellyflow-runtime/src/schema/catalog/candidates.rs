use jellyflow_core::core::{NodeKindKey, Port, PortKey};
use jellyflow_core::interaction::NodeGraphConnectionMode;
use jellyflow_core::types::TypeCompatibility;

use super::NodeCatalog;
use crate::rules::{ConnectionPortFacts, connection_semantics_accepts};

/// One semantic node kind and real port that can complete a dropped wire.
///
/// The stable keys are owned so native search state can safely retain results across frames.
/// Product discovery metadata and native renderer mappings are joined outside the runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateFromWireCandidate {
    node_kind: NodeKindKey,
    kind_version: u32,
    port: PortKey,
}

impl CreateFromWireCandidate {
    pub fn node_kind(&self) -> &NodeKindKey {
        &self.node_kind
    }

    pub const fn kind_version(&self) -> u32 {
        self.kind_version
    }

    pub fn port(&self) -> &PortKey {
        &self.port
    }
}

impl NodeCatalog {
    /// Returns semantic create-node candidates in canonical kind and declared port order.
    ///
    /// This is an optimistic prefilter for native search. Final connection commands still validate
    /// graph/profile/capacity policy against the current revision before committing.
    pub fn create_from_wire_candidates(
        &self,
        source: &Port,
        mode: NodeGraphConnectionMode,
        compatibility: &dyn TypeCompatibility,
    ) -> Vec<CreateFromWireCandidate> {
        self.create_from_wire_candidates_from_facts(
            ConnectionPortFacts::new(source.dir, source.kind, source.ty.as_ref()),
            mode,
            compatibility,
        )
    }

    pub(crate) fn create_from_wire_candidates_from_facts(
        &self,
        source: ConnectionPortFacts<'_>,
        mode: NodeGraphConnectionMode,
        compatibility: &dyn TypeCompatibility,
    ) -> Vec<CreateFromWireCandidate> {
        let mut candidates = Vec::new();
        for node_kind in self.kinds() {
            for port in node_kind.ports() {
                if connection_semantics_accepts(
                    source,
                    ConnectionPortFacts::new(port.direction(), port.kind(), port.ty()),
                    mode,
                    compatibility,
                ) {
                    candidates.push(CreateFromWireCandidate {
                        node_kind: node_kind.kind().clone(),
                        kind_version: node_kind.latest_kind_version(),
                        port: port.key().clone(),
                    });
                }
            }
        }
        candidates
    }
}
