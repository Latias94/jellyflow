use std::collections::{BTreeMap, BTreeSet};

use crate::rules::{
    Diagnostic, DiagnosticSeverity, DiagnosticTarget, edge_type_compatibility_diagnostic,
};
use jellyflow_core::core::{EdgeKind, Graph, NodeId};

use super::super::GraphProfile;
use super::error::ApplyPipelineError;

pub(crate) fn validate_profile_graph(
    profile: &dyn GraphProfile,
    graph: &Graph,
) -> Result<(), ApplyPipelineError> {
    reject_error_diagnostics(profile.validate_graph(graph), "transaction rejected")
}

pub(crate) fn validate_profile_document(
    profile: &dyn GraphProfile,
    graph: &Graph,
) -> Result<(), ApplyPipelineError> {
    validate_profile_graph(profile, graph)?;

    let mut diagnostics = Vec::new();
    let compatibility = profile.type_compatibility();
    for edge in graph.edges().keys().copied() {
        if let Some(diagnostic) = edge_type_compatibility_diagnostic(
            graph,
            edge,
            |graph, port| profile.type_of_port(graph, port),
            compatibility,
        ) {
            diagnostics.push(diagnostic);
        }
    }
    for kind in [EdgeKind::Data, EdgeKind::Exec] {
        if !profile.allow_cycles(kind) && contains_cycle(graph, kind) {
            diagnostics.push(Diagnostic::error(
                "connect.cycle",
                DiagnosticTarget::Graph,
                format!("{kind:?} edges contain a cycle disallowed by the graph profile"),
            ));
        }
    }

    reject_error_diagnostics(diagnostics, "graph profile rejected document")
}

fn contains_cycle(graph: &Graph, kind: EdgeKind) -> bool {
    let mut indegree = BTreeMap::<NodeId, usize>::new();
    let mut successors = BTreeMap::<NodeId, Vec<NodeId>>::new();
    for edge in graph.edges().values().filter(|edge| edge.kind == kind) {
        let Some(from) = graph.port(&edge.from).map(|port| port.node) else {
            continue;
        };
        let Some(to) = graph.port(&edge.to).map(|port| port.node) else {
            continue;
        };
        indegree.entry(from).or_default();
        *indegree.entry(to).or_default() += 1;
        successors.entry(from).or_default().push(to);
    }

    let mut ready = indegree
        .iter()
        .filter_map(|(node, degree)| (*degree == 0).then_some(*node))
        .collect::<BTreeSet<_>>();
    let mut visited = 0;
    while let Some(node) = ready.pop_first() {
        visited += 1;
        for successor in successors.get(&node).into_iter().flatten() {
            let Some(degree) = indegree.get_mut(successor) else {
                continue;
            };
            *degree -= 1;
            if *degree == 0 {
                ready.insert(*successor);
            }
        }
    }
    visited != indegree.len()
}

fn reject_error_diagnostics(
    diagnostics: Vec<Diagnostic>,
    fallback_message: &str,
) -> Result<(), ApplyPipelineError> {
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
    {
        return Err(rejected_diagnostics(diagnostics, fallback_message));
    }
    Ok(())
}

fn rejected_diagnostics(
    diagnostics: Vec<Diagnostic>,
    fallback_message: &str,
) -> ApplyPipelineError {
    let message = diagnostics
        .first()
        .map(|d| d.message.clone())
        .unwrap_or_else(|| fallback_message.to_string());
    ApplyPipelineError::Rejected {
        message,
        diagnostics,
    }
}
