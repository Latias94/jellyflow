use super::super::{HandleConnection, NodeGraphLookups};
use jellyflow_core::core::Graph;
use jellyflow_core::core::{Edge, EdgeId, EdgeKind, EdgeReconnectable};
use jellyflow_core::ops::EdgeEndpoints;

impl NodeGraphLookups {
    pub(super) fn apply_add_edge(&mut self, graph: &Graph, id: EdgeId, edge: &Edge) -> bool {
        let Some((entry, conn)) = Self::edge_lookup_entry_from_graph(
            graph,
            id,
            edge.kind,
            edge_endpoints(edge),
            edge.reconnectable,
        ) else {
            return false;
        };
        self.edge_lookup.insert(id, entry);
        self.add_edge_connection(conn);
        true
    }

    pub(super) fn apply_remove_edge(&mut self, id: EdgeId) -> bool {
        if let Some(conn) = self.connection_from_edge_lookup(id) {
            self.remove_edge_connection(conn);
        } else {
            self.slow_remove_edge_from_connection_lookup(id);
        }
        self.edge_lookup.remove(&id);
        true
    }

    pub(super) fn apply_set_edge_kind(&mut self, id: EdgeId, kind: EdgeKind) -> bool {
        if let Some(e) = self.edge_lookup.get_mut(&id) {
            e.kind = kind;
        }
        let Some(conn) = self.connection_from_edge_lookup(id) else {
            self.slow_update_edge_kind_in_connection_lookup(id, kind);
            return true;
        };
        self.update_edge_kind_in_connection_lookup(conn, kind);
        true
    }

    pub(super) fn apply_set_edge_reconnectable(
        &mut self,
        graph: &Graph,
        id: EdgeId,
        reconnectable: Option<EdgeReconnectable>,
    ) -> bool {
        if let Some(e) = self.edge_lookup.get_mut(&id) {
            e.reconnectable = reconnectable;
            return true;
        }
        let Some(edge) = graph.edges.get(&id) else {
            return false;
        };
        let Some((entry, _conn)) = Self::edge_lookup_entry_from_graph(
            graph,
            id,
            edge.kind,
            edge_endpoints(edge),
            reconnectable,
        ) else {
            return false;
        };
        self.edge_lookup.insert(id, entry);
        true
    }

    pub(super) fn apply_set_edge_endpoints(
        &mut self,
        graph: &Graph,
        id: EdgeId,
        _from: EdgeEndpoints,
        to: EdgeEndpoints,
    ) -> bool {
        if let Some(prev) = self.edge_lookup.get(&id).copied() {
            self.remove_edge_connection(HandleConnection::from_edge_lookup(id, prev));
        } else {
            self.slow_remove_edge_from_connection_lookup(id);
        }

        let Some((entry, conn)) = Self::edge_lookup_entry_from_graph(
            graph,
            id,
            self.edge_kind_for_endpoint_update(graph, id),
            to,
            self.edge_reconnectable_for_endpoint_update(graph, id),
        ) else {
            return false;
        };
        self.edge_lookup.insert(id, entry);
        self.add_edge_connection(conn);
        true
    }

    fn edge_kind_for_endpoint_update(&self, graph: &Graph, id: EdgeId) -> EdgeKind {
        graph
            .edges
            .get(&id)
            .map(|edge| edge.kind)
            .or_else(|| self.edge_lookup.get(&id).map(|entry| entry.kind))
            .unwrap_or(EdgeKind::Data)
    }

    fn edge_reconnectable_for_endpoint_update(
        &self,
        graph: &Graph,
        id: EdgeId,
    ) -> Option<EdgeReconnectable> {
        graph
            .edges
            .get(&id)
            .and_then(|edge| edge.reconnectable)
            .or_else(|| {
                self.edge_lookup
                    .get(&id)
                    .and_then(|entry| entry.reconnectable)
            })
    }
}

fn edge_endpoints(edge: &Edge) -> EdgeEndpoints {
    EdgeEndpoints {
        from: edge.from,
        to: edge.to,
    }
}
