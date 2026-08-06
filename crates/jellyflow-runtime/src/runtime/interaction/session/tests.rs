use jellyflow_core::core::{
    CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeId, EdgeKind, EdgeReconnectable,
    EdgeReconnectableEndpoint, Graph, GraphBuilder, GraphId, Node, NodeId, NodeKindKey, Port,
    PortCapacity, PortDirection, PortId, PortKey, PortKind,
};
use jellyflow_core::interaction::NodeGraphConnectionMode;

use super::*;
use crate::rules::EdgeEndpoint;
use crate::runtime::adapter_protocol::{FrameRequest, RendererStamp, ViewRect};
use crate::runtime::commands::{
    ConnectionEdit, GraphCommand, IncidentEdgePolicy, TopologyEdit, TopologyPort,
};
use crate::runtime::geometry::{
    EdgeEndpointPosition, EdgePosition, HandlePosition, resolve_edge_route_path,
};
use crate::runtime::graph_runtime::GraphRuntime;
use crate::runtime::tests::fixtures::{
    node_drag_start_by_id, port_connection_drag_intent_by_id, set_selection_by_id,
};
use crate::runtime::view_session::ViewSessionOptions;
use crate::runtime::viewport::ViewportTransform;

#[test]
fn native_node_activation_selects_without_arming_drag_or_allocating_generation() {
    let node = NodeId::from_u128(10);
    let graph = GraphBuilder::new(GraphId::from_u128(10))
        .with_node(node, test_node(CanvasPoint::default()))
        .build()
        .expect("valid activation graph");
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("valid activation runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("activation view");
    let lifetime = runtime
        .snapshot()
        .expect("activation snapshot")
        .node_lifetime_key(node)
        .expect("activation node lifetime");
    let mut session = CanvasInteractionSession::attach(&view, CanvasInteractionConfig::default())
        .expect("valid activation interaction session");

    let update = session
        .activate_node(lifetime, SelectionMode::Replace)
        .expect("idle native activation");
    assert!(update.effects().is_empty());
    assert_eq!(view.selection().unwrap().nodes(), [node]);
    assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
    assert_eq!(session.presentation().generation(), None);
    assert!(session.presentation().node_drag().is_none());

    let press_position = bound_position(&session, &view, 0.0, 0.0);
    session
        .press(CanvasPressTarget::Pan, press_position)
        .expect("arm another interaction");
    assert!(matches!(
        session.activate_node(lifetime, SelectionMode::Add),
        Err(CanvasInteractionError::AlreadyActive)
    ));
}

#[test]
fn native_connection_activation_uses_real_view_authority_without_pointer_motion() {
    let (_runtime, view, mut session) = bound_test_session();
    let drag_intent =
        port_connection_drag_intent_by_id(&view, source_port()).expect("source drag facts");
    let start = view
        .connection_start_from_port_drag_intent(
            drag_intent,
            EdgeId::from_u128(42),
            NodeGraphConnectionMode::Strict,
            CanvasPoint::default(),
            HandlePosition::Right,
            HandlePosition::Left,
            true,
        )
        .expect("current source facts")
        .expect("source permits connection");
    let sample = view
        .viewport_pointer_sample(ViewPoint::new(12.0, 18.0))
        .expect("bound pointer sample");

    let activated = session
        .activate_connection_search(start.clone(), sample)
        .expect("native activation enters search");
    let [CanvasInteractionEffect::Search(request)] = activated.effects() else {
        panic!("native activation must publish one search request");
    };
    assert_eq!(&request.token.binding, &start.binding());
    assert_eq!(request.view_position(), ViewPoint::new(12.0, 18.0));
    assert_eq!(request.canvas_position(), CanvasPoint { x: 12.0, y: 18.0 });
    assert_eq!(
        session.presentation().phase(),
        CanvasInteractionPhase::ConnectionSearch
    );
}

#[test]
fn connection_search_retry_revokes_the_previous_request_capability() {
    let (_runtime, view, mut session) = bound_test_session();
    let drag_intent =
        port_connection_drag_intent_by_id(&view, source_port()).expect("source drag facts");
    let start = view
        .connection_start_from_port_drag_intent(
            drag_intent,
            EdgeId::from_u128(42),
            NodeGraphConnectionMode::Strict,
            CanvasPoint::default(),
            HandlePosition::Right,
            HandlePosition::Left,
            true,
        )
        .expect("current source facts")
        .expect("source permits connection");
    let sample = view
        .viewport_pointer_sample(ViewPoint::new(12.0, 18.0))
        .expect("bound pointer sample");
    let opened = session
        .activate_connection_search(start, sample)
        .expect("native activation enters search");
    let [CanvasInteractionEffect::Search(first)] = opened.effects() else {
        panic!("native activation must publish one search request");
    };
    let first = first.clone();

    session
        .search_failed(&first)
        .expect("current search may report failure");
    assert_eq!(
        session
            .presentation()
            .connection()
            .and_then(|connection| connection.search()),
        Some(ConnectionSearchStatus::Error)
    );

    let retry = session
        .retry_search(&first)
        .expect("current request starts one successor attempt");
    let [CanvasInteractionEffect::Search(successor)] = retry.effects() else {
        panic!("retry must publish one successor request");
    };
    let successor = successor.clone();
    assert_ne!(successor, first);
    assert!(matches!(
        session.cancel_search(&first),
        Err(CanvasInteractionError::ConnectionSearchStaleGeneration)
    ));

    session
        .cancel_search(&successor)
        .expect("only the successor request may cancel search");
    assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
    assert_eq!(
        session.presentation().terminal(),
        Some(InteractionTerminalReason::Cancelled)
    );
}

#[test]
fn bound_owner_retirement_clears_connection_query_exactly_once() {
    let (runtime, view, mut session) = bound_test_session();
    let drag_intent =
        port_connection_drag_intent_by_id(&view, source_port()).expect("source drag facts");
    let start = view
        .connection_start_from_port_drag_intent(
            drag_intent,
            EdgeId::from_u128(42),
            NodeGraphConnectionMode::Strict,
            CanvasPoint::default(),
            HandlePosition::Right,
            HandlePosition::Left,
            false,
        )
        .expect("current source facts")
        .expect("source permits connection");
    let snapshot = runtime.snapshot().expect("owner snapshot");
    let source_port = snapshot
        .port_instance_key(source_port())
        .expect("source port lifetime");
    let source_node = snapshot
        .node_lifetime_key(source_node())
        .expect("source node lifetime");

    session
        .press(
            CanvasPressTarget::Connection(start),
            view.viewport_pointer_sample(ViewPoint::new(0.0, 0.0))
                .expect("press sample"),
        )
        .expect("arm connection");
    let requested = session
        .move_to(
            view.viewport_pointer_sample(ViewPoint::new(12.0, 4.0))
                .expect("move sample"),
        )
        .expect("schedule connection query");
    let owner = InteractionOwner::Port(source_port);
    assert!(requested.query_work_pending());
    assert!(session.uses_owner(owner));
    assert!(session.uses_owner(InteractionOwner::Node(source_node)));

    let retired = session.owner_retired(owner);
    assert!(!retired.query_work_pending());
    assert!(!session.query_work_pending());
    assert!(!session.uses_owner(owner));
    assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
    assert_eq!(
        session.presentation().terminal(),
        Some(InteractionTerminalReason::OwnerRetired)
    );

    let repeated = session.owner_retired(owner);
    assert_eq!(repeated.damage(), InteractionDamage::default());
    let settled = session.advance_query_work(1);
    assert_eq!(settled.receipt().pages_attempted(), 0);
    assert!(!settled.needs_tick());
    assert!(settled.terminal_error().is_none());
}

#[test]
fn attached_session_rejects_foreign_before_reprojecting_stale_samples_without_state_changes() {
    let runtime = GraphRuntime::builder(Default::default())
        .build(prospective_graph())
        .expect("sample authority runtime");
    let first = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("first view");
    let second = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("second view");
    let session = CanvasInteractionSession::attach(&first, CanvasInteractionConfig::default())
        .expect("attached interaction");
    let first_sample = first
        .viewport_pointer_sample(ViewPoint::new(20.0, 10.0))
        .unwrap();
    let foreign_sample = second
        .viewport_pointer_sample(ViewPoint::new(20.0, 10.0))
        .unwrap();
    let before = session_fingerprint(&session);

    first
        .set_viewport(ViewportTransform::new(CanvasPoint { x: 5.0, y: -2.0 }, 2.0).unwrap())
        .unwrap();
    second
        .set_viewport(ViewportTransform::new(CanvasPoint { x: -3.0, y: 4.0 }, 0.5).unwrap())
        .unwrap();

    let mut session = session;
    assert!(matches!(
        session.press(CanvasPressTarget::NativeControl, foreign_sample),
        Err(CanvasInteractionError::ForeignInteractionFacts)
    ));
    assert_eq!(session_fingerprint(&session), before);
    let stale = session
        .press(CanvasPressTarget::NativeControl, first_sample)
        .expect("same-view stale samples are reprojected internally");
    assert!(stale.effects().is_empty());
    assert!(!stale.query_work_pending());
    assert_eq!(session_fingerprint(&session), before);

    let current = first
        .viewport_pointer_sample(ViewPoint::new(20.0, 10.0))
        .unwrap();
    let (position, snapshot) = session
        .normalize_pointer_sample(current)
        .expect("current sample");
    assert_eq!(position.provenance(), first.provenance().unwrap());
    assert_eq!(position.viewport_revision(), snapshot.revision());
    assert_eq!(session_fingerprint(&session), before);
}

#[test]
fn foreign_marquee_baseline_is_rejected_before_arming() {
    let runtime = GraphRuntime::builder(Default::default())
        .build(prospective_graph())
        .expect("baseline runtime");
    let first = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("first view");
    let second = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("second view");
    let foreign_baseline = second.selection_baseline().expect("foreign baseline");
    let sample = first
        .viewport_pointer_sample(ViewPoint::new(4.0, 8.0))
        .expect("first-view pointer sample");
    let mut session = CanvasInteractionSession::attach(&first, CanvasInteractionConfig::default())
        .expect("attached interaction");

    assert!(matches!(
        session.press(
            CanvasPressTarget::Marquee {
                selection_mode: SelectionMode::Replace,
                baseline: foreign_baseline,
            },
            sample,
        ),
        Err(CanvasInteractionError::ForeignInteractionFacts)
    ));
    assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
}

#[test]
fn reprojected_position_is_resigned_to_the_exact_current_viewport_snapshot() {
    let runtime = GraphRuntime::builder(Default::default())
        .build(prospective_graph())
        .expect("reprojection runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("reprojection view");
    let sample = view
        .viewport_pointer_sample(ViewPoint::new(20.0, 10.0))
        .unwrap();
    let stale = InteractionPosition::from_sample(sample).unwrap();
    let current = view
        .set_viewport(ViewportTransform::new(CanvasPoint { x: 5.0, y: -2.0 }, 2.0).unwrap())
        .unwrap();

    let reprojected = stale
        .reproject_preserving_view(current)
        .expect("same-view reprojection");
    assert_eq!(reprojected.provenance(), current.provenance());
    assert_eq!(reprojected.viewport_revision(), current.revision());
    assert_eq!(reprojected.view(), ViewPoint::new(20.0, 10.0));
    assert_eq!(reprojected.canvas(), CanvasPoint { x: 5.0, y: 7.0 });

    let other = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("foreign view");
    assert!(
        stale
            .reproject_preserving_view(other.viewport_snapshot().unwrap())
            .is_none()
    );
}

#[test]
fn external_viewport_rebase_preserves_no_move_release_across_pointer_states() {
    for zoom in [0.5, 1.0, 2.0] {
        let external = ViewportTransform::new(CanvasPoint { x: 13.0, y: -7.0 }, zoom)
            .expect("finite external viewport");

        let (_runtime, view, mut armed) = bound_test_session();
        let start = node_drag_start_by_id(&view, source_node(), SelectionMode::Replace)
            .unwrap()
            .expect("draggable source node");
        let press = view
            .viewport_pointer_sample(ViewPoint::new(0.0, 0.0))
            .unwrap();
        armed
            .press(CanvasPressTarget::NodeDrag(start), press)
            .unwrap();
        let external_snapshot = view.set_viewport(external).unwrap();
        let released = armed.release(press).unwrap();
        assert!(released.effects().is_empty());
        assert_eq!(view.selection().unwrap().nodes(), [source_node()]);
        assert_eq!(armed.presentation().phase(), CanvasInteractionPhase::Idle);
        assert_eq!(view.viewport_snapshot().unwrap(), external_snapshot);

        let (_runtime, view, mut dragging) = bound_test_session();
        let start = node_drag_start_by_id(&view, source_node(), SelectionMode::Replace)
            .unwrap()
            .expect("draggable source node");
        let press = view
            .viewport_pointer_sample(ViewPoint::new(0.0, 0.0))
            .unwrap();
        let pointer = view
            .viewport_pointer_sample(ViewPoint::new(8.0, 0.0))
            .unwrap();
        dragging
            .press(CanvasPressTarget::NodeDrag(start), press)
            .unwrap();
        dragging.move_to(pointer).unwrap();
        view.set_viewport(external).unwrap();
        let released = dragging.release(pointer).unwrap();
        assert!(matches!(
            released.effects(),
            [CanvasInteractionEffect::SubmitGraph { .. }]
        ));
        let delta = dragging
            .presentation()
            .node_drag()
            .expect("committing drag presentation")
            .delta();
        assert!((delta.x - 8.0 / zoom).abs() <= f32::EPSILON * 8.0);
        assert_eq!(delta.y, 0.0);

        let (_runtime, view, mut marquee) = bound_test_session();
        let baseline = view.selection_baseline().unwrap();
        let press = view
            .viewport_pointer_sample(ViewPoint::new(0.0, 0.0))
            .unwrap();
        let pointer = view
            .viewport_pointer_sample(ViewPoint::new(8.0, 4.0))
            .unwrap();
        marquee
            .press(
                CanvasPressTarget::Marquee {
                    selection_mode: SelectionMode::Replace,
                    baseline,
                },
                press,
            )
            .unwrap();
        marquee.move_to(pointer).unwrap();
        view.set_viewport(external).unwrap();
        let released = marquee.release(pointer).unwrap();
        assert!(released.query_work_pending());
        assert!(marquee.query_work_pending());
        let rect = marquee
            .presentation()
            .marquee()
            .expect("release-pending marquee");
        assert!((rect.size.width - 8.0 / zoom).abs() <= f32::EPSILON * 8.0);
        assert!((rect.size.height - 4.0 / zoom).abs() <= f32::EPSILON * 4.0);

        let (_runtime, view, mut edge_pick) = bound_test_session();
        set_selection_by_id(&view, [source_node()], [], Vec::new()).unwrap();
        let press = view
            .viewport_pointer_sample(ViewPoint::new(4.0, 5.0))
            .unwrap();
        let pressed = edge_pick
            .press(
                CanvasPressTarget::Canvas(CanvasPressFallback::Pan {
                    selection_mode: SelectionMode::Replace,
                }),
                press,
            )
            .unwrap();
        let query = edge_query_id(&edge_pick, &pressed);
        let external_snapshot = view.set_viewport(external).unwrap();
        edge_pick.release(press).unwrap();
        let resolved = edge_pick
            .resolve_canvas_press_edge_resolution(query, CanvasEdgePickResolution::Complete(None))
            .unwrap();
        assert!(resolved.effects().is_empty());
        assert!(view.selection().unwrap().nodes().is_empty());
        assert!(!resolved.damage().viewport);
        assert_eq!(
            edge_pick.presentation().phase(),
            CanvasInteractionPhase::Idle
        );
        assert_eq!(view.viewport_snapshot().unwrap(), external_snapshot);
    }
}

#[test]
fn pan_move_invalid_projection_has_zero_side_effects() {
    let (_runtime, view, mut session) = bound_test_session();
    let press = view
        .viewport_pointer_sample(ViewPoint::new(-f32::MAX, 0.0))
        .unwrap();
    let moved = view
        .viewport_pointer_sample(ViewPoint::new(f32::MAX, 0.0))
        .unwrap();
    session.press(CanvasPressTarget::Pan, press).unwrap();
    let before = session_fingerprint(&session);
    let viewport_before = view.viewport_snapshot().unwrap();
    assert!(matches!(
        session.move_to(moved),
        Err(CanvasInteractionError::InvalidPosition)
    ));
    assert_eq!(session_fingerprint(&session), before);
    assert_eq!(view.viewport_snapshot().unwrap(), viewport_before);
}

#[test]
fn selection_subscriber_panic_retains_an_undelivered_graph_submission() {
    let runtime = GraphRuntime::builder(Default::default())
        .build(prospective_graph())
        .expect("selection panic runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("selection panic view");
    let mut session = CanvasInteractionSession::attach(
        &view,
        CanvasInteractionConfig {
            alignment_snap: None,
            ..CanvasInteractionConfig::default()
        },
    )
    .expect("selection panic interaction");
    let start = node_drag_start_by_id(&view, source_node(), SelectionMode::Replace)
        .expect("node-drag facts")
        .expect("draggable source node");
    let press = view
        .viewport_pointer_sample(ViewPoint::new(0.0, 0.0))
        .expect("press sample");
    let release = view
        .viewport_pointer_sample(ViewPoint::new(8.0, 0.0))
        .expect("release sample");
    session
        .press(CanvasPressTarget::NodeDrag(start), press)
        .expect("arm unselected node drag");
    view.subscribe(|event| {
        if matches!(
            event,
            crate::runtime::view_session::ViewSessionEvent::SelectionChanged { .. }
        ) {
            panic!("interaction selection subscriber panic");
        }
    })
    .expect("selection subscriber");

    let panic = catch_unwind(AssertUnwindSafe(|| session.release(release)));
    assert!(panic.is_err());
    assert_eq!(view.selection().unwrap().nodes(), [source_node()]);
    assert!(
        session.query_work_pending(),
        "the privately owned graph submission must remain deliverable after the callback panic"
    );

    let delivered = session.advance_query_work(1);
    assert!(delivered.terminal_error().is_none());
    assert!(!delivered.needs_tick());
    assert_eq!(delivered.receipt().pages_attempted(), 0);
    assert!(matches!(
        delivered.update().effects(),
        [CanvasInteractionEffect::SubmitGraph { .. }]
    ));
}

#[test]
fn create_from_wire_selection_cannot_cross_runtime_or_view() {
    let first_runtime = GraphRuntime::builder(Default::default())
        .build(prospective_graph())
        .expect("first runtime");
    let first_view = first_runtime
        .attach_view(ViewSessionOptions::default())
        .expect("first view");
    let second_runtime = GraphRuntime::builder(Default::default())
        .build(prospective_graph())
        .expect("second runtime");
    let second_view = second_runtime
        .attach_view(ViewSessionOptions::default())
        .expect("second view");
    let mut session =
        CanvasInteractionSession::attach(&first_view, CanvasInteractionConfig::default())
            .expect("interaction session");
    let drag_intent =
        port_connection_drag_intent_by_id(&first_view, source_port()).expect("source drag facts");
    let start = first_view
        .connection_start_from_port_drag_intent(
            drag_intent,
            EdgeId::from_u128(42),
            NodeGraphConnectionMode::Strict,
            CanvasPoint::default(),
            HandlePosition::Right,
            HandlePosition::Left,
            true,
        )
        .expect("facts belong to first view runtime")
        .expect("source permits connection");
    let pressed = bound_position(&session, &first_view, 0.0, 0.0);
    session
        .press(CanvasPressTarget::Connection(start), pressed)
        .unwrap();
    let moved_position = bound_position(&session, &first_view, 8.0, 0.0);
    let moved = session.move_to(moved_position).unwrap();
    resolve_bound_connection_query(&mut session, moved);
    let release_position = bound_position(&session, &first_view, 20.0, 0.0);
    let released = session.release(release_position).unwrap();
    resolve_bound_connection_query(&mut session, released);
    let request = session.search_request().expect("active search request");
    session.search_ready(&request, 1).unwrap();
    let second_session =
        CanvasInteractionSession::attach(&second_view, CanvasInteractionConfig::default())
            .expect("second interaction session");
    assert!(matches!(
        second_session.create_search_selection(&request, target_port(), prospective_creation(),),
        Err(CanvasInteractionError::ForeignInteractionSession { .. }),
    ));

    let local_selection = session
        .create_search_selection(&request, target_port(), prospective_creation())
        .expect("local view remains available")
        .expect("local topology is prospective");
    let committing = session
        .select_search_result(local_selection)
        .expect("local selection enters its bound session");
    let submission = take_submission(committing);
    session
        .resume_submission_after_prefix(submission)
        .expect("the session resumes through its originally attached view");
}

#[test]
fn zero_page_grant_preserves_runtime_owned_query_without_accessing_the_view() {
    let (_runtime, view, mut session) = bound_test_session();
    let issued = session
        .request_edge_pick(bound_position(&session, &view, 2.0, 2.0))
        .expect("issue runtime-owned query");
    let query = edge_query_id(&session, &issued);

    let (update, needs_tick, receipt, terminal_error) = session.advance_query_work(0).into_parts();
    assert_eq!(receipt.pages_granted(), 0);
    assert_eq!(receipt.pages_attempted(), 0);
    assert!(receipt.page_receipts().is_empty());
    assert!(terminal_error.is_none());
    assert!(needs_tick);
    assert!(update.query_work_pending());

    assert_eq!(edge_query_id(&session, &issued), query);
}

#[test]
fn stale_query_restart_cap_terminates_and_clears_runtime_owned_work() {
    let (_runtime, view, mut session) = bound_test_session();
    session
        .request_edge_pick(bound_position(&session, &view, 2.0, 2.0))
        .expect("issue runtime-owned edge-pick query");

    for expected_restarts in 1..=MAX_STALE_QUERY_RESTARTS {
        let stale = session
            .query_work
            .take()
            .expect("restartable query remains runtime-owned");
        session
            .restart_stale_query(stale.id())
            .expect("restartable stale query");
        assert_eq!(
            session
                .pending_edge_pick
                .expect("replacement edge-pick query")
                .stale_query_restarts,
            expected_restarts,
        );
        assert!(session.query_work.is_pending());
    }

    let exhausted = session
        .query_work
        .take()
        .expect("last permitted replacement remains runtime-owned");
    session
        .restart_stale_query(exhausted.id())
        .expect("restart exhaustion is a typed terminal result");

    assert_eq!(
        session.presentation().terminal(),
        Some(InteractionTerminalReason::BudgetExhausted),
    );
    assert!(session.pending_edge_pick.is_none());
    assert!(!session.query_work.is_pending());
}

#[test]
fn query_failure_terminal_distinguishes_protocol_transport_and_lifecycle_failures() {
    let session = InteractionSessionId(41);
    let query = InteractionQueryId(InteractionIdentity::new(session, 7));
    let other_query = InteractionQueryId(InteractionIdentity::new(session, 8));

    for error in [
        CanvasInteractionError::MissingInteractionQueryProof { query },
        CanvasInteractionError::MismatchedInteractionQueryProof {
            expected: query,
            actual: other_query,
        },
        CanvasInteractionError::ForeignInteractionFacts,
    ] {
        assert_eq!(
            query_failure_terminal(error),
            InteractionTerminalReason::ProtocolFailure
        );
    }
    assert_eq!(
        query_failure_terminal(CanvasInteractionError::ViewUnavailable),
        InteractionTerminalReason::TransportFailure
    );
    assert_eq!(
        query_failure_terminal(CanvasInteractionError::Closed),
        InteractionTerminalReason::ViewClosed
    );
}

#[test]
fn command_completion_kinds_map_to_their_public_terminal_reason() {
    for (kind, terminal) in [
        (
            CommandExecutionKind::Applied,
            InteractionTerminalReason::Applied,
        ),
        (CommandExecutionKind::Noop, InteractionTerminalReason::Noop),
        (
            CommandExecutionKind::Rejected,
            InteractionTerminalReason::Rejected,
        ),
        (
            CommandExecutionKind::Conflict,
            InteractionTerminalReason::Conflict,
        ),
        (
            CommandExecutionKind::Failed,
            InteractionTerminalReason::ExecutionFailed,
        ),
    ] {
        assert_eq!(completion_terminal(kind), terminal);
    }
}

#[test]
fn mismatched_and_foreign_query_receipts_fail_without_settling_current_work() {
    let runtime = GraphRuntime::builder(Default::default())
        .build(prospective_graph())
        .expect("query receipt runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("query receipt view");
    let foreign_view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("foreign receipt view");
    let mut session = CanvasInteractionSession::attach(&view, CanvasInteractionConfig::default())
        .expect("query receipt session");
    let requested = session
        .request_edge_pick(bound_position(&session, &view, 2.0, 3.0))
        .expect("current edge-pick query");
    let current = edge_query_id(&session, &requested);

    let direct_outcome = view
        .edge_pick(EdgePickQuery::new(ViewPoint::new(2.0, 3.0), 10.0, 1))
        .expect("direct query receipt");
    let before_missing_proof = session_fingerprint(&session);
    assert!(matches!(
        session.resolve_edge_pick_outcome(current, direct_outcome),
        Err(CanvasInteractionError::MissingInteractionQueryProof { query })
            if query == current
    ));
    assert_eq!(session_fingerprint(&session), before_missing_proof);

    let mismatched = session.allocate_query().expect("second query identity");
    let mismatched_outcome = view
        .edge_pick(
            EdgePickQuery::new(ViewPoint::new(2.0, 3.0), 10.0, 1)
                .bind_interaction(session.interaction_query_binding(mismatched)),
        )
        .expect("same-view mismatched receipt");
    let before_mismatch = session_fingerprint(&session);
    assert!(matches!(
        session.resolve_edge_pick_outcome(current, mismatched_outcome),
        Err(CanvasInteractionError::MismatchedInteractionQueryProof {
            expected,
            actual,
        }) if expected == current && actual == mismatched
    ));
    assert_eq!(session_fingerprint(&session), before_mismatch);
    assert_eq!(edge_query_id(&session, &requested), current);

    let foreign_binding = InteractionQueryBinding::new(
        foreign_view.provenance().expect("foreign view provenance"),
        current,
    );
    let foreign_outcome = foreign_view
        .edge_pick(
            EdgePickQuery::new(ViewPoint::new(2.0, 3.0), 10.0, 1).bind_interaction(foreign_binding),
        )
        .expect("foreign-view receipt");
    let before_foreign = session_fingerprint(&session);
    assert!(matches!(
        session.resolve_edge_pick_outcome(current, foreign_outcome),
        Err(CanvasInteractionError::ForeignInteractionFacts)
    ));
    assert_eq!(session_fingerprint(&session), before_foreign);
    assert_eq!(edge_query_id(&session, &requested), current);
}

#[test]
fn runtime_owned_query_work_is_isolated_per_session_and_cancel_clears_it() {
    let runtime = GraphRuntime::builder(Default::default())
        .build(prospective_graph())
        .expect("query receipt runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("query receipt view");
    let mut session = CanvasInteractionSession::attach(&view, CanvasInteractionConfig::default())
        .expect("query receipt session");
    let mut first_session =
        CanvasInteractionSession::attach(&view, CanvasInteractionConfig::default())
            .expect("first query session");
    let mut second_session =
        CanvasInteractionSession::attach(&view, CanvasInteractionConfig::default())
            .expect("second query session");
    let first_position = bound_position(&first_session, &view, 3.0, 0.0);
    let first_update = first_session.request_edge_pick(first_position).unwrap();
    let first_query = edge_query_id(&first_session, &first_update);
    let second_position = bound_position(&second_session, &view, 4.0, 0.0);
    let second_update = second_session.request_edge_pick(second_position).unwrap();
    let second_query = edge_query_id(&second_session, &second_update);
    assert_ne!(first_query, second_query);

    let (resolved, needs_tick, receipt, terminal_error) =
        second_session.advance_query_work(1).into_parts();
    assert_eq!(receipt.pages_granted(), 1);
    assert_eq!(receipt.pages_attempted(), 1);
    assert!(matches!(
        receipt.page_receipts(),
        [CanvasInteractionQueryPageReceipt::EdgePick(_)]
    ));
    assert!(!needs_tick);
    assert!(terminal_error.is_none());
    assert!(resolved.effects().is_empty());
    assert!(!second_session.query_work_pending());
    assert!(first_session.query_work_pending());
    assert_eq!(edge_query_id(&first_session, &first_update), first_query);

    let (resolved, needs_tick, receipt, terminal_error) =
        first_session.advance_query_work(1).into_parts();
    assert_eq!(receipt.pages_granted(), 1);
    assert_eq!(receipt.pages_attempted(), 1);
    assert!(matches!(
        receipt.page_receipts(),
        [CanvasInteractionQueryPageReceipt::EdgePick(_)]
    ));
    assert!(!needs_tick);
    assert!(terminal_error.is_none());
    assert!(resolved.effects().is_empty());
    assert!(!first_session.query_work_pending());

    let pending = session
        .request_edge_pick(bound_position(&session, &view, 2.0, 0.0))
        .unwrap();
    assert!(pending.query_work_pending());
    let cancelled = session.cancel().unwrap();
    assert!(!cancelled.query_work_pending());
    assert!(!session.query_work_pending());
    let before_advance = session_fingerprint(&session);
    let (stale, needs_tick, receipt, terminal_error) = session.advance_query_work(1).into_parts();
    assert_eq!(receipt.pages_granted(), 1);
    assert_eq!(receipt.pages_attempted(), 0);
    assert!(receipt.page_receipts().is_empty());
    assert!(!needs_tick);
    assert!(terminal_error.is_none());
    assert!(stale.effects().is_empty());
    assert!(!stale.query_work_pending());
    assert_eq!(session_fingerprint(&session), before_advance);
}

#[test]
fn settlement_protocol_errors_retain_bound_evidence_until_the_exact_completion_settles_once() {
    let (runtime, view, mut session) = bound_test_session();
    let submission = bound_nudge_submission(&view, &mut session);
    let mut bound = session
        .bind_submission(submission)
        .expect("current nudge submission binds");
    let command = bound
        .take_command()
        .expect("nudge submission emits a command");

    let version = runtime
        .snapshot()
        .expect("unrelated command snapshot")
        .version();
    let unrelated = runtime.execute(GraphCommand::set_node_position(
        version,
        source_node(),
        CanvasPoint { x: 7.0, y: 0.0 },
    ));
    assert_eq!(unrelated.kind(), CommandExecutionKind::Applied);

    let InteractionSubmissionSettlementResult::StillBound(failure) = session.settle_submission(
        bound,
        InteractionSubmissionSettlementEvidence::Command(unrelated),
    ) else {
        panic!("identity mismatch must return the exact capability and evidence");
    };
    let (retained, evidence, error) = failure.into_parts();
    assert!(matches!(
        evidence,
        InteractionSubmissionSettlementEvidence::Command(_)
    ));
    assert_eq!(
        error,
        CanvasInteractionError::CommandCompletionIdentityMismatch
    );
    bound = retained;
    assert_eq!(
        session.presentation().phase(),
        CanvasInteractionPhase::Committing
    );

    let InteractionSubmissionSettlementResult::StillBound(failure) =
        session.settle_submission(bound, InteractionSubmissionSettlementEvidence::NoCommand)
    else {
        panic!("phase mismatch must return the exact capability and evidence");
    };
    let (retained, evidence, error) = failure.into_parts();
    assert!(matches!(
        evidence,
        InteractionSubmissionSettlementEvidence::NoCommand
    ));
    assert_eq!(
        error,
        CanvasInteractionError::SubmissionCompletionStateMismatch
    );
    bound = retained;

    let completion = runtime.execute(command);
    assert_eq!(completion.kind(), CommandExecutionKind::Conflict);
    assert!(matches!(
        session.settle_submission(
            bound,
            InteractionSubmissionSettlementEvidence::Command(completion),
        ),
        InteractionSubmissionSettlementResult::Settled(_)
    ));
    assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
    assert_eq!(
        session.presentation().terminal(),
        Some(InteractionTerminalReason::Conflict)
    );

    let late = session
        .cancel()
        .expect("settled capability cannot settle twice");
    assert!(late.effects().is_empty());
    assert_eq!(late.damage(), InteractionDamage::default());
}

#[test]
fn dropping_bound_submission_before_dispatch_fails_closed_as_not_dispatched() {
    let (_runtime, view, mut session) = bound_test_session();
    let submission = bound_nudge_submission(&view, &mut session);
    let bound = session.bind_submission(submission).unwrap();
    assert_eq!(session.tick_demand(), InteractionTickDemand::WhileActive);

    drop(bound);
    session
        .tick(1.0 / 60.0, ViewSize::new(200.0, 120.0))
        .unwrap();

    assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
    assert_eq!(
        session.presentation().terminal(),
        Some(InteractionTerminalReason::TransportFailure)
    );
}

#[test]
fn outcome_unknown_requires_the_command_to_leave_the_bound_capability() {
    let (_runtime, view, mut session) = bound_test_session();
    let submission = bound_nudge_submission(&view, &mut session);
    let bound = session.bind_submission(submission).unwrap();

    let InteractionSubmissionSettlementResult::StillBound(failure) = session.settle_submission(
        bound,
        InteractionSubmissionSettlementEvidence::OutcomeUnknown,
    ) else {
        panic!("an unclaimed command cannot have an unknown transport outcome");
    };
    let (bound, evidence, error) = failure.into_parts();
    assert!(matches!(
        evidence,
        InteractionSubmissionSettlementEvidence::OutcomeUnknown
    ));
    assert_eq!(
        error,
        CanvasInteractionError::SubmissionCompletionStateMismatch
    );

    assert!(matches!(
        session.settle_submission(
            bound,
            InteractionSubmissionSettlementEvidence::NotDispatched,
        ),
        InteractionSubmissionSettlementResult::Settled(_)
    ));
    assert_eq!(
        session.presentation().terminal(),
        Some(InteractionTerminalReason::TransportFailure)
    );
}

#[test]
fn dropping_bound_submission_after_taking_command_fails_closed_as_outcome_unknown() {
    let (_runtime, view, mut session) = bound_test_session();
    let panic = catch_unwind(AssertUnwindSafe(|| {
        let submission = bound_nudge_submission(&view, &mut session);
        let mut bound = session.bind_submission(submission).unwrap();
        let command = bound.take_command().expect("nudge command");
        drop(command);
        panic!("transport panicked after taking the command");
    }));
    assert!(panic.is_err());
    assert_eq!(session.tick_demand(), InteractionTickDemand::WhileActive);
    session
        .tick(1.0 / 60.0, ViewSize::new(200.0, 120.0))
        .unwrap();

    assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
    assert_eq!(
        session.presentation().terminal(),
        Some(InteractionTerminalReason::Uncertain)
    );
}

#[test]
fn successful_bound_settlement_disarms_drop_without_a_second_terminal_transition() {
    let (runtime, view, mut session) = bound_test_session();
    let submission = bound_nudge_submission(&view, &mut session);
    let mut bound = session.bind_submission(submission).unwrap();
    let command = bound.take_command().expect("nudge command");
    let completion = runtime.execute(command);

    assert!(matches!(
        session.settle_submission(
            bound,
            InteractionSubmissionSettlementEvidence::Command(completion),
        ),
        InteractionSubmissionSettlementResult::Settled(_)
    ));
    let terminal = session.presentation().terminal();
    assert_eq!(session.tick_demand(), InteractionTickDemand::None);

    let late = session
        .tick(1.0 / 60.0, ViewSize::new(200.0, 120.0))
        .unwrap();
    assert!(late.effects().is_empty());
    assert_eq!(late.damage(), InteractionDamage::default());
    assert_eq!(session.presentation().terminal(), terminal);
}

#[test]
fn uncertain_connection_retry_binds_against_the_fresh_revision() {
    let (runtime, view, mut session) = bound_test_session();
    let submission = begin_bound_search_submission(&mut session, &view);
    let mut first_bound = session
        .bind_submission(submission)
        .expect("first submission binds");
    let first_ticket = first_bound.ticket();
    let first_command = first_bound
        .take_command()
        .expect("first submission emits a command");
    drop(first_command);
    assert!(matches!(
        session.settle_submission(
            first_bound,
            InteractionSubmissionSettlementEvidence::OutcomeUnknown,
        ),
        InteractionSubmissionSettlementResult::Settled(_)
    ));

    advance_source_position(&runtime, 1.0);
    session
        .reconcile_graph()
        .expect("later authoritative absence permits retry");
    let first_capability = match session.presentation().connection_settlement() {
        Some(ConnectionSettlementPresentation::Uncertain {
            capability,
            can_retry: true,
        }) => capability,
        settlement => panic!("expected retryable uncertainty, got {settlement:?}"),
    };

    let retry = session
        .retry_connection_settlement(first_capability)
        .expect("the exact capability starts a fresh attempt");
    let retry_submission = take_submission(retry);
    let mut retry_bound = session
        .bind_submission(retry_submission)
        .expect("retry submission binds");
    let retry_ticket = retry_bound.ticket();
    assert_ne!(retry_ticket, first_ticket);
    let retry_command = retry_bound
        .take_command()
        .expect("retry emits a command at the current revision");
    let completion = runtime.execute(retry_command);
    assert_eq!(completion.kind(), CommandExecutionKind::Applied);
    assert!(matches!(
        session.settle_submission(
            retry_bound,
            InteractionSubmissionSettlementEvidence::Command(completion),
        ),
        InteractionSubmissionSettlementResult::Settled(_)
    ));
    assert_eq!(
        session.presentation().connection_settlement(),
        Some(ConnectionSettlementPresentation::AwaitingPublication)
    );

    publish_connection_fixture_geometry(&view);
    session
        .reconcile_authoritative_state()
        .expect("the published route closes settlement");
    assert_eq!(
        session.presentation().terminal(),
        Some(InteractionTerminalReason::Applied)
    );
}

#[test]
fn cancelling_uncertainty_after_a_lost_applied_completion_waits_for_publication() {
    let (runtime, view, mut session) = bound_test_session();
    let submission = begin_bound_search_submission(&mut session, &view);
    let mut bound = session
        .bind_submission(submission)
        .expect("submission binds");
    let command = bound.take_command().expect("submission emits a command");
    assert!(matches!(
        session.settle_submission(
            bound,
            InteractionSubmissionSettlementEvidence::OutcomeUnknown,
        ),
        InteractionSubmissionSettlementResult::Settled(_)
    ));
    let capability = match session.presentation().connection_settlement() {
        Some(ConnectionSettlementPresentation::Uncertain { capability, .. }) => capability,
        settlement => panic!("expected uncertainty, got {settlement:?}"),
    };
    assert_eq!(
        runtime.execute(command).kind(),
        CommandExecutionKind::Applied
    );

    session
        .cancel_connection_settlement(capability)
        .expect("cancel first reconciles the authoritative graph");
    assert_eq!(
        session.presentation().connection_settlement(),
        Some(ConnectionSettlementPresentation::AwaitingPublication)
    );
    assert!(session.presentation().connection().is_some());

    publish_connection_fixture_geometry(&view);
    session
        .reconcile_authoritative_state()
        .expect("the published route settles the lost completion");
    assert_eq!(
        session.presentation().terminal(),
        Some(InteractionTerminalReason::Applied)
    );
}

#[test]
fn conflicted_retry_with_exact_authoritative_satisfaction_normalizes_to_applied() {
    let (runtime, view, mut session) = bound_test_session();
    let submission = begin_bound_search_submission(&mut session, &view);
    let mut first_bound = session
        .bind_submission(submission)
        .expect("first submission binds");
    let first_command = first_bound
        .take_command()
        .expect("first submission emits a command");
    drop(first_command);
    assert!(matches!(
        session.settle_submission(
            first_bound,
            InteractionSubmissionSettlementEvidence::OutcomeUnknown,
        ),
        InteractionSubmissionSettlementResult::Settled(_)
    ));

    advance_source_position(&runtime, 1.0);
    session
        .reconcile_graph()
        .expect("authoritative absence permits retry");
    let capability = match session.presentation().connection_settlement() {
        Some(ConnectionSettlementPresentation::Uncertain {
            capability,
            can_retry: true,
        }) => capability,
        settlement => panic!("expected retryable uncertainty, got {settlement:?}"),
    };
    let retry_submission = take_submission(
        session
            .retry_connection_settlement(capability)
            .expect("retry emits a fresh submission"),
    );
    let mut retry_bound = session
        .bind_submission(retry_submission)
        .expect("retry submission binds");
    let retry_command = retry_bound.take_command().expect("retry emits a command");

    assert_eq!(
        runtime.execute(retry_command.clone()).kind(),
        CommandExecutionKind::Applied
    );
    let conflict = runtime.execute(retry_command);
    assert_eq!(conflict.kind(), CommandExecutionKind::Conflict);
    assert!(matches!(
        session.settle_submission(
            retry_bound,
            InteractionSubmissionSettlementEvidence::Command(conflict),
        ),
        InteractionSubmissionSettlementResult::Settled(_)
    ));

    assert_eq!(
        session.presentation().connection_settlement(),
        Some(ConnectionSettlementPresentation::AwaitingPublication)
    );
    assert_eq!(
        session.presentation().terminal(),
        Some(InteractionTerminalReason::Applied)
    );
}

#[test]
fn connection_noop_consumes_the_already_published_exact_route() {
    let edge = EdgeId::from_u128(9);
    let runtime = GraphRuntime::builder(Default::default())
        .build(reconnect_graph(edge))
        .expect("noop reconnect runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("noop reconnect view");
    publish_connection_fixture_geometry(&view);
    let snapshot = runtime.snapshot().expect("noop reconnect snapshot");
    let intent = bound_reconnect_settlement_intent(&view, &snapshot, edge);

    let completion = runtime.execute(GraphCommand::reconnect(
        snapshot.version(),
        edge,
        EdgeEndpoint::To,
        target_port(),
        NodeGraphConnectionMode::Strict,
    ));
    let noop = match completion.result().expect("semantic reconnect outcome") {
        crate::runtime::commands::GraphCommandOutcome::Noop(noop) => noop,
        outcome => panic!("same-target reconnect must be a noop, got {outcome:?}"),
    };
    let owner = ConnectionSettlementOwner::awaiting_publication(intent, noop.snapshot())
        .expect("the noop snapshot proves the exact reconnect intent");

    assert!(
        owner.is_published(
            noop.snapshot(),
            &view
                .geometry_snapshot()
                .expect("already published reconnect geometry"),
        )
    );
}

#[test]
fn connection_publication_rejects_an_authoritative_snapshot_without_the_exact_intent() {
    let edge = EdgeId::from_u128(9);
    let runtime = GraphRuntime::builder(Default::default())
        .build(reconnect_graph(edge))
        .expect("publication mismatch runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("publication mismatch view");
    let connected = runtime.snapshot().expect("connected snapshot");
    runtime
        .execute(GraphCommand::disconnect(connected.version(), edge))
        .into_result()
        .expect("disconnect before publication mismatch")
        .applied()
        .expect("disconnect applies");
    let authoritative = runtime.snapshot().expect("authoritative mismatch snapshot");
    let target = authoritative
        .port_instance_key(target_port())
        .expect("target remains current");
    let intent = bound_connect_settlement_intent(&view, &authoritative, edge, target);

    assert!(
        ConnectionSettlementOwner::awaiting_publication(intent, &authoritative).is_none(),
        "an Applied/Noop projection must not wait forever when authoritative facts do not prove its exact edge",
    );
}

#[test]
fn uncertain_reconnect_rejects_a_same_id_edge_successor() {
    let edge = EdgeId::from_u128(9);
    let runtime = GraphRuntime::builder(Default::default())
        .build(reconnect_graph(edge))
        .expect("edge ABA runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("edge ABA view");
    let original = runtime.snapshot().expect("original edge snapshot");
    let original_edge = original
        .edge_instance_key(edge)
        .expect("original edge lifetime");
    let intent = bound_reconnect_settlement_intent(&view, &original, edge);
    let mut owner =
        ConnectionSettlementOwner::uncertain(intent).expect("uncertain reconnect owner");

    runtime
        .execute(GraphCommand::disconnect(original.version(), edge))
        .into_result()
        .expect("disconnect successor preparation")
        .applied()
        .expect("disconnect applies");
    let disconnected = runtime.snapshot().expect("disconnected snapshot");
    runtime
        .execute(GraphCommand::connect(
            disconnected.version(),
            edge,
            source_port(),
            target_port(),
            NodeGraphConnectionMode::Strict,
        ))
        .into_result()
        .expect("same-id successor connect")
        .applied()
        .expect("same-id successor applies");
    let successor = runtime.snapshot().expect("edge successor snapshot");
    assert_ne!(
        successor.edge_instance_key(edge),
        Some(original_edge),
        "the fixture must actually replace the edge lifetime",
    );

    assert_eq!(
        owner.reconcile(
            &successor,
            view.graph_policy().expect("edge ABA graph policy"),
        ),
        ConnectionSettlementReconciliation::OwnerRetired,
    );
}

#[test]
fn uncertain_new_connect_rejects_a_same_id_edge_successor() {
    let edge = EdgeId::from_u128(9);
    let runtime = GraphRuntime::builder(Default::default())
        .build(reconnect_graph(edge))
        .expect("new-edge ABA runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("new-edge ABA view");
    let connected = runtime.snapshot().expect("initial edge snapshot");
    runtime
        .execute(GraphCommand::disconnect(connected.version(), edge))
        .into_result()
        .expect("disconnect before uncertain connect")
        .applied()
        .expect("disconnect applies");
    let pending = runtime.snapshot().expect("pending new-edge snapshot");
    let target = pending
        .port_instance_key(target_port())
        .expect("pending target lifetime");
    let intent = bound_connect_settlement_intent(&view, &pending, edge, target);
    let mut owner = ConnectionSettlementOwner::uncertain(intent).expect("uncertain new edge");

    runtime
        .execute(GraphCommand::connect(
            pending.version(),
            edge,
            source_port(),
            target_port(),
            NodeGraphConnectionMode::Strict,
        ))
        .into_result()
        .expect("original uncertain edge installation")
        .applied()
        .expect("original uncertain edge applies");
    let installed = runtime.snapshot().expect("installed uncertain edge");
    let installed_edge = installed
        .edge_instance_key(edge)
        .expect("installed edge lifetime");
    runtime
        .execute(GraphCommand::disconnect(installed.version(), edge))
        .into_result()
        .expect("remove original uncertain edge")
        .applied()
        .expect("original uncertain edge removal applies");
    let removed = runtime.snapshot().expect("removed uncertain edge");
    runtime
        .execute(GraphCommand::connect(
            removed.version(),
            edge,
            source_port(),
            target_port(),
            NodeGraphConnectionMode::Strict,
        ))
        .into_result()
        .expect("same-id new-edge successor")
        .applied()
        .expect("same-id new-edge successor applies");
    let successor = runtime.snapshot().expect("new-edge successor snapshot");
    assert_ne!(successor.edge_instance_key(edge), Some(installed_edge));

    assert_eq!(
        owner.reconcile(
            &successor,
            view.graph_policy().expect("new-edge ABA graph policy"),
        ),
        ConnectionSettlementReconciliation::OwnerRetired,
    );
}

#[test]
fn uncertain_connect_rejects_a_same_id_target_port_successor() {
    let edge = EdgeId::from_u128(9);
    let runtime = GraphRuntime::builder(Default::default())
        .build(reconnect_graph(edge))
        .expect("port ABA runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("port ABA view");
    let connected = runtime.snapshot().expect("connected port snapshot");
    runtime
        .execute(GraphCommand::disconnect(connected.version(), edge))
        .into_result()
        .expect("disconnect before prospective connect")
        .applied()
        .expect("disconnect applies");
    let original = runtime.snapshot().expect("prospective port snapshot");
    let original_target = original
        .port_instance_key(target_port())
        .expect("original target lifetime");
    let intent = bound_connect_settlement_intent(&view, &original, edge, original_target);
    let mut owner = ConnectionSettlementOwner::uncertain(intent).expect("uncertain connect owner");

    runtime
        .execute(GraphCommand::topology(
            original.version(),
            TopologyEdit::new().replace_node_ports(
                NodeId::from_u128(2),
                [],
                IncidentEdgePolicy::Cascade,
            ),
        ))
        .into_result()
        .expect("remove target port")
        .applied()
        .expect("target-port removal applies");
    let removed = runtime.snapshot().expect("target port removed snapshot");
    runtime
        .execute(GraphCommand::topology(
            removed.version(),
            TopologyEdit::new().replace_node_ports(
                NodeId::from_u128(2),
                [TopologyPort::new(
                    target_port(),
                    test_port(NodeId::from_u128(2), "in", PortDirection::In),
                )],
                IncidentEdgePolicy::Reject,
            ),
        ))
        .into_result()
        .expect("re-add target port")
        .applied()
        .expect("target-port replacement applies");
    let replaced = runtime.snapshot().expect("target port successor snapshot");
    let successor_target = replaced
        .port_instance_key(target_port())
        .expect("successor target lifetime");
    assert_ne!(successor_target, original_target);
    runtime
        .execute(GraphCommand::connect(
            replaced.version(),
            edge,
            source_port(),
            target_port(),
            NodeGraphConnectionMode::Strict,
        ))
        .into_result()
        .expect("connect through target successor")
        .applied()
        .expect("successor connect applies");
    let successor = runtime.snapshot().expect("connected successor snapshot");

    assert_eq!(
        owner.reconcile(
            &successor,
            view.graph_policy().expect("port ABA graph policy"),
        ),
        ConnectionSettlementReconciliation::OwnerRetired,
    );
}

#[test]
fn create_from_wire_reconnect_preserves_the_preexisting_edge_identity() {
    let edge = EdgeId::from_u128(9);
    let created_node = NodeId::from_u128(3);
    let created_port = PortId::from_u128(3);
    let runtime = GraphRuntime::builder(Default::default())
        .build(reconnect_graph(edge))
        .expect("create-from-wire reconnect runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("create-from-wire reconnect view");
    let before = runtime.snapshot().expect("preexisting reconnect graph");
    let original_edge = before
        .edge_instance_key(edge)
        .expect("preexisting edge lifetime");
    let fixed = before
        .port_instance_key(source_port())
        .expect("fixed source lifetime");
    let detached = before
        .port_instance_key(target_port())
        .expect("detached target lifetime");
    let binding = ConnectionGestureBinding::reconnect(
        original_edge,
        EdgeEndpoint::To,
        detached,
        fixed,
        NodeGraphConnectionMode::Strict,
    )
    .expect("exact reconnect binding");
    let mut node = test_node(CanvasPoint { x: 200.0, y: 0.0 });
    node.ports = vec![created_port];
    let creation = NodeCreation::from_parts_for_test(
        created_node,
        node,
        vec![(
            created_port,
            test_port(created_node, "in", PortDirection::In),
        )],
    );
    let target =
        ConnectionCommitTarget::from_node_creation(&before, created_port, creation.clone())
            .expect("prospective reconnect target");
    let intent = ConnectionSettlementIntent::new(
        view.provenance().expect("reconnect provenance"),
        binding,
        target,
        before.revision(),
    );
    let edit = creation
        .into_topology_edit()
        .connection(ConnectionEdit::reconnect(
            edge,
            EdgeEndpoint::To,
            created_port,
            NodeGraphConnectionMode::Strict,
        ));
    let result = runtime
        .execute(GraphCommand::topology(before.version(), edit))
        .into_result()
        .expect("create-from-wire reconnect executes");
    let applied = result
        .applied()
        .expect("create-from-wire reconnect applies");
    let installed = applied.patch().snapshot();

    assert_eq!(installed.edge_instance_key(edge), Some(original_edge));
    assert!(
        ConnectionSettlementOwner::awaiting_publication(intent, installed)
            .expect("the applied snapshot proves the exact reconnect intent")
            .publication_proof()
            .is_some(),
        "reconnect settlement must not require the preexisting edge to be born with the new node",
    );
}

#[test]
fn uncertain_connect_accepts_later_satisfaction_without_issuing_a_retry() {
    let edge = EdgeId::from_u128(9);
    let runtime = GraphRuntime::builder(Default::default())
        .build(reconnect_graph(edge))
        .expect("later satisfaction runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("later satisfaction view");
    let connected = runtime.snapshot().expect("initial connected snapshot");
    runtime
        .execute(GraphCommand::disconnect(connected.version(), edge))
        .into_result()
        .expect("disconnect before uncertain connect")
        .applied()
        .expect("disconnect applies");
    let pending = runtime.snapshot().expect("uncertain connect snapshot");
    let target = pending
        .port_instance_key(target_port())
        .expect("uncertain target lifetime");
    let intent = bound_connect_settlement_intent(&view, &pending, edge, target);
    let mut owner = ConnectionSettlementOwner::uncertain(intent).expect("uncertain connect owner");

    runtime
        .execute(GraphCommand::connect(
            pending.version(),
            edge,
            source_port(),
            target_port(),
            NodeGraphConnectionMode::Strict,
        ))
        .into_result()
        .expect("late authoritative connect")
        .applied()
        .expect("late authoritative connect applies");
    let satisfied = runtime.snapshot().expect("later satisfied snapshot");
    assert_eq!(
        owner.reconcile(
            &satisfied,
            view.graph_policy()
                .expect("later satisfaction graph policy"),
        ),
        ConnectionSettlementReconciliation::AwaitingPublication,
    );
    assert!(!owner.can_retry());
    assert!(
        !owner.is_published(
            &satisfied,
            &view
                .geometry_snapshot()
                .expect("unpublished satisfied geometry"),
        )
    );

    publish_connection_fixture_geometry(&view);
    assert!(
        owner.is_published(
            &satisfied,
            &view
                .geometry_snapshot()
                .expect("published satisfied geometry"),
        )
    );
}

#[test]
fn hidden_final_route_is_published_by_exact_authoritative_absence() {
    let edge = EdgeId::from_u128(9);
    let runtime = GraphRuntime::builder(Default::default())
        .build(hidden_reconnect_graph(edge))
        .expect("hidden-route runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("hidden-route view");
    let snapshot = runtime.snapshot().expect("hidden-route snapshot");
    let intent = bound_reconnect_settlement_intent(&view, &snapshot, edge);
    let owner = ConnectionSettlementOwner::awaiting_publication(intent, &snapshot)
        .expect("the hidden edge still proves the exact reconnect intent");
    let geometry = view
        .geometry_snapshot()
        .expect("hidden route geometry snapshot");

    assert!(geometry.route(edge).is_none());
    assert!(owner.is_published(&snapshot, &geometry));
}

#[test]
fn reconnect_start_resolves_graph_topology_and_endpoint_specific_route_facts() {
    let edge = EdgeId::from_u128(22);
    let snapshot = graph_snapshot(reconnect_graph(edge));
    let route = published_edge_route_for_snapshot(
        &snapshot,
        edge,
        true,
        EdgeReconnectable::Endpoint(EdgeReconnectableEndpoint::Source),
    );
    let source = ConnectionInteractionStart::reconnect_from_graph_route(
        ViewProvenance::unavailable(),
        &snapshot,
        &route,
        EdgeEndpoint::From,
        route.facts().endpoints.target,
        NodeGraphConnectionMode::Loose,
        false,
    )
    .expect("source reconnect is permitted");
    assert_eq!(source.owner_node(), source_node());
    assert_eq!(source.binding().fixed_port_instance().port(), target_port());
    assert_eq!(
        source
            .binding()
            .detached_port_instance()
            .map(|port| port.port()),
        Some(source_port())
    );
    assert_eq!(source.binding().mode(), NodeGraphConnectionMode::Loose);

    let explicit_fixed = EdgeEndpointPosition {
        point: CanvasPoint { x: 77.0, y: 88.0 },
        position: HandlePosition::Bottom,
    };
    let explicit = ConnectionInteractionStart::reconnect_from_graph_route(
        ViewProvenance::unavailable(),
        &snapshot,
        &route,
        EdgeEndpoint::From,
        explicit_fixed,
        NodeGraphConnectionMode::Loose,
        false,
    )
    .expect("source reconnect accepts adapter-resolved fixed geometry");
    assert_eq!(explicit.fixed_anchor, explicit_fixed.point);
    assert_eq!(explicit.fixed_outward, explicit_fixed.position);

    assert!(
        ConnectionInteractionStart::reconnect_from_graph_route(
            ViewProvenance::unavailable(),
            &snapshot,
            &route,
            EdgeEndpoint::To,
            route.facts().endpoints.source,
            NodeGraphConnectionMode::Strict,
            false,
        )
        .is_none()
    );
    let missing = published_edge_route(EdgeId::from_u128(23), true, EdgeReconnectable::Bool(true));
    assert!(
        ConnectionInteractionStart::reconnect_from_graph_route(
            ViewProvenance::unavailable(),
            &snapshot,
            &missing,
            EdgeEndpoint::From,
            missing.facts().endpoints.target,
            NodeGraphConnectionMode::Strict,
            false,
        )
        .is_none()
    );
}

#[test]
fn auto_pan_caps_long_frames_and_does_not_pan_from_a_narrow_viewport_center() {
    let long_frame = auto_pan_delta(
        ViewPoint::new(0.0, 0.0),
        ViewSize::new(200.0, 120.0),
        36.0,
        720.0,
        MAX_AUTO_PAN_ELAPSED_SECONDS,
    );
    assert_eq!(long_frame, ViewDelta { x: 36.0, y: 36.0 });
    assert_eq!(auto_pan_axis(10.0, 20.0, 36.0), 0.0);
    assert_eq!(auto_pan_axis(0.0, 20.0, 36.0), 1.0);
    assert_eq!(auto_pan_axis(20.0, 20.0, 36.0), -1.0);
}

#[test]
fn drag_threshold_crosses_only_beyond_the_exact_view_distance() {
    assert!(!threshold_crossed_distance(
        view_distance_squared(ViewPoint::new(0.0, 0.0), ViewPoint::new(4.0, 0.0)),
        4.0,
    ));
    assert!(threshold_crossed_distance(
        view_distance_squared(ViewPoint::new(0.0, 0.0), ViewPoint::new(4.01, 0.0)),
        4.0,
    ));
}

fn bound_test_session() -> (GraphRuntime, ViewSession, CanvasInteractionSession) {
    let runtime = GraphRuntime::builder(Default::default())
        .build(prospective_graph())
        .expect("bound interaction runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("bound interaction view");
    let session = CanvasInteractionSession::attach(&view, CanvasInteractionConfig::default())
        .expect("bound interaction session");
    (runtime, view, session)
}

fn begin_bound_search_submission(
    session: &mut CanvasInteractionSession,
    view: &ViewSession,
) -> InteractionGraphSubmission {
    let start = bound_connection_start(view);
    let position = bound_position(session, view, 0.0, 0.0);
    session
        .activate_connection_search(start, position)
        .expect("bound search activation");
    let request = session.search_request().expect("bound search request");
    session
        .search_ready(&request, 1)
        .expect("bound search becomes ready");
    let selection = session
        .create_search_selection(&request, target_port(), prospective_creation())
        .expect("bound search selection query")
        .expect("bound search selection");
    take_submission(
        session
            .select_search_result(selection)
            .expect("bound search selection submits"),
    )
}

fn bound_connection_start(view: &ViewSession) -> ConnectionInteractionStart {
    let drag_intent =
        port_connection_drag_intent_by_id(view, source_port()).expect("bound source facts");
    view.connection_start_from_port_drag_intent(
        drag_intent,
        EdgeId::from_u128(9),
        NodeGraphConnectionMode::Strict,
        CanvasPoint { x: 0.0, y: 8.0 },
        HandlePosition::Right,
        HandlePosition::Left,
        true,
    )
    .expect("bound connection start query")
    .expect("bound source permits connection")
}

fn advance_source_position(runtime: &GraphRuntime, x: f32) {
    let snapshot = runtime.snapshot().expect("source-position snapshot");
    runtime
        .execute(GraphCommand::set_node_position(
            snapshot.version(),
            source_node(),
            CanvasPoint { x, y: 0.0 },
        ))
        .into_result()
        .expect("source-position command executes")
        .applied()
        .expect("source-position command applies");
}

fn bound_nudge_submission(
    view: &ViewSession,
    session: &mut CanvasInteractionSession,
) -> InteractionGraphSubmission {
    set_selection_by_id(view, [source_node()], [], Vec::new())
        .expect("select source node for nudge");
    take_submission(
        session
            .nudge_selection(NodeNudgeDirection::Right, false)
            .expect("selected source emits one nudge submission"),
    )
}

fn published_edge_route(
    edge: EdgeId,
    deletable: bool,
    reconnectable: EdgeReconnectable,
) -> PublishedEdgeRoute {
    let mut model = Edge::new(EdgeKind::Data, source_port(), target_port());
    model.deletable = Some(deletable);
    model.reconnectable = Some(reconnectable);
    let endpoints = EdgePosition {
        source: EdgeEndpointPosition {
            point: CanvasPoint { x: 0.0, y: 0.0 },
            position: HandlePosition::Right,
        },
        target: EdgeEndpointPosition {
            point: CanvasPoint { x: 100.0, y: 0.0 },
            position: HandlePosition::Left,
        },
    };
    let facts = resolve_edge_route_path(&model, endpoints).expect("test edge route");
    PublishedEdgeRoute::new_for_test(edge, facts)
}

fn published_edge_route_for_snapshot(
    snapshot: &GraphSnapshot,
    edge: EdgeId,
    deletable: bool,
    reconnectable: EdgeReconnectable,
) -> PublishedEdgeRoute {
    let mut model = Edge::new(EdgeKind::Data, source_port(), target_port());
    model.deletable = Some(deletable);
    model.reconnectable = Some(reconnectable);
    let endpoints = EdgePosition {
        source: EdgeEndpointPosition {
            point: CanvasPoint { x: 0.0, y: 0.0 },
            position: HandlePosition::Right,
        },
        target: EdgeEndpointPosition {
            point: CanvasPoint { x: 100.0, y: 0.0 },
            position: HandlePosition::Left,
        },
    };
    let facts = resolve_edge_route_path(&model, endpoints).expect("test edge route");
    PublishedEdgeRoute::new(
        ViewProvenance::new(
            ViewId::new(snapshot.provenance().runtime_id(), 0),
            ViewGeneration::initial(),
        ),
        snapshot
            .edge_instance_key(edge)
            .expect("fixture edge lifetime"),
        snapshot
            .port_instance_key(source_port())
            .expect("fixture source-port lifetime"),
        snapshot
            .port_instance_key(target_port())
            .expect("fixture target-port lifetime"),
        facts,
    )
    .expect("fixture view and edge provenance match")
}

fn reconnect_graph(edge: EdgeId) -> Graph {
    let mut source = test_node(CanvasPoint { x: 0.0, y: 0.0 });
    source.ports.push(source_port());
    let mut target = test_node(CanvasPoint { x: 100.0, y: 0.0 });
    target.ports.push(target_port());
    GraphBuilder::new(GraphId::from_u128(1))
        .with_node(source_node(), source)
        .with_port(
            source_port(),
            test_port(source_node(), "out", PortDirection::Out),
        )
        .with_node(NodeId::from_u128(2), target)
        .with_port(
            target_port(),
            test_port(NodeId::from_u128(2), "in", PortDirection::In),
        )
        .with_edge(
            edge,
            Edge::new(EdgeKind::Data, source_port(), target_port()),
        )
        .build()
        .expect("valid reconnect graph")
}

fn hidden_reconnect_graph(edge: EdgeId) -> Graph {
    let mut source = test_node(CanvasPoint { x: 0.0, y: 0.0 });
    source.ports.push(source_port());
    let mut target = test_node(CanvasPoint { x: 100.0, y: 0.0 });
    target.ports.push(target_port());
    let mut hidden_edge = Edge::new(EdgeKind::Data, source_port(), target_port());
    hidden_edge.hidden = true;
    GraphBuilder::new(GraphId::from_u128(1))
        .with_node(source_node(), source)
        .with_port(
            source_port(),
            test_port(source_node(), "out", PortDirection::Out),
        )
        .with_node(NodeId::from_u128(2), target)
        .with_port(
            target_port(),
            test_port(NodeId::from_u128(2), "in", PortDirection::In),
        )
        .with_edge(edge, hidden_edge)
        .build()
        .expect("valid hidden reconnect graph")
}

fn publish_connection_fixture_geometry(view: &ViewSession) {
    let frame = view
        .frame(FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint {
                    x: -100.0,
                    y: -100.0,
                },
                size: CanvasSize {
                    width: 400.0,
                    height: 300.0,
                },
            },
            RendererStamp::new("test.node", 1),
        ))
        .expect("connection geometry frame");
    let collector = frame.collector();
    for node in frame.nodes() {
        let source = node.id() == source_node();
        let node_x = if source { 0.0 } else { 100.0 };
        let outward = if source {
            HandlePosition::Right
        } else {
            HandlePosition::Left
        };
        let node_bounds = ViewRect::new(ViewPoint::new(node_x, 0.0), ViewSize::new(80.0, 80.0));
        let port_bounds = ViewRect::new(
            ViewPoint::new(if source { 74.0 } else { 94.0 }, 34.0),
            ViewSize::new(12.0, 12.0),
        );
        let route_anchor = ViewPoint::new(if source { 80.0 } else { 100.0 }, 40.0);
        let [port] = node.ports() else {
            panic!("connection fixture nodes each expose one port");
        };
        let mut layout = node.begin_layout(node_bounds);
        layout
            .port_rendered(
                port.layout_target(),
                port_bounds,
                port_bounds,
                route_anchor,
                outward,
            )
            .expect("connection port geometry");
        collector
            .layout(layout.finish().expect("complete connection node layout"))
            .expect("collect connection node layout");
    }
    view.submit_feedback(
        collector
            .finish()
            .expect("complete connection geometry feedback"),
    )
    .expect("publish connection route geometry");
}

fn test_node(pos: CanvasPoint) -> Node {
    Node {
        kind: NodeKindKey::new("test.node"),
        kind_version: 1,
        pos,
        origin: None,
        selectable: None,
        draggable: None,
        connectable: None,
        deletable: None,
        parent: None,
        extent: None,
        expand_parent: None,
        size: Some(CanvasSize {
            width: 160.0,
            height: 96.0,
        }),
        hidden: false,
        collapsed: false,
        ports: Vec::new(),
        data: serde_json::Value::Null,
    }
}

fn test_port(node: NodeId, key: &str, dir: PortDirection) -> Port {
    Port {
        node,
        key: PortKey::new(key),
        dir,
        kind: PortKind::Data,
        capacity: PortCapacity::Multi,
        connectable: None,
        connectable_start: None,
        connectable_end: None,
        ty: None,
        data: serde_json::Value::Null,
    }
}

fn bound_reconnect_settlement_intent(
    view: &ViewSession,
    snapshot: &GraphSnapshot,
    edge: EdgeId,
) -> ConnectionSettlementIntent {
    let source = snapshot
        .port_instance_key(source_port())
        .expect("settlement source lifetime");
    let target = snapshot
        .port_instance_key(target_port())
        .expect("settlement target lifetime");
    let binding = ConnectionGestureBinding::reconnect(
        snapshot
            .edge_instance_key(edge)
            .expect("settlement edge lifetime"),
        EdgeEndpoint::To,
        target,
        source,
        NodeGraphConnectionMode::Strict,
    )
    .expect("exact reconnect binding");
    let start = ConnectionInteractionStart::new_with_provenance(
        view.provenance().expect("settlement view provenance"),
        snapshot.version(),
        binding,
        EdgeEndpoint::From,
        CanvasPoint { x: 80.0, y: 40.0 },
        HandlePosition::Right,
        HandlePosition::Left,
        false,
    )
    .expect("bound reconnect settlement start");
    ConnectionSettlementIntent::new(
        start.provenance,
        start.binding,
        ConnectionCommitTarget::Existing(target),
        snapshot.revision(),
    )
}

fn bound_connect_settlement_intent(
    view: &ViewSession,
    snapshot: &GraphSnapshot,
    edge: EdgeId,
    target: PortInstanceKey,
) -> ConnectionSettlementIntent {
    let source = snapshot
        .port_instance_key(source_port())
        .expect("settlement source lifetime");
    let binding = ConnectionGestureBinding::connect(
        source,
        EdgeEndpoint::From,
        edge,
        NodeGraphConnectionMode::Strict,
    );
    let start = ConnectionInteractionStart::new_with_provenance(
        view.provenance().expect("settlement view provenance"),
        snapshot.version(),
        binding,
        EdgeEndpoint::From,
        CanvasPoint { x: 80.0, y: 40.0 },
        HandlePosition::Right,
        HandlePosition::Left,
        false,
    )
    .expect("bound connect settlement start");
    ConnectionSettlementIntent::new(
        start.provenance,
        start.binding,
        ConnectionCommitTarget::Existing(target),
        snapshot.revision(),
    )
}

fn bound_position(
    session: &CanvasInteractionSession,
    view: &ViewSession,
    x: f32,
    y: f32,
) -> ViewportPointerSample {
    let _ = session;
    view.viewport_pointer_sample(ViewPoint::new(x, y))
        .expect("bound viewport sample")
}

#[derive(Debug, PartialEq, Eq)]
struct SessionFingerprint {
    next_generation: Option<u64>,
    next_query: Option<u64>,
    next_ticket: Option<u64>,
    next_search_attempt: Option<u64>,
    state: String,
    pending_edge_pick: Option<InteractionQueryId>,
    hovered_edge: Option<EdgeInstanceKey>,
    terminal: Option<InteractionTerminalReason>,
    query_work_pending: bool,
    submission_binding: Option<(InteractionTicketId, &'static str)>,
    pending_selection_transition: bool,
    lifecycle: InteractionSessionLifecycle,
}

fn session_fingerprint(session: &CanvasInteractionSession) -> SessionFingerprint {
    SessionFingerprint {
        next_generation: session.next_generation,
        next_query: session.next_query,
        next_ticket: session.next_ticket,
        next_search_attempt: session.next_search_attempt,
        state: format!("{:?}", session.state),
        pending_edge_pick: session.pending_edge_pick.map(|pending| pending.id),
        hovered_edge: session.hovered_edge,
        terminal: session.terminal,
        query_work_pending: session.query_work.is_pending(),
        submission_binding: session.submission_binding.as_ref().map(|binding| {
            let phase = match binding.phase {
                SubmissionBindingPhase::Pending => "pending",
                SubmissionBindingPhase::NoCommand => "no-command",
                SubmissionBindingPhase::Command(_) => "command",
            };
            (binding.ticket, phase)
        }),
        pending_selection_transition: session.pending_selection_transition.is_some(),
        lifecycle: session.lifecycle,
    }
}

fn prospective_creation() -> NodeCreation {
    let node = NodeId::from_u128(2);
    let mut model = test_node(CanvasPoint { x: 100.0, y: 0.0 });
    model.ports = vec![target_port()];
    NodeCreation::from_parts_for_test(
        node,
        model,
        vec![(target_port(), test_port(node, "in", PortDirection::In))],
    )
}

fn prospective_graph() -> Graph {
    let mut source = test_node(CanvasPoint { x: 0.0, y: 0.0 });
    source.ports.push(source_port());
    GraphBuilder::new(GraphId::from_u128(1))
        .with_node(source_node(), source)
        .with_port(
            source_port(),
            test_port(source_node(), "out", PortDirection::Out),
        )
        .build()
        .expect("valid prospective graph")
}

fn graph_snapshot(graph: Graph) -> GraphSnapshot {
    GraphRuntime::builder(Default::default())
        .build(graph)
        .expect("test graph runtime")
        .snapshot()
        .expect("test graph snapshot")
}

fn resolve_bound_connection_query(
    session: &mut CanvasInteractionSession,
    update: CanvasInteractionUpdate,
) -> CanvasInteractionUpdate {
    let dispatch = update.into_dispatch();
    assert!(dispatch.effects.is_empty());
    assert!(dispatch.query_work_pending);
    let (update, needs_tick, receipt, terminal_error) = session.advance_query_work(1).into_parts();
    assert_eq!(receipt.pages_granted(), 1);
    assert_eq!(receipt.pages_attempted(), 1);
    assert!(matches!(
        receipt.page_receipts(),
        [CanvasInteractionQueryPageReceipt::ConnectionProbe(_)]
    ));
    assert!(!needs_tick);
    assert!(terminal_error.is_none());
    update
}

fn edge_query_id(
    session: &CanvasInteractionSession,
    update: &CanvasInteractionUpdate,
) -> InteractionQueryId {
    assert!(update.query_work_pending());
    if let InteractionState::EdgePickPending(pending) = &session.state {
        return pending.query;
    }
    session
        .pending_edge_pick
        .expect("expected one edge-pick query")
        .id
}

fn take_submission(update: CanvasInteractionUpdate) -> InteractionGraphSubmission {
    let mut effects = update.into_dispatch().effects.into_iter();
    let Some(CanvasInteractionEffect::SubmitGraph { submission }) = effects.next() else {
        panic!("expected one graph submission");
    };
    assert!(
        effects.next().is_none(),
        "expected exactly one graph submission"
    );
    submission
}

fn source_node() -> NodeId {
    NodeId::from_u128(1)
}

fn source_port() -> PortId {
    PortId::from_u128(1)
}

fn target_port() -> PortId {
    PortId::from_u128(2)
}
