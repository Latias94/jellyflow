use std::cell::{Cell, RefCell};
use std::rc::Rc;

use jellyflow_core::core::{CanvasPoint, NodeId};

use crate::io::{ViewSelectionSeed, ViewSessionSeed};
use crate::profile::ApplyPipelineError;
use crate::runtime::commands::{GraphCommand, IncidentEdgePolicy, TopologyEdit};
use crate::runtime::graph_runtime::GraphRuntime;
use crate::runtime::middleware::{GraphRuntimeMiddleware, GraphRuntimeMiddlewareSnapshot};
use crate::runtime::view_session::{
    ViewCloseReason, ViewCloseRequest, ViewSessionError, ViewSessionEvent, WeakViewSession,
};
use crate::runtime::viewport::ViewportTransform;

use super::fixtures::{graph_with_nodes, view_options};

#[test]
fn weak_viewport_control_normalizes_input_and_publishes_the_applied_transform() {
    let (graph, _) = graph_with_nodes(1);
    let graph_id = graph.graph_id();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("runtime");
    let view = runtime
        .attach_view(view_options(ViewSessionSeed::new(graph_id)))
        .expect("view");
    let weak = view.downgrade();
    let transforms = Rc::new(RefCell::new(Vec::new()));
    let callback_transforms = transforms.clone();
    let callback_weak = weak.clone();
    weak.subscribe(move |event| {
        if let ViewSessionEvent::ViewportChanged { transform } = event {
            assert_eq!(callback_weak.viewport_transform().unwrap(), transform);
            callback_transforms.borrow_mut().push(transform);
        }
    })
    .expect("weak subscription");

    let requested =
        ViewportTransform::new(CanvasPoint { x: 12.0, y: -8.0 }, 1.5).expect("valid transform");
    weak.set_viewport(requested.pan, requested.zoom)
        .expect("valid viewport update");
    weak.set_viewport(
        CanvasPoint {
            x: f32::INFINITY,
            y: 4.0,
        },
        -2.0,
    )
    .expect("invalid viewport input is normalized");
    weak.set_viewport(CanvasPoint::default(), 0.0)
        .expect("repeated normalized viewport is a no-op");

    let normalized = ViewportTransform::new(CanvasPoint::default(), 1.0).unwrap();
    assert_eq!(weak.viewport_transform().unwrap(), normalized);
    assert_eq!(view.viewport_transform().unwrap(), normalized);
    assert_eq!(&*transforms.borrow(), &[requested, normalized]);
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
    runtime
        .execute(GraphCommand::topology(
            0,
            TopologyEdit::new().remove_node(ids[1], IncidentEdgePolicy::Reject),
        ))
        .expect("remove missing selection candidate")
        .applied()
        .expect("remove applies");

    let selections = Rc::new(RefCell::new(Vec::new()));
    let callback_selections = selections.clone();
    weak.subscribe(move |event| {
        if let ViewSessionEvent::SelectionChanged { selection } = event {
            callback_selections.borrow_mut().push(selection);
        }
    })
    .expect("weak subscription");

    weak.set_selection(
        vec![ids[0], ids[1], NodeId::from_u128(0xffff), ids[0]],
        Vec::new(),
        Vec::new(),
    )
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
            1,
            TopologyEdit::new().remove_node(ids[0], IncidentEdgePolicy::Reject),
        ))
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

    view.with_state_borrowed_for_test(|| {
        assert!(matches!(weak.selection(), Err(ViewSessionError::Busy)));
        assert!(matches!(
            weak.subscribe(|_| {}),
            Err(ViewSessionError::Busy)
        ));
        assert!(matches!(
            weak.execute(GraphCommand::undo(0)),
            Err(ViewSessionError::Busy)
        ));
        assert!(matches!(
            weak.geometry_snapshot(),
            Err(ViewSessionError::Busy)
        ));
    });
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
        _snapshot: GraphRuntimeMiddlewareSnapshot,
        _command: &GraphCommand,
    ) -> Result<(), ApplyPipelineError> {
        let weak = self.weak.borrow();
        let weak = weak.as_ref().expect("weak view installed");
        let execute_is_closing = matches!(
            weak.execute(GraphCommand::undo(0)),
            Err(ViewSessionError::Closing)
        );
        let subscribe_is_closing = matches!(weak.subscribe(|_| {}), Err(ViewSessionError::Closing));
        self.observed_closing
            .set(execute_is_closing && subscribe_is_closing);
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

    view.close_with_request(
        ViewCloseRequest::new().with_command(GraphCommand::set_node_data(
            0,
            ids[0],
            serde_json::json!({ "closed": true }),
        )),
    )
    .expect("close with command");

    assert!(observed_closing.get());
}
