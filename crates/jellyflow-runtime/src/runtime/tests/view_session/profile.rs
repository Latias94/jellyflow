use std::cell::{Cell, RefCell};
use std::rc::Rc;

use jellyflow_core::core::{EdgeKind, Graph, GraphId, PortId};
use jellyflow_core::types::TypeDesc;

use crate::profile::{ApplyPipelineError, GraphProfile};
use crate::rules::{Diagnostic, DiagnosticTarget};
use crate::runtime::commands::GraphCommand;
use crate::runtime::graph_runtime::{GraphRuntime, GraphRuntimeError};
use crate::runtime::view_session::ViewCloseReason;

use super::fixtures::graph_with_nodes;

struct RejectNodesProfile;

struct DropRuntimeDuringConcretizeBound {
    runtime: Rc<RefCell<Option<GraphRuntime>>>,
    concretize_calls: Rc<Cell<usize>>,
    validation_calls: Rc<Cell<usize>>,
}

impl GraphProfile for DropRuntimeDuringConcretizeBound {
    fn type_of_port(&self, graph: &Graph, port: PortId) -> Option<TypeDesc> {
        graph.port(&port).and_then(|port| port.ty.clone())
    }

    fn validate_graph(&self, _graph: &Graph) -> Vec<Diagnostic> {
        self.validation_calls
            .set(self.validation_calls.get().saturating_add(1));
        Vec::new()
    }

    fn concretize_bound(&self) -> usize {
        drop(self.runtime.borrow_mut().take());
        8
    }

    fn concretize(&self, _graph: &Graph) -> crate::runtime::commands::TopologyEdit {
        self.concretize_calls
            .set(self.concretize_calls.get().saturating_add(1));
        crate::runtime::commands::TopologyEdit::new()
    }
}

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
    assert_eq!(event_count.get(), 0);
}

#[test]
fn runtime_drop_in_profile_hook_stops_later_planning_hooks() {
    let (graph, ids) = graph_with_nodes(1);
    let runtime_slot = Rc::new(RefCell::new(None));
    let concretize_calls = Rc::new(Cell::new(0));
    let validation_calls = Rc::new(Cell::new(0));
    let runtime = GraphRuntime::builder(Default::default())
        .with_profile(Box::new(DropRuntimeDuringConcretizeBound {
            runtime: runtime_slot.clone(),
            concretize_calls: concretize_calls.clone(),
            validation_calls: validation_calls.clone(),
        }))
        .build(graph)
        .unwrap();
    let view = runtime.attach_view(Default::default()).unwrap();
    let before = runtime.snapshot().unwrap();
    concretize_calls.set(0);
    validation_calls.set(0);
    *runtime_slot.borrow_mut() = Some(runtime);

    let completion = view
        .execute(GraphCommand::set_node_data(
            before.version(),
            ids[0],
            serde_json::json!({ "must_not_apply": true }),
        ))
        .unwrap();

    assert!(matches!(
        completion.result(),
        Err(GraphRuntimeError::RuntimeUnavailable)
    ));
    assert_eq!(concretize_calls.get(), 0);
    assert_eq!(validation_calls.get(), 0);
    assert_eq!(view.close_reason(), Some(ViewCloseReason::RuntimeDropped));
}
