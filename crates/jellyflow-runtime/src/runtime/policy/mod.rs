//! Authoritative graph mutation policy and per-element override resolution.
//!
//! `jellyflow-core` persists optional element-level overrides. `GraphRuntimePolicy` supplies the
//! renderer-neutral fallback used consistently by queries and committed commands.

mod edge;
mod node;
mod port;

use jellyflow_core::core::EdgeKind;

pub(crate) use edge::resolve_edge_policy;
pub(crate) use node::resolve_node_policy;
pub(crate) use port::{ResolvedPortPolicy, resolve_port_policy};

/// Renderer-neutral fallbacks for graph mutations governed by [`GraphRuntime`](
/// crate::runtime::graph_runtime::GraphRuntime).
///
/// Persisted node, port, and edge overrides take precedence over these defaults. Input bindings,
/// gesture thresholds, focus, selection, viewport behavior, and renderer caches belong to the
/// adapter and are intentionally absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphRuntimePolicy {
    node_connections: bool,
    node_deletion: bool,
    edge_deletion: bool,
    edge_reconnection: bool,
    data_cycles: bool,
    exec_cycles: bool,
}

impl GraphRuntimePolicy {
    /// Sets the connection fallback for nodes without a persisted `connectable` override.
    pub const fn with_node_connections(mut self, enabled: bool) -> Self {
        self.node_connections = enabled;
        self
    }

    /// Sets the deletion fallback for nodes without a persisted `deletable` override.
    pub const fn with_node_deletion(mut self, enabled: bool) -> Self {
        self.node_deletion = enabled;
        self
    }

    /// Sets the deletion fallback for edges without a persisted `deletable` override.
    pub const fn with_edge_deletion(mut self, enabled: bool) -> Self {
        self.edge_deletion = enabled;
        self
    }

    /// Sets the reconnection fallback for edges without a persisted `reconnectable` override.
    pub const fn with_edge_reconnection(mut self, enabled: bool) -> Self {
        self.edge_reconnection = enabled;
        self
    }

    /// Sets whether commands may create cycles for one semantic edge kind.
    pub const fn with_cycles(mut self, edge_kind: EdgeKind, allowed: bool) -> Self {
        match edge_kind {
            EdgeKind::Data => self.data_cycles = allowed,
            EdgeKind::Exec => self.exec_cycles = allowed,
        }
        self
    }

    pub const fn node_connections_enabled(self) -> bool {
        self.node_connections
    }

    pub const fn node_deletion_enabled(self) -> bool {
        self.node_deletion
    }

    pub const fn edge_deletion_enabled(self) -> bool {
        self.edge_deletion
    }

    pub const fn edge_reconnection_enabled(self) -> bool {
        self.edge_reconnection
    }

    /// Returns whether commands may create cycles for `edge_kind`.
    pub const fn cycles_allowed(self, edge_kind: EdgeKind) -> bool {
        match edge_kind {
            EdgeKind::Data => self.data_cycles,
            EdgeKind::Exec => self.exec_cycles,
        }
    }
}

impl Default for GraphRuntimePolicy {
    fn default() -> Self {
        Self {
            node_connections: true,
            node_deletion: true,
            edge_deletion: true,
            edge_reconnection: true,
            data_cycles: true,
            exec_cycles: true,
        }
    }
}

#[cfg(test)]
mod tests;
