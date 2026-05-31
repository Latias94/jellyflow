//! Effective editor policy resolution for graph elements.
//!
//! `jellyflow-core` stores per-element policy overrides because they are part of the persisted
//! graph document. Runtime adapters should resolve those overrides through this module instead of
//! duplicating precedence rules.

mod edge;
mod node;
mod port;

pub use edge::{NodeGraphEdgeInteractionPolicy, resolve_edge_interaction_policy};
pub use node::{NodeGraphNodeInteractionPolicy, resolve_node_interaction_policy};
pub use port::{NodeGraphPortInteractionPolicy, resolve_port_interaction_policy};

#[cfg(test)]
mod tests;
