use std::cell::Cell;
use std::rc::Rc;

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
    let history_before = runtime.history_state().unwrap();
    let installed_impact_before = runtime.installed_impact_state_for_test();
    let event_count = Rc::new(Cell::new(0));
    let callback_count = event_count.clone();
    runtime
        .subscribe(move |_| callback_count.set(callback_count.get() + 1))
        .unwrap();
    let (replacement, _) = graph_with_nodes(1);
    let replacement = runtime.catalog().canonicalize_graph(&replacement).unwrap();

    assert!(matches!(
        runtime.replace_document(replacement),
        Err(GraphRuntimeError::Apply(
            ApplyPipelineError::Rejected { .. }
        ))
    ));
    assert!(runtime.snapshot().unwrap().ptr_eq(&before));
    assert_eq!(runtime.history_state().unwrap(), history_before);
    assert_eq!(
        runtime.installed_impact_state_for_test(),
        installed_impact_before,
        "rejected document replacement must not update indexes or node identities"
    );
    assert_eq!(event_count.get(), 0);
}
