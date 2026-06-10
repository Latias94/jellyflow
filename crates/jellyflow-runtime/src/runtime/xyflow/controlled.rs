use super::apply::{
    ApplyChangesReport, apply_edge_changes, apply_graph_changes, apply_node_changes,
};
use super::changes::{EdgeChange, NodeChange, NodeGraphChanges, NodeGraphPatch};
use super::projection::XyFlowCommitProjection;
use jellyflow_core::core::Graph;

/// XyFlow-style controlled graph state for adapter-owned node/edge arrays.
#[derive(Debug, Clone)]
pub struct ControlledGraph {
    graph: Graph,
}

impl ControlledGraph {
    pub fn new(graph: Graph) -> Self {
        Self { graph }
    }

    pub fn graph(&self) -> &Graph {
        &self.graph
    }

    pub fn graph_mut(&mut self) -> &mut Graph {
        &mut self.graph
    }

    pub fn into_graph(self) -> Graph {
        self.graph
    }

    pub fn apply_changes(&mut self, changes: &NodeGraphChanges) -> ApplyChangesReport {
        apply_graph_changes(&mut self.graph, changes)
    }

    pub fn apply_patch_changes(&mut self, patch: &NodeGraphPatch) -> ApplyChangesReport {
        let projection = XyFlowCommitProjection::from_patch(patch);
        self.apply_changes(projection.node_edge_changes())
    }

    pub fn apply_node_changes(&mut self, changes: &[NodeChange]) -> ApplyChangesReport {
        apply_node_changes(&mut self.graph, changes)
    }

    pub fn apply_edge_changes(&mut self, changes: &[EdgeChange]) -> ApplyChangesReport {
        apply_edge_changes(&mut self.graph, changes)
    }
}
