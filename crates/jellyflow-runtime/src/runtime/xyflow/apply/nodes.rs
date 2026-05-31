use std::collections::HashSet;

use crate::runtime::xyflow::changes::NodeChange;
use jellyflow_core::core::PortId;

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
                let Some(removed) = self.graph.nodes.remove(id) else {
                    self.mark_ignored();
                    return;
                };

                let port_ids: HashSet<PortId> =
                    removed.ports.iter().copied().collect::<HashSet<_>>();
                if !port_ids.is_empty() {
                    self.graph.ports.retain(|pid, _| !port_ids.contains(pid));
                    self.graph
                        .edges
                        .retain(|_, e| !port_ids.contains(&e.from) && !port_ids.contains(&e.to));
                }
                self.mark_applied();
            }
            NodeChange::Position { id, position } => {
                self.mutate_existing_node(*id, |node| node.pos = *position);
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
}
