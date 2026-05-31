use super::super::{HandleConnection, NodeGraphLookups};
use jellyflow_core::core::Graph;
use jellyflow_core::core::{Edge, EdgeId, EdgeKind, EdgeReconnectable};
use jellyflow_core::ops::EdgeEndpoints;

impl NodeGraphLookups {
    pub(super) fn apply_add_edge(&mut self, graph: &Graph, id: EdgeId) -> bool {
        let Some(edge) = graph.edges.get(&id) else {
            return false;
        };
        self.insert_edge_lookup_from_parts(
            graph,
            id,
            edge.kind,
            EdgeEndpoints::from_edge(edge),
            edge.reconnectable,
        )
    }

    pub(super) fn apply_remove_edge(&mut self, id: EdgeId) -> bool {
        self.remove_edge_from_lookups(id);
        true
    }

    pub(super) fn remove_edge_from_lookups(&mut self, edge_id: EdgeId) {
        if let Some(conn) = self.connection_from_edge_lookup(edge_id) {
            self.remove_edge_connection(conn);
        } else {
            self.slow_remove_edge_from_connection_lookup(edge_id);
        }
        self.edge_lookup.remove(&edge_id);
    }

    pub(super) fn remove_edges_from_lookups(&mut self, edges: &[(EdgeId, Edge)]) {
        for (edge_id, _edge) in edges {
            self.remove_edge_from_lookups(*edge_id);
        }
    }

    pub(super) fn apply_set_edge_kind(
        &mut self,
        graph: &Graph,
        id: EdgeId,
        kind: EdgeKind,
    ) -> bool {
        if let Some(e) = self.edge_lookup.get_mut(&id) {
            e.kind = kind;
        } else {
            let Some(edge) = graph.edges.get(&id) else {
                return false;
            };
            return self.recover_edge_lookup_from_graph(graph, id, kind, edge.reconnectable);
        }
        if let Some(conn) = self.connection_from_edge_lookup(id) {
            self.add_edge_connection(conn);
        }
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
            if let Some(conn) = self.connection_from_edge_lookup(id) {
                self.add_edge_connection(conn);
            }
            return true;
        }
        let Some(edge) = graph.edges.get(&id) else {
            return false;
        };
        self.recover_edge_lookup_from_graph(graph, id, edge.kind, reconnectable)
    }

    fn recover_edge_lookup_from_graph(
        &mut self,
        graph: &Graph,
        id: EdgeId,
        kind: EdgeKind,
        reconnectable: Option<EdgeReconnectable>,
    ) -> bool {
        let Some(edge) = graph.edges.get(&id) else {
            return false;
        };
        self.insert_edge_lookup_from_parts(
            graph,
            id,
            kind,
            EdgeEndpoints::from_edge(edge),
            reconnectable,
        )
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

        let Some(kind) = self.edge_kind_for_endpoint_update(graph, id) else {
            return false;
        };

        self.insert_edge_lookup_from_parts(
            graph,
            id,
            kind,
            to,
            self.edge_reconnectable_for_endpoint_update(graph, id),
        )
    }

    fn insert_edge_lookup_from_parts(
        &mut self,
        graph: &Graph,
        id: EdgeId,
        kind: EdgeKind,
        endpoints: EdgeEndpoints,
        reconnectable: Option<EdgeReconnectable>,
    ) -> bool {
        let Some((entry, conn)) =
            Self::edge_lookup_entry_from_graph(graph, id, kind, endpoints, reconnectable)
        else {
            return false;
        };
        self.edge_lookup.insert(id, entry);
        self.add_edge_connection(conn);
        true
    }

    fn edge_kind_for_endpoint_update(&self, graph: &Graph, id: EdgeId) -> Option<EdgeKind> {
        graph
            .edges
            .get(&id)
            .map(|edge| edge.kind)
            .or_else(|| self.edge_lookup.get(&id).map(|entry| entry.kind))
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
