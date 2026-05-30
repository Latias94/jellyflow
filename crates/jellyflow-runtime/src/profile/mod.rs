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

use crate::io::NodeGraphInteractionState;
use crate::rules::{ConnectPlan, Diagnostic};
use jellyflow_core::core::{EdgeKind, Graph, PortId};
use jellyflow_core::interaction::NodeGraphConnectionMode;
use jellyflow_core::types::{DefaultTypeCompatibility, TypeDesc};
pub use pipeline::{
    ApplyPipelineError, apply_connect_plan_with_profile, apply_transaction_with_profile,
};

/// Profile hooks for typed graphs and domain specialization.
pub trait GraphProfile {
    /// Returns the current type of a port.
    ///
    /// Default implementations may read `Port::ty` and/or derive from node payloads.
    fn type_of_port(&mut self, graph: &Graph, port: PortId) -> Option<TypeDesc>;

    /// Plans connecting two ports under this profile.
    ///
    /// Implementations should call into `crate::rules` and then enforce extra constraints
    /// (type compatibility, cycle policy, exec/data policy, etc.).
    fn plan_connect(
        &mut self,
        graph: &Graph,
        a: PortId,
        b: PortId,
        mode: NodeGraphConnectionMode,
    ) -> ConnectPlan {
        let mut compat = DefaultTypeCompatibility::default();
        crate::rules::plan_connect_typed_with_mode_and_policy(
            graph,
            a,
            b,
            mode,
            &NodeGraphInteractionState::default(),
            |graph, port| self.type_of_port(graph, port),
            &mut compat,
        )
    }

    /// Validates a graph and returns diagnostics.
    fn validate_graph(&mut self, graph: &Graph) -> Vec<Diagnostic>;

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

    /// Runs concretization for dynamic ports and returns additional ops to apply.
    ///
    /// The returned ops must be deterministic and undoable.
    fn concretize(&mut self, _graph: &Graph) -> Vec<jellyflow_core::ops::GraphOp> {
        Vec::new()
    }
}
