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
use jellyflow_core::core::{Graph, PortId};

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
                let Some(node) = self.graph.nodes.get_mut(id) else {
                    self.mark_ignored();
                    return;
                };
                node.pos = *position;
                self.mark_applied();
            }
            NodeChange::Kind { id, kind } => {
                let Some(node) = self.graph.nodes.get_mut(id) else {
                    self.mark_ignored();
                    return;
                };
                node.kind = kind.clone();
                self.mark_applied();
            }
            NodeChange::KindVersion { id, kind_version } => {
                let Some(node) = self.graph.nodes.get_mut(id) else {
                    self.mark_ignored();
                    return;
                };
                node.kind_version = *kind_version;
                self.mark_applied();
            }
            NodeChange::Selectable { id, selectable } => {
                let Some(node) = self.graph.nodes.get_mut(id) else {
                    self.mark_ignored();
                    return;
                };
                node.selectable = *selectable;
                self.mark_applied();
            }
            NodeChange::Draggable { id, draggable } => {
                let Some(node) = self.graph.nodes.get_mut(id) else {
                    self.mark_ignored();
                    return;
                };
                node.draggable = *draggable;
                self.mark_applied();
            }
            NodeChange::Connectable { id, connectable } => {
                let Some(node) = self.graph.nodes.get_mut(id) else {
                    self.mark_ignored();
                    return;
                };
                node.connectable = *connectable;
                self.mark_applied();
            }
            NodeChange::Deletable { id, deletable } => {
                let Some(node) = self.graph.nodes.get_mut(id) else {
                    self.mark_ignored();
                    return;
                };
                node.deletable = *deletable;
                self.mark_applied();
            }
            NodeChange::Parent { id, parent } => {
                let Some(node) = self.graph.nodes.get_mut(id) else {
                    self.mark_ignored();
                    return;
                };
                node.parent = *parent;
                self.mark_applied();
            }
            NodeChange::Extent { id, extent } => {
                let Some(node) = self.graph.nodes.get_mut(id) else {
                    self.mark_ignored();
                    return;
                };
                node.extent = *extent;
                self.mark_applied();
            }
            NodeChange::ExpandParent { id, expand_parent } => {
                let Some(node) = self.graph.nodes.get_mut(id) else {
                    self.mark_ignored();
                    return;
                };
                node.expand_parent = *expand_parent;
                self.mark_applied();
            }
            NodeChange::Size { id, size } => {
                let Some(node) = self.graph.nodes.get_mut(id) else {
                    self.mark_ignored();
                    return;
                };
                node.size = *size;
                self.mark_applied();
            }
            NodeChange::Hidden { id, hidden } => {
                let Some(node) = self.graph.nodes.get_mut(id) else {
                    self.mark_ignored();
                    return;
                };
                node.hidden = *hidden;
                self.mark_applied();
            }
            NodeChange::Collapsed { id, collapsed } => {
                let Some(node) = self.graph.nodes.get_mut(id) else {
                    self.mark_ignored();
                    return;
                };
                node.collapsed = *collapsed;
                self.mark_applied();
            }
            NodeChange::Data { id, data } => {
                let Some(node) = self.graph.nodes.get_mut(id) else {
                    self.mark_ignored();
                    return;
                };
                node.data = data.clone();
                self.mark_applied();
            }
            NodeChange::Ports { id, ports } => {
                let Some(node) = self.graph.nodes.get_mut(id) else {
                    self.mark_ignored();
                    return;
                };
                node.ports = ports.clone();
                self.mark_applied();
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
                let Some(edge) = self.graph.edges.get_mut(id) else {
                    self.mark_ignored();
                    return;
                };
                edge.kind = *kind;
                self.mark_applied();
            }
            EdgeChange::Selectable { id, selectable } => {
                let Some(edge) = self.graph.edges.get_mut(id) else {
                    self.mark_ignored();
                    return;
                };
                edge.selectable = *selectable;
                self.mark_applied();
            }
            EdgeChange::Deletable { id, deletable } => {
                let Some(edge) = self.graph.edges.get_mut(id) else {
                    self.mark_ignored();
                    return;
                };
                edge.deletable = *deletable;
                self.mark_applied();
            }
            EdgeChange::Reconnectable { id, reconnectable } => {
                let Some(edge) = self.graph.edges.get_mut(id) else {
                    self.mark_ignored();
                    return;
                };
                edge.reconnectable = *reconnectable;
                self.mark_applied();
            }
            EdgeChange::Endpoints { id, from, to } => {
                let Some(edge) = self.graph.edges.get_mut(id) else {
                    self.mark_ignored();
                    return;
                };
                edge.from = *from;
                edge.to = *to;
                self.mark_applied();
            }
        }
    }

    fn mark_applied(&mut self) {
        self.report.applied += 1;
    }

    fn mark_ignored(&mut self) {
        self.report.ignored += 1;
    }
}
