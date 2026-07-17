use std::cell::{Cell, RefCell};
use std::rc::Rc;

use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize};
use jellyflow_core::interaction::NodeGraphConnectionMode;

use crate::profile::ApplyPipelineError;
use crate::rules::{Diagnostic, DiagnosticTarget};
use crate::runtime::adapter_protocol::{FrameRequest, RendererStamp};
use crate::runtime::commands::GraphCommand;
use crate::runtime::graph_runtime::{GraphRuntime, GraphRuntimeEvent};
use crate::runtime::middleware::{GraphRuntimeMiddleware, GraphRuntimeMiddlewareSnapshot};
use crate::runtime::view_session::{
    ViewCloseReason, ViewCloseRequest, ViewSessionError, ViewSessionEvent,
};

use super::fixtures::graph_with_nodes;

struct TraceMiddleware {
    before_calls: Rc<Cell<usize>>,
    after_calls: Rc<RefCell<Vec<(u64, usize, usize)>>>,
}

impl GraphRuntimeMiddleware for TraceMiddleware {
    fn before_command(
        &mut self,
        snapshot: GraphRuntimeMiddlewareSnapshot,
        command: &GraphCommand,
    ) -> Result<(), ApplyPipelineError> {
        self.before_calls.set(self.before_calls.get() + 1);
        assert_eq!(snapshot.graph().revision(), 0);
        assert_eq!(command.expected_revision(), 0);
        assert_eq!(command.label(), Some("move node"));
        assert_eq!(command.declared_footprint().nodes.len(), 1);
        Ok(())
    }

    fn after_command(
        &mut self,
        snapshot: GraphRuntimeMiddlewareSnapshot,
        patch: &crate::runtime::graph_runtime::RuntimePatch,
    ) {
        self.after_calls.borrow_mut().push((
            snapshot.graph().revision(),
            snapshot.history().undo_len(),
            patch.facts().len(),
        ));
    }
}

struct PanicBeforeMiddleware {
    calls: Rc<Cell<usize>>,
    panic_on_call: usize,
}

impl GraphRuntimeMiddleware for PanicBeforeMiddleware {
    fn before_command(
        &mut self,
        _snapshot: GraphRuntimeMiddlewareSnapshot,
        _command: &GraphCommand,
    ) -> Result<(), ApplyPipelineError> {
        let calls = self.calls.get().saturating_add(1);
        self.calls.set(calls);
        assert_ne!(calls, self.panic_on_call, "before middleware panic");
        Ok(())
    }
}

#[test]
fn panicking_before_middleware_is_restored_and_preserves_uncommitted_state() {
    let (graph, ids) = graph_with_nodes(1);
    let calls = Rc::new(Cell::new(0));
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap()
        .with_middleware(PanicBeforeMiddleware {
            calls: calls.clone(),
            panic_on_call: 1,
        });
    let before = runtime.snapshot().unwrap();
    let history_before = runtime.history_state().unwrap();

    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        runtime.execute(GraphCommand::set_node_data(
            before.revision(),
            ids[0],
            serde_json::json!({ "value": 1 }),
        ))
    }));

    assert!(panic.is_err());
    assert!(before.ptr_eq(&runtime.snapshot().unwrap()));
    assert_eq!(runtime.history_state().unwrap(), history_before);

    runtime
        .execute(GraphCommand::set_node_data(
            before.revision(),
            ids[0],
            serde_json::json!({ "value": 2 }),
        ))
        .unwrap()
        .applied()
        .expect("command after middleware panic applies");

    let after = runtime.snapshot().unwrap();
    assert_eq!(calls.get(), 2);
    assert_eq!(after.revision(), 1);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);
    assert_eq!(
        after.graph().node(&ids[0]).unwrap().data,
        serde_json::json!({ "value": 2 })
    );
}

