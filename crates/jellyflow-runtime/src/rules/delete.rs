use std::collections::BTreeSet;

use crate::io::NodeGraphInteractionState;
use crate::runtime::policy::{resolve_edge_interaction_policy, resolve_node_interaction_policy};
use jellyflow_core::core::{EdgeId, Graph, NodeId};
use jellyflow_core::ops::{GraphMutationPlanner, GraphOp, GraphTransaction};

use super::{DeleteDecision, DeletePlan, Diagnostic, DiagnosticSeverity, DiagnosticTarget};

/// Plans deleting a node with default interaction policy.
pub fn plan_delete_node(graph: &Graph, node: NodeId) -> DeletePlan {
    plan_delete_node_with_policy(graph, node, &NodeGraphInteractionState::default())
}

/// Plans deleting a node with explicit interaction policy.
pub fn plan_delete_node_with_policy(
    graph: &Graph,
    node: NodeId,
    state: &NodeGraphInteractionState,
) -> DeletePlan {
    plan_delete_elements_with_policy(graph, [node], std::iter::empty::<EdgeId>(), state)
}

/// Plans deleting an edge with default interaction policy.
pub fn plan_delete_edge(graph: &Graph, edge: EdgeId) -> DeletePlan {
    plan_delete_edge_with_policy(graph, edge, &NodeGraphInteractionState::default())
}

/// Plans deleting an edge with explicit interaction policy.
pub fn plan_delete_edge_with_policy(
    graph: &Graph,
    edge: EdgeId,
    state: &NodeGraphInteractionState,
) -> DeletePlan {
    plan_delete_elements_with_policy(graph, std::iter::empty::<NodeId>(), [edge], state)
}

/// Plans deleting nodes and edges with default interaction policy.
pub fn plan_delete_elements(
    graph: &Graph,
    nodes: impl IntoIterator<Item = NodeId>,
    edges: impl IntoIterator<Item = EdgeId>,
) -> DeletePlan {
    plan_delete_elements_with_policy(graph, nodes, edges, &NodeGraphInteractionState::default())
}

/// Plans deleting nodes and edges with explicit interaction policy.
pub fn plan_delete_elements_with_policy(
    graph: &Graph,
    nodes: impl IntoIterator<Item = NodeId>,
    edges: impl IntoIterator<Item = EdgeId>,
    state: &NodeGraphInteractionState,
) -> DeletePlan {
    DeletePlanner { graph, state }.plan(nodes, edges)
}

struct DeletePlanner<'a> {
    graph: &'a Graph,
    state: &'a NodeGraphInteractionState,
}

impl<'a> DeletePlanner<'a> {
    fn plan(
        self,
        nodes: impl IntoIterator<Item = NodeId>,
        edges: impl IntoIterator<Item = EdgeId>,
    ) -> DeletePlan {
        let nodes = nodes.into_iter().collect::<BTreeSet<_>>();
        let edges = edges.into_iter().collect::<BTreeSet<_>>();

        if nodes.is_empty() && edges.is_empty() {
            return DeletePlan::accept();
        }

        let cascaded_edges = self.cascaded_edges_for_nodes(&nodes);
        let mut diagnostics = Vec::new();
        self.validate_nodes(&nodes, &mut diagnostics);
        self.validate_direct_edges(&edges, &cascaded_edges, &mut diagnostics);
        if !diagnostics.is_empty() {
            return rejected(diagnostics);
        }

        match self.build_ops(&nodes, &edges, &cascaded_edges) {
            Ok(ops) => DeletePlan::from_ops(ops),
            Err(diagnostic) => rejected(vec![diagnostic]),
        }
    }

    fn validate_nodes(&self, nodes: &BTreeSet<NodeId>, diagnostics: &mut Vec<Diagnostic>) {
        for node_id in nodes {
            let Some(node) = self.graph.nodes.get(node_id) else {
                diagnostics.push(delete_diagnostic(
                    "delete.missing_node",
                    DiagnosticTarget::Node { id: *node_id },
                    format!("missing node: {node_id:?}"),
                ));
                continue;
            };

            if !resolve_node_interaction_policy(node, self.state).deletable {
                diagnostics.push(delete_diagnostic(
                    "delete.node_not_deletable",
                    DiagnosticTarget::Node { id: *node_id },
                    format!("node is not deletable: {node_id:?}"),
                ));
            }
        }
    }

