use std::cell::{Cell, RefCell};
use std::rc::Rc;

use jellyflow_core::core::CanvasPoint;

use crate::io::{ViewSelectionSeed, ViewSessionSeed};
use crate::profile::ApplyPipelineError;
use crate::runtime::adapter_protocol::{ViewDelta, ViewPoint};
use crate::runtime::commands::{GraphCommand, IncidentEdgePolicy, TopologyEdit};
use crate::runtime::graph_runtime::GraphRuntime;
use crate::runtime::middleware::{GraphRuntimeMiddleware, GraphRuntimeMiddlewareSnapshot};
use crate::runtime::view_session::{
    ViewCloseReason, ViewCloseRequest, ViewSessionError, ViewSessionEvent, WeakViewSession,
};
use crate::runtime::viewport::{ViewportIntent, ViewportPanRequest, ViewportTransform};

use super::fixtures::{graph_with_nodes, view_options};

#[test]
fn weak_viewport_control_is_strict_atomic_and_publishes_exact_snapshots() {
    let (graph, _) = graph_with_nodes(1);
    let graph_id = graph.graph_id();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let view = runtime
        .attach_view(view_options(ViewSessionSeed::new(graph_id)))
        .expect("view");
    let weak = view.downgrade();
    let snapshots = Rc::new(RefCell::new(Vec::new()));
    let callback_snapshots = snapshots.clone();
    let callback_weak = weak.clone();
    weak.subscribe(move |event| {
        if let ViewSessionEvent::ViewportChanged { snapshot } = event {
            assert_eq!(callback_weak.viewport_snapshot().unwrap(), snapshot);
            callback_snapshots.borrow_mut().push(snapshot);
        }
    })
    .expect("weak subscription");

    let initial = weak.viewport_snapshot().unwrap();
    let requested =
        ViewportTransform::new(CanvasPoint { x: 12.0, y: -8.0 }, 1.5).expect("valid transform");
    let applied = weak.set_viewport(requested).expect("valid viewport update");
    assert_eq!(applied.transform(), requested);
    assert_eq!(applied.provenance(), initial.provenance());
    assert_ne!(applied.revision(), initial.revision());

    assert_eq!(
        weak.set_viewport(requested)
            .expect("same viewport is a no-op"),
        applied
    );
    let invalid = ViewportTransform {
        pan: CanvasPoint {
            x: f32::INFINITY,
            y: 4.0,
        },
        zoom: -2.0,
    };
    assert!(matches!(
        weak.set_viewport(invalid),
        Err(ViewSessionError::InvalidViewport)
    ));
    assert!(matches!(
        weak.apply_viewport_intent(ViewportIntent::pan(ViewportPanRequest::new(
            ViewDelta::new(f32::NAN, 1.0),
        ))),
        Err(ViewSessionError::InvalidViewportIntent)
    ));

    assert_eq!(weak.viewport_snapshot().unwrap(), applied);
    let panned = weak
        .apply_viewport_intent(ViewportIntent::pan(ViewportPanRequest::new(
            ViewDelta::new(15.0, -3.0),
        )))
        .expect("valid pan intent");
    assert_eq!(
        panned.transform(),
        ViewportTransform::new(CanvasPoint { x: 22.0, y: -10.0 }, 1.5).unwrap()
    );
    assert_eq!(view.viewport_snapshot().unwrap(), panned);
    assert_eq!(snapshots.borrow().as_slice(), &[applied, panned]);

    let sample = weak
        .viewport_pointer_sample(ViewPoint::new(24.0, 18.0))
        .expect("finite point");
    assert_eq!(sample.snapshot(), panned);
    assert_eq!(sample.raw(), ViewPoint::new(24.0, 18.0));
    assert!(matches!(
        weak.viewport_pointer_sample(ViewPoint::new(f32::NAN, 0.0)),
        Err(ViewSessionError::InvalidViewportPointer)
    ));
}