#[test]
fn close_before_middleware_panic_after_prior_commit_still_terminalizes() {
    let (graph, ids) = graph_with_nodes(1);
    let calls = Rc::new(Cell::new(0));
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap()
        .with_middleware(PanicBeforeMiddleware {
            calls: calls.clone(),
            panic_on_call: 2,
        });
    let closing_view = runtime.attach_view(Default::default()).unwrap();
    let observer = runtime.attach_view(Default::default()).unwrap();
    let observed_revisions = Rc::new(RefCell::new(Vec::new()));
    let callback_revisions = observed_revisions.clone();
    observer
        .subscribe(move |event| {
            if let ViewSessionEvent::RuntimePatched { patch } = event {
                callback_revisions.borrow_mut().push(patch.to_revision());
            }
        })
        .unwrap();
    let terminal_reasons = Rc::new(RefCell::new(Vec::new()));
    let callback_reasons = terminal_reasons.clone();
    closing_view
        .subscribe(move |event| {
            if let ViewSessionEvent::Closed { reason } = event {
                callback_reasons.borrow_mut().push(reason);
            }
        })
        .unwrap();

    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        closing_view.close_with_request(
            ViewCloseRequest::new()
                .with_command(GraphCommand::set_node_data(
                    0,
                    ids[0],
                    serde_json::json!({ "step": 1 }),
                ))
                .with_command(GraphCommand::set_node_data(
                    1,
                    ids[0],
                    serde_json::json!({ "step": 2 }),
                )),
        )
    }));

    assert!(panic.is_err());
    assert_eq!(calls.get(), 2);
    assert_eq!(runtime.snapshot().unwrap().revision(), 1);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);
    assert_eq!(&*observed_revisions.borrow(), &[1]);
    assert_eq!(&*terminal_reasons.borrow(), &[ViewCloseReason::HostClosed]);
    assert_eq!(
        closing_view.close_reason(),
        Some(ViewCloseReason::HostClosed)
    );
    assert!(matches!(
        closing_view.close(),
        Err(ViewSessionError::Closed {
            reason: ViewCloseReason::HostClosed
        })
    ));
    assert_eq!(runtime.live_view_count(), 1);
}

struct PanicAfterMiddleware {
    before_calls: Rc<Cell<usize>>,
    after_calls: Rc<Cell<usize>>,
    panic_on_call: usize,
}

impl GraphRuntimeMiddleware for PanicAfterMiddleware {
    fn before_command(
        &mut self,
        _snapshot: GraphRuntimeMiddlewareSnapshot,
        _command: &GraphCommand,
    ) -> Result<(), ApplyPipelineError> {
        self.before_calls
            .set(self.before_calls.get().saturating_add(1));
        Ok(())
    }

    fn after_command(
        &mut self,
        _snapshot: GraphRuntimeMiddlewareSnapshot,
        _patch: &crate::runtime::graph_runtime::RuntimePatch,
    ) {
        let calls = self.after_calls.get().saturating_add(1);
        self.after_calls.set(calls);
        assert_ne!(calls, self.panic_on_call, "after middleware panic");
    }
}