    fn validate_direct_edges(
        &self,
        edges: &BTreeSet<EdgeId>,
        cascaded_edges: &BTreeSet<EdgeId>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        for edge_id in edges {
            if cascaded_edges.contains(edge_id) {
                continue;
            }

            let Some(edge) = self.graph.edges.get(edge_id) else {
                diagnostics.push(delete_diagnostic(
                    "delete.missing_edge",
                    DiagnosticTarget::Edge { id: *edge_id },
                    format!("missing edge: {edge_id:?}"),
                ));
                continue;
            };

            if !resolve_edge_interaction_policy(edge, self.state).deletable {
                diagnostics.push(delete_diagnostic(
                    "delete.edge_not_deletable",
                    DiagnosticTarget::Edge { id: *edge_id },
                    format!("edge is not deletable: {edge_id:?}"),
                ));
            }
        }
    }

    fn cascaded_edges_for_nodes(&self, nodes: &BTreeSet<NodeId>) -> BTreeSet<EdgeId> {
        let port_ids = self
            .graph
            .ports
            .iter()
            .filter_map(|(port_id, port)| nodes.contains(&port.node).then_some(*port_id))
            .collect::<BTreeSet<_>>();

        self.graph
            .edges
            .iter()
            .filter_map(|(edge_id, edge)| {
                (port_ids.contains(&edge.from) || port_ids.contains(&edge.to)).then_some(*edge_id)
            })
            .collect()
    }

    fn build_ops(
        &self,
        nodes: &BTreeSet<NodeId>,
        edges: &BTreeSet<EdgeId>,
        cascaded_edges: &BTreeSet<EdgeId>,
    ) -> Result<Vec<GraphOp>, Diagnostic> {
        let mut scratch = self.graph.clone();
        let mut ops = Vec::new();

        for node_id in nodes {
            let op = GraphMutationPlanner::new(&scratch)
                .remove_node_op(*node_id)
                .map_err(|error| {
                    planning_diagnostic(format!("failed to plan node deletion: {error}"))
                })?;
            apply_planned_op(&mut scratch, &op)?;
            ops.push(op);
        }

        for edge_id in edges {
            if cascaded_edges.contains(edge_id) || !scratch.edges.contains_key(edge_id) {
                continue;
            }

            let op = GraphMutationPlanner::new(&scratch)
                .remove_edge_op(*edge_id)
                .map_err(|error| {
                    planning_diagnostic(format!("failed to plan edge deletion: {error}"))
                })?;
            apply_planned_op(&mut scratch, &op)?;
            ops.push(op);
        }

        Ok(ops)
    }
}

fn apply_planned_op(graph: &mut Graph, op: &GraphOp) -> Result<(), Diagnostic> {
    let tx = GraphTransaction {
        label: None,
        ops: vec![op.clone()],
    };
    tx.apply_to(graph)
        .map_err(|error| planning_diagnostic(format!("failed to apply planned deletion: {error}")))
}

fn rejected(diagnostics: Vec<Diagnostic>) -> DeletePlan {
    DeletePlan {
        decision: DeleteDecision::Reject,
        diagnostics,
        ops: Vec::new(),
    }
}

fn delete_diagnostic(
    key: impl Into<String>,
    target: DiagnosticTarget,
    message: impl Into<String>,
) -> Diagnostic {
    Diagnostic {
        key: key.into(),
        severity: DiagnosticSeverity::Error,
        target,
        message: message.into(),
        fixes: Vec::new(),
    }
}

fn planning_diagnostic(message: impl Into<String>) -> Diagnostic {
    delete_diagnostic("delete.planning_failed", DiagnosticTarget::Graph, message)
}
