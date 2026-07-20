use std::cell::{Cell, RefCell};
use std::rc::Rc;

use jellyflow_core::core::{CanvasPoint, CanvasRect, CanvasSize, NodeId};

use crate::profile::ApplyPipelineError;
use crate::runtime::adapter_protocol::{
    AdapterFrame, FeedbackCollectorError, FeedbackOperation, FeedbackOperationReceipt,
    FrameRequest, LayoutNodeStatus, NodeLayout, RendererStamp, ViewPoint, ViewRect, ViewSize,
};
use crate::runtime::commands::GraphCommand;
use crate::runtime::graph_runtime::{GraphRuntime, GraphRuntimeEvent, RuntimePatch};
use crate::runtime::middleware::{GraphRuntimeMiddleware, GraphRuntimeMiddlewareSnapshot};
use crate::runtime::view_session::{
    ViewCloseReason, ViewSession, ViewSessionEvent, ViewSessionOptions,
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
            0,
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
    collector.selection([nodes[0]], [], []).unwrap();
    collector
        .command(GraphCommand::set_node_data(
            0,
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
    host.selection([nodes[0]], [], []).unwrap();
    prepaint.layout(layout_for(&frame)).unwrap();
    host.command(GraphCommand::set_node_data(
        0,
        nodes[0],
        serde_json::json!({ "source": "retained-callback" }),
    ))
    .unwrap();

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
    assert!(matches!(
        receipt.operations(),
        [
            FeedbackOperationReceipt::Selection { .. },
            FeedbackOperationReceipt::Layout {
                status: LayoutNodeStatus::Stale,
                ..
            },
            FeedbackOperationReceipt::Command { .. }
        ]
    ));
}

#[test]
fn invalid_viewports_are_rejected_before_the_collector_appends_them() {
    let (graph, nodes) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let invalid_viewports = [
        ("zero zoom", CanvasPoint::default(), 0.0),
        ("negative zoom", CanvasPoint::default(), -1.0),
        ("NaN zoom", CanvasPoint::default(), f32::NAN),
        ("infinite zoom", CanvasPoint::default(), f32::INFINITY),
        (
            "NaN pan",
            CanvasPoint {
                x: f32::NAN,
                y: 0.0,
            },
            1.0,
        ),
        (
            "infinite pan",
            CanvasPoint {
                x: 0.0,
                y: f32::NEG_INFINITY,
            },
            1.0,
        ),
    ];

    for (case, pan, zoom) in invalid_viewports {
        let frame = one_node_frame(&session);
        let collector = frame.collector();
        collector.selection([nodes[0]], [], []).unwrap();

        assert_eq!(
            collector.viewport(pan, zoom),
            Err(FeedbackCollectorError::InvalidViewport),
            "{case}"
        );

        let feedback = collector.finish().unwrap();
        assert!(
            matches!(
                feedback.operations.as_slice(),
                [FeedbackOperation::Selection { nodes: selected, edges, groups }]
                    if selected == &[nodes[0]] && edges.is_empty() && groups.is_empty()
            ),
            "{case} appended an invalid viewport operation"
        );

        let fresh_frame = one_node_frame(&session);
        let fresh_collector = fresh_frame.collector();
        assert_eq!(
            fresh_collector.viewport(pan, zoom),
            Err(FeedbackCollectorError::InvalidViewport),
            "{case}"
        );
        assert!(
            fresh_collector.finish().unwrap().operations.is_empty(),
            "{case} appended an operation to a fresh collector"
        );
    }
}

#[test]
fn feedback_receipts_preserve_host_order_and_applied_commands_stale_layout() {
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
    collector.selection([nodes[0]], [], []).unwrap();
    collector
        .command(GraphCommand::set_node_data(
            0,
            nodes[0],
            serde_json::json!({ "expanded": true }),
        ))
        .unwrap();
    collector.layout(layout_for(&frame)).unwrap();

    let receipt = session
        .submit_feedback(collector.finish().unwrap())
        .unwrap();
    assert!(matches!(
        &receipt.operations()[0],
        FeedbackOperationReceipt::Selection { .. }
    ));
    assert!(matches!(
        &receipt.operations()[1],
        FeedbackOperationReceipt::Command { .. }
    ));
    assert!(matches!(
        &receipt.operations()[2],
        FeedbackOperationReceipt::Layout {
            status: LayoutNodeStatus::Stale,
            ..
        }
    ));
    assert_eq!(
        session.export_seed().unwrap().selection().nodes(),
        [nodes[0]]
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
fn selection_feedback_does_not_stale_layout_from_the_same_frame() {
    let (graph, nodes) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let frame = one_node_frame(&session);
    let collector = frame.collector();
    collector.selection([nodes[0]], [], []).unwrap();
    collector.layout(layout_for(&frame)).unwrap();

    let receipt = session
        .submit_feedback(collector.finish().unwrap())
        .unwrap();

    assert!(matches!(
        receipt.operations(),
        [
            FeedbackOperationReceipt::Selection { .. },
            FeedbackOperationReceipt::Layout {
                status: LayoutNodeStatus::Published,
                ..
            }
        ]
    ));
    assert_eq!(receipt.publication().unwrap().published_nodes(), [nodes[0]]);
    assert!(!receipt.request_next_frame());
    assert_eq!(runtime.snapshot().unwrap().revision(), 0);
}

#[test]
fn view_feedback_receipts_report_the_normalized_state_that_was_applied() {
    let (graph, nodes) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let frame = one_node_frame(&session);
    let collector = frame.collector();
    collector
        .selection([nodes[0], NodeId::from_u128(9_999), nodes[0]], [], [])
        .unwrap();
    collector
        .viewport(CanvasPoint { x: 24.0, y: 12.0 }, 1.25)
        .unwrap();

    let receipt = session
        .submit_feedback(collector.finish().unwrap())
        .unwrap();

    let FeedbackOperationReceipt::Selection { selection } = &receipt.operations()[0] else {
        panic!("expected selection receipt");
    };
    assert_eq!(selection.nodes(), [nodes[0]]);
    let FeedbackOperationReceipt::Viewport { transform } = &receipt.operations()[1] else {
        panic!("expected viewport receipt");
    };
    assert_eq!(*transform, session.viewport_transform().unwrap());
    assert_eq!(transform.pan, CanvasPoint { x: 24.0, y: 12.0 });
    assert_eq!(transform.zoom, 1.25);
}

#[test]
fn selection_callback_close_waits_for_the_accepted_feedback_log() {
    let (graph, nodes) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = Rc::new(runtime.attach_view(ViewSessionOptions::default()).unwrap());
    let events = Rc::new(RefCell::new(Vec::new()));
    let callback_events = events.clone();
    let callback_session = session.clone();
    let callback_revision = Rc::new(Cell::new(None));
    let observed_revision = callback_revision.clone();
    session
        .subscribe(move |event| match event {
            ViewSessionEvent::SelectionChanged { .. } => {
                callback_events.borrow_mut().push("selection");
                observed_revision.set(Some(
                    callback_session
                        .graph_snapshot()
                        .expect("accepted feedback is complete before callbacks")
                        .revision(),
                ));
                callback_session
                    .close()
                    .expect("selection callback closes view");
            }
            ViewSessionEvent::Closed { .. } => callback_events.borrow_mut().push("closed"),
            ViewSessionEvent::ViewportChanged { .. }
            | ViewSessionEvent::RuntimePatched { .. }
            | ViewSessionEvent::LayoutPublished { .. } => {}
        })
        .unwrap();
    let frame = one_node_frame(&session);
    let collector = frame.collector();
    collector.selection([nodes[0]], [], []).unwrap();
    collector
        .command(GraphCommand::set_node_data(
            0,
            nodes[0],
            serde_json::json!({ "step": "after-selection" }),
        ))
        .unwrap();
    collector.layout(layout_for(&frame)).unwrap();

    let receipt = session
        .submit_feedback(collector.finish().unwrap())
        .unwrap();

    assert!(matches!(
        receipt.operations(),
        [
            FeedbackOperationReceipt::Selection { .. },
            FeedbackOperationReceipt::Command { .. },
            FeedbackOperationReceipt::Layout {
                status: LayoutNodeStatus::Stale,
                ..
            }
        ]
    ));
    assert!(
        receipt.operations()[1]
            .command_outcome()
            .unwrap()
            .as_ref()
            .unwrap()
            .applied()
            .is_some()
    );
    assert_eq!(callback_revision.get(), Some(1));
    assert_eq!(events.borrow().as_slice(), &["selection", "closed"]);
    assert_eq!(session.close_reason(), Some(ViewCloseReason::HostClosed));
    assert_eq!(runtime.snapshot().unwrap().revision(), 1);
}

#[test]
fn viewport_callback_close_preserves_queued_feedback_event_order() {
    let (graph, nodes) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = Rc::new(runtime.attach_view(ViewSessionOptions::default()).unwrap());
    let events = Rc::new(RefCell::new(Vec::new()));
    let callback_events = events.clone();
    let callback_session = session.clone();
    let callback_revision = Rc::new(Cell::new(None));
    let observed_revision = callback_revision.clone();
    session
        .subscribe(move |event| match event {
            ViewSessionEvent::SelectionChanged { .. } => {
                callback_events.borrow_mut().push("selection");
            }
            ViewSessionEvent::ViewportChanged { .. } => {
                callback_events.borrow_mut().push("viewport");
                observed_revision.set(Some(
                    callback_session
                        .graph_snapshot()
                        .expect("accepted feedback is complete before callbacks")
                        .revision(),
                ));
                callback_session
                    .close()
                    .expect("viewport callback closes view");
            }
            ViewSessionEvent::Closed { .. } => callback_events.borrow_mut().push("closed"),
            ViewSessionEvent::RuntimePatched { .. } | ViewSessionEvent::LayoutPublished { .. } => {}
        })
        .unwrap();
    let frame = one_node_frame(&session);
    let collector = frame.collector();
    collector.selection([nodes[0]], [], []).unwrap();
    collector
        .viewport(CanvasPoint { x: 8.0, y: 12.0 }, 1.5)
        .unwrap();
    collector.layout(layout_for(&frame)).unwrap();

    let receipt = session
        .submit_feedback(collector.finish().unwrap())
        .unwrap();

    assert!(matches!(
        receipt.operations(),
        [
            FeedbackOperationReceipt::Selection { .. },
            FeedbackOperationReceipt::Viewport { .. },
            FeedbackOperationReceipt::Layout {
                status: LayoutNodeStatus::Stale,
                ..
            }
        ]
    ));
    assert!(receipt.publication().is_none());
    assert!(receipt.request_next_frame());
    assert_eq!(callback_revision.get(), Some(0));
    assert_eq!(
        events.borrow().as_slice(),
        &["selection", "viewport", "closed"]
    );
    assert_eq!(session.close_reason(), Some(ViewCloseReason::HostClosed));
    assert_eq!(runtime.snapshot().unwrap().revision(), 0);
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
    collector
        .command(GraphCommand::set_node_data(
            0,
            NodeId::from_u128(9_999),
            serde_json::json!({ "must_not_apply": true }),
        ))
        .unwrap();
    collector.layout(layout_for(&frame)).unwrap();

    let receipt = session
        .submit_feedback(collector.finish().unwrap())
        .unwrap();

    assert!(
        receipt.operations()[0]
            .command_outcome()
            .unwrap()
            .as_ref()
            .unwrap()
            .rejection()
            .is_some()
    );
    assert!(matches!(
        receipt.operations()[1],
        FeedbackOperationReceipt::Layout {
            status: LayoutNodeStatus::Published,
            ..
        }
    ));
    assert_eq!(receipt.publication().unwrap().published_nodes(), [nodes[0]]);
    assert!(!receipt.request_next_frame());
    assert_eq!(runtime.snapshot().unwrap().revision(), 0);
}

#[test]
fn selection_viewport_rejected_command_and_layout_keep_exact_host_order() {
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
    collector.selection([nodes[0]], [], []).unwrap();
    collector
        .viewport(CanvasPoint { x: 8.0, y: 12.0 }, 1.5)
        .unwrap();
    collector
        .command(GraphCommand::set_node_data(
            99,
            nodes[0],
            serde_json::json!({ "must_not_apply": true }),
        ))
        .unwrap();
    collector.layout(layout_for(&frame)).unwrap();

    let receipt = session
        .submit_feedback(collector.finish().unwrap())
        .unwrap();

    assert!(matches!(
        receipt.operations(),
        [
            FeedbackOperationReceipt::Selection { .. },
            FeedbackOperationReceipt::Viewport { .. },
            FeedbackOperationReceipt::Command { .. },
            FeedbackOperationReceipt::Layout {
                status: LayoutNodeStatus::Stale,
                ..
            }
        ]
    ));
    assert!(
        receipt.operations()[2]
            .command_outcome()
            .unwrap()
            .as_ref()
            .unwrap()
            .conflict()
            .is_some()
    );
    assert_eq!(runtime.snapshot().unwrap().revision(), 0);
    let seed = session.export_seed().unwrap();
    assert_eq!(seed.selection().nodes(), [nodes[0]]);
    assert_eq!(seed.viewport().pan(), CanvasPoint { x: 8.0, y: 12.0 });
    assert_eq!(seed.viewport().zoom(), 1.5);
}

#[test]
fn feedback_notifications_observe_only_the_completed_command_log() {
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
            let ViewSessionEvent::RuntimePatched { patch } = event else {
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
    collector
        .command(GraphCommand::set_node_data(
            0,
            nodes[0],
            serde_json::json!({ "step": 1 }),
        ))
        .unwrap();
    collector
        .command(GraphCommand::set_node_data(
            1,
            nodes[1],
            serde_json::json!({ "step": 2 }),
        ))
        .unwrap();

    let receipt = session
        .submit_feedback(collector.finish().unwrap())
        .unwrap();

    assert_eq!(receipt.operations().len(), 2);
    assert!(receipt.operations().iter().all(|operation| matches!(
        operation,
        FeedbackOperationReceipt::Command {
            outcome,
        } if matches!(outcome.as_ref(), Ok(outcome) if outcome.applied().is_some())
    )));
    assert_eq!(runtime.snapshot().unwrap().revision(), 2);
    assert_eq!(observations.borrow().as_slice(), &[(1, 2), (2, 2)]);
}
