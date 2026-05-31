use super::super::{NodeGraphLookups, NodeLookupEntry};
use jellyflow_core::core::{
    CanvasPoint, CanvasSize, Edge, EdgeId, Graph, GroupId, Node, NodeId, NodeKindKey, PortId,
};

impl NodeGraphLookups {
    pub(super) fn apply_add_node(&mut self, id: NodeId, node: &Node) -> bool {
        self.node_lookup
            .insert(id, NodeLookupEntry::from_node(node));
        true
    }

    pub(super) fn apply_remove_node(&mut self, id: NodeId, edges: &[(EdgeId, Edge)]) -> bool {
        for (edge_id, _edge) in edges {
            self.remove_edge_from_lookups(*edge_id);
        }
        self.node_lookup.remove(&id);
        true
    }

    pub(super) fn apply_set_node_pos(
        &mut self,
        graph: &Graph,
        id: NodeId,
        pos: CanvasPoint,
    ) -> bool {
        if let Some(n) = self.node_lookup.get_mut(&id) {
            n.pos = pos;
            return true;
        }
        self.insert_node_lookup_from_graph(graph, id)
    }

    pub(super) fn apply_set_node_kind(
        &mut self,
        graph: &Graph,
        id: NodeId,
        kind: &NodeKindKey,
    ) -> bool {
        if let Some(n) = self.node_lookup.get_mut(&id) {
            n.kind = kind.clone();
            return true;
        }
        self.insert_node_lookup_from_graph(graph, id)
    }

    pub(super) fn apply_set_node_kind_version(
        &mut self,
        graph: &Graph,
        id: NodeId,
        version: u32,
    ) -> bool {
        if let Some(n) = self.node_lookup.get_mut(&id) {
            n.kind_version = version;
            return true;
        }
        self.insert_node_lookup_from_graph(graph, id)
    }

    pub(super) fn apply_set_node_parent(&mut self, id: NodeId, parent: Option<GroupId>) -> bool {
        if let Some(n) = self.node_lookup.get_mut(&id) {
            n.parent = parent;
            return true;
        }
        false
    }

    pub(super) fn apply_set_node_size(&mut self, id: NodeId, size: Option<CanvasSize>) -> bool {
        if let Some(n) = self.node_lookup.get_mut(&id) {
            n.size = size;
            return true;
        }
        false
    }

    pub(super) fn apply_set_node_hidden(&mut self, id: NodeId, hidden: bool) -> bool {
        if let Some(n) = self.node_lookup.get_mut(&id) {
            n.hidden = hidden;
            return true;
        }
        false
    }

    pub(super) fn apply_set_node_collapsed(&mut self, id: NodeId, collapsed: bool) -> bool {
        if let Some(n) = self.node_lookup.get_mut(&id) {
            n.collapsed = collapsed;
            return true;
        }
        false
    }

    pub(super) fn apply_set_node_ports(&mut self, id: NodeId, ports: &[PortId]) -> bool {
        if let Some(n) = self.node_lookup.get_mut(&id) {
            n.ports = ports.to_vec();
            return true;
        }
        false
    }
}