#[test]
fn panicking_after_middleware_is_restored_and_preserves_committed_state() {
    let (graph, ids) = graph_with_nodes(1);
    let before_calls = Rc::new(Cell::new(0));
    let after_calls = Rc::new(Cell::new(0));
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap()
        .with_middleware(PanicAfterMiddleware {
            before_calls: before_calls.clone(),
            after_calls: after_calls.clone(),
            panic_on_call: 1,
        });
    let view = runtime.attach_view(Default::default()).unwrap();
    view.seed_all_node_facts_for_test();
    let runtime_revisions = Rc::new(RefCell::new(Vec::new()));
    let observed_runtime_revisions = runtime_revisions.clone();
    runtime
        .subscribe(move |event| {
            let GraphRuntimeEvent::Patched { patch } = event else {
                return;
            };
            observed_runtime_revisions
                .borrow_mut()
                .push(patch.to_revision());
        })
        .unwrap();
    let view_revisions = Rc::new(RefCell::new(Vec::new()));
    let observed_view_revisions = view_revisions.clone();
    view.subscribe(move |event| {
        let ViewSessionEvent::RuntimePatched { patch } = event else {
            return;
        };
        observed_view_revisions
            .borrow_mut()
            .push(patch.to_revision());
    })
    .unwrap();
    let binding_revisions = Rc::new(RefCell::new(Vec::new()));
    let observed_binding_revisions = binding_revisions.clone();
    view.subscribe_node(ids[0], move |event| {
        observed_binding_revisions
            .borrow_mut()
            .push(event.patch().to_revision());
    })
    .unwrap();
    let initial_frame = view.frame(frame_request()).unwrap();
    assert_eq!(initial_frame.graph_revision(), 0);
    assert_eq!(initial_frame.nodes()[0].binding_revision(), 0);
    assert!(
        view.geometry_snapshot()
            .unwrap()
            .node(ids[0])
            .unwrap()
            .is_fresh()
    );

    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        runtime.execute(GraphCommand::set_node_data(
            0,
            ids[0],
            serde_json::json!({ "value": 1 }),
        ))
    }));

    assert!(panic.is_err());
    let after_panic = runtime.snapshot().unwrap();
    assert_eq!(after_panic.revision(), 1);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);
    assert_eq!(
        after_panic.graph().node(&ids[0]).unwrap().data,
        serde_json::json!({ "value": 1 })
    );
    assert_eq!(&*runtime_revisions.borrow(), &[1]);
    assert_eq!(&*view_revisions.borrow(), &[1]);
    assert_eq!(&*binding_revisions.borrow(), &[1]);
    assert_eq!(view.stats().unwrap().runtime_patches, 1);
    let after_panic_frame = view.frame(frame_request()).unwrap();
    assert_eq!(after_panic_frame.graph_revision(), 1);
    assert_eq!(after_panic_frame.nodes()[0].binding_revision(), 1);
    assert_eq!(after_panic_frame.nodes()[0].layout_generation(), 1);
    assert!(
        !view
            .geometry_snapshot()
            .unwrap()
            .node(ids[0])
            .unwrap()
            .is_fresh()
    );

    runtime
        .execute(GraphCommand::set_node_data(
            after_panic.revision(),
            ids[0],
            serde_json::json!({ "value": 2 }),
        ))
        .unwrap()
        .applied()
        .expect("command after middleware panic applies");

    let after_recovery = runtime.snapshot().unwrap();
    assert_eq!(before_calls.get(), 2);
    assert_eq!(after_calls.get(), 2);
    assert_eq!(after_recovery.revision(), 2);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 2);
    assert_eq!(
        after_recovery.graph().node(&ids[0]).unwrap().data,
        serde_json::json!({ "value": 2 })
    );
    assert_eq!(&*runtime_revisions.borrow(), &[1, 2]);
    assert_eq!(&*view_revisions.borrow(), &[1, 2]);
    assert_eq!(&*binding_revisions.borrow(), &[1, 2]);
}