#[test]
fn weak_selection_filters_missing_and_duplicate_ids_and_sanitizes_retirement() {
    let (graph, ids) = graph_with_nodes(2);
    let graph_id = graph.graph_id();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let view = runtime
        .attach_view(view_options(ViewSessionSeed::new(graph_id)))
        .expect("view");
    let weak = view.downgrade();
    let initial_snapshot = runtime.snapshot().expect("initial snapshot");
    let initial_version = initial_snapshot.version();
    let live_node = initial_snapshot
        .node_lifetime_key(ids[0])
        .expect("live selection capability");
    let retired_node = initial_snapshot
        .node_lifetime_key(ids[1])
        .expect("retired selection capability");
    let removed_version = runtime
        .execute(GraphCommand::topology(
            initial_version,
            TopologyEdit::new().remove_node(ids[1], IncidentEdgePolicy::Reject),
        ))
        .into_result()
        .expect("remove missing selection candidate")
        .applied()
        .expect("remove applies")
        .version();

    let selections = Rc::new(RefCell::new(Vec::new()));
    let callback_selections = selections.clone();
    weak.subscribe(move |event| {
        if let ViewSessionEvent::SelectionChanged { selection } = event {
            callback_selections.borrow_mut().push(selection);
        }
    })
    .expect("weak subscription");

    assert!(matches!(
        weak.set_selection(vec![live_node, retired_node], Vec::new(), Vec::new()),
        Err(ViewSessionError::StaleInteractionFacts)
    ));
    assert_eq!(weak.selection().unwrap(), ViewSelectionSeed::default());
    assert!(selections.borrow().is_empty());
    weak.set_selection(vec![live_node, live_node], Vec::new(), Vec::new())
        .expect("selection update");

    let selected = ViewSelectionSeed::new(vec![ids[0]], Vec::new(), Vec::new());
    assert_eq!(weak.selection().unwrap(), selected);
    assert_eq!(view.selection().unwrap(), selected);
    assert_eq!(
        selections.borrow().as_slice(),
        std::slice::from_ref(&selected)
    );

    runtime
        .execute(GraphCommand::topology(
            removed_version,
            TopologyEdit::new().remove_node(ids[0], IncidentEdgePolicy::Reject),
        ))
        .into_result()
        .expect("retire selected node")
        .applied()
        .expect("remove applies");

    assert_eq!(weak.selection().unwrap(), ViewSelectionSeed::default());
    assert_eq!(
        &*selections.borrow(),
        &[selected, ViewSelectionSeed::default()]
    );
}

#[test]
fn weak_control_plane_does_not_extend_the_view_lifetime() {
    let (graph, _) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let view = runtime.attach_view(Default::default()).expect("view");
    let weak = view.downgrade();

    drop(view);

    assert_eq!(runtime.live_view_count(), 0);
    assert!(!weak.is_open());
    assert!(matches!(
        weak.export_seed(),
        Err(ViewSessionError::Closed {
            reason: ViewCloseReason::HostClosed,
        })
    ));
}

#[test]
fn weak_subscriptions_receive_terminal_close_and_are_then_revoked() {
    let (graph, _) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let view = runtime.attach_view(Default::default()).expect("view");
    let weak = view.downgrade();
    let reasons = Rc::new(RefCell::new(Vec::new()));
    let callback_reasons = reasons.clone();
    let token = weak
        .subscribe(move |event| {
            if let ViewSessionEvent::Closed { reason } = event {
                callback_reasons.borrow_mut().push(reason);
            }
        })
        .expect("weak subscription");

    view.close().expect("close");

    assert_eq!(&*reasons.borrow(), &[ViewCloseReason::HostClosed]);
    assert!(matches!(
        weak.unsubscribe(token),
        Err(ViewSessionError::Closed {
            reason: ViewCloseReason::HostClosed,
        })
    ));
    assert!(matches!(
        weak.subscribe(|_| {}),
        Err(ViewSessionError::Closed {
            reason: ViewCloseReason::HostClosed,
        })
    ));
}

#[test]
fn weak_control_plane_reports_runtime_drop_and_closes_subscriptions() {
    let (graph, _) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let view = runtime.attach_view(Default::default()).expect("view");
    let weak = view.downgrade();
    let reasons = Rc::new(RefCell::new(Vec::new()));
    let callback_reasons = reasons.clone();
    weak.subscribe(move |event| {
        if let ViewSessionEvent::Closed { reason } = event {
            callback_reasons.borrow_mut().push(reason);
        }
    })
    .expect("weak subscription");

    drop(runtime);

    assert_eq!(&*reasons.borrow(), &[ViewCloseReason::RuntimeDropped]);
    assert!(matches!(
        weak.viewport_transform(),
        Err(ViewSessionError::Closed {
            reason: ViewCloseReason::RuntimeDropped,
        })
    ));
}

