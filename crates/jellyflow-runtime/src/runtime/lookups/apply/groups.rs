use super::super::NodeGraphLookups;
use jellyflow_core::core::{GroupId, NodeId};

impl NodeGraphLookups {
    pub(super) fn apply_remove_group(&mut self, detached: &[(NodeId, Option<GroupId>)]) -> bool {
        for (node_id, _previous_parent) in detached {
            if let Some(n) = self.node_lookup.get_mut(node_id) {
                n.parent = None;
            }
        }
        true
    }
}