#[test]
fn close_after_middleware_panic_fans_out_committed_patch_and_terminalizes_view() {
    let (graph, ids) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap()
        .with_middleware(PanicAfterMiddleware {
            before_calls: Rc::new(Cell::new(0)),
            after_calls: Rc::new(Cell::new(0)),
            panic_on_call: 2,
        });
    let closing_view = runtime.attach_view(Default::default()).unwrap();
    let observer = runtime.attach_view(Default::default()).unwrap();
    observer.seed_all_node_facts_for_test();

    let runtime_revisions = Rc::new(RefCell::new(Vec::new()));
    let observed_runtime_revisions = runtime_revisions.clone();
    runtime
        .subscribe(move |event| {
            let GraphRuntimeEvent::Patched { patch } = event else {
                return;
            };
            observed_runtime_revisions
                .borrow_mut()
                .push(patch.to_revision());
        })
        .unwrap();
    let observer_revisions = Rc::new(RefCell::new(Vec::new()));
    let observed_observer_revisions = observer_revisions.clone();
    observer
        .subscribe(move |event| {
            let ViewSessionEvent::RuntimePatched { patch } = event else {
                return;
            };
            observed_observer_revisions
                .borrow_mut()
                .push(patch.to_revision());
        })
        .unwrap();
    let binding_revisions = Rc::new(RefCell::new(Vec::new()));
    let observed_binding_revisions = binding_revisions.clone();
    observer
        .subscribe_node(ids[0], move |event| {
            observed_binding_revisions
                .borrow_mut()
                .push(event.patch().to_revision());
        })
        .unwrap();
    let terminal_reasons = Rc::new(RefCell::new(Vec::new()));
    let observed_terminal_reasons = terminal_reasons.clone();
    closing_view
        .subscribe(move |event| {
            if let ViewSessionEvent::Closed { reason } = event {
                observed_terminal_reasons.borrow_mut().push(reason);
            }
        })
        .unwrap();

    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        closing_view.close_with_request(
            ViewCloseRequest::new()
                .with_command(GraphCommand::set_node_data(
                    0,
                    ids[0],
                    serde_json::json!({ "step": 1 }),
                ))
                .with_command(GraphCommand::set_node_data(
                    1,
                    ids[0],
                    serde_json::json!({ "step": 2 }),
                )),
        )
    }));

    assert!(panic.is_err());
    let snapshot = runtime.snapshot().unwrap();
    assert_eq!(snapshot.revision(), 2);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 2);
    assert_eq!(
        snapshot.graph().node(&ids[0]).unwrap().data,
        serde_json::json!({ "step": 2 })
    );
    assert_eq!(&*runtime_revisions.borrow(), &[1, 2]);
    assert_eq!(&*observer_revisions.borrow(), &[1, 2]);
    assert_eq!(&*binding_revisions.borrow(), &[1, 2]);
    assert_eq!(observer.stats().unwrap().runtime_patches, 2);
    let frame = observer.frame(frame_request()).unwrap();
    assert_eq!(frame.graph_revision(), 2);
    assert_eq!(frame.nodes()[0].binding_revision(), 2);
    assert_eq!(frame.nodes()[0].layout_generation(), 2);
    assert!(
        !observer
            .geometry_snapshot()
            .unwrap()
            .node(ids[0])
            .unwrap()
            .is_fresh()
    );
    assert_eq!(&*terminal_reasons.borrow(), &[ViewCloseReason::HostClosed]);
    assert_eq!(
        closing_view.close_reason(),
        Some(ViewCloseReason::HostClosed)
    );
    assert!(matches!(
        closing_view.close_with_request(ViewCloseRequest::new()),
        Err(ViewSessionError::Closed {
            reason: ViewCloseReason::HostClosed
        })
    ));
    assert_eq!(runtime.live_view_count(), 1);
}

fn frame_request() -> FrameRequest {
    FrameRequest::new(
        CanvasRect {
            origin: CanvasPoint::default(),
            size: CanvasSize {
                width: 320.0,
                height: 240.0,
            },
        },
        RendererStamp::new("test.seed", 1),
    )
}

#[test]
fn graph_runtime_middleware_rewrites_and_observes_authoritative_commits() {
    let (graph, ids) = graph_with_nodes(1);
    let before_calls = Rc::new(Cell::new(0));
    let after_calls = Rc::new(RefCell::new(Vec::new()));
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap()
        .with_middleware(TraceMiddleware {
            before_calls: before_calls.clone(),
            after_calls: after_calls.clone(),
        });

    let outcome = runtime
        .execute(
            GraphCommand::set_node_position(0, ids[0], CanvasPoint { x: 12.0, y: 24.0 })
                .with_label("move node"),
        )
        .unwrap()
        .applied()
        .expect("accepted command")
        .clone();

    let snapshot = runtime.snapshot().unwrap();
    let node = snapshot.graph().node(&ids[0]).unwrap();
    assert_eq!(node.pos, CanvasPoint { x: 12.0, y: 24.0 });
    assert!(!node.hidden);
    assert_eq!(outcome.patch().facts().len(), 1);
    assert_eq!(before_calls.get(), 1);
    assert_eq!(&*after_calls.borrow(), &[(1, 1, 1)]);
}