#[test]
fn weak_operations_report_busy_when_the_strong_session_is_borrowed() {
    let (graph, _) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let view = runtime.attach_view(Default::default()).expect("view");
    let weak = view.downgrade();
    let current_version = view.graph_snapshot().expect("current snapshot").version();
    let viewport_before = view.viewport_snapshot().unwrap();
    let viewport_events = Rc::new(Cell::new(0));
    let callback_events = viewport_events.clone();
    view.subscribe(move |event| {
        if matches!(event, ViewSessionEvent::ViewportChanged { .. }) {
            callback_events.set(callback_events.get() + 1);
        }
    })
    .unwrap();

    view.with_state_borrowed_for_test(|| {
        assert!(matches!(weak.selection(), Err(ViewSessionError::Busy)));
        assert!(matches!(
            weak.subscribe(|_| {}),
            Err(ViewSessionError::Busy)
        ));
        assert!(matches!(
            weak.execute(GraphCommand::undo(current_version)),
            Err(ViewSessionError::Busy)
        ));
        assert!(matches!(
            weak.geometry_snapshot(),
            Err(ViewSessionError::Busy)
        ));
        assert!(matches!(
            weak.set_viewport(ViewportTransform::new(CanvasPoint { x: 1.0, y: 2.0 }, 1.0).unwrap()),
            Err(ViewSessionError::Busy)
        ));
        assert!(matches!(
            weak.apply_viewport_intent(ViewportIntent::pan(ViewportPanRequest::new(
                ViewDelta::new(1.0, 2.0),
            ))),
            Err(ViewSessionError::Busy)
        ));
        assert!(matches!(
            weak.viewport_pointer_sample(ViewPoint::new(1.0, 2.0)),
            Err(ViewSessionError::Busy)
        ));
    });
    assert_eq!(view.viewport_snapshot().unwrap(), viewport_before);
    assert_eq!(viewport_events.get(), 0);
}

#[test]
fn viewport_revision_exhaustion_has_zero_state_or_event_side_effects() {
    let (graph, _) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let view = runtime.attach_view(Default::default()).expect("view");
    view.exhaust_viewport_revision_for_test();
    let before = view.viewport_snapshot().unwrap();
    let events = Rc::new(Cell::new(0));
    let callback_events = events.clone();
    view.subscribe(move |event| {
        if matches!(event, ViewSessionEvent::ViewportChanged { .. }) {
            callback_events.set(callback_events.get() + 1);
        }
    })
    .unwrap();

    assert!(matches!(
        view.set_viewport(ViewportTransform::new(CanvasPoint { x: 4.0, y: 8.0 }, 1.25).unwrap()),
        Err(ViewSessionError::ViewportRevisionExhausted)
    ));
    assert!(matches!(
        view.apply_viewport_intent(ViewportIntent::pan(ViewportPanRequest::new(
            ViewDelta::new(5.0, -2.0),
        ))),
        Err(ViewSessionError::ViewportRevisionExhausted)
    ));
    assert_eq!(view.viewport_snapshot().unwrap(), before);
    assert_eq!(events.get(), 0);
    assert_eq!(view.set_viewport(before.transform()).unwrap(), before);
}

