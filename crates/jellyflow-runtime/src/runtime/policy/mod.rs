//! Effective editor policy resolution for graph elements.
//!
//! `jellyflow-core` stores per-element policy overrides because they are part of the persisted
//! graph document. Runtime adapters should resolve those overrides through this module instead of
//! duplicating precedence rules.

mod edge;
mod node;
mod port;

use crate::io::NodeGraphInteractionState;
use jellyflow_core::core::{Edge, Node, Port};

pub use edge::{
    NodeGraphEdgeInteractionPolicy, resolve_edge_hit_test_options, resolve_edge_interaction_policy,
};
pub use node::{NodeGraphNodeInteractionPolicy, resolve_node_interaction_policy};
pub use port::{NodeGraphPortInteractionPolicy, resolve_port_interaction_policy};

impl NodeGraphInteractionState {
    pub fn node_interaction_policy(&self, node: &Node) -> NodeGraphNodeInteractionPolicy {
        resolve_node_interaction_policy(node, self)
    }

    pub fn port_interaction_policy(
        &self,
        node: &Node,
        port: &Port,
    ) -> NodeGraphPortInteractionPolicy {
        resolve_port_interaction_policy(node, port, self)
    }

    pub fn edge_interaction_policy(&self, edge: &Edge) -> NodeGraphEdgeInteractionPolicy {
        resolve_edge_interaction_policy(edge, self)
    }

    pub fn edge_hit_test_options_for(
        &self,
        edge: &Edge,
    ) -> crate::runtime::geometry::EdgeHitTestOptions {
        resolve_edge_hit_test_options(edge, self)
    }
}

#[cfg(test)]
mod tests;
