use std::collections::BTreeMap;

use super::NodeGraphLookups;
use jellyflow_core::core::{GroupId, NodeId};

impl NodeGraphLookups {
    pub fn parent_for_node(&self, node: NodeId) -> Option<GroupId> {
        self.node_lookup.get(&node).and_then(|entry| entry.parent)
    }

    pub fn child_nodes_for_parent(&self, parent: GroupId) -> Vec<NodeId> {
        let mut children: Vec<NodeId> = self
            .node_lookup
            .iter()
            .filter_map(|(node, entry)| (entry.parent == Some(parent)).then_some(*node))
            .collect();
        children.sort();
        children
    }

    pub fn child_nodes_by_parent(&self) -> BTreeMap<GroupId, Vec<NodeId>> {
        let mut out: BTreeMap<GroupId, Vec<NodeId>> = BTreeMap::new();
        for (node, entry) in &self.node_lookup {
            let Some(parent) = entry.parent else {
                continue;
            };
            out.entry(parent).or_default().push(*node);
        }
        for children in out.values_mut() {
            children.sort();
        }
        out
    }

    pub fn root_nodes(&self) -> Vec<NodeId> {
        let mut roots: Vec<NodeId> = self
            .node_lookup
            .iter()
            .filter_map(|(node, entry)| entry.parent.is_none().then_some(*node))
            .collect();
        roots.sort();
        roots
    }
}
