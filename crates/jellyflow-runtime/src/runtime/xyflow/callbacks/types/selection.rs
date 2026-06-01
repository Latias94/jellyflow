use jellyflow_core::core::{EdgeId, GroupId, NodeId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionChange {
    pub nodes: Vec<NodeId>,
    pub edges: Vec<EdgeId>,
    pub groups: Vec<GroupId>,
}

impl SelectionChange {
    pub fn new(nodes: Vec<NodeId>, edges: Vec<EdgeId>, groups: Vec<GroupId>) -> Self {
        Self {
            nodes,
            edges,
            groups,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty() && self.edges.is_empty() && self.groups.is_empty()
    }

    pub fn nodes(&self) -> &[NodeId] {
        &self.nodes
    }

    pub fn edges(&self) -> &[EdgeId] {
        &self.edges
    }

    pub fn groups(&self) -> &[GroupId] {
        &self.groups
    }

    pub fn into_parts(self) -> (Vec<NodeId>, Vec<EdgeId>, Vec<GroupId>) {
        (self.nodes, self.edges, self.groups)
    }
}
