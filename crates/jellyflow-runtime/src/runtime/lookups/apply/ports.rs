use super::super::NodeGraphLookups;
use jellyflow_core::core::{Edge, EdgeId, Port, PortId};

impl NodeGraphLookups {
    pub(super) fn apply_remove_port(
        &mut self,
        id: PortId,
        port: &Port,
        edges: &[(EdgeId, Edge)],
    ) -> bool {
        if let Some(n) = self.node_lookup.get_mut(&port.node) {
            n.ports.retain(|port_id| *port_id != id);
        }
        for (edge_id, _edge) in edges {
            self.remove_edge_from_lookups(*edge_id);
        }
        true
    }
}
