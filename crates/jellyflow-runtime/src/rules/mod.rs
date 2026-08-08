//! Constraint evaluation, connection planning, and diagnostics.
//!
//! This module is intentionally small in v1: the contracts are more important than the algorithms.

mod diagnostic;
mod plans;

mod connection;
mod delete;

pub(crate) use connection::{
    ConnectionPlanningContext, ConnectionPortFacts, connection_semantics_accepts,
    edge_type_compatibility_diagnostic, plan_connect_typed_with_mode_and_policy_using,
    plan_reconnect_edge_typed_with_mode_and_policy_using,
};
pub(crate) use delete::{validate_delete_edge_policy, validate_delete_node_policy};
pub use diagnostic::{Diagnostic, DiagnosticSeverity, DiagnosticTarget};
pub(crate) use plans::ConnectPlan;
pub use plans::{ConnectDecision, EdgeEndpoint};
