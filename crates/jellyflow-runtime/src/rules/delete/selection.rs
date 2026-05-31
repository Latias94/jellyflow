use std::collections::BTreeSet;

use jellyflow_core::core::{EdgeId, Graph, NodeId};

pub(super) struct DeleteSelection {
    pub(super) nodes: BTreeSet<NodeId>,
    pub(super) edges: BTreeSet<EdgeId>,
    cascaded_edges: BTreeSet<EdgeId>,
}

impl DeleteSelection {
    pub(super) fn from_requested(
        graph: &Graph,
        nodes: impl IntoIterator<Item = NodeId>,
        edges: impl IntoIterator<Item = EdgeId>,
    ) -> Self {
        let nodes = nodes.into_iter().collect::<BTreeSet<_>>();
        let edges = edges.into_iter().collect::<BTreeSet<_>>();
        let cascaded_edges = Self::cascaded_edges_for_nodes(graph, &nodes);

        Self {
            nodes,
            edges,
            cascaded_edges,
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.nodes.is_empty() && self.edges.is_empty()
    }

    pub(super) fn edge_is_cascaded(&self, edge_id: &EdgeId) -> bool {
        self.cascaded_edges.contains(edge_id)
    }

    pub(super) fn cascaded_edges(&self) -> &BTreeSet<EdgeId> {
        &self.cascaded_edges
    }

    fn cascaded_edges_for_nodes(graph: &Graph, nodes: &BTreeSet<NodeId>) -> BTreeSet<EdgeId> {
        let port_ids = graph
            .ports
            .iter()
            .filter_map(|(port_id, port)| nodes.contains(&port.node).then_some(*port_id))
            .collect::<BTreeSet<_>>();

        graph
            .edges
            .iter()
            .filter_map(|(edge_id, edge)| {
                (port_ids.contains(&edge.from) || port_ids.contains(&edge.to)).then_some(*edge_id)
            })
            .collect()
    }
}
