use std::cell::{Cell, RefCell};
use std::rc::Rc;

use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize, NodeId};

use crate::profile::ApplyPipelineError;
use crate::runtime::adapter_protocol::{
    AdapterFrame, FeedbackCollectorError, FeedbackSubmissionError, FrameRequest, LayoutNodeStatus,
    NodeLayout, RendererStamp, ViewPoint, ViewRect, ViewSize,
};
use crate::runtime::commands::{CommandCompletionCorrelationError, GraphCommand};
use crate::runtime::graph_runtime::{
    GraphRuntime, GraphRuntimeError, GraphRuntimeEvent, RuntimePatch,
};
use crate::runtime::middleware::{GraphRuntimeMiddleware, GraphRuntimeMiddlewareSnapshot};
use crate::runtime::view_session::{
    ViewCloseReason, ViewSession, ViewSessionError, ViewSessionEvent, ViewSessionOptions,
};

use super::fixtures::{graph_with_nodes, publish_graph_layouts};

fn one_node_frame(session: &ViewSession) -> AdapterFrame {
    session
        .frame(FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint {
                    x: -100.0,
                    y: -100.0,
                },
                size: CanvasSize {
                    width: 600.0,
                    height: 400.0,
                },
            },
            RendererStamp::new("test.node", 1),
        ))
        .unwrap()
}

fn layout_for(frame: &AdapterFrame) -> NodeLayout {
    frame.nodes()[0]
        .begin_layout(ViewRect::new(
            ViewPoint::default(),
            ViewSize::new(200.0, 120.0),
        ))
        .finish()
        .unwrap()
}

fn layout_for_node(frame: &AdapterFrame, node: NodeId) -> NodeLayout {
    frame
        .nodes()
        .iter()
        .find(|frame_node| frame_node.id() == node)
        .expect("requested node is present in the frame")
        .begin_layout(ViewRect::new(
            ViewPoint::default(),
            ViewSize::new(200.0, 120.0),
        ))
        .finish()
        .unwrap()
}

struct PanicAfterFeedbackCommand;

impl GraphRuntimeMiddleware for PanicAfterFeedbackCommand {
    fn after_command(&mut self, _snapshot: GraphRuntimeMiddlewareSnapshot, _patch: &RuntimePatch) {
        panic!("feedback body middleware panic");
    }
}

struct CloseViewBeforeFeedbackCommand {
    view: Rc<RefCell<Option<Rc<ViewSession>>>>,
}

impl GraphRuntimeMiddleware for CloseViewBeforeFeedbackCommand {
    fn before_command(
        &mut self,
        _snapshot: GraphRuntimeMiddlewareSnapshot,
        _command: &GraphCommand,
    ) -> Result<(), ApplyPipelineError> {
        self.view
            .borrow()
            .as_ref()
            .expect("feedback view is installed")
            .close()
            .expect("middleware closes the feedback view");
        Ok(())
    }
}

#[test]
fn duplicate_command_identity_rejects_before_consuming_the_frame_or_mutating_state() {
    let (graph, nodes) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let frame = one_node_frame(&session);
    let collector = frame.collector();
    let command = GraphCommand::set_node_data(
        frame.graph_version(),
        nodes[0],
        serde_json::json!({ "value": "applied-on-retry" }),
    );
    let command_identity = command.identity().clone();
    collector.command(command.clone()).unwrap();
    collector.command(command).unwrap();
    let invalid_feedback = collector.finish().unwrap();
    let mut retry_feedback = invalid_feedback.duplicate_for_test();
    retry_feedback
        .operations
        .pop()
        .expect("retry removes exactly the duplicate command");

    let before_snapshot = runtime.snapshot().unwrap();
    let before_history = runtime.history_state().unwrap();

    let error = session
        .submit_feedback(invalid_feedback)
        .expect_err("duplicate command identities must fail before admission");
    let FeedbackSubmissionError::DuplicateCommandIdentity { identity } = error else {
        panic!("expected DuplicateCommandIdentity, got {error:?}");
    };

    assert_eq!(identity, command_identity);
    assert!(runtime.snapshot().unwrap().ptr_eq(&before_snapshot));
    assert_eq!(runtime.history_state().unwrap(), before_history);

    let receipt = session
        .submit_feedback(retry_feedback)
        .expect("duplicate rejection must leave the same frame available for retry");
    assert!(matches!(
        receipt.command_completion(&command_identity).unwrap().result(),
        Ok(outcome) if outcome.applied().is_some()
    ));
}

