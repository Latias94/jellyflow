use crate::graph_kernel::GraphMutationFootprint;
use jellyflow_core::core::{Edge, EdgeId, Graph, Node, NodeId, Port, PortId, Symbol, SymbolId};

pub(crate) trait PortTypeImpactQuery {
    fn record_fact_reads(&mut self, count: usize);
    fn incident_edge_ids_before(&mut self, port: PortId) -> Vec<EdgeId>;
    fn incident_edge_ids_after(&mut self, port: PortId) -> Vec<EdgeId>;
    fn symbol_referrer_ids_before(&mut self, symbol: SymbolId) -> Vec<NodeId>;
    fn symbol_referrer_ids_after(&mut self, symbol: SymbolId) -> Vec<NodeId>;
}

/// Counted, index-backed graph queries for declaring direct port-type dependencies.
///
/// The context deliberately exposes individual facts rather than a raw [`Graph`]. This keeps
/// profile-driven command planning auditable: local fact and adjacency reads contribute to runtime
/// planning counters, while the runtime owns transitive dependency traversal.
pub struct PortTypeImpactContext<'a> {
    before: &'a Graph,
    after: &'a Graph,
    footprint: &'a GraphMutationFootprint,
    query: &'a mut dyn PortTypeImpactQuery,
}

impl<'a> PortTypeImpactContext<'a> {
    pub(crate) fn new(
        before: &'a Graph,
        after: &'a Graph,
        footprint: &'a GraphMutationFootprint,
        query: &'a mut dyn PortTypeImpactQuery,
    ) -> Self {
        Self {
            before,
            after,
            footprint,
            query,
        }
    }

    /// Stable IDs touched by the staged atomic command so far.
    pub fn footprint(&self) -> &GraphMutationFootprint {
        self.footprint
    }

    /// Reads a node from the committed graph and records one fact read.
    pub fn node_before(&mut self, node: NodeId) -> Option<&Node> {
        self.query.record_fact_reads(1);
        self.before.node(&node)
    }

    /// Reads a node from the staged graph and records one fact read.
    pub fn node_after(&mut self, node: NodeId) -> Option<&Node> {
        self.query.record_fact_reads(1);
        self.after.node(&node)
    }

    /// Reads a port from the committed graph and records one fact read.
    pub fn port_before(&mut self, port: PortId) -> Option<&Port> {
        self.query.record_fact_reads(1);
        self.before.port(&port)
    }

    /// Reads a port from the staged graph and records one fact read.
    pub fn port_after(&mut self, port: PortId) -> Option<&Port> {
        self.query.record_fact_reads(1);
        self.after.port(&port)
    }

    /// Reads an edge from the committed graph and records one fact read.
    pub fn edge_before(&mut self, edge: EdgeId) -> Option<&Edge> {
        self.query.record_fact_reads(1);
        self.before.edge(&edge)
    }

    /// Reads an edge from the staged graph and records one fact read.
    pub fn edge_after(&mut self, edge: EdgeId) -> Option<&Edge> {
        self.query.record_fact_reads(1);
        self.after.edge(&edge)
    }

    /// Reads a symbol from the committed graph and records one fact read.
    pub fn symbol_before(&mut self, symbol: SymbolId) -> Option<&Symbol> {
        self.query.record_fact_reads(1);
        self.before.symbol(&symbol)
    }

    /// Reads a symbol from the staged graph and records one fact read.
    pub fn symbol_after(&mut self, symbol: SymbolId) -> Option<&Symbol> {
        self.query.record_fact_reads(1);
        self.after.symbol(&symbol)
    }

    /// Reads the committed owner of a port and records one fact read.
    pub fn port_owner_before(&mut self, port: PortId) -> Option<NodeId> {
        self.port_before(port).map(|port| port.node)
    }

    /// Reads the staged owner of a port and records one fact read.
    pub fn port_owner_after(&mut self, port: PortId) -> Option<NodeId> {
        self.port_after(port).map(|port| port.node)
    }

    /// Reads a committed node's ordered port IDs and records one fact read.
    pub fn node_port_ids_before(&mut self, node: NodeId) -> Vec<PortId> {
        self.node_before(node)
            .map(|node| node.ports.clone())
            .unwrap_or_default()
    }

    /// Reads a staged node's ordered port IDs and records one fact read.
    pub fn node_port_ids_after(&mut self, node: NodeId) -> Vec<PortId> {
        self.node_after(node)
            .map(|node| node.ports.clone())
            .unwrap_or_default()
    }

    /// Reads committed endpoints for an edge and records one fact read.
    pub fn edge_endpoints_before(&mut self, edge: EdgeId) -> Option<(PortId, PortId)> {
        self.edge_before(edge).map(|edge| (edge.from, edge.to))
    }

    /// Reads staged endpoints for an edge and records one fact read.
    pub fn edge_endpoints_after(&mut self, edge: EdgeId) -> Option<(PortId, PortId)> {
        self.edge_after(edge).map(|edge| (edge.from, edge.to))
    }

    /// Reads one committed adjacency bucket through the runtime index.
    pub fn incident_edge_ids_before(&mut self, port: PortId) -> Vec<EdgeId> {
        self.query.incident_edge_ids_before(port)
    }

    /// Reads one staged adjacency bucket, including command-local edge changes.
    pub fn incident_edge_ids_after(&mut self, port: PortId) -> Vec<EdgeId> {
        self.query.incident_edge_ids_after(port)
    }

    /// Reads committed symbol-reference nodes through the runtime index.
    pub fn symbol_referrer_ids_before(&mut self, symbol: SymbolId) -> Vec<NodeId> {
        self.query.symbol_referrer_ids_before(symbol)
    }

    /// Reads staged symbol-reference nodes through the runtime index and command-local overlay.
    pub fn symbol_referrer_ids_after(&mut self, symbol: SymbolId) -> Vec<NodeId> {
        self.query.symbol_referrer_ids_after(symbol)
    }
}