struct RejectMiddleware;

impl GraphRuntimeMiddleware for RejectMiddleware {
    fn before_command(
        &mut self,
        _snapshot: GraphRuntimeMiddlewareSnapshot,
        _command: &GraphCommand,
    ) -> Result<(), ApplyPipelineError> {
        Err(ApplyPipelineError::Rejected {
            message: "middleware rejected transaction".to_owned(),
            diagnostics: vec![Diagnostic::error(
                "test.runtime_middleware.reject",
                DiagnosticTarget::Graph,
                "middleware rejected transaction",
            )],
        })
    }
}

#[test]
fn graph_runtime_middleware_rejection_has_zero_side_effects() {
    let (graph, ids) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap()
        .with_middleware(RejectMiddleware);
    let before = runtime.snapshot().unwrap();

    let result = runtime.execute(GraphCommand::set_node_data(
        0,
        ids[0],
        serde_json::json!({ "value": 1 }),
    ));

    assert!(result.unwrap().rejection().is_some());
    assert!(runtime.snapshot().unwrap().ptr_eq(&before));
    assert_eq!(runtime.history_state().unwrap().undo_len(), 0);
    assert_eq!(runtime.stats().unwrap().patch_builds, 0);
}

struct CountBeforeMiddleware(Rc<Cell<usize>>);

impl GraphRuntimeMiddleware for CountBeforeMiddleware {
    fn before_command(
        &mut self,
        _snapshot: GraphRuntimeMiddlewareSnapshot,
        _command: &GraphCommand,
    ) -> Result<(), ApplyPipelineError> {
        self.0.set(self.0.get().saturating_add(1));
        Ok(())
    }
}

#[test]
fn stale_and_rule_rejected_commands_do_not_reach_middleware() {
    let (graph, ids) = graph_with_nodes(1);
    let before_calls = Rc::new(Cell::new(0));
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap()
        .with_middleware(CountBeforeMiddleware(before_calls.clone()));

    let stale = runtime
        .execute(GraphCommand::set_node_data(
            1,
            ids[0],
            serde_json::json!({ "value": 1 }),
        ))
        .unwrap();
    assert!(stale.conflict().is_some());

    let invalid = runtime
        .execute(GraphCommand::connect(
            0,
            jellyflow_core::core::EdgeId::from_u128(500),
            jellyflow_core::core::PortId::from_u128(501),
            jellyflow_core::core::PortId::from_u128(502),
            NodeGraphConnectionMode::Strict,
        ))
        .unwrap();
    assert!(invalid.rejection().is_some());
    assert_eq!(before_calls.get(), 0);
}

struct HistoryMiddleware {
    reject_label: Option<&'static str>,
    before: Rc<RefCell<Vec<(String, u64, usize, usize)>>>,
    after: Rc<RefCell<Vec<(u64, usize, usize)>>>,
}

impl GraphRuntimeMiddleware for HistoryMiddleware {
    fn before_command(
        &mut self,
        snapshot: GraphRuntimeMiddlewareSnapshot,
        command: &GraphCommand,
    ) -> Result<(), ApplyPipelineError> {
        let label = command.label().unwrap_or("unlabeled").to_owned();
        self.before.borrow_mut().push((
            label.clone(),
            snapshot.graph().revision(),
            snapshot.history().undo_len(),
            snapshot.history().redo_len(),
        ));
        if self.reject_label == Some(label.as_str()) {
            return Err(ApplyPipelineError::Rejected {
                message: format!("middleware rejected {label}"),
                diagnostics: vec![Diagnostic::error(
                    "test.runtime_middleware.reject_history",
                    DiagnosticTarget::Graph,
                    format!("middleware rejected {label}"),
                )],
            });
        }
        Ok(())
    }