#[test]
fn interleaved_feedback_correlates_an_execution_error_without_shifting_its_neighbor() {
    let (graph, nodes) = graph_with_nodes(2);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let frame = one_node_frame(&session);
    let collector = frame.collector();

    let (foreign_graph, _) = graph_with_nodes(1);
    let foreign_runtime = GraphRuntime::builder(Default::default())
        .build(foreign_graph)
        .unwrap();
    let failed = GraphCommand::set_node_data(
        foreign_runtime.snapshot().unwrap().version(),
        nodes[1],
        serde_json::json!({ "must_not_apply": true }),
    );
    let failed_identity = failed.identity().clone();
    let applied = GraphCommand::set_node_data(
        frame.graph_version(),
        nodes[0],
        serde_json::json!({ "applied": true }),
    );
    let applied_identity = applied.identity().clone();

    collector.layout(layout_for_node(&frame, nodes[0])).unwrap();
    collector.command(failed).unwrap();
    collector.layout(layout_for_node(&frame, nodes[1])).unwrap();
    collector.command(applied).unwrap();

    let receipt = session
        .submit_feedback(collector.finish().unwrap())
        .unwrap();

    let failed_completion = receipt.command_completion(&failed_identity).unwrap();
    assert_eq!(failed_completion.identity(), &failed_identity);
    assert!(matches!(
        failed_completion.result(),
        Err(GraphRuntimeError::ForeignCommandRuntime { .. })
    ));
    let applied_completion = receipt.command_completion(&applied_identity).unwrap();
    assert_eq!(applied_completion.identity(), &applied_identity);
    assert!(matches!(
        applied_completion.result(),
        Ok(outcome) if outcome.applied().is_some()
    ));
    assert_eq!(receipt.command_completions().count(), 2);
    assert_eq!(
        receipt
            .layout_receipts()
            .map(|layout| layout.node())
            .collect::<Vec<_>>(),
        nodes
    );
}

#[test]
fn feedback_receipt_batch_consumption_is_keyed_ordered_and_atomic() {
    let (graph, nodes) = graph_with_nodes(2);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let frame = one_node_frame(&session);
    let collector = frame.collector();

    let first = GraphCommand::set_node_data(
        frame.graph_version(),
        nodes[0],
        serde_json::json!({ "step": 1 }),
    );
    let first_identity = first.identity().clone();
    let second = GraphCommand::set_node_data(
        frame.graph_version(),
        nodes[1],
        serde_json::json!({ "step": 2 }),
    );
    let second_identity = second.identity().clone();
    let missing_identity = GraphCommand::set_node_data(
        frame.graph_version(),
        nodes[1],
        serde_json::json!({ "not_submitted": true }),
    )
    .identity()
    .clone();
    collector.command(first).unwrap();
    collector.layout(layout_for_node(&frame, nodes[0])).unwrap();
    collector.command(second).unwrap();

    let mut receipt = session
        .submit_feedback(collector.finish().unwrap())
        .unwrap();
    assert_eq!(receipt.layout_receipts().count(), 1);

    let error = receipt
        .take_exact_command_completions(std::slice::from_ref(&first_identity))
        .expect_err("an exact subset must reject its foreign completion");
    assert!(matches!(
        error,
        CommandCompletionCorrelationError::ForeignIdentity { identity }
            if identity == second_identity
    ));
    assert_eq!(receipt.command_completions().count(), 2);

    let error = receipt
        .take_command_completions(&[first_identity.clone(), first_identity.clone()])
        .expect_err("duplicate expected identities must reject atomically");
    assert!(matches!(
        error,
        CommandCompletionCorrelationError::DuplicateIdentity { identity }
            if identity == first_identity
    ));
    assert_eq!(receipt.command_completions().count(), 2);

    let completions = receipt
        .take_command_completions(&[second_identity.clone(), first_identity.clone()])
        .unwrap();
    assert_eq!(completions[0].identity(), &second_identity);
    assert_eq!(completions[1].identity(), &first_identity);
    assert!(receipt.command_completions().next().is_none());

    assert!(matches!(
        receipt.take_command_completion(&first_identity),
        Err(CommandCompletionCorrelationError::AlreadyConsumed { identity })
            if identity == first_identity
    ));
    assert!(matches!(
        receipt.take_command_completion(&missing_identity),
        Err(CommandCompletionCorrelationError::MissingIdentity { identity })
            if identity == missing_identity
    ));
}

