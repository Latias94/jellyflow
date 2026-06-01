use super::super::{NodeGraphLookups, NodeLookupEntry};
use jellyflow_core::core::{
    CanvasPoint, CanvasSize, Edge, EdgeId, Graph, GroupId, Node, NodeId, NodeKindKey, NodeOrigin,
    PortId,
};

impl NodeGraphLookups {
    pub(super) fn apply_add_node(&mut self, id: NodeId, node: &Node) -> bool {
        self.node_lookup
            .insert(id, NodeLookupEntry::from_node(node));
        true
    }

    pub(super) fn apply_remove_node(&mut self, id: NodeId, edges: &[(EdgeId, Edge)]) -> bool {
        self.remove_edges_from_lookups(edges);
        self.node_lookup.remove(&id);
        true
    }

    pub(super) fn apply_set_node_pos(
        &mut self,
        graph: &Graph,
        id: NodeId,
        pos: CanvasPoint,
    ) -> bool {
        self.update_node_lookup_or_insert(graph, id, |node| node.pos = pos)
    }

    pub(super) fn apply_set_node_origin(&mut self, id: NodeId, origin: Option<NodeOrigin>) -> bool {
        self.update_existing_node_lookup(id, |node| node.origin = origin)
    }

    pub(super) fn apply_set_node_kind(
        &mut self,
        graph: &Graph,
        id: NodeId,
        kind: &NodeKindKey,
    ) -> bool {
        self.update_node_lookup_or_insert(graph, id, |node| node.kind = kind.clone())
    }

    pub(super) fn apply_set_node_kind_version(
        &mut self,
        graph: &Graph,
        id: NodeId,
        version: u32,
    ) -> bool {
        self.update_node_lookup_or_insert(graph, id, |node| node.kind_version = version)
    }

    pub(super) fn apply_set_node_parent(&mut self, id: NodeId, parent: Option<GroupId>) -> bool {
        self.update_existing_node_lookup(id, |node| node.parent = parent)
    }

    pub(super) fn apply_set_node_size(&mut self, id: NodeId, size: Option<CanvasSize>) -> bool {
        self.update_existing_node_lookup(id, |node| node.size = size)
    }

    pub(super) fn apply_set_node_hidden(&mut self, id: NodeId, hidden: bool) -> bool {
        self.update_existing_node_lookup(id, |node| node.hidden = hidden)
    }

    pub(super) fn apply_set_node_collapsed(&mut self, id: NodeId, collapsed: bool) -> bool {
        self.update_existing_node_lookup(id, |node| node.collapsed = collapsed)
    }

    pub(super) fn apply_set_node_ports(&mut self, id: NodeId, ports: &[PortId]) -> bool {
        self.update_existing_node_lookup(id, |node| node.ports = ports.to_vec())
    }

    fn update_node_lookup_or_insert(
        &mut self,
        graph: &Graph,
        id: NodeId,
        update: impl FnOnce(&mut NodeLookupEntry),
    ) -> bool {
        if self.update_existing_node_lookup(id, update) {
            true
        } else {
            self.insert_node_lookup_from_graph(graph, id)
        }
    }

    fn update_existing_node_lookup(
        &mut self,
        id: NodeId,
        update: impl FnOnce(&mut NodeLookupEntry),
    ) -> bool {
        let Some(node) = self.node_lookup.get_mut(&id) else {
            return false;
        };
        update(node);
        true
    }
}
