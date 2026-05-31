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

use std::collections::HashSet;

use crate::runtime::xyflow::changes::{EdgeChange, NodeChange, NodeGraphChanges};
use jellyflow_core::core::{Edge, EdgeId, Graph, Node, NodeId, PortId};

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

    fn apply_nodes(&mut self, changes: &[NodeChange]) {
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

    fn apply_edges(&mut self, changes: &[EdgeChange]) {
        for change in changes {
            self.apply_edge_change(change);
        }
    }

    fn apply_edge_change(&mut self, change: &EdgeChange) {
        match change {
            EdgeChange::Add { id, edge } => {
                self.graph.edges.insert(*id, edge.clone());
                self.mark_applied();
            }
            EdgeChange::Remove { id } => {
                if self.graph.edges.remove(id).is_some() {
                    self.mark_applied();
                } else {
                    self.mark_ignored();
                }
            }
            EdgeChange::Kind { id, kind } => {
                self.mutate_existing_edge(*id, |edge| edge.kind = *kind);
            }
            EdgeChange::Selectable { id, selectable } => {
                self.mutate_existing_edge(*id, |edge| edge.selectable = *selectable);
            }
            EdgeChange::Deletable { id, deletable } => {
                self.mutate_existing_edge(*id, |edge| edge.deletable = *deletable);
            }
            EdgeChange::Reconnectable { id, reconnectable } => {
                self.mutate_existing_edge(*id, |edge| edge.reconnectable = *reconnectable);
            }
            EdgeChange::Endpoints { id, from, to } => {
                self.mutate_existing_edge(*id, |edge| {
                    edge.from = *from;
                    edge.to = *to;
                });
            }
        }
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