#[test]
fn layout_error_after_command_admission_preserves_the_keyed_completion() {
    let (graph, nodes) = graph_with_nodes(1);
    let view_slot = Rc::new(RefCell::new(None));
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap()
        .with_middleware(CloseViewBeforeFeedbackCommand {
            view: view_slot.clone(),
        });
    let session = Rc::new(runtime.attach_view(ViewSessionOptions::default()).unwrap());
    *view_slot.borrow_mut() = Some(session.clone());
    let frame = one_node_frame(&session);
    let collector = frame.collector();
    let command = GraphCommand::set_node_data(
        frame.graph_version(),
        nodes[0],
        serde_json::json!({ "value": "admitted-before-layout-error" }),
    );
    let command_identity = command.identity().clone();
    collector.command(command).unwrap();
    collector.layout(layout_for(&frame)).unwrap();

    let error = session
        .submit_feedback(collector.finish().unwrap())
        .expect_err("closing the view during command execution must fail layout publication");
    let FeedbackSubmissionError::AfterCommandAdmission { source, receipt } = error else {
        panic!("expected AfterCommandAdmission, got {error:?}");
    };
    assert!(matches!(
        source,
        ViewSessionError::Closed {
            reason: ViewCloseReason::HostClosed
        }
    ));
    let receipt = receipt.expect("post-admission failure must preserve admitted completions");
    assert!(matches!(
        receipt.command_completion(&command_identity).unwrap().result(),
        Ok(outcome) if outcome.applied().is_some()
    ));
    assert_eq!(receipt.command_completions().count(), 1);
    assert_eq!(receipt.layout_receipts().count(), 1);
    assert_eq!(receipt.layout_receipts().next().unwrap().status(), None);
    assert_eq!(runtime.snapshot().unwrap().revision(), 1);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 1);
    assert_eq!(session.close_reason(), Some(ViewCloseReason::HostClosed));
}

#[test]
fn feedback_body_panic_wins_after_both_subscription_layers_finish() {
    let (graph, nodes) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap()
        .with_middleware(PanicAfterFeedbackCommand);
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    publish_graph_layouts(&session);

    runtime
        .subscribe(|event| {
            if matches!(event, GraphRuntimeEvent::Patched { .. }) {
                panic!("runtime subscriber panic");
            }
        })
        .unwrap();
    let runtime_calls = Rc::new(Cell::new(0));
    let callback_calls = runtime_calls.clone();
    runtime
        .subscribe(move |event| {
            if matches!(event, GraphRuntimeEvent::Patched { .. }) {
                callback_calls.set(callback_calls.get() + 1);
            }
        })
        .unwrap();
    session
        .subscribe(|event| {
            if matches!(event, ViewSessionEvent::RuntimePatched { .. }) {
                panic!("view subscriber panic");
            }
        })
        .unwrap();
    let view_calls = Rc::new(Cell::new(0));
    let callback_calls = view_calls.clone();
    session
        .subscribe(move |event| {
            if matches!(event, ViewSessionEvent::RuntimePatched { .. }) {
                callback_calls.set(callback_calls.get() + 1);
            }
        })
        .unwrap();
    let node_calls = Rc::new(Cell::new(0));
    let callback_calls = node_calls.clone();
    session
        .subscribe_node(nodes[0], move |_| {
            callback_calls.set(callback_calls.get() + 1);
        })
        .unwrap();

    let frame = one_node_frame(&session);
    let collector = frame.collector();
    collector
        .command(GraphCommand::set_node_data(
            frame.graph_version(),
            nodes[0],
            serde_json::json!({ "value": 1 }),
        ))
        .unwrap();
    collector.layout(layout_for(&frame)).unwrap();

    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        session.submit_feedback(collector.finish().unwrap())
    }))
    .expect_err("the feedback body panic remains first");

    assert_eq!(
        panic.downcast_ref::<&str>(),
        Some(&"feedback body middleware panic")
    );
    assert_eq!(runtime.snapshot().unwrap().revision(), 1);
    assert_eq!(runtime_calls.get(), 1);
    assert_eq!(view_calls.get(), 1);
    assert_eq!(node_calls.get(), 1);
}

