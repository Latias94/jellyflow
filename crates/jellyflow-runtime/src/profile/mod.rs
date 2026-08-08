//! Domain specializations for node graphs.
//!
//! A profile selects:
//! - how port types are resolved (schema vs inferred vs domain-owned),
//! - which compatibility rules apply,
//! - how concretization (dynamic ports) is scheduled,
//! - how validation diagnostics are produced.
//!
//! This module is intentionally headless (no `fret-ui` dependency).

mod pipeline;
mod query;

use crate::rules::Diagnostic;
use jellyflow_core::core::{EdgeKind, Graph, PortId};
use jellyflow_core::types::{DefaultTypeCompatibility, TypeCompatibility, TypeDesc};
pub use pipeline::ApplyPipelineError;
pub(crate) use pipeline::{validate_profile_document, validate_profile_graph};
pub use query::PortTypeImpactContext;
pub(crate) use query::PortTypeImpactQuery;

static DEFAULT_TYPE_COMPATIBILITY: DefaultTypeCompatibility = DefaultTypeCompatibility;

pub(crate) fn resolved_port_type(
    profile: Option<&dyn GraphProfile>,
    graph: &Graph,
    port: PortId,
) -> Option<TypeDesc> {
    match profile {
        Some(profile) => profile.type_of_port(graph, port),
        None => graph.port(&port).and_then(|port| port.ty.clone()),
    }
}

pub(crate) fn type_compatibility_or_default(
    profile: Option<&dyn GraphProfile>,
) -> &dyn TypeCompatibility {
    profile
        .map(GraphProfile::type_compatibility)
        .unwrap_or(&DEFAULT_TYPE_COMPATIBILITY)
}

/// Scope of port types directly affected by one mutation dependency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortTypeRevalidation {
    /// Revalidate every port in the staged graph. This is an explicit, counted full-graph fallback.
    All,
    /// Revalidate exactly these direct dependents. The runtime computes their transitive closure
    /// and rejects IDs absent from both the committed and staged graph.
    Exact(Vec<PortId>),
}

/// Profile hooks for typed graphs and domain specialization.
///
/// Every hook is a logical query. Implementations may record metrics through interior mutability,
/// but returned policy and topology must depend only on the supplied graph and immutable profile
/// configuration. Evaluation and rejected commands do not checkpoint or commit profile state.
pub trait GraphProfile {
    /// Returns the current type of a port.
    ///
    /// Implementations may read `Port::ty` and/or derive from graph topology and payloads. Resolved
    /// types must not depend on view-only position or size because those changes intentionally skip
    /// type propagation.
    fn type_of_port(&self, graph: &Graph, port: PortId) -> Option<TypeDesc>;

    /// Returns the pure assignability policy used by connection planning and document validation.
    fn type_compatibility(&self) -> &dyn TypeCompatibility {
        &DEFAULT_TYPE_COMPATIBILITY
    }

    /// Returns ports whose resolved type depends on non-port semantic facts in this mutation.
    ///
    /// The supplied context exposes the complete atomic mutation footprint plus counted before/after
    /// queries. The runtime already seeds directly edited ports and every port owned by an edited
    /// node. Returning [`PortTypeRevalidation::Exact`] declares any additional direct dependents;
    /// an empty set declares that those runtime-owned local seeds are sufficient.
    ///
    /// The conservative default revalidates every port. Profiles should override this hook when
    /// their type function has a narrower dependency boundary, especially for frequent node or
    /// edge payload edits.
    fn port_types_affected_by_mutation(
        &self,
        _context: &mut PortTypeImpactContext<'_>,
    ) -> PortTypeRevalidation {
        PortTypeRevalidation::All
    }

    /// Returns ports whose resolved type directly depends on `changed_port`.
    ///
    /// The runtime seeds directly edited ports and every port owned by an edited node, then computes
    /// the transitive closure of this hook with a bounded visited set. Profiles should use the
    /// counted before/after queries in [`PortTypeImpactContext`] for topology-dependent propagation.
    /// The default declares no cross-port dependency. [`PortTypeRevalidation::All`] remains an
    /// explicit escape hatch for genuinely graph-global type systems.
    fn dependent_port_types(
        &self,
        _context: &mut PortTypeImpactContext<'_>,
        _changed_port: PortId,
    ) -> PortTypeRevalidation {
        PortTypeRevalidation::Exact(Vec::new())
    }

    /// Validates a graph and returns diagnostics.
    ///
    fn validate_graph(&self, graph: &Graph) -> Vec<Diagnostic>;

    /// Whether the profile allows cycles for the given edge kind.
    fn allow_cycles(&self, _edge_kind: EdgeKind) -> bool {
        true
    }

    /// Maximum number of concretization iterations per edit.
    ///
    /// This bounds fixed-point scheduling to prevent oscillation.
    fn concretize_bound(&self) -> usize {
        8
    }

    /// Plans one deterministic concretization step for dynamic payload and topology.
    ///
    /// The runtime resolves the returned intent against the supplied graph, constructs the
    /// authoritative reversible operations, and records them with the originating command. History
    /// replay never invokes this hook again.
    fn concretize(&self, _graph: &Graph) -> crate::runtime::commands::TopologyEdit {
        crate::runtime::commands::TopologyEdit::new()
    }
}