    fn after_command(
        &mut self,
        snapshot: GraphRuntimeMiddlewareSnapshot,
        _patch: &crate::runtime::graph_runtime::RuntimePatch,
    ) {
        self.after.borrow_mut().push((
            snapshot.graph().revision(),
            snapshot.history().undo_len(),
            snapshot.history().redo_len(),
        ));
    }
}

#[test]
fn graph_runtime_middleware_observes_undo_and_redo_commands() {
    let (graph, ids) = graph_with_nodes(1);
    let before = Rc::new(RefCell::new(Vec::new()));
    let after = Rc::new(RefCell::new(Vec::new()));
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap()
        .with_middleware(HistoryMiddleware {
            reject_label: None,
            before: before.clone(),
            after: after.clone(),
        });

    runtime
        .execute(
            GraphCommand::set_node_data(0, ids[0], serde_json::json!({ "value": 1 }))
                .with_label("seed"),
        )
        .unwrap()
        .applied()
        .expect("seed applies");
    runtime
        .execute(GraphCommand::undo(1))
        .unwrap()
        .applied()
        .expect("undo applies");
    runtime
        .execute(GraphCommand::redo(2))
        .unwrap()
        .applied()
        .expect("redo applies");

    assert_eq!(
        &*before.borrow(),
        &[
            ("seed".to_owned(), 0, 0, 0),
            ("undo".to_owned(), 1, 1, 0),
            ("redo".to_owned(), 2, 0, 1),
        ]
    );
    assert_eq!(&*after.borrow(), &[(1, 1, 0), (2, 0, 1), (3, 1, 0)]);
}

#[test]
fn graph_runtime_middleware_can_reject_undo_and_redo_without_side_effects() {
    assert_history_middleware_rejection("undo");
    assert_history_middleware_rejection("redo");
}

fn assert_history_middleware_rejection(rejected_label: &'static str) {
    let (graph, ids) = graph_with_nodes(1);
    let before_calls = Rc::new(RefCell::new(Vec::new()));
    let after_calls = Rc::new(RefCell::new(Vec::new()));
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap()
        .with_middleware(HistoryMiddleware {
            reject_label: Some(rejected_label),
            before: before_calls,
            after: after_calls,
        });
    runtime
        .execute(GraphCommand::set_node_data(
            0,
            ids[0],
            serde_json::json!({ "value": 1 }),
        ))
        .unwrap()
        .applied()
        .expect("seed applies");
    if rejected_label == "redo" {
        runtime
            .execute(GraphCommand::undo(1))
            .unwrap()
            .applied()
            .expect("undo prepares redo history");
    }
    let before = runtime.snapshot().unwrap();
    let history_before = runtime.history_state().unwrap();
    let stats_before = runtime.stats().unwrap();
    let event_count = Rc::new(Cell::new(0));
    let callback_count = event_count.clone();
    runtime
        .subscribe(move |_| callback_count.set(callback_count.get() + 1))
        .unwrap();

    let revision = before.revision();
    let command = match rejected_label {
        "undo" => GraphCommand::undo(revision),
        "redo" => GraphCommand::redo(revision),
        _ => unreachable!("test only covers history commands"),
    };
    let outcome = runtime.execute(command).unwrap();
    let rejection = outcome
        .rejection()
        .expect("middleware rejects history command");

    assert_eq!(
        rejection.diagnostics()[0].key,
        "test.runtime_middleware.reject_history"
    );
    assert!(before.ptr_eq(&runtime.snapshot().unwrap()));
    assert_eq!(runtime.history_state().unwrap(), history_before);
    assert_eq!(runtime.stats().unwrap(), stats_before);
    assert_eq!(event_count.get(), 0);
}