#[test]
fn feedback_error_still_restores_the_first_drain_panic_after_both_layers_finish() {
    let (graph, nodes) = graph_with_nodes(1);
    let view_slot = Rc::new(RefCell::new(None));
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap()
        .with_middleware(CloseViewBeforeFeedbackCommand {
            view: view_slot.clone(),
        });
    let session = Rc::new(runtime.attach_view(ViewSessionOptions::default()).unwrap());
    *view_slot.borrow_mut() = Some(session.clone());

    runtime
        .subscribe(|event| {
            if matches!(event, GraphRuntimeEvent::Patched { .. }) {
                panic!("runtime subscriber panic while returning feedback error");
            }
        })
        .unwrap();
    let runtime_calls = Rc::new(Cell::new(0));
    let callback_calls = runtime_calls.clone();
    runtime
        .subscribe(move |event| {
            if matches!(event, GraphRuntimeEvent::Patched { .. }) {
                callback_calls.set(callback_calls.get() + 1);
            }
        })
        .unwrap();
    session
        .subscribe(|event| {
            if matches!(event, ViewSessionEvent::Closed { .. }) {
                panic!("view subscriber panic while returning feedback error");
            }
        })
        .unwrap();
    let terminal_calls = Rc::new(Cell::new(0));
    let callback_calls = terminal_calls.clone();
    session
        .subscribe(move |event| {
            if matches!(event, ViewSessionEvent::Closed { .. }) {
                callback_calls.set(callback_calls.get() + 1);
            }
        })
        .unwrap();

    let frame = one_node_frame(&session);
    let collector = frame.collector();
    collector
        .command(GraphCommand::set_node_data(
            frame.graph_version(),
            nodes[0],
            serde_json::json!({ "value": 1 }),
        ))
        .unwrap();
    collector.layout(layout_for(&frame)).unwrap();

    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        session.submit_feedback(collector.finish().unwrap())
    }))
    .expect_err("a drain panic must supersede the ordinary feedback error");

    assert_eq!(
        panic.downcast_ref::<&str>(),
        Some(&"runtime subscriber panic while returning feedback error")
    );
    assert_eq!(runtime.snapshot().unwrap().revision(), 1);
    assert_eq!(runtime_calls.get(), 1);
    assert_eq!(terminal_calls.get(), 1);
    assert_eq!(session.close_reason(), Some(ViewCloseReason::HostClosed));
    assert_eq!(runtime.live_view_count(), 0);
}

#[test]
fn cloned_collectors_share_order_and_reject_late_writes_after_finish() {
    let (graph, nodes) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let frame = session
        .frame(FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint {
                    x: -100.0,
                    y: -100.0,
                },
                size: CanvasSize {
                    width: 600.0,
                    height: 400.0,
                },
            },
            RendererStamp::new("test.node", 1),
        ))
        .unwrap();
    let host = frame.collector();
    let prepaint = host.clone();
    prepaint.layout(layout_for(&frame)).unwrap();
    let command = GraphCommand::set_node_data(
        frame.graph_version(),
        nodes[0],
        serde_json::json!({ "source": "retained-callback" }),
    );
    let command_identity = command.identity().clone();
    host.command(command).unwrap();

    let feedback = prepaint.finish().unwrap();

    assert_eq!(
        host.layout(layout_for(&frame)),
        Err(FeedbackCollectorError::CollectorClosed)
    );
    assert!(matches!(
        frame.collector().finish(),
        Err(FeedbackCollectorError::CollectorClosed)
    ));
    let receipt = session.submit_feedback(feedback).unwrap();
    assert!(receipt.command_completion(&command_identity).is_ok());
    assert_eq!(receipt.command_completions().count(), 1);
    assert_eq!(receipt.layout_receipts().count(), 1);
    assert_eq!(
        receipt.layout_receipts().next().unwrap().status().unwrap(),
        LayoutNodeStatus::Stale
    );
}

#[test]
fn feedback_receipts_correlate_applied_commands_and_stale_layout() {
    let (graph, nodes) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let frame = session
        .frame(FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint {
                    x: -100.0,
                    y: -100.0,
                },
                size: CanvasSize {
                    width: 600.0,
                    height: 400.0,
                },
            },
            RendererStamp::new("test.node", 1),
        ))
        .unwrap();
    let collector = frame.collector();
    let command = GraphCommand::set_node_data(
        frame.graph_version(),
        nodes[0],
        serde_json::json!({ "expanded": true }),
    );
    let command_identity = command.identity().clone();
    collector.command(command).unwrap();
    collector.layout(layout_for(&frame)).unwrap();

    let receipt = session
        .submit_feedback(collector.finish().unwrap())
        .unwrap();
    assert!(matches!(
        receipt.command_completion(&command_identity).unwrap().result(),
        Ok(outcome) if outcome.applied().is_some()
    ));
    assert_eq!(
        receipt.layout_receipts().next().unwrap().status().unwrap(),
        LayoutNodeStatus::Stale
    );
    assert!(
        session
            .geometry_snapshot()
            .unwrap()
            .node(nodes[0])
            .is_none()
    );
}

