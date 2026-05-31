use super::{EdgeLookupEntry, HandleConnection, NodeGraphLookups, NodeLookupEntry};
use jellyflow_core::core::{EdgeId, EdgeKind, EdgeReconnectable, Graph, NodeId};
use jellyflow_core::ops::EdgeEndpoints;

impl NodeGraphLookups {
    pub fn rebuild_from(&mut self, graph: &Graph) {
        self.node_lookup.clear();
        self.edge_lookup.clear();
        self.connection_lookup.clear();

        for (id, node) in &graph.nodes {
            self.node_lookup
                .insert(*id, NodeLookupEntry::from_node(node));
        }

        for (id, edge) in &graph.edges {
            let endpoints = EdgeEndpoints::from_edge(edge);
            let Some((entry, conn)) = Self::edge_lookup_entry_from_graph(
                graph,
                *id,
                edge.kind,
                endpoints,
                edge.reconnectable,
            ) else {
                continue;
            };

            self.edge_lookup.insert(*id, entry);
            self.add_edge_connection(conn);
        }
    }

    pub(super) fn edge_lookup_entry_from_graph(
        graph: &Graph,
        id: EdgeId,
        kind: EdgeKind,
        endpoints: EdgeEndpoints,
        reconnectable: Option<EdgeReconnectable>,
    ) -> Option<(EdgeLookupEntry, HandleConnection)> {
        let from_port = graph.ports.get(&endpoints.from)?;
        let to_port = graph.ports.get(&endpoints.to)?;
        let entry = EdgeLookupEntry::with_parts(
            kind,
            endpoints,
            from_port.node,
            to_port.node,
            reconnectable,
        );
        let conn = HandleConnection::from_edge_lookup(id, entry);
        Some((entry, conn))
    }

    pub(super) fn insert_node_lookup_from_graph(&mut self, graph: &Graph, id: NodeId) -> bool {
        let Some(node) = graph.nodes.get(&id) else {
            return false;
        };
        self.node_lookup
            .insert(id, NodeLookupEntry::from_node(node));
        true
    }
}
