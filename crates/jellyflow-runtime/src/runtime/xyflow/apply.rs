//! Apply helpers for XyFlow-style changes (controlled mode).
//!
//! In ReactFlow, consumers that keep their own node/edge state receive `NodeChange`/`EdgeChange`
//! and apply them via helpers (`applyNodeChanges`, `applyEdgeChanges`).
//!
//! In Jellyflow, the authoritative representation is a `Graph` (hash maps) and undo/redo is
//! powered by reversible `GraphTransaction`. This module provides best-effort, order-preserving
//! application of change events to a `Graph` for controlled integrations.
//!
//! Use `NodeGraphStore::dispatch_changes` when an integration wants canonical transaction
//! validation, profile middleware, undo/redo, and atomic failure semantics. These helpers
//! intentionally keep the XyFlow-style "apply what exists, ignore what does not" contract.

mod edges;
mod nodes;

use crate::runtime::xyflow::changes::{EdgeChange, NodeChange, NodeGraphChanges};
use jellyflow_core::core::{Edge, EdgeId, Graph, Node, NodeId};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ApplyChangesReport {
    pub applied: usize,
    pub ignored: usize,
}

impl ApplyChangesReport {
    pub fn did_change(&self) -> bool {
        self.applied > 0
    }
}

pub fn apply_graph_changes(graph: &mut Graph, changes: &NodeGraphChanges) -> ApplyChangesReport {
    ApplyChangesPlanner::new(graph).apply_graph_changes(changes)
}

pub fn apply_node_changes(graph: &mut Graph, changes: &[NodeChange]) -> ApplyChangesReport {
    ApplyChangesPlanner::new(graph).apply_node_changes(changes)
}

pub fn apply_edge_changes(graph: &mut Graph, changes: &[EdgeChange]) -> ApplyChangesReport {
    ApplyChangesPlanner::new(graph).apply_edge_changes(changes)
}

struct ApplyChangesPlanner<'a> {
    graph: &'a mut Graph,
    report: ApplyChangesReport,
}

impl<'a> ApplyChangesPlanner<'a> {
    fn new(graph: &'a mut Graph) -> Self {
        Self {
            graph,
            report: ApplyChangesReport::default(),
        }
    }

    fn apply_graph_changes(mut self, changes: &NodeGraphChanges) -> ApplyChangesReport {
        self.apply_nodes(&changes.nodes);
        self.apply_edges(&changes.edges);
        self.finish()
    }

    fn apply_node_changes(mut self, changes: &[NodeChange]) -> ApplyChangesReport {
        self.apply_nodes(changes);
        self.finish()
    }

    fn apply_edge_changes(mut self, changes: &[EdgeChange]) -> ApplyChangesReport {
        self.apply_edges(changes);
        self.finish()
    }

    fn finish(self) -> ApplyChangesReport {
        self.report
    }

    fn mutate_existing_node(&mut self, id: NodeId, f: impl FnOnce(&mut Node)) {
        let Some(node) = self.graph.nodes.get_mut(&id) else {
            self.mark_ignored();
            return;
        };
        f(node);
        self.mark_applied();
    }

    fn mutate_existing_edge(&mut self, id: EdgeId, f: impl FnOnce(&mut Edge)) {
        let Some(edge) = self.graph.edges.get_mut(&id) else {
            self.mark_ignored();
            return;
        };
        f(edge);
        self.mark_applied();
    }

    fn mark_applied(&mut self) {
        self.report.applied += 1;
    }

    fn mark_ignored(&mut self) {
        self.report.ignored += 1;
    }
}