#[test]
fn rejected_command_feedback_does_not_stale_layout_from_the_same_frame() {
    let (graph, nodes) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let frame = one_node_frame(&session);
    let collector = frame.collector();
    let command = GraphCommand::set_node_data(
        frame.graph_version(),
        NodeId::from_u128(9_999),
        serde_json::json!({ "must_not_apply": true }),
    );
    let command_identity = command.identity().clone();
    collector.command(command).unwrap();
    collector.layout(layout_for(&frame)).unwrap();

    let receipt = session
        .submit_feedback(collector.finish().unwrap())
        .unwrap();

    assert!(
        receipt
            .command_completion(&command_identity)
            .unwrap()
            .result()
            .unwrap()
            .rejection()
            .is_some()
    );
    assert_eq!(
        receipt.layout_receipts().next().unwrap().status().unwrap(),
        LayoutNodeStatus::Published
    );
    assert_eq!(receipt.publication().unwrap().published_nodes(), [nodes[0]]);
    assert!(!receipt.request_next_frame());
    assert_eq!(runtime.snapshot().unwrap().revision(), 0);
}

#[test]
fn conflicted_command_correlates_without_staling_layout() {
    let (graph, nodes) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let stale_version = runtime.snapshot().unwrap().version();
    runtime
        .execute(GraphCommand::set_node_data(
            stale_version,
            nodes[0],
            serde_json::json!({ "admitted": true }),
        ))
        .into_result()
        .unwrap()
        .applied()
        .unwrap();
    let frame = session
        .frame(FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint {
                    x: -100.0,
                    y: -100.0,
                },
                size: CanvasSize {
                    width: 600.0,
                    height: 400.0,
                },
            },
            RendererStamp::new("test.node", 1),
        ))
        .unwrap();
    let collector = frame.collector();
    let command = GraphCommand::set_node_data(
        stale_version,
        nodes[0],
        serde_json::json!({ "must_not_apply": true }),
    );
    let command_identity = command.identity().clone();
    collector.command(command).unwrap();
    collector.layout(layout_for(&frame)).unwrap();

    let receipt = session
        .submit_feedback(collector.finish().unwrap())
        .unwrap();

    assert!(
        receipt
            .command_completion(&command_identity)
            .unwrap()
            .result()
            .unwrap()
            .conflict()
            .is_some()
    );
    assert_eq!(
        receipt.layout_receipts().next().unwrap().status().unwrap(),
        LayoutNodeStatus::Published
    );
    assert_eq!(runtime.snapshot().unwrap().revision(), 1);
    assert_eq!(receipt.publication().unwrap().published_nodes(), [nodes[0]]);
    assert!(!receipt.request_next_frame());
}

#[test]
fn feedback_notifications_observe_completed_command_log_before_conflicted_followup() {
    let (graph, nodes) = graph_with_nodes(2);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let observations = Rc::new(RefCell::new(Vec::new()));
    let callback_observations = observations.clone();
    let weak = session.downgrade();
    session
        .subscribe(move |event| {
            let ViewSessionEvent::RuntimePatched { patch, .. } = event else {
                return;
            };
            callback_observations.borrow_mut().push((
                patch.to_revision(),
                weak.graph_snapshot().unwrap().revision(),
            ));
        })
        .unwrap();

    let frame = session
        .frame(FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint {
                    x: -100.0,
                    y: -100.0,
                },
                size: CanvasSize {
                    width: 600.0,
                    height: 400.0,
                },
            },
            RendererStamp::new("test.node", 1),
        ))
        .unwrap();
    let collector = frame.collector();
    let applied = GraphCommand::set_node_data(
        frame.graph_version(),
        nodes[0],
        serde_json::json!({ "step": 1 }),
    );
    let applied_identity = applied.identity().clone();
    collector.command(applied).unwrap();
    let conflicted = GraphCommand::set_node_data(
        frame.graph_version(),
        nodes[1],
        serde_json::json!({ "step": 2 }),
    );
    let conflicted_identity = conflicted.identity().clone();
    collector.command(conflicted).unwrap();

    let receipt = session
        .submit_feedback(collector.finish().unwrap())
        .unwrap();

    assert_eq!(receipt.command_completions().count(), 2);
    assert!(matches!(
        receipt.command_completion(&applied_identity).unwrap().result(),
        Ok(outcome) if outcome.applied().is_some()
    ));
    assert!(matches!(
        receipt.command_completion(&conflicted_identity).unwrap().result(),
        Ok(outcome) if outcome.conflict().is_some()
    ));
    assert_eq!(runtime.snapshot().unwrap().revision(), 1);
    assert_eq!(observations.borrow().as_slice(), &[(1, 1)]);
}
