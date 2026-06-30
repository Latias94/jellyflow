use super::super::NodeGraphLookups;
use crate::runtime::measurement::NodeInternalsInvalidationReason;
use jellyflow_core::core::{Edge, EdgeId, Graph, Port, PortId};

impl NodeGraphLookups {
    pub(super) fn apply_remove_port(
        &mut self,
        id: PortId,
        port: &Port,
        edges: &[(EdgeId, Edge)],
    ) -> bool {
        if let Some(n) = self.node_lookup.get_mut(&port.node) {
            n.ports.retain(|port_id| *port_id != id);
            n.measured_handles
                .retain(|measured| measured.handle.port != id);
            n.measured_anchors.retain(|anchor| {
                anchor.port != Some(id)
                    && anchor
                        .port_key
                        .as_ref()
                        .is_none_or(|port_key| port_key != &port.key)
            });
            n.mark_measurement_dirty_if_present(
                NodeInternalsInvalidationReason::ComponentStateChanged,
            );
        }
        self.remove_edges_from_lookups(edges);
        true
    }

    pub(super) fn apply_port_node_internals_change(&mut self, graph: &Graph, port: PortId) -> bool {
        let Some(port) = graph.ports().get(&port) else {
            return true;
        };
        if let Some(node) = self.node_lookup.get_mut(&port.node) {
            node.mark_measurement_dirty_if_present(
                NodeInternalsInvalidationReason::ComponentStateChanged,
            );
        }
        true
    }
}
