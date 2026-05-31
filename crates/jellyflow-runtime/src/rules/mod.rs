//! Constraint evaluation, connection planning, and diagnostics.
//!
//! This module is intentionally small in v1: the contracts are more important than the algorithms.

mod diagnostic;
mod plans;
mod template;

mod connection;
mod delete;

pub use connection::{
    plan_connect, plan_connect_by_inserting_node, plan_connect_by_inserting_node_with_policy,
    plan_connect_typed, plan_connect_typed_with_mode_and_policy, plan_connect_typed_with_policy,
    plan_connect_with_mode, plan_connect_with_mode_and_policy, plan_reconnect_edge,
    plan_reconnect_edge_with_mode, plan_reconnect_edge_with_mode_and_policy,
    plan_split_edge_by_inserting_node,
};
pub use delete::{
    plan_delete_edge, plan_delete_edge_with_policy, plan_delete_elements,
    plan_delete_elements_with_policy, plan_delete_node, plan_delete_node_with_policy,
};
pub use diagnostic::{Diagnostic, DiagnosticSeverity, DiagnosticTarget};
pub use plans::{ConnectDecision, ConnectPlan, DeleteDecision, DeletePlan, EdgeEndpoint};
pub use template::{InsertNodeSpec, InsertNodeTemplate, PortTemplate};

#[cfg(test)]
mod tests;
