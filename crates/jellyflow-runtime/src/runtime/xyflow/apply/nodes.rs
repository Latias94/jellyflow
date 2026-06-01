use std::collections::HashSet;

use crate::runtime::xyflow::changes::NodeChange;
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
            NodeChange::Position { id, position } => {
                self.mutate_existing_node(*id, |node| node.pos = *position);
            }
            NodeChange::Origin { id, origin } => {
                self.mutate_existing_node(*id, |node| node.origin = *origin);
            }
            NodeChange::Kind { id, kind } => {
                self.mutate_existing_node(*id, |node| node.kind = kind.clone());
            }
            NodeChange::KindVersion { id, kind_version } => {
                self.mutate_existing_node(*id, |node| node.kind_version = *kind_version);
            }
            NodeChange::Selectable { id, selectable } => {
                self.mutate_existing_node(*id, |node| node.selectable = *selectable);
            }
            NodeChange::Focusable { id, focusable } => {
                self.mutate_existing_node(*id, |node| node.focusable = *focusable);
            }
            NodeChange::Draggable { id, draggable } => {
                self.mutate_existing_node(*id, |node| node.draggable = *draggable);
            }
            NodeChange::Connectable { id, connectable } => {
                self.mutate_existing_node(*id, |node| node.connectable = *connectable);
            }
            NodeChange::Deletable { id, deletable } => {
                self.mutate_existing_node(*id, |node| node.deletable = *deletable);
            }
            NodeChange::Parent { id, parent } => {
                self.mutate_existing_node(*id, |node| node.parent = *parent);
            }
            NodeChange::Extent { id, extent } => {
                self.mutate_existing_node(*id, |node| node.extent = *extent);
            }
            NodeChange::ExpandParent { id, expand_parent } => {
                self.mutate_existing_node(*id, |node| node.expand_parent = *expand_parent);
            }
            NodeChange::Size { id, size } => {
                self.mutate_existing_node(*id, |node| node.size = *size);
            }
            NodeChange::Hidden { id, hidden } => {
                self.mutate_existing_node(*id, |node| node.hidden = *hidden);
            }
            NodeChange::Collapsed { id, collapsed } => {
                self.mutate_existing_node(*id, |node| node.collapsed = *collapsed);
            }
            NodeChange::Data { id, data } => {
                self.mutate_existing_node(*id, |node| node.data = data.clone());
            }
            NodeChange::Ports { id, ports } => {
                self.mutate_existing_node(*id, |node| node.ports = ports.clone());
            }
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