#[test]
fn viewport_subscriber_panic_preserves_one_committed_snapshot_and_full_fanout() {
    let (graph, _) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let view = runtime.attach_view(Default::default()).expect("view");
    view.subscribe(|event| {
        if matches!(event, ViewSessionEvent::ViewportChanged { .. }) {
            panic!("viewport subscriber panic");
        }
    })
    .unwrap();
    let observed = Rc::new(RefCell::new(Vec::new()));
    let callback_observed = observed.clone();
    view.subscribe(move |event| {
        if let ViewSessionEvent::ViewportChanged { snapshot } = event {
            callback_observed.borrow_mut().push(snapshot);
        }
    })
    .unwrap();
    let next = ViewportTransform::new(CanvasPoint { x: 3.0, y: -7.0 }, 2.0).unwrap();

    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| view.set_viewport(next)))
        .expect_err("subscriber panic remains observable");
    assert_eq!(
        panic.downcast_ref::<&str>(),
        Some(&"viewport subscriber panic")
    );
    let committed = view.viewport_snapshot().unwrap();
    assert_eq!(committed.transform(), next);
    assert_eq!(observed.borrow().as_slice(), &[committed]);
    assert_eq!(view.set_viewport(next).unwrap(), committed);
    assert_eq!(observed.borrow().as_slice(), &[committed]);
}

#[test]
fn explicit_close_reason_survives_a_later_runtime_drop() {
    let (graph, _) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let view = runtime.attach_view(Default::default()).expect("view");
    let weak = view.downgrade();

    view.close().expect("close view");
    drop(runtime);

    assert!(matches!(
        weak.export_seed(),
        Err(ViewSessionError::Closed {
            reason: ViewCloseReason::HostClosed,
        })
    ));
    assert!(matches!(
        weak.geometry_snapshot(),
        Err(ViewSessionError::Closed {
            reason: ViewCloseReason::HostClosed,
        })
    ));
}

struct ClosingProbeMiddleware {
    weak: Rc<RefCell<Option<WeakViewSession>>>,
    observed_closing: Rc<Cell<bool>>,
}

impl GraphRuntimeMiddleware for ClosingProbeMiddleware {
    fn before_command(
        &mut self,
        snapshot: GraphRuntimeMiddlewareSnapshot,
        _command: &GraphCommand,
    ) -> Result<(), ApplyPipelineError> {
        let weak = self.weak.borrow();
        let weak = weak.as_ref().expect("weak view installed");
        let execute_is_closing = matches!(
            weak.execute(GraphCommand::undo(snapshot.graph().version())),
            Err(ViewSessionError::Closing)
        );
        let subscribe_is_closing = matches!(weak.subscribe(|_| {}), Err(ViewSessionError::Closing));
        let set_viewport_is_closing = matches!(
            weak.set_viewport(
                ViewportTransform::new(CanvasPoint { x: 2.0, y: 3.0 }, 1.25).unwrap()
            ),
            Err(ViewSessionError::Closing)
        );
        let intent_is_closing = matches!(
            weak.apply_viewport_intent(ViewportIntent::pan(ViewportPanRequest::new(
                ViewDelta::new(3.0, 4.0),
            ))),
            Err(ViewSessionError::Closing)
        );
        let sample_is_closing = matches!(
            weak.viewport_pointer_sample(ViewPoint::new(3.0, 4.0)),
            Err(ViewSessionError::Closing)
        );
        self.observed_closing.set(
            execute_is_closing
                && subscribe_is_closing
                && set_viewport_is_closing
                && intent_is_closing
                && sample_is_closing,
        );
        Ok(())
    }
}

#[test]
fn weak_operations_are_rejected_while_the_strong_session_is_closing() {
    let (graph, ids) = graph_with_nodes(1);
    let weak_slot = Rc::new(RefCell::new(None));
    let observed_closing = Rc::new(Cell::new(false));
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime")
        .with_middleware(ClosingProbeMiddleware {
            weak: weak_slot.clone(),
            observed_closing: observed_closing.clone(),
        });
    let view = runtime.attach_view(Default::default()).expect("view");
    *weak_slot.borrow_mut() = Some(view.downgrade());
    let current_version = view.graph_snapshot().expect("current snapshot").version();
    let viewport_events = Rc::new(Cell::new(0));
    let callback_events = viewport_events.clone();
    view.subscribe(move |event| {
        if matches!(event, ViewSessionEvent::ViewportChanged { .. }) {
            callback_events.set(callback_events.get() + 1);
        }
    })
    .unwrap();

    view.close_with_request(
        ViewCloseRequest::new().with_command(GraphCommand::set_node_data(
            current_version,
            ids[0],
            serde_json::json!({ "closed": true }),
        )),
    )
    .expect("close with command");

    assert!(observed_closing.get());
    assert_eq!(viewport_events.get(), 0);
}
