use std::collections::HashSet;

use crate::runtime::xyflow::changes::NodeChange;
use crate::runtime::xyflow::dialect::{
    apply_node_update_change as apply_node_update, node_update_id,
};
use jellyflow_core::core::{Edge, Node, NodeId, PortId};

use super::ApplyChangesPlanner;

impl<'a> ApplyChangesPlanner<'a> {
    pub(super) fn apply_nodes(&mut self, changes: &[NodeChange]) {
        for change in changes {
            self.apply_node_change(change);
        }
    }

    fn apply_node_change(&mut self, change: &NodeChange) {
        match change {
            NodeChange::Add { id, node } => {
                self.graph.nodes.insert(*id, node.clone());
                self.mark_applied();
            }
            NodeChange::Remove { id } => {
                self.remove_node_change(*id);
            }
            _ => self.apply_node_update_change(change),
        }
    }

    fn apply_node_update_change(&mut self, change: &NodeChange) {
        let Some(id) = node_update_id(change) else {
            self.mark_ignored();
            return;
        };
        let Some(node) = self.graph.nodes.get_mut(&id) else {
            self.mark_ignored();
            return;
        };
        if apply_node_update(change, node) {
            self.mark_applied();
        } else {
            self.mark_ignored();
        }
    }

    fn remove_node_change(&mut self, id: NodeId) {
        let Some(removed) = self.graph.nodes.remove(&id) else {
            self.mark_ignored();
            return;
        };

        self.remove_node_owned_resources(&removed);
        self.mark_applied();
    }

    fn remove_node_owned_resources(&mut self, removed: &Node) {
        let cascade = RemovedNodeCascade::from_node(removed);
        if cascade.is_empty() {
            return;
        }

        self.graph.ports.retain(|pid, _| !cascade.owns_port(pid));
        self.graph
            .edges
            .retain(|_, edge| !cascade.edge_is_incident(edge));
    }
}

struct RemovedNodeCascade {
    port_ids: HashSet<PortId>,
}

impl RemovedNodeCascade {
    fn from_node(node: &Node) -> Self {
        Self {
            port_ids: node.ports.iter().copied().collect(),
        }
    }

    fn is_empty(&self) -> bool {
        self.port_ids.is_empty()
    }

    fn owns_port(&self, id: &PortId) -> bool {
        self.port_ids.contains(id)
    }

    fn edge_is_incident(&self, edge: &Edge) -> bool {
        self.port_ids.contains(&edge.from) || self.port_ids.contains(&edge.to)
    }
}
