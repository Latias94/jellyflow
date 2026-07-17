use jellyflow_core::core::{EdgeKind, Graph, GraphId, PortId};
use jellyflow_core::types::TypeDesc;

use crate::profile::{ApplyPipelineError, GraphProfile};
use crate::rules::{Diagnostic, DiagnosticTarget};
use crate::runtime::graph_runtime::{GraphRuntime, GraphRuntimeError};

use super::fixtures::graph_with_nodes;

struct RejectNodesProfile;

impl GraphProfile for RejectNodesProfile {
    fn type_of_port(&self, _graph: &Graph, _port: PortId) -> Option<TypeDesc> {
        None
    }

    fn validate_graph(&self, graph: &Graph) -> Vec<Diagnostic> {
        if graph.nodes().is_empty() {
            Vec::new()
        } else {
            vec![Diagnostic::error(
                "test.profile.nodes_forbidden",
                DiagnosticTarget::Graph,
                "nodes are forbidden",
            )]
        }
    }

    fn allow_cycles(&self, _edge_kind: EdgeKind) -> bool {
        true
    }
}

#[test]
fn profile_invalid_initial_graph_is_never_published() {
    let (graph, _) = graph_with_nodes(1);

    assert!(matches!(
        GraphRuntime::builder(Default::default())
            .with_profile(Box::new(RejectNodesProfile))
            .build(graph),
        Err(GraphRuntimeError::Apply(
            ApplyPipelineError::Rejected { .. }
        ))
    ));
}

#[test]
fn profile_invalid_document_replacement_is_atomic() {
    let runtime = GraphRuntime::builder(Default::default())
        .with_profile(Box::new(RejectNodesProfile))
        .build(Graph::new(GraphId::from_u128(0x701)))
        .unwrap();
    let before = runtime.snapshot().unwrap();
    let (replacement, _) = graph_with_nodes(1);
    let replacement = runtime.catalog().canonicalize_graph(&replacement).unwrap();

    assert!(matches!(
        runtime.replace_document(replacement),
        Err(GraphRuntimeError::Apply(
            ApplyPipelineError::Rejected { .. }
        ))
    ));
    assert!(runtime.snapshot().unwrap().ptr_eq(&before));
    assert_eq!(runtime.history_state().unwrap().undo_len(), 0);
    assert_eq!(runtime.stats().unwrap().patch_builds, 0);
}
