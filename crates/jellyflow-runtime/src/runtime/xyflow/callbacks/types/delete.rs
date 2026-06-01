use jellyflow_core::core::{EdgeId, GroupId, NodeId, StickyNoteId};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeleteChange {
    pub nodes: Vec<NodeId>,
    pub edges: Vec<EdgeId>,
    pub groups: Vec<GroupId>,
    pub sticky_notes: Vec<StickyNoteId>,
}

impl DeleteChange {
    pub fn from_parts(
        nodes: Vec<NodeId>,
        edges: Vec<EdgeId>,
        groups: Vec<GroupId>,
        sticky_notes: Vec<StickyNoteId>,
    ) -> Self {
        Self {
            nodes,
            edges,
            groups,
            sticky_notes,
        }
    }

    pub fn into_parts(self) -> (Vec<NodeId>, Vec<EdgeId>, Vec<GroupId>, Vec<StickyNoteId>) {
        (self.nodes, self.edges, self.groups, self.sticky_notes)
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
            && self.edges.is_empty()
            && self.groups.is_empty()
            && self.sticky_notes.is_empty()
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

    pub fn sticky_notes(&self) -> &[StickyNoteId] {
        &self.sticky_notes
    }

    pub(in crate::runtime::xyflow) fn push_node(&mut self, node: NodeId) {
        self.nodes.push(node);
    }

    pub(in crate::runtime::xyflow) fn push_edge(&mut self, edge: EdgeId) {
        self.edges.push(edge);
    }

    pub(in crate::runtime::xyflow) fn push_group(&mut self, group: GroupId) {
        self.groups.push(group);
    }

    pub(in crate::runtime::xyflow) fn push_sticky_note(&mut self, sticky_note: StickyNoteId) {
        self.sticky_notes.push(sticky_note);
    }

    pub(in crate::runtime::xyflow) fn extend_edges(
        &mut self,
        edges: impl IntoIterator<Item = EdgeId>,
    ) {
        self.edges.extend(edges);
    }

    pub(in crate::runtime::xyflow) fn sort_dedup(&mut self) {
        sort_dedup_items(&mut self.nodes);
        sort_dedup_items(&mut self.edges);
        sort_dedup_items(&mut self.groups);
        sort_dedup_items(&mut self.sticky_notes);
    }
}

fn sort_dedup_items<T: Ord>(items: &mut Vec<T>) {
    items.sort_unstable();
    items.dedup();
}
