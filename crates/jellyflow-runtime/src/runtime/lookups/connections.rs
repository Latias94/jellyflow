use std::collections::HashMap;

use super::{ConnectionLookupKey, ConnectionSide, HandleConnection, NodeGraphLookups};
use jellyflow_core::core::{EdgeId, NodeId, PortId};

impl NodeGraphLookups {
    pub fn connections(
        &self,
        key: ConnectionLookupKey,
    ) -> Option<&HashMap<EdgeId, HandleConnection>> {
        self.connection_lookup.get(&key)
    }

    pub fn connections_for_node(&self, node: NodeId) -> Option<&HashMap<EdgeId, HandleConnection>> {
        self.connections(ConnectionLookupKey::Node(node))
    }

    pub fn connections_for_node_side(
        &self,
        node: NodeId,
        side: ConnectionSide,
    ) -> Option<&HashMap<EdgeId, HandleConnection>> {
        self.connections(ConnectionLookupKey::NodeSide { node, side })
    }

    pub fn connections_for_port(
        &self,
        node: NodeId,
        side: ConnectionSide,
        port: PortId,
    ) -> Option<&HashMap<EdgeId, HandleConnection>> {
        self.connections(ConnectionLookupKey::NodeSidePort { node, side, port })
    }

    fn add_connection(&mut self, key: ConnectionLookupKey, conn: HandleConnection) {
        self.connection_lookup
            .entry(key)
            .or_default()
            .insert(conn.edge, conn);
    }

    fn remove_connection(&mut self, key: ConnectionLookupKey, edge: EdgeId) {
        let Some(map) = self.connection_lookup.get_mut(&key) else {
            return;
        };
        map.remove(&edge);
        if map.is_empty() {
            self.connection_lookup.remove(&key);
        }
    }

    pub(super) fn add_edge_connection(&mut self, entry: HandleConnection) {
        for key in entry.lookup_keys() {
            self.add_connection(key, entry);
        }
    }

    pub(super) fn remove_edge_connection(&mut self, entry: HandleConnection) {
        for key in entry.lookup_keys() {
            self.remove_connection(key, entry.edge);
        }
    }

    pub(super) fn slow_remove_edge_from_connection_lookup(&mut self, edge: EdgeId) {
        let mut empty: Vec<ConnectionLookupKey> = Vec::new();
        for (key, map) in &mut self.connection_lookup {
            map.remove(&edge);
            if map.is_empty() {
                empty.push(*key);
            }
        }
        for k in empty {
            self.connection_lookup.remove(&k);
        }
    }

    pub(super) fn connection_from_edge_lookup(&self, edge: EdgeId) -> Option<HandleConnection> {
        let entry = *self.edge_lookup.get(&edge)?;
        Some(HandleConnection::from_edge_lookup(edge, entry))
    }
}
