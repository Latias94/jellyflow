use jellyflow_core::core::{
    CanvasPoint, CanvasSize, Edge, EdgeId, EdgeKind, EdgeReconnectable, EdgeReconnectableEndpoint,
    Graph, GraphBuilder, GraphId, Node, NodeId, NodeKindKey, Port, PortCapacity, PortDirection,
    PortId, PortKey, PortKind,
};
use jellyflow_core::interaction::NodeGraphConnectionMode;

use super::*;
use crate::rules::EdgeEndpoint;
use crate::runtime::commands::GraphCommand;
use crate::runtime::geometry::{
    EdgeEndpointPosition, EdgePosition, HandlePosition, resolve_edge_route_path,
};
use crate::runtime::graph_runtime::GraphRuntime;
use crate::runtime::interaction::AlignmentSnapWork;
use crate::runtime::interaction::ConnectionProbeTarget;
use crate::runtime::tests::fixtures::{
    node_drag_start_by_id, port_connection_drag_intent_by_id, set_selection_by_id,
};
use crate::runtime::view_session::{
    ConnectionProbeWork, EdgePickQueryOutcome, EdgePickWork, ViewSessionOptions,
};

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
    assert!(stale.queries().is_empty());
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
fn bound_authority_busy_closed_and_foreign_fail_without_session_side_effects() {
    let runtime = GraphRuntime::builder(Default::default())
        .build(prospective_graph())
        .expect("authority runtime");
    let first = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("first view");
    let second = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("second view");
    let sample = first
        .viewport_pointer_sample(ViewPoint::new(4.0, 8.0))
        .unwrap();
    let mut session = CanvasInteractionSession::attach(&first, CanvasInteractionConfig::default())
        .expect("attached interaction");
    let before = session_fingerprint(&session);

    first.with_state_borrowed_for_test(|| {
        assert!(matches!(
            session.press(CanvasPressTarget::NativeControl, sample),
            Err(CanvasInteractionError::ViewUnavailable)
        ));
    });
    assert_eq!(session_fingerprint(&session), before);

    let mut foreign_bound =
        CanvasInteractionSession::attach(&first, CanvasInteractionConfig::default())
            .expect("foreign capability probe");
    foreign_bound.bound_view = second.downgrade();
    let foreign_before = session_fingerprint(&foreign_bound);
    assert!(matches!(
        foreign_bound.set_config(CanvasInteractionConfig {
            edge_pick_radius_in_view: 11.0,
            ..CanvasInteractionConfig::default()
        }),
        Err(CanvasInteractionError::ForeignInteractionFacts)
    ));
    assert_eq!(session_fingerprint(&foreign_bound), foreign_before);

    first.close().expect("close first view");
    assert!(matches!(
        session.press(CanvasPressTarget::NativeControl, sample),
        Err(CanvasInteractionError::ViewUnavailable)
    ));
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
fn selection_commit_failure_has_explicit_press_query_and_marquee_policies() {
    let edge = EdgeId::from_u128(0x51ec_7100);
    let runtime = GraphRuntime::builder(Default::default())
        .build(reconnect_graph(edge))
        .expect("selection failure runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("selection failure view");
    let instance = runtime
        .snapshot()
        .expect("selection failure snapshot")
        .edge_instance_key(edge)
        .expect("fixture edge instance");
    let press = view
        .viewport_pointer_sample(ViewPoint::new(4.0, 8.0))
        .expect("press sample");

    let mut direct = CanvasInteractionSession::attach(&view, CanvasInteractionConfig::default())
        .expect("direct edge interaction");
    direct.selection_commit_failure = Some(CanvasInteractionError::ViewUnavailable);
    assert!(matches!(
        direct.press(
            CanvasPressTarget::Edge {
                instance,
                selection_mode: SelectionMode::Replace,
            },
            press,
        ),
        Err(CanvasInteractionError::ViewUnavailable)
    ));
    assert_eq!(direct.presentation().phase(), CanvasInteractionPhase::Idle);
    assert!(direct.presentation().hovered_edge().is_none());
    assert!(view.selection().unwrap().edges().is_empty());

    let mut edge_pick = CanvasInteractionSession::attach(&view, CanvasInteractionConfig::default())
        .expect("edge-pick interaction");
    let pressed = edge_pick
        .press(
            CanvasPressTarget::Canvas(CanvasPressFallback::Pan {
                selection_mode: SelectionMode::Replace,
            }),
            press,
        )
        .expect("edge-pick press");
    let query = edge_query_id(&pressed);
    edge_pick.selection_commit_failure = Some(CanvasInteractionError::ViewUnavailable);
    assert!(matches!(
        edge_pick.resolve_canvas_press_edge_resolution(
            query,
            CanvasEdgePickResolution::Complete(Some(instance)),
        ),
        Err(CanvasInteractionError::ViewUnavailable)
    ));
    assert_eq!(
        edge_pick.presentation().phase(),
        CanvasInteractionPhase::Idle
    );
    assert!(edge_pick.presentation().hovered_edge().is_none());
    assert!(view.selection().unwrap().edges().is_empty());

    let mut marquee = CanvasInteractionSession::attach(&view, CanvasInteractionConfig::default())
        .expect("marquee interaction");
    let baseline = view.selection_baseline().expect("marquee baseline");
    marquee
        .press(
            CanvasPressTarget::Marquee {
                selection_mode: SelectionMode::Replace,
                baseline,
            },
            press,
        )
        .expect("marquee press");
    let moved = view
        .viewport_pointer_sample(ViewPoint::new(40.0, 32.0))
        .expect("marquee move");
    let query = marquee_query_id(&marquee.move_to(moved).expect("marquee query"));
    let snapshot = runtime.snapshot().expect("marquee snapshot");
    let before = session_fingerprint(&marquee);
    marquee.selection_commit_failure = Some(CanvasInteractionError::ViewUnavailable);
    assert!(matches!(
        marquee.resolve_node_intersection_current(
            query,
            NodeIntersectionQueryOutcome::Complete {
                nodes: vec![source_node()],
                work: Default::default(),
            },
            Some(&snapshot),
        ),
        Err(CanvasInteractionError::ViewUnavailable)
    ));
    assert_eq!(session_fingerprint(&marquee), before);
    assert!(view.selection().unwrap().nodes().is_empty());

    let retry = view
        .viewport_pointer_sample(ViewPoint::new(44.0, 36.0))
        .expect("marquee retry move");
    let retry_query = marquee_query_id(&marquee.move_to(retry).expect("retry query"));
    marquee
        .resolve_node_intersection_current(
            retry_query,
            NodeIntersectionQueryOutcome::Complete {
                nodes: vec![source_node()],
                work: Default::default(),
            },
            Some(&snapshot),
        )
        .expect("retry selection commit");
    assert_eq!(view.selection().unwrap().nodes(), [source_node()]);
    marquee.cancel().expect("cancel recovered marquee");
}

#[test]
fn armed_click_selection_failure_restores_the_complete_retriable_session_fingerprint() {
    let mut session = test_session(CanvasInteractionConfig::default());
    session
        .press_for_test(
            CanvasPressTarget::NodeDrag(node_drag_start(
                source_node(),
                [],
                CanvasPoint::default(),
                SelectionMode::Replace,
            )),
            position(0.0, 0.0),
        )
        .expect("arm an unselected node click");
    let before = session_fingerprint(&session);
    session.selection_commit_failure = Some(CanvasInteractionError::ViewUnavailable);

    assert!(matches!(
        session.release(position(0.0, 0.0)),
        Err(CanvasInteractionError::ViewUnavailable)
    ));
    assert_eq!(session_fingerprint(&session), before);

    session
        .release(position(0.0, 0.0))
        .expect("the exact armed click remains retryable");
    assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
}

#[test]
fn cancel_selection_restore_is_retryable_while_view_authority_is_unavailable() {
    for unavailable in ["busy", "closing"] {
        let (_runtime, view, mut session) = bound_test_session();
        set_selection_by_id(&view, [source_node()], [], Vec::new()).expect("initial selection");
        let baseline = view.selection_baseline().expect("selection baseline");
        let press = view
            .viewport_pointer_sample(ViewPoint::new(0.0, 0.0))
            .expect("marquee press");
        let moved = view
            .viewport_pointer_sample(ViewPoint::new(40.0, 32.0))
            .expect("marquee move");
        session
            .press(
                CanvasPressTarget::Marquee {
                    selection_mode: SelectionMode::Replace,
                    baseline,
                },
                press,
            )
            .expect("arm marquee");
        session.move_to(moved).expect("activate marquee");
        let before = session_fingerprint(&session);

        let result = match unavailable {
            "busy" => view.with_state_borrowed_for_test(|| session.cancel()),
            "closing" => view.with_closing_for_test(|| session.cancel()),
            _ => unreachable!(),
        };
        assert!(matches!(
            result,
            Err(CanvasInteractionError::ViewUnavailable)
        ));
        assert_eq!(session_fingerprint(&session), before);
        assert_eq!(view.selection().unwrap().nodes(), [source_node()]);

        session.cancel().expect("retry cancellation");
        assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
        assert_eq!(
            session.presentation().terminal(),
            Some(InteractionTerminalReason::Cancelled)
        );
    }
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

    let reprojected = stale.reproject(current).expect("same-view reprojection");
    assert_eq!(reprojected.provenance(), current.provenance());
    assert_eq!(reprojected.viewport_revision(), current.revision());
    assert_eq!(reprojected.view(), ViewPoint::new(20.0, 10.0));
    assert_eq!(reprojected.canvas(), CanvasPoint { x: 5.0, y: 7.0 });

    let other = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("foreign view");
    assert!(
        stale
            .reproject(other.viewport_snapshot().unwrap())
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
        assert!(matches!(
            released.queries(),
            [query] if matches!(
                query.kind_for_test(),
                CanvasInteractionQueryKind::NodeIntersection { .. }
            )
        ));
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
        let query = edge_query_id(&pressed);
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
fn pan_move_busy_closing_and_invalid_projection_have_zero_side_effects() {
    for unavailable in ["busy", "closing"] {
        let (_runtime, view, mut session) = bound_test_session();
        let press = view
            .viewport_pointer_sample(ViewPoint::new(0.0, 0.0))
            .unwrap();
        let moved = view
            .viewport_pointer_sample(ViewPoint::new(12.0, 4.0))
            .unwrap();
        session.press(CanvasPressTarget::Pan, press).unwrap();
        let before = session_fingerprint(&session);
        let viewport_before = view.viewport_snapshot().unwrap();

        let result = match unavailable {
            "busy" => view.with_state_borrowed_for_test(|| session.move_to(moved)),
            "closing" => view.with_closing_for_test(|| session.move_to(moved)),
            _ => unreachable!(),
        };
        assert!(matches!(
            result,
            Err(CanvasInteractionError::ViewUnavailable)
        ));
        assert_eq!(session_fingerprint(&session), before);
        assert_eq!(view.viewport_snapshot().unwrap(), viewport_before);
    }

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
fn pan_subscriber_panic_resumes_after_viewport_and_pointer_commit() {
    let (_runtime, view, mut session) = bound_test_session();
    let press = view
        .viewport_pointer_sample(ViewPoint::new(0.0, 0.0))
        .unwrap();
    let moved = view
        .viewport_pointer_sample(ViewPoint::new(12.0, 4.0))
        .unwrap();
    session.press(CanvasPressTarget::Pan, press).unwrap();
    view.subscribe(|event| {
        if matches!(
            event,
            crate::runtime::view_session::ViewSessionEvent::ViewportChanged { .. }
        ) {
            panic!("interaction viewport subscriber panic");
        }
    })
    .unwrap();

    let panic = catch_unwind(AssertUnwindSafe(|| session.move_to(moved)));
    assert!(panic.is_err());
    let current = view.viewport_snapshot().unwrap();
    let InteractionState::Pan(pan) = &session.state else {
        panic!("the interaction state must commit before subscriber panic resumes");
    };
    assert_eq!(pan.pointer.viewport_revision(), current.revision());
    assert_eq!(pan.pointer.provenance(), current.provenance());
    assert_eq!(pan.pointer.view(), ViewPoint::new(12.0, 4.0));
    assert_eq!(pan.pointer.canvas(), CanvasPoint::default());
}

#[test]
fn connection_stays_armed_below_threshold_and_release_is_a_noop() {
    let mut session = test_session(CanvasInteractionConfig::default());
    let pressed = session
        .press_for_test(connection_target(false), position(0.0, 0.0))
        .unwrap();
    assert_eq!(pressed.tick_demand(), InteractionTickDemand::None);

    let moved = session.move_to(position(3.0, 0.0)).unwrap();
    assert_eq!(
        session.presentation().phase(),
        CanvasInteractionPhase::Armed
    );
    assert!(moved.queries().is_empty());
    assert_eq!(moved.tick_demand(), InteractionTickDemand::None);

    let released = session.release(position(3.0, 0.0)).unwrap();
    assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
    assert!(released.effects().is_empty());
}

#[test]
fn semantic_owner_query_tracks_connection_before_and_after_activation() {
    let edge = EdgeId::from_u128(9);
    let detached = source_port();
    let fixed = target_port();
    let node = source_node();
    let start = ConnectionInteractionStart::new(
        ConnectionGestureBinding::reconnect(
            edge_instance(edge),
            EdgeEndpoint::To,
            port_instance(node, detached),
            port_instance(NodeId::from_u128(2), fixed),
            NodeGraphConnectionMode::Strict,
        )
        .unwrap(),
        test_graph_version(),
        CanvasPoint { x: 0.0, y: 8.0 },
        HandlePosition::Right,
        false,
    )
    .unwrap();
    let mut session = test_session(CanvasInteractionConfig::default());

    session
        .press_for_test(
            CanvasPressTarget::Connection(start.clone()),
            position(0.0, 0.0),
        )
        .unwrap();
    for owner in [
        InteractionOwner::Node(node_lifetime(node)),
        InteractionOwner::Port(port_instance(node, detached)),
        InteractionOwner::Port(port_instance(NodeId::from_u128(2), fixed)),
        InteractionOwner::Edge(edge_instance(edge)),
    ] {
        assert!(session.uses_owner(owner));
    }
    assert!(
        !session.uses_owner(InteractionOwner::Node(node_lifetime(NodeId::from_u128(
            999
        ),)))
    );

    session.move_to(position(8.0, 0.0)).unwrap();
    assert_eq!(
        session.presentation().phase(),
        CanvasInteractionPhase::ConnectionDrag
    );
    assert!(session.uses_owner(InteractionOwner::Port(port_instance(node, detached))));

    session.cancel().unwrap();
    assert!(!session.uses_owner(InteractionOwner::Port(port_instance(node, detached))));
}

#[test]
fn independent_drag_thresholds_cross_only_beyond_the_exact_view_distance() {
    assert!(!threshold_crossed_distance(
        view_distance_squared(ViewPoint::new(0.0, 0.0), ViewPoint::new(4.0, 0.0)),
        4.0,
    ));
    assert!(threshold_crossed_distance(
        view_distance_squared(ViewPoint::new(0.0, 0.0), ViewPoint::new(4.01, 0.0)),
        4.0,
    ));

    let node = NodeId::from_u128(10);
    let start = node_drag_start(
        node,
        [node],
        CanvasPoint { x: 0.0, y: 0.0 },
        SelectionMode::Replace,
    );
    let mut session = test_session(CanvasInteractionConfig::default());
    let pressed = session
        .press_for_test(CanvasPressTarget::NodeDrag(start), position(0.0, 0.0))
        .unwrap();
    assert_eq!(pressed.tick_demand(), InteractionTickDemand::None);
    let boundary = session.move_to(position(4.0, 0.0)).unwrap();
    assert_eq!(boundary.tick_demand(), InteractionTickDemand::None);
    assert_eq!(
        session.presentation().phase(),
        CanvasInteractionPhase::Armed
    );
    let dragged = session.move_to(position(4.01, 0.0)).unwrap();
    assert_eq!(dragged.tick_demand(), InteractionTickDemand::WhileActive);
    assert_eq!(
        session.presentation().phase(),
        CanvasInteractionPhase::NodeDrag
    );
}

#[test]
fn node_click_tolerance_is_independent_from_the_drag_threshold() {
    let node = NodeId::from_u128(10);
    let start = || node_drag_start(node, [], CanvasPoint::default(), SelectionMode::Replace);

    let mut click = test_session(CanvasInteractionConfig::default());
    click
        .press_for_test(CanvasPressTarget::NodeDrag(start()), position(0.0, 0.0))
        .unwrap();
    let selected = click.release(position(2.0, 0.0)).unwrap();
    assert!(matches!(
        selected.selection_mutations_for_test(),
        [SelectionMutation::Node {
            node: selected,
            mode: SelectionMode::Replace,
            ..
        }] if selected.node() == node
    ));

    let mut dead_zone = test_session(CanvasInteractionConfig::default());
    dead_zone
        .press_for_test(CanvasPressTarget::NodeDrag(start()), position(0.0, 0.0))
        .unwrap();
    let released = dead_zone.release(position(2.01, 0.0)).unwrap();
    assert!(released.selection_mutations_for_test().is_empty());
    assert_eq!(
        dead_zone.presentation().phase(),
        CanvasInteractionPhase::Idle
    );
}

#[test]
fn blank_canvas_click_clears_only_for_replace_mode() {
    for mode in [
        SelectionMode::Replace,
        SelectionMode::Add,
        SelectionMode::Toggle,
    ] {
        let mut session = test_session(CanvasInteractionConfig::default());
        let pressed = session
            .press_for_test(canvas_marquee_target(mode), position(8.0, 8.0))
            .unwrap();
        session
            .resolve_canvas_press_edge_resolution(
                edge_query_id(&pressed),
                CanvasEdgePickResolution::Complete(None),
            )
            .unwrap();
        let released = session.release(position(8.0, 8.0)).unwrap();
        match mode {
            SelectionMode::Replace => assert!(matches!(
                released.selection_mutations_for_test(),
                [SelectionMutation::Clear { .. }]
            )),
            SelectionMode::Add | SelectionMode::Toggle => {
                assert!(released.selection_mutations_for_test().is_empty())
            }
        }
    }
}

#[test]
fn toggle_node_click_toggles_but_toggle_drag_adds_without_deselecting() {
    let node = NodeId::from_u128(10);
    let start = || node_drag_start(node, [], CanvasPoint::default(), SelectionMode::Toggle);

    let mut click = test_session(CanvasInteractionConfig::default());
    click
        .press_for_test(CanvasPressTarget::NodeDrag(start()), position(0.0, 0.0))
        .unwrap();
    let selected = click.release(position(0.0, 0.0)).unwrap();
    assert!(matches!(
        selected.selection_mutations_for_test(),
        [SelectionMutation::Node {
            mode: SelectionMode::Toggle,
            ..
        }]
    ));

    let mut drag = test_session(CanvasInteractionConfig::default());
    drag.press_for_test(CanvasPressTarget::NodeDrag(start()), position(0.0, 0.0))
        .unwrap();
    let activated = drag.move_to(position(8.0, 0.0)).unwrap();
    assert!(matches!(
        activated.selection_mutations_for_test(),
        [SelectionMutation::Node {
            mode: SelectionMode::Add,
            ..
        }]
    ));
    assert_eq!(
        drag.presentation().phase(),
        CanvasInteractionPhase::NodeDrag
    );
}

#[test]
fn asynchronous_search_and_commit_states_do_not_request_animation_ticks() {
    let mut search = test_session(CanvasInteractionConfig::default());
    let activated = search
        .activate_connection_search_for_test(connection_start(true), position(20.0, 10.0))
        .unwrap();
    assert_eq!(activated.tick_demand(), InteractionTickDemand::None);
    let request = search.search_request().expect("active search request");
    let ready = search.search_ready(&request, 1).unwrap();
    assert_eq!(ready.tick_demand(), InteractionTickDemand::None);

    let route = published_edge_route(EdgeId::from_u128(24), true, EdgeReconnectable::Bool(true));
    let mut deletion = test_session(CanvasInteractionConfig::default());
    let submitted = deletion
        .delete_edge_at_version(&route, test_graph_version())
        .unwrap();
    assert_eq!(submitted.tick_demand(), InteractionTickDemand::None);
}

#[test]
fn direct_connection_reprobes_on_release_and_keeps_preview_until_terminal_receipt() {
    let mut session = test_session(CanvasInteractionConfig::default());
    session
        .press_for_test(connection_target(false), position(0.0, 0.0))
        .unwrap();
    let moved = session.move_to(position(8.0, 0.0)).unwrap();
    let move_query = connection_query_id(&moved);
    session
        .resolve_connection_probe_for_test(move_query, complete(valid_probe(target_port())))
        .unwrap();

    let released = session.release(position(20.0, 0.0)).unwrap();
    let release_query = connection_query_id(&released);
    assert_ne!(move_query, release_query);
    assert!(
        session
            .presentation()
            .connection()
            .expect("drag presentation")
            .is_release_pending()
    );

    let committing = session
        .resolve_connection_probe_for_test(release_query, complete(valid_probe(target_port())))
        .unwrap();
    let ticket = submit_ticket(&committing);
    let presentation = session.presentation();
    assert_eq!(presentation.phase(), CanvasInteractionPhase::Committing);
    let connection = presentation.connection().expect("commit preview");
    assert_eq!(connection.ticket(), Some(ticket));
    assert_eq!(connection.preview_target(), CanvasPoint { x: 24.0, y: 8.0 });

    complete_with_kind(&mut session, ticket, CommandExecutionKind::Rejected).unwrap();
    assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
    assert_eq!(
        session.presentation().terminal(),
        Some(InteractionTerminalReason::Rejected)
    );

    let late = session.submission_outcome_unknown(ticket).unwrap();
    assert!(late.effects().is_empty());
    assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
}

#[test]
fn connection_wire_projection_uses_probe_time_geometry_and_stable_pointer_direction() {
    let mut session = test_session(CanvasInteractionConfig::default());
    session
        .press_for_test(connection_target(false), position(0.0, 0.0))
        .unwrap();
    let moved = session.move_to(position(8.0, 4.0)).unwrap();
    let unresolved = session
        .presentation()
        .connection()
        .expect("unresolved connection presentation")
        .wire();
    assert_eq!(
        unresolved.endpoints().source,
        EdgeEndpointPosition {
            point: CanvasPoint { x: 0.0, y: 8.0 },
            position: HandlePosition::Right,
        }
    );
    assert_eq!(
        unresolved.endpoints().target,
        EdgeEndpointPosition {
            point: CanvasPoint { x: 8.0, y: 4.0 },
            position: HandlePosition::Left,
        }
    );
    assert_eq!(
        unresolved.probe_validity(),
        super::super::ConnectionProbeValidity::Unresolved
    );

    let invalid = ConnectionProbe::Invalid {
        version: test_graph_version(),
        target: ConnectionProbeTarget::new(
            PortInstanceKey::unavailable(NodeId::from_u128(2), target_port()),
            CanvasPoint { x: 90.0, y: 60.0 },
            HandlePosition::Top,
            0.0,
        ),
        diagnostics: Vec::new(),
    };
    session
        .resolve_connection_probe_for_test(connection_query_id(&moved), complete(invalid))
        .unwrap();
    let invalid = session
        .presentation()
        .connection()
        .expect("invalid connection presentation")
        .wire();
    assert_eq!(
        invalid.endpoints().target.point,
        CanvasPoint { x: 8.0, y: 4.0 }
    );
    assert_eq!(invalid.endpoints().target.position, HandlePosition::Left);
    assert_eq!(
        invalid.probe_validity(),
        super::super::ConnectionProbeValidity::Invalid
    );

    let moved = session.move_to(position(12.0, 6.0)).unwrap();
    let target_anchor = CanvasPoint { x: 96.0, y: 64.0 };
    let valid = ConnectionProbe::Valid {
        version: test_graph_version(),
        target: ConnectionProbeTarget::new(
            PortInstanceKey::unavailable(NodeId::from_u128(2), target_port()),
            target_anchor,
            HandlePosition::Top,
            0.0,
        ),
    };
    session
        .resolve_connection_probe_for_test(connection_query_id(&moved), complete(valid))
        .unwrap();
    let valid = session
        .presentation()
        .connection()
        .expect("valid connection presentation")
        .wire();
    assert_eq!(valid.endpoints().source, unresolved.endpoints().source);
    assert_eq!(
        valid.endpoints().target,
        EdgeEndpointPosition {
            point: target_anchor,
            position: HandlePosition::Top,
        }
    );
    assert_eq!(valid.generation(), unresolved.generation());
    assert_eq!(
        valid.probe_validity(),
        super::super::ConnectionProbeValidity::Valid
    );
}

#[test]
fn released_connection_at_auto_pan_edge_preserves_its_final_probe() {
    let mut session = test_session(CanvasInteractionConfig::default());
    session
        .press_for_test(connection_target(false), position(100.0, 40.0))
        .unwrap();
    let moved = session.move_to(position(190.0, 40.0)).unwrap();
    session
        .resolve_connection_probe_for_test(
            connection_query_id(&moved),
            complete(valid_probe(target_port())),
        )
        .unwrap();

    let released = session.release(position(195.0, 40.0)).unwrap();
    let release_query = connection_query_id(&released);
    let tick = session.tick(0.1, ViewSize::new(200.0, 120.0)).unwrap();

    assert!(tick.effects().is_empty());
    assert!(tick.queries().is_empty());
    assert!(
        session
            .presentation()
            .connection()
            .expect("released connection preview")
            .is_release_pending()
    );

    let committing = session
        .resolve_connection_probe_for_test(release_query, complete(valid_probe(target_port())))
        .unwrap();
    assert!(matches!(
        committing.effects(),
        [CanvasInteractionEffect::SubmitGraph { .. }]
    ));
}

#[test]
fn stale_drag_probe_restart_replaces_identity_and_ignores_the_old_page() {
    let mut session = test_session(CanvasInteractionConfig {
        query_entry_budget: 1,
        ..CanvasInteractionConfig::default()
    });
    session
        .press_for_test(connection_target(false), position(0.0, 0.0))
        .unwrap();
    let started = session.move_to(position(12.0, 4.0)).unwrap();
    let stale_id = connection_query_id(&started);

    let restarted = session.restart_stale_query(stale_id).unwrap();
    let fresh_id = connection_query_id(&restarted);
    assert_ne!(fresh_id, stale_id);
    assert_eq!(
        session.presentation().phase(),
        CanvasInteractionPhase::ConnectionDrag
    );

    let late = session
        .resolve_connection_probe_for_test(stale_id, complete(valid_probe(target_port())))
        .unwrap();
    assert!(late.queries().is_empty());
    assert!(late.effects().is_empty());
    let resolved = session
        .resolve_connection_probe_for_test(fresh_id, complete(valid_probe(target_port())))
        .unwrap();
    assert!(resolved.effects().is_empty());
    assert!(
        session
            .presentation()
            .connection()
            .expect("active connection preview")
            .probe()
            .is_valid()
    );
}

#[test]
fn stale_release_probe_restart_preserves_release_pending_until_commit() {
    let mut session = test_session(CanvasInteractionConfig {
        query_entry_budget: 1,
        ..CanvasInteractionConfig::default()
    });
    session
        .press_for_test(connection_target(false), position(0.0, 0.0))
        .unwrap();
    let moved = session.move_to(position(8.0, 0.0)).unwrap();
    session
        .resolve_connection_probe_for_test(
            connection_query_id(&moved),
            complete(valid_probe(target_port())),
        )
        .unwrap();
    let released = session.release(position(20.0, 0.0)).unwrap();
    let stale_id = connection_query_id(&released);

    let restarted = session.restart_stale_query(stale_id).unwrap();
    let fresh_id = connection_query_id(&restarted);
    assert_ne!(fresh_id, stale_id);
    assert!(
        session
            .presentation()
            .connection()
            .expect("release preview")
            .is_release_pending()
    );

    let committing = session
        .resolve_connection_probe_for_test(fresh_id, complete(valid_probe(target_port())))
        .unwrap();
    assert!(matches!(
        committing.effects(),
        [CanvasInteractionEffect::SubmitGraph { .. }]
    ));
    assert_eq!(
        session.presentation().phase(),
        CanvasInteractionPhase::Committing
    );
}

#[test]
fn stale_commit_revalidation_restart_reuses_ticket_and_keeps_budget_terminal() {
    let mut session = test_session(CanvasInteractionConfig {
        query_entry_budget: 1,
        ..CanvasInteractionConfig::default()
    });
    let ticket = begin_direct_connection_commit(&mut session);
    let requested = session
        .request_connection_commit_revalidation(ticket)
        .unwrap();
    let stale_id = connection_query_id(&requested);
    let restarted = session.restart_stale_query(stale_id).unwrap();
    let fresh_id = connection_query_id(&restarted);
    assert_ne!(fresh_id, stale_id);
    assert_eq!(
        session
            .presentation()
            .connection()
            .expect("commit preview")
            .ticket(),
        Some(ticket)
    );

    let exhausted = session
        .resolve_connection_probe_for_test(
            fresh_id,
            ConnectionProbeQueryOutcome::BudgetExhausted {
                work: ConnectionProbeWork::default(),
            },
        )
        .unwrap();
    assert!(exhausted.effects().is_empty());
    assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
    assert_eq!(
        session.presentation().terminal(),
        Some(InteractionTerminalReason::BudgetExhausted)
    );
}

#[test]
fn stale_edge_pick_restart_replaces_identity_without_relaxing_budget_failure() {
    let mut session = test_session(CanvasInteractionConfig {
        query_entry_budget: 1,
        ..CanvasInteractionConfig::default()
    });
    let requested = session.request_edge_pick(position(20.0, 10.0)).unwrap();
    let stale_id = edge_query_id(&requested);
    let restarted = session.restart_stale_query(stale_id).unwrap();
    let fresh_id = edge_query_id(&restarted);
    assert_ne!(fresh_id, stale_id);

    let late = session
        .resolve_edge_pick_for_test(
            stale_id,
            EdgePickQueryOutcome::Complete {
                pick: None,
                work: EdgePickWork::default(),
            },
        )
        .unwrap();
    assert!(late.queries().is_empty());
    let exhausted = session
        .resolve_edge_pick_for_test(
            fresh_id,
            EdgePickQueryOutcome::BudgetExhausted {
                work: EdgePickWork::default(),
            },
        )
        .unwrap();
    assert!(exhausted.effects().is_empty());
    assert_eq!(
        session.presentation().terminal(),
        Some(InteractionTerminalReason::BudgetExhausted)
    );
}

#[test]
fn repeated_stale_drag_probe_restarts_are_bounded_per_pointer_query() {
    let mut session = test_session(CanvasInteractionConfig::default());
    session
        .press_for_test(connection_target(false), position(0.0, 0.0))
        .unwrap();
    let started = session.move_to(position(12.0, 4.0)).unwrap();
    let mut current = connection_query_id(&started);

    for _ in 0..MAX_STALE_QUERY_RESTARTS {
        let restarted = session.restart_stale_query(current).unwrap();
        current = connection_query_id(&restarted);
    }
    let exhausted = session.restart_stale_query(current).unwrap();

    assert!(exhausted.queries().is_empty());
    assert!(exhausted.effects().is_empty());
    assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
    assert_eq!(
        session.presentation().terminal(),
        Some(InteractionTerminalReason::BudgetExhausted)
    );
}

#[test]
fn repeated_stale_hover_edge_picks_are_bounded() {
    let mut session = test_session(CanvasInteractionConfig::default());
    let requested = session.request_edge_pick(position(20.0, 10.0)).unwrap();
    let mut current = edge_query_id(&requested);

    for _ in 0..MAX_STALE_QUERY_RESTARTS {
        let restarted = session.restart_stale_query(current).unwrap();
        current = edge_query_id(&restarted);
    }
    let exhausted = session.restart_stale_query(current).unwrap();

    assert!(exhausted.queries().is_empty());
    assert!(exhausted.effects().is_empty());
    assert_eq!(session.presentation().hovered_edge(), None);
    assert_eq!(
        session.presentation().terminal(),
        Some(InteractionTerminalReason::BudgetExhausted)
    );
}

#[test]
fn repeated_stale_commit_revalidation_rejects_without_graph_mutation() {
    let mut session = test_session(CanvasInteractionConfig::default());
    let ticket = begin_direct_connection_commit(&mut session);
    let requested = session
        .request_connection_commit_revalidation(ticket)
        .unwrap();
    let mut current = connection_query_id(&requested);

    for _ in 0..MAX_STALE_QUERY_RESTARTS {
        let restarted = session.restart_stale_query(current).unwrap();
        current = connection_query_id(&restarted);
    }
    let exhausted = session.restart_stale_query(current).unwrap();

    assert!(exhausted.queries().is_empty());
    assert!(exhausted.effects().is_empty());
    assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
    assert_eq!(
        session.presentation().terminal(),
        Some(InteractionTerminalReason::BudgetExhausted)
    );
}

#[test]
fn exact_canvas_press_selects_its_resolved_edge_instead_of_a_previous_hover() {
    let stale_hover = EdgeId::from_u128(40);
    let pressed_edge = EdgeId::from_u128(41);
    let mut session = test_session(CanvasInteractionConfig::default());
    session.hovered_edge = Some(edge_instance(stale_hover));

    let pressed = session
        .press_for_test(
            canvas_marquee_target(SelectionMode::Toggle),
            position(20.0, 10.0),
        )
        .unwrap();
    let query = edge_query_id(&pressed);
    assert_eq!(session.presentation().hovered_edge(), None);
    assert_eq!(
        session.presentation().phase(),
        CanvasInteractionPhase::Armed
    );

    let resolved = session
        .resolve_canvas_press_edge_resolution(
            query,
            CanvasEdgePickResolution::Complete(Some(edge_instance(pressed_edge))),
        )
        .unwrap();

    assert!(matches!(
        resolved.selection_mutations_for_test(),
        [SelectionMutation::Edge {
            instance,
            mode: SelectionMode::Toggle,
            ..
        }] if instance.edge() == pressed_edge
    ));
    assert_eq!(
        session.presentation().phase(),
        CanvasInteractionPhase::EdgeActive
    );
    assert_eq!(
        session.presentation().active_edge(),
        Some(edge_instance(pressed_edge))
    );
    assert_eq!(
        session.presentation().hovered_edge(),
        Some(edge_instance(pressed_edge))
    );
}

#[test]
fn empty_exact_pick_activates_marquee_from_press_to_latest_pointer() {
    let mut session = test_session(CanvasInteractionConfig::default());
    let pressed = session
        .press_for_test(
            canvas_marquee_target(SelectionMode::Add),
            position(2.0, 3.0),
        )
        .unwrap();
    let query = edge_query_id(&pressed);
    let moved = session.move_to(position(14.0, 11.0)).unwrap();
    assert!(moved.effects().is_empty());

    let resolved = session
        .resolve_canvas_press_edge_resolution(query, CanvasEdgePickResolution::Complete(None))
        .unwrap();
    assert!(resolved.effects().is_empty());
    let selected = resolve_marquee_query(&mut session, &resolved, []);
    assert!(matches!(
        selected.selection_mutations_for_test(),
        [SelectionMutation::Marquee {
            nodes,
            mode: SelectionMode::Add,
            ..
        }] if nodes.is_empty()
    ));
    assert_eq!(
        session.presentation().phase(),
        CanvasInteractionPhase::Marquee
    );
    assert_eq!(
        session.presentation().marquee(),
        Some(CanvasRect {
            origin: CanvasPoint { x: 2.0, y: 3.0 },
            size: CanvasSize {
                width: 12.0,
                height: 8.0,
            },
        })
    );
}

#[test]
fn release_before_exact_pick_completion_suppresses_delayed_edge_click_and_finishes_fallback() {
    let edge = EdgeId::from_u128(50);
    let mut edge_session = test_session(CanvasInteractionConfig::default());
    let pressed = edge_session
        .press_for_test(
            canvas_marquee_target(SelectionMode::Replace),
            position(0.0, 0.0),
        )
        .unwrap();
    let query = edge_query_id(&pressed);
    let released = edge_session.release(position(12.0, 0.0)).unwrap();
    assert!(released.effects().is_empty());
    let resolved = edge_session
        .resolve_canvas_press_edge_resolution(
            query,
            CanvasEdgePickResolution::Complete(Some(edge_instance(edge))),
        )
        .unwrap();
    assert!(resolved.effects().is_empty());
    assert_eq!(
        edge_session.presentation().phase(),
        CanvasInteractionPhase::Idle
    );

    let mut empty_session = test_session(CanvasInteractionConfig::default());
    let pressed = empty_session
        .press_for_test(
            canvas_marquee_target(SelectionMode::Replace),
            position(0.0, 0.0),
        )
        .unwrap();
    let query = edge_query_id(&pressed);
    empty_session.release(position(12.0, 8.0)).unwrap();
    let resolved = empty_session
        .resolve_canvas_press_edge_resolution(query, CanvasEdgePickResolution::Complete(None))
        .unwrap();
    assert!(resolved.effects().is_empty());
    let selected = resolve_marquee_query(&mut empty_session, &resolved, []);
    assert!(matches!(
        selected.selection_mutations_for_test(),
        [SelectionMutation::Marquee { .. }]
    ));
    assert_eq!(
        empty_session.presentation().phase(),
        CanvasInteractionPhase::Idle
    );
}

#[test]
fn exact_press_pagination_latches_maximum_displacement_until_completion() {
    let edge = EdgeId::from_u128(60);
    let mut session = test_session(CanvasInteractionConfig {
        query_entry_budget: 1,
        ..CanvasInteractionConfig::default()
    });
    let pressed = session
        .press_for_test(
            canvas_marquee_target(SelectionMode::Replace),
            position(4.0, 5.0),
        )
        .unwrap();
    let query = edge_query_id(&pressed);
    session.move_to(position(40.0, 50.0)).unwrap();
    let continued = session
        .resolve_canvas_press_edge_resolution(
            query,
            CanvasEdgePickResolution::Continue(EdgePickQuery::new(
                ViewPoint::new(4.0, 5.0),
                10.0,
                1,
            )),
        )
        .unwrap();
    assert_eq!(edge_query_id(&continued), query);
    assert!(continued.effects().is_empty());
    let InteractionState::EdgePickPending(pending) = &session.state else {
        panic!("exact press must remain pending between query pages");
    };
    assert_eq!(pending.press, interaction_position(4.0, 5.0));
    assert_eq!(pending.pointer, interaction_position(40.0, 50.0));

    session.release(position(44.0, 55.0)).unwrap();
    let resolved = session
        .resolve_canvas_press_edge_resolution(
            query,
            CanvasEdgePickResolution::Complete(Some(edge_instance(edge))),
        )
        .unwrap();
    assert!(resolved.effects().is_empty());
    assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
}

#[test]
fn exact_press_latches_a_far_move_even_after_returning_to_the_press_point() {
    let edge = EdgeId::from_u128(60);
    let mut session = test_session(CanvasInteractionConfig::default());
    let pressed = session
        .press_for_test(
            canvas_marquee_target(SelectionMode::Replace),
            position(4.0, 5.0),
        )
        .unwrap();
    let query = edge_query_id(&pressed);
    session.move_to(position(40.0, 50.0)).unwrap();
    session.move_to(position(4.0, 5.0)).unwrap();
    session.release(position(4.0, 5.0)).unwrap();

    let resolved = session
        .resolve_canvas_press_edge_resolution(
            query,
            CanvasEdgePickResolution::Complete(Some(edge_instance(edge))),
        )
        .unwrap();
    assert!(resolved.effects().is_empty());
    assert_eq!(session.presentation().active_edge(), None);
}

#[test]
fn stale_exact_press_restart_requeries_the_press_point_and_ignores_the_old_identity() {
    let edge = EdgeId::from_u128(61);
    let mut session = test_session(CanvasInteractionConfig::default());
    let pressed = session
        .press_for_test(
            canvas_marquee_target(SelectionMode::Replace),
            position(4.0, 5.0),
        )
        .unwrap();
    let stale = edge_query_id(&pressed);
    session.move_to(position(40.0, 50.0)).unwrap();

    let restarted = session.restart_stale_query(stale).unwrap();
    let fresh = edge_query_id(&restarted);
    assert_ne!(fresh, stale);
    let InteractionState::EdgePickPending(pending) = &session.state else {
        panic!("exact press restart must remain pending");
    };
    assert_eq!(pending.press, interaction_position(4.0, 5.0));
    assert_eq!(pending.pointer, interaction_position(40.0, 50.0));

    let late = session
        .resolve_canvas_press_edge_resolution(
            stale,
            CanvasEdgePickResolution::Complete(Some(edge_instance(edge))),
        )
        .unwrap();
    assert!(late.effects().is_empty());
    assert_eq!(session.presentation().active_edge(), None);

    let resolved = session
        .resolve_canvas_press_edge_resolution(
            fresh,
            CanvasEdgePickResolution::Complete(Some(edge_instance(edge))),
        )
        .unwrap();
    assert!(resolved.effects().is_empty());
}

#[test]
fn exact_press_supersedes_fast_hover_without_accepting_its_late_result() {
    let pressed_edge = EdgeId::from_u128(63);
    let mut session = test_session(CanvasInteractionConfig::default());
    let hover = session.request_edge_pick(position(2.0, 3.0)).unwrap();
    let hover_query = edge_query_id(&hover);
    let pressed = session
        .press_for_test(
            canvas_marquee_target(SelectionMode::Replace),
            position(8.0, 9.0),
        )
        .unwrap();
    let press_query = edge_query_id(&pressed);
    assert_ne!(press_query, hover_query);

    let late_hover = session
        .resolve_edge_pick_for_test(
            hover_query,
            EdgePickQueryOutcome::BudgetExhausted {
                work: EdgePickWork::default(),
            },
        )
        .unwrap();
    assert!(late_hover.effects().is_empty());
    assert_eq!(session.presentation().hovered_edge(), None);
    assert_eq!(session.presentation().terminal(), None);

    session
        .resolve_canvas_press_edge_resolution(
            press_query,
            CanvasEdgePickResolution::Complete(Some(edge_instance(pressed_edge))),
        )
        .unwrap();
    assert_eq!(
        session.presentation().active_edge(),
        Some(edge_instance(pressed_edge))
    );
    assert_eq!(
        session.presentation().hovered_edge(),
        Some(edge_instance(pressed_edge))
    );
}

#[test]
fn exact_press_budget_geometry_cancel_and_late_results_never_select() {
    for (resolution, terminal) in [
        (
            CanvasEdgePickResolution::BudgetExhausted,
            InteractionTerminalReason::BudgetExhausted,
        ),
        (
            CanvasEdgePickResolution::GeometryIndeterminate,
            InteractionTerminalReason::GeometryIndeterminate,
        ),
    ] {
        let mut session = test_session(CanvasInteractionConfig::default());
        let pressed = session
            .press_for_test(
                canvas_marquee_target(SelectionMode::Replace),
                position(0.0, 0.0),
            )
            .unwrap();
        let resolved = session
            .resolve_canvas_press_edge_resolution(edge_query_id(&pressed), resolution)
            .unwrap();
        assert!(resolved.effects().is_empty());
        assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
        assert_eq!(session.presentation().terminal(), Some(terminal));
    }

    let edge = EdgeId::from_u128(70);
    let mut session = test_session(CanvasInteractionConfig::default());
    let pressed = session
        .press_for_test(
            canvas_marquee_target(SelectionMode::Replace),
            position(0.0, 0.0),
        )
        .unwrap();
    let query = edge_query_id(&pressed);
    let cancelled = session.cancel().unwrap();
    assert!(cancelled.effects().is_empty());
    assert_eq!(
        session.presentation().terminal(),
        Some(InteractionTerminalReason::Cancelled)
    );
    let late = session
        .resolve_canvas_press_edge_resolution(
            query,
            CanvasEdgePickResolution::Complete(Some(edge_instance(edge))),
        )
        .unwrap();
    assert!(late.effects().is_empty());
    assert_eq!(session.presentation().active_edge(), None);

    let mut closed_session = test_session(CanvasInteractionConfig::default());
    let pressed = closed_session
        .press_for_test(
            canvas_marquee_target(SelectionMode::Replace),
            position(0.0, 0.0),
        )
        .unwrap();
    let query = edge_query_id(&pressed);
    let closed = closed_session.close();
    assert!(closed.effects().is_empty());
    assert_eq!(
        closed_session.presentation().terminal(),
        Some(InteractionTerminalReason::ViewClosed)
    );
    let late = closed_session
        .resolve_canvas_press_edge_resolution(
            query,
            CanvasEdgePickResolution::Complete(Some(edge_instance(edge))),
        )
        .unwrap();
    assert!(late.effects().is_empty());
    assert_eq!(closed_session.presentation().active_edge(), None);
}

#[test]
fn empty_exact_pick_can_activate_pan_without_a_second_press() {
    let mut session = test_session(CanvasInteractionConfig::default());
    let pressed = session
        .press_for_test(
            CanvasPressTarget::Canvas(CanvasPressFallback::Pan {
                selection_mode: SelectionMode::Replace,
            }),
            position(1.0, 2.0),
        )
        .unwrap();
    let query = edge_query_id(&pressed);
    session.move_to(position(11.0, 14.0)).unwrap();
    let resolved = session
        .resolve_canvas_press_edge_resolution(query, CanvasEdgePickResolution::Complete(None))
        .unwrap();
    assert!(resolved.effects().is_empty());
    assert!(resolved.damage().viewport);
    assert_eq!(
        session.test_viewport.transform().pan,
        CanvasPoint { x: 10.0, y: 12.0 }
    );
    assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Pan);
}

#[test]
fn reconnect_suppresses_old_edge_through_commit_and_restores_it_after_conflict() {
    let edge = EdgeId::from_u128(9);
    let start = reconnect_start(edge, PortId::from_u128(4), false);
    let mut session = test_session(CanvasInteractionConfig::default());
    session
        .press_for_test(CanvasPressTarget::Connection(start), position(0.0, 0.0))
        .unwrap();
    let moved = session.move_to(position(8.0, 0.0)).unwrap();
    session
        .resolve_connection_probe_for_test(
            connection_query_id(&moved),
            complete(valid_probe(target_port())),
        )
        .unwrap();
    assert_eq!(
        session
            .presentation()
            .connection()
            .unwrap()
            .suppressed_edge(),
        Some(edge)
    );

    let released = session.release(position(20.0, 0.0)).unwrap();
    let committing = session
        .resolve_connection_probe_for_test(
            connection_query_id(&released),
            complete(valid_probe(target_port())),
        )
        .unwrap();
    let ticket = submit_ticket(&committing);
    assert_eq!(
        session
            .presentation()
            .connection()
            .unwrap()
            .suppressed_edge(),
        Some(edge)
    );

    complete_with_kind(&mut session, ticket, CommandExecutionKind::Conflict).unwrap();
    assert!(session.presentation().connection().is_none());
    assert_eq!(
        session.presentation().terminal(),
        Some(InteractionTerminalReason::Conflict)
    );
}

#[test]
fn connection_commit_revalidation_reuses_ticket_and_refreshes_optimistic_preview() {
    let edge = EdgeId::from_u128(9);
    let start = reconnect_start(edge, PortId::from_u128(4), false);
    let mut session = test_session(CanvasInteractionConfig::default());
    session
        .press_for_test(
            CanvasPressTarget::Connection(start.clone()),
            position(0.0, 0.0),
        )
        .unwrap();
    let moved = session.move_to(position(8.0, 0.0)).unwrap();
    session
        .resolve_connection_probe_for_test(
            connection_query_id(&moved),
            complete(valid_probe(target_port())),
        )
        .unwrap();
    let released = session.release(position(20.0, 0.0)).unwrap();
    let committing = session
        .resolve_connection_probe_for_test(
            connection_query_id(&released),
            complete(valid_probe(target_port())),
        )
        .unwrap();
    let ticket = submit_ticket(&committing);
    let fresh_target = PortId::from_u128(3);
    let fresh_version = test_graph_version();
    let fresh_probe = valid_probe_at(
        fresh_target,
        fresh_version,
        CanvasPoint { x: 48.0, y: 12.0 },
    );

    let requested = session
        .request_connection_commit_revalidation(ticket)
        .unwrap();
    let query = connection_query_id(&requested);
    assert_eq!(
        session
            .presentation()
            .connection()
            .expect("commit preview")
            .suppressed_edge(),
        Some(edge)
    );
    let revalidated = session
        .resolve_connection_probe_for_test(query, complete(fresh_probe))
        .unwrap();
    let [CanvasInteractionEffect::SubmitGraph { submission }] = revalidated.effects() else {
        panic!("valid revalidation must emit one replacement connection mutation");
    };
    let (binding, target, validated_revision, create_from_wire) = submission
        .connection_facts_for_test()
        .expect("replacement is a connection mutation");
    assert_eq!(submission.ticket(), ticket);
    assert_eq!(binding, start.binding());
    assert_eq!(target, fresh_target);
    assert_eq!(validated_revision, fresh_version);
    assert!(!create_from_wire);

    let presentation = session.presentation();
    assert_eq!(presentation.phase(), CanvasInteractionPhase::Committing);
    let connection = presentation.connection().expect("commit preview");
    assert_eq!(connection.ticket(), Some(ticket));
    assert_eq!(connection.suppressed_edge(), Some(edge));
    assert_eq!(
        connection.preview_target(),
        CanvasPoint { x: 48.0, y: 12.0 }
    );
}

#[test]
fn invalid_commit_revalidation_conflicts_without_emitting_a_mutation() {
    let mut session = test_session(CanvasInteractionConfig::default());
    let ticket = begin_direct_connection_commit(&mut session);
    let invalid = ConnectionProbe::Invalid {
        version: test_graph_version(),
        target: probe_target(target_port(), CanvasPoint { x: 32.0, y: 8.0 }),
        diagnostics: Vec::new(),
    };

    let requested = session
        .request_connection_commit_revalidation(ticket)
        .unwrap();
    let query = connection_query_id(&requested);
    let rejected = session
        .resolve_connection_probe_for_test(query, complete(invalid))
        .unwrap();
    assert!(rejected.effects().is_empty());
    assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
    assert_eq!(
        session.presentation().terminal(),
        Some(InteractionTerminalReason::Conflict)
    );

    let late = session
        .resolve_connection_probe_for_test(query, complete(valid_probe(target_port())))
        .unwrap();
    assert!(late.effects().is_empty());
    assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
    assert_eq!(
        session.presentation().terminal(),
        Some(InteractionTerminalReason::Conflict)
    );
}

#[test]
fn create_from_wire_refresh_reuses_atomic_topology_and_returns_to_search_on_conflict() {
    let mut session = test_session(CanvasInteractionConfig::default());
    let ticket = begin_search_connection_commit(&mut session);
    let request = session
        .search_request()
        .expect("authoritative search request");
    assert_eq!(
        request.token.binding,
        ConnectionGestureBinding::connect_for_test(
            EdgeId::from_u128(9),
            source_port(),
            NodeGraphConnectionMode::Strict,
        )
    );

    let wrong_ticket = next_ticket_identity(ticket);
    let snapshot = prospective_snapshot();
    let stale = session
        .refresh_create_from_wire_commit_from_snapshot(wrong_ticket, &snapshot)
        .unwrap();
    assert!(stale.effects().is_empty());

    assert!(matches!(
        session.request_connection_commit_revalidation(ticket),
        Err(CanvasInteractionError::ConnectionCommitRequiresFreshRevision)
    ));
    let existing_target_snapshot = graph_snapshot(reconnect_graph(EdgeId::from_u128(42)));
    assert!(matches!(
        session.refresh_create_from_wire_commit_from_snapshot(ticket, &existing_target_snapshot),
        Err(CanvasInteractionError::ConnectionCommitTargetNoLongerProspective { target })
            if target == target_port()
    ));
    let refreshed = session
        .refresh_create_from_wire_commit_from_snapshot(ticket, &snapshot)
        .unwrap();
    let [CanvasInteractionEffect::SubmitGraph { submission }] = refreshed.effects() else {
        panic!("fresh create-from-wire revision must resubmit one atomic mutation");
    };
    let (_, target, validated_revision, create_from_wire) = submission
        .connection_facts_for_test()
        .expect("refresh emits a connection mutation");
    assert_eq!(submission.ticket(), ticket);
    assert_eq!(target, target_port());
    assert_eq!(validated_revision, snapshot.version());
    assert!(create_from_wire);
    assert_eq!(
        session.search_request().unwrap().generation(),
        session.presentation().generation().unwrap()
    );

    let duplicate = session
        .refresh_create_from_wire_commit_from_snapshot(ticket, &snapshot)
        .unwrap();
    assert!(duplicate.effects().is_empty());
    complete_with_kind(&mut session, ticket, CommandExecutionKind::Conflict).unwrap();
    assert_eq!(
        session.presentation().phase(),
        CanvasInteractionPhase::ConnectionSearch
    );
    assert_eq!(
        session.presentation().connection().unwrap().search(),
        Some(ConnectionSearchStatus::CommitRejected)
    );
    assert_eq!(
        session.presentation().terminal(),
        Some(InteractionTerminalReason::Conflict)
    );
}

#[test]
fn create_from_wire_selection_rejects_an_existing_target() {
    let snapshot = graph_snapshot(reconnect_graph(EdgeId::from_u128(42)));
    let token = search_token_for_test();

    assert!(
        ConnectionSearchSelection::from_snapshot(
            token,
            &snapshot,
            target_port(),
            prospective_creation(),
        )
        .is_none()
    );
}

#[test]
fn create_from_wire_selection_rejects_a_prospective_node_id_collision() {
    let prospective_node = NodeId::from_u128(2);
    let mut source = test_node(CanvasPoint::default());
    source.ports = vec![source_port()];
    let graph = GraphBuilder::new(GraphId::from_u128(1))
        .with_node(source_node(), source)
        .with_port(
            source_port(),
            test_port(source_node(), "out", PortDirection::Out),
        )
        .with_node(prospective_node, test_node(CanvasPoint::default()))
        .build()
        .unwrap();
    let snapshot = graph_snapshot(graph);
    let token = search_token_for_test();

    assert!(
        ConnectionSearchSelection::from_snapshot(
            token,
            &snapshot,
            target_port(),
            prospective_creation(),
        )
        .is_none()
    );
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
fn stale_connection_revalidation_ticket_does_not_change_pending_commit() {
    let mut session = test_session(CanvasInteractionConfig::default());
    let ticket = begin_direct_connection_commit(&mut session);
    let original_preview = session
        .presentation()
        .connection()
        .expect("commit preview")
        .preview_target();
    let stale_ticket = next_ticket_identity(ticket);

    let stale = session
        .request_connection_commit_revalidation(stale_ticket)
        .unwrap();
    assert!(stale.queries().is_empty());
    assert!(stale.effects().is_empty());
    assert_eq!(stale.damage(), InteractionDamage::default());
    let presentation = session.presentation();
    assert_eq!(presentation.phase(), CanvasInteractionPhase::Committing);
    let connection = presentation.connection().expect("commit preview");
    assert_eq!(connection.ticket(), Some(ticket));
    assert_eq!(connection.preview_target(), original_preview);

    let requested = session
        .request_connection_commit_revalidation(ticket)
        .unwrap();
    assert_eq!(requested.queries().len(), 1);
}

#[test]
fn ordinary_connection_cannot_bypass_probe_with_revision_refresh() {
    let mut session = test_session(CanvasInteractionConfig::default());
    let ticket = begin_direct_connection_commit(&mut session);
    let snapshot = prospective_snapshot();

    assert!(matches!(
        session.refresh_create_from_wire_commit_from_snapshot(ticket, &snapshot),
        Err(CanvasInteractionError::ConnectionCommitRequiresProbe)
    ));
    assert_eq!(
        session.presentation().phase(),
        CanvasInteractionPhase::Committing
    );
    let requested = session
        .request_connection_commit_revalidation(ticket)
        .unwrap();
    assert_eq!(requested.queries().len(), 1);
}

#[test]
fn connection_commit_revalidation_budget_exhaustion_is_terminal() {
    let mut session = test_session(CanvasInteractionConfig::default());
    let ticket = begin_direct_connection_commit(&mut session);
    let requested = session
        .request_connection_commit_revalidation(ticket)
        .unwrap();
    let exhausted = session
        .resolve_connection_probe_for_test(
            connection_query_id(&requested),
            ConnectionProbeQueryOutcome::BudgetExhausted {
                work: ConnectionProbeWork::default(),
            },
        )
        .unwrap();

    assert!(exhausted.effects().is_empty());
    assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
    assert_eq!(
        session.presentation().terminal(),
        Some(InteractionTerminalReason::BudgetExhausted)
    );
}

#[test]
fn release_budget_exhaustion_is_terminal_without_graph_mutation() {
    let mut session = test_session(CanvasInteractionConfig::default());
    session
        .press_for_test(connection_target(false), position(0.0, 0.0))
        .unwrap();
    let moved = session.move_to(position(8.0, 0.0)).unwrap();
    session
        .resolve_connection_probe_for_test(
            connection_query_id(&moved),
            complete(valid_probe(target_port())),
        )
        .unwrap();
    let released = session.release(position(20.0, 0.0)).unwrap();
    let exhausted = session
        .resolve_connection_probe_for_test(
            connection_query_id(&released),
            ConnectionProbeQueryOutcome::BudgetExhausted {
                work: ConnectionProbeWork::default(),
            },
        )
        .unwrap();

    assert!(exhausted.effects().is_empty());
    assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
    assert_eq!(
        session.presentation().terminal(),
        Some(InteractionTerminalReason::BudgetExhausted)
    );
}

#[test]
fn search_failure_retry_and_rejected_selection_keep_one_search_owner() {
    let mut session = test_session(CanvasInteractionConfig::default());
    session
        .press_for_test(connection_target(true), position(0.0, 0.0))
        .unwrap();
    let moved = session.move_to(position(8.0, 0.0)).unwrap();
    session
        .resolve_connection_probe_for_test(
            connection_query_id(&moved),
            complete(ConnectionProbe::None),
        )
        .unwrap();
    let released = session.release(position(20.0, 0.0)).unwrap();
    let searching = session
        .resolve_connection_probe_for_test(
            connection_query_id(&released),
            complete(ConnectionProbe::None),
        )
        .unwrap();
    let [CanvasInteractionEffect::Search(request)] = searching.effects() else {
        panic!("expected one connection search request");
    };
    assert_eq!(
        request.token.binding.fixed_port_instance().port(),
        source_port()
    );
    assert_eq!(
        request.token.binding.mode(),
        NodeGraphConnectionMode::Strict
    );
    assert_eq!(
        session.presentation().connection().unwrap().search(),
        Some(ConnectionSearchStatus::Loading)
    );

    session.search_failed(request).unwrap();
    assert_eq!(
        session.presentation().connection().unwrap().search(),
        Some(ConnectionSearchStatus::Error)
    );
    let retry = session.retry_search(request).unwrap();
    let [CanvasInteractionEffect::Search(retry_request)] = retry.effects() else {
        panic!("retry must issue one successor search request");
    };
    assert_ne!(retry_request, request);
    assert!(matches!(
        session.search_ready(request, 99),
        Err(CanvasInteractionError::ConnectionSearchStaleGeneration)
    ));
    session.search_ready(retry_request, 2).unwrap();

    let selected = session
        .select_search_result(search_selection(retry_request))
        .unwrap();
    let ticket = submit_ticket(&selected);
    complete_with_kind(&mut session, ticket, CommandExecutionKind::Rejected).unwrap();
    assert_eq!(
        session.presentation().phase(),
        CanvasInteractionPhase::ConnectionSearch
    );
    assert_eq!(
        session.presentation().connection().unwrap().search(),
        Some(ConnectionSearchStatus::CommitRejected)
    );

    session.cancel().unwrap();
    assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
}

#[test]
fn search_callbacks_reject_inactive_and_stale_requests_without_state_changes() {
    let mut session = test_session(CanvasInteractionConfig::default());
    let CanvasPressTarget::Connection(start) = connection_target(true) else {
        unreachable!("connection fixture must produce a connection target");
    };
    session
        .activate_connection_search_for_test(start.clone(), position(12.0, 18.0))
        .unwrap();
    let stale_request = session.search_request().expect("first request");
    let retried = session.retry_search(&stale_request).unwrap();
    let [CanvasInteractionEffect::Search(current_request)] = retried.effects() else {
        panic!("retry must publish a successor request");
    };
    let stale_selection = search_selection(&stale_request);
    assert!(matches!(
        session.search_ready(&stale_request, 1),
        Err(CanvasInteractionError::ConnectionSearchStaleGeneration)
    ));
    assert!(matches!(
        session.search_failed(&stale_request),
        Err(CanvasInteractionError::ConnectionSearchStaleGeneration)
    ));
    assert!(matches!(
        session.retry_search(&stale_request),
        Err(CanvasInteractionError::ConnectionSearchStaleGeneration)
    ));
    assert!(matches!(
        session.select_search_result(stale_selection),
        Err(CanvasInteractionError::ConnectionSearchStaleGeneration)
    ));
    assert_eq!(
        session.presentation().phase(),
        CanvasInteractionPhase::ConnectionSearch
    );
    assert_eq!(session.search_request(), Some(current_request.clone()));

    session.search_ready(current_request, 1).unwrap();
    let current_generation = current_request.generation();
    session
        .select_search_result(search_selection(current_request))
        .unwrap();
    assert_eq!(
        session.presentation().phase(),
        CanvasInteractionPhase::Committing
    );
    assert!(matches!(
        session.search_ready(current_request, 1),
        Err(CanvasInteractionError::ConnectionSearchInactive)
    ));
    assert!(matches!(
        session.search_failed(current_request),
        Err(CanvasInteractionError::ConnectionSearchInactive)
    ));
    assert!(matches!(
        session.retry_search(current_request),
        Err(CanvasInteractionError::ConnectionSearchInactive)
    ));
    assert_eq!(
        session.presentation().phase(),
        CanvasInteractionPhase::Committing
    );
    assert_eq!(
        session.presentation().generation(),
        Some(current_generation)
    );
}

#[test]
fn interaction_identities_are_namespaced_per_canvas_session() {
    let mut first = test_session(CanvasInteractionConfig::default());
    let mut second = test_session(CanvasInteractionConfig::default());
    first
        .activate_connection_search_for_test(connection_start(true), position(0.0, 0.0))
        .unwrap();
    second
        .activate_connection_search_for_test(connection_start(true), position(0.0, 0.0))
        .unwrap();
    let first_request = first.search_request().expect("first search request");
    let second_request = second.search_request().expect("second search request");
    assert_eq!(first_request.generation().0.sequence, 1);
    assert_eq!(second_request.generation().0.sequence, 1);
    assert_ne!(first_request.generation(), second_request.generation());

    assert!(matches!(
        second.search_ready(&first_request, 1),
        Err(CanvasInteractionError::ForeignInteractionSession { .. })
    ));
    assert_eq!(second.search_request(), Some(second_request.clone()));
    let foreign_selection = search_selection(&first_request);
    assert!(matches!(
        second.select_search_result(foreign_selection),
        Err(CanvasInteractionError::ForeignInteractionSession { .. })
    ));
    assert_eq!(second.search_request(), Some(second_request.clone()));
    second.search_ready(&second_request, 1).unwrap();
    second
        .select_search_result(search_selection(&second_request))
        .unwrap();
    assert_eq!(
        second.presentation().phase(),
        CanvasInteractionPhase::Committing
    );

    let mut first_query_session = test_session(CanvasInteractionConfig::default());
    let mut second_query_session = test_session(CanvasInteractionConfig::default());
    let first_query = edge_query_id(
        &first_query_session
            .request_edge_pick(position(0.0, 0.0))
            .unwrap(),
    );
    let second_query = edge_query_id(
        &second_query_session
            .request_edge_pick(position(0.0, 0.0))
            .unwrap(),
    );
    assert_eq!(first_query.0.sequence, 1);
    assert_eq!(second_query.0.sequence, 1);
    assert_ne!(first_query, second_query);

    let mut first_commit = test_session(CanvasInteractionConfig::default());
    let mut second_commit = test_session(CanvasInteractionConfig::default());
    let first_submission = begin_direct_connection_submission(&mut first_commit);
    let first_ticket = first_submission.ticket();
    let second_submission = begin_direct_connection_submission(&mut second_commit);
    let second_ticket = second_submission.ticket();
    assert_eq!(first_ticket.0.sequence, 1);
    assert_eq!(second_ticket.0.sequence, 1);
    assert_ne!(first_ticket, second_ticket);
    assert!(matches!(
        second_commit.submission_not_dispatched(first_ticket),
        Err(CanvasInteractionError::ForeignInteractionSession { .. })
    ));
    assert_eq!(
        second_commit.presentation().connection().unwrap().ticket(),
        Some(second_ticket)
    );

    let runtime = GraphRuntime::builder(Default::default())
        .build(prospective_graph())
        .expect("prefix test runtime");
    let before = runtime.snapshot().expect("snapshot before foreign prefix");
    assert!(matches!(
        second_commit.resume_submission_after_prefix(first_submission),
        Err(CanvasInteractionError::ForeignInteractionSession { .. })
    ));
    let after = runtime.snapshot().expect("snapshot after foreign prefix");
    assert!(before.ptr_eq(&after));
    assert_eq!(
        second_commit.presentation().connection().unwrap().ticket(),
        Some(second_ticket)
    );
    complete_with_kind(
        &mut second_commit,
        second_ticket,
        CommandExecutionKind::Rejected,
    )
    .unwrap();
    assert_eq!(
        second_commit.presentation().phase(),
        CanvasInteractionPhase::Idle
    );
}

#[test]
fn cancelled_submission_cannot_cross_the_transport_acceptance_boundary() {
    let (runtime, view, mut session) = bound_test_session();
    let submission = bound_nudge_submission(&view, &mut session);
    let ticket = submission.ticket();

    session.cancel().unwrap();

    assert!(matches!(
        session.bind_submission(submission),
        Err(InteractionGraphSubmissionBindError::Superseded {
            ticket: rejected,
            current: None,
        }) if rejected == ticket
    ));
    assert_eq!(
        runtime
            .snapshot()
            .expect("unchanged cancelled snapshot")
            .graph()
            .node(&source_node())
            .expect("source node remains present")
            .pos,
        CanvasPoint::default()
    );
}

#[test]
fn closed_session_rejects_an_unbound_submission() {
    let (_runtime, view, mut session) = bound_test_session();
    let submission = bound_nudge_submission(&view, &mut session);

    session.close();

    assert!(matches!(
        session.bind_submission(submission),
        Err(InteractionGraphSubmissionBindError::Closed)
    ));
}

#[test]
fn older_same_session_submission_is_superseded_by_the_current_ticket() {
    let (_runtime, view, mut session) = bound_test_session();
    let stale = bound_nudge_submission(&view, &mut session);
    let stale_ticket = stale.ticket();
    complete_with_kind(&mut session, stale_ticket, CommandExecutionKind::Rejected)
        .expect("first nudge settles before the successor");
    let current = bound_nudge_submission(&view, &mut session);
    let current_ticket = current.ticket();

    assert!(matches!(
        session.bind_submission(stale),
        Err(InteractionGraphSubmissionBindError::Superseded {
            ticket: rejected,
            current: Some(active),
        }) if rejected == stale_ticket && active == current_ticket
    ));
    assert!(
        session
            .bind_submission(current)
            .expect("current ticket owns transport acceptance")
            .is_some()
    );
}

#[test]
fn foreign_submission_and_ticket_fail_before_an_unavailable_bound_view_is_touched() {
    let (_runtime, view, target) = bound_test_session();
    let mut bind_source =
        CanvasInteractionSession::attach(&view, CanvasInteractionConfig::default())
            .expect("foreign bind source");
    let mut resume_source =
        CanvasInteractionSession::attach(&view, CanvasInteractionConfig::default())
            .expect("foreign resume source");
    let bind_submission = bound_nudge_submission(&view, &mut bind_source);
    let resume_submission = bound_nudge_submission(&view, &mut resume_source);
    let foreign_ticket = resume_submission.ticket();
    view.close().expect("bound view closes");
    let mut target = target;

    assert!(matches!(
        target.bind_submission(bind_submission),
        Err(InteractionGraphSubmissionBindError::ForeignInteractionSession {
            expected,
            actual,
        }) if expected == target.id() && actual == bind_source.id()
    ));
    assert!(matches!(
        target.resume_submission_after_prefix(resume_submission),
        Err(CanvasInteractionError::ForeignInteractionSession {
            expected,
            actual,
        }) if expected == target.id() && actual == resume_source.id()
    ));
    assert!(matches!(
        target.submission_not_dispatched(foreign_ticket),
        Err(CanvasInteractionError::ForeignInteractionSession {
            expected,
            actual,
        }) if expected == target.id() && actual == resume_source.id()
    ));
}

#[test]
fn callback_authority_orders_foreign_stale_closed_then_view_unavailable() {
    let (_runtime, view, mut session) = bound_test_session();
    let foreign = CanvasInteractionSession::attach(&view, CanvasInteractionConfig::default())
        .expect("foreign callback session");

    view.with_state_borrowed_for_test(|| {
        assert!(matches!(
            session.callback_authority(foreign.id(), None, false),
            Err(CanvasInteractionError::ForeignInteractionSession { .. })
        ));
        assert_eq!(
            session
                .callback_authority(session.id(), None, false)
                .expect("stale work never consults the view"),
            CallbackAuthority::Stale
        );
        assert!(matches!(
            session.callback_authority(session.id(), None, true),
            Err(CanvasInteractionError::ViewUnavailable)
        ));
    });

    session.closed = true;
    assert!(matches!(
        session.callback_authority(session.id(), None, true),
        Err(CanvasInteractionError::Closed)
    ));
    assert_eq!(
        session
            .callback_authority(session.id(), None, false)
            .expect("same-session stale work precedes closed authority"),
        CallbackAuthority::Stale
    );
}

#[test]
fn late_same_session_callbacks_keep_their_stale_semantics_while_the_view_is_busy() {
    let (_runtime, view, mut query_session) = bound_test_session();
    let query = take_opaque_query(
        query_session
            .request_edge_pick(bound_position(&query_session, &view, 2.0, 2.0))
            .expect("issue opaque query"),
    );
    query_session
        .clear_edge_hover()
        .expect("supersede opaque query");

    let mut settle_session =
        CanvasInteractionSession::attach(&view, CanvasInteractionConfig::default())
            .expect("settlement callback session");
    let settle_submission = bound_nudge_submission(&view, &mut settle_session);
    let stale_ticket = settle_submission.ticket();
    settle_session
        .cancel()
        .expect("supersede settlement ticket");

    let mut prefix_session =
        CanvasInteractionSession::attach(&view, CanvasInteractionConfig::default())
            .expect("prefix callback session");
    let stale_prefix = bound_nudge_submission(&view, &mut prefix_session);
    prefix_session.cancel().expect("supersede prefix ticket");

    let mut search_session =
        CanvasInteractionSession::attach(&view, CanvasInteractionConfig::default())
            .expect("search callback session");
    let search_position = bound_position(&search_session, &view, 0.0, 0.0);
    search_session
        .activate_connection_search(bound_connection_start(&view), search_position)
        .expect("activate bound search");
    let stale_search = search_session
        .search_request()
        .expect("first search request");
    search_session
        .retry_search(&stale_search)
        .expect("supersede search request");

    let query_before = session_fingerprint(&query_session);
    let settle_before = session_fingerprint(&settle_session);
    let prefix_before = session_fingerprint(&prefix_session);
    let search_before = session_fingerprint(&search_session);
    view.with_state_borrowed_for_test(|| {
        let query_update = query_session
            .resolve_query(query)
            .expect("stale opaque query is a no-op");
        assert!(query_update.queries().is_empty());
        assert!(query_update.effects().is_empty());

        let settle_update = settle_session
            .submission_outcome_unknown(stale_ticket)
            .expect("stale settlement is a no-op");
        assert!(settle_update.queries().is_empty());
        assert!(settle_update.effects().is_empty());

        let prefix_update = prefix_session
            .resume_submission_after_prefix(stale_prefix)
            .expect("stale prefix continuation is a no-op");
        assert!(prefix_update.queries().is_empty());
        assert!(prefix_update.effects().is_empty());

        assert!(matches!(
            search_session.search_ready(&stale_search, 1),
            Err(CanvasInteractionError::ConnectionSearchStaleGeneration)
        ));
    });
    assert_eq!(session_fingerprint(&query_session), query_before);
    assert_eq!(session_fingerprint(&settle_session), settle_before);
    assert_eq!(session_fingerprint(&prefix_session), prefix_before);
    assert_eq!(session_fingerprint(&search_session), search_before);
}

#[test]
fn opaque_query_resolution_is_session_owned_and_superseded_work_is_a_noop() {
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
    let first_query = take_opaque_query(first_session.request_edge_pick(first_position).unwrap());
    let second_position = bound_position(&second_session, &view, 4.0, 0.0);
    let second_query =
        take_opaque_query(second_session.request_edge_pick(second_position).unwrap());
    let before = session_fingerprint(&second_session);
    assert!(matches!(
        second_session.resolve_query(first_query),
        Err(CanvasInteractionError::ForeignInteractionSession { .. })
    ));
    assert_eq!(session_fingerprint(&second_session), before);
    let resolved = second_session
        .resolve_query(second_query)
        .expect("the bound session resolves its own opaque query");
    assert!(resolved.effects().is_empty());

    let stale_update = session
        .request_edge_pick(bound_position(&session, &view, 2.0, 0.0))
        .unwrap();
    let stale_query = take_opaque_query(stale_update);
    session.clear_edge_hover().unwrap();
    let before_stale = session_fingerprint(&session);
    let stale = session.resolve_query(stale_query).unwrap();
    assert!(stale.effects().is_empty());
    assert!(stale.queries().is_empty());
    assert_eq!(session_fingerprint(&session), before_stale);
}

#[test]
fn opaque_prefix_continuation_reemits_direct_submission_without_classification() {
    let (_runtime, view, mut session) = bound_test_session();
    set_selection_by_id(&view, [source_node()], [], Vec::new())
        .expect("select movable source node");
    let submission = take_submission(
        session
            .nudge_selection(NodeNudgeDirection::Right, false)
            .expect("nudge emits one direct submission"),
    );
    let ticket = submission.ticket();

    let resumed = session
        .resume_submission_after_prefix(submission)
        .expect("direct prefix continuation is session owned");
    let rebound = take_submission(resumed);

    assert_eq!(rebound.ticket(), ticket);
    assert_eq!(
        session.presentation().phase(),
        CanvasInteractionPhase::Committing
    );
    assert!(
        session
            .bind_submission(rebound)
            .expect("direct submission binds after prefix")
            .is_some()
    );
}

#[test]
fn old_prefix_ticket_does_not_reconcile_a_retired_current_owner() {
    let runtime = GraphRuntime::builder(Default::default())
        .build(prospective_graph())
        .expect("retirement runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("retirement view");
    let start = bound_connection_start(&view);
    let mut session = CanvasInteractionSession::attach(&view, CanvasInteractionConfig::default())
        .expect("retirement interaction session");

    let first_submission = begin_bound_search_submission(&mut session, &view, start.clone());
    let first_ticket = first_submission.ticket();
    complete_with_kind(&mut session, first_ticket, CommandExecutionKind::Applied).unwrap();
    let current_submission = begin_bound_search_submission(&mut session, &view, start);
    let current_ticket = current_submission.ticket();
    assert_ne!(first_ticket, current_ticket);

    let snapshot = runtime.snapshot().expect("snapshot before retirement");
    runtime
        .execute(crate::runtime::commands::GraphCommand::topology(
            snapshot.version(),
            crate::runtime::commands::TopologyEdit::new().remove_node(
                source_node(),
                crate::runtime::commands::IncidentEdgePolicy::Reject,
            ),
        ))
        .into_result()
        .expect("owner retirement command")
        .applied()
        .expect("owner retirement applies");

    let terminal_before = session.presentation().terminal();
    let tick_demand_before = session.tick_demand();
    let stale = session
        .resume_submission_after_prefix(first_submission)
        .expect("old same-session ticket is an ignored late callback");
    assert!(stale.effects().is_empty());
    assert_eq!(stale.damage(), InteractionDamage::default());
    assert_eq!(
        session.presentation().phase(),
        CanvasInteractionPhase::Committing
    );
    assert_eq!(
        session.presentation().connection().unwrap().ticket(),
        Some(current_ticket)
    );
    assert_eq!(session.presentation().terminal(), terminal_before);
    assert_eq!(session.tick_demand(), tick_demand_before);

    let reconciled = session
        .resume_submission_after_prefix(current_submission)
        .expect("current ticket performs reconciliation");
    assert!(reconciled.effects().is_empty());
    assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
    assert_eq!(
        session.presentation().terminal(),
        Some(InteractionTerminalReason::OwnerRetired)
    );
}

#[test]
fn replaced_source_port_cannot_reuse_an_old_search_capability() {
    let runtime = GraphRuntime::builder(Default::default())
        .build(prospective_graph())
        .expect("replacement runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("replacement view");
    let start = bound_connection_start(&view);
    let old_source = start.binding.fixed_port_instance();
    let mut session = CanvasInteractionSession::attach(&view, CanvasInteractionConfig::default())
        .expect("replacement interaction session");
    let search_position = bound_position(&session, &view, 0.0, 0.0);
    session
        .activate_connection_search(start, search_position)
        .expect("bound search activation");
    let request = session.search_request().expect("old source request");
    let selection = session
        .create_search_selection(&request, target_port(), prospective_creation())
        .expect("selection snapshot")
        .expect("prospective selection");

    let admitted = runtime.snapshot().expect("admitted source snapshot");
    runtime
        .execute(crate::runtime::commands::GraphCommand::topology(
            admitted.version(),
            crate::runtime::commands::TopologyEdit::new().remove_node(
                source_node(),
                crate::runtime::commands::IncidentEdgePolicy::Reject,
            ),
        ))
        .into_result()
        .expect("source retirement command")
        .applied()
        .expect("source retirement applies");
    let retired = runtime.snapshot().expect("retired source snapshot");
    runtime
        .execute(crate::runtime::commands::GraphCommand::undo(
            retired.version(),
        ))
        .into_result()
        .expect("source restore command")
        .applied()
        .expect("source restore applies");
    let restored = runtime.snapshot().expect("restored source snapshot");
    let restored_source = restored
        .port_instance_key(source_port())
        .expect("restored raw port id");
    assert_ne!(old_source, restored_source);
    assert!(matches!(
        session.create_search_selection(&request, target_port(), prospective_creation()),
        Err(CanvasInteractionError::StaleInteractionFacts)
    ));

    let submitted = session
        .select_search_result(selection)
        .expect("request identity remains exact until graph bind");
    let mut effects = submitted.into_dispatch().effects.into_iter();
    let Some(CanvasInteractionEffect::SubmitGraph { submission }) = effects.next() else {
        panic!("selection emits one sealed mutation");
    };
    assert!(effects.next().is_none());
    assert!(matches!(
        session.bind_submission(submission),
        Err(InteractionGraphSubmissionBindError::Mutation(
            super::super::FreshGraphMutationBindError::ConnectionBindingRetired { edge },
        ))
            if edge == EdgeId::from_u128(9)
    ));
    let after = runtime.snapshot().expect("snapshot after rejected bind");
    assert!(restored.ptr_eq(&after));
    assert!(after.graph().node(&NodeId::from_u128(2)).is_none());
}

#[test]
fn native_activation_enters_the_same_connection_search_without_fake_pointer_motion() {
    let mut session = test_session(CanvasInteractionConfig::default());
    let CanvasPressTarget::Connection(start) = connection_target(true) else {
        unreachable!("connection fixture must produce a connection target");
    };

    let activated = session
        .activate_connection_search_for_test(start.clone(), position(12.0, 18.0))
        .unwrap();
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
    assert_eq!(session.search_request(), Some(request.clone()));

    let mut unavailable = test_session(CanvasInteractionConfig::default());
    let CanvasPressTarget::Connection(start) = connection_target(false) else {
        unreachable!("connection fixture must produce a connection target");
    };
    assert!(matches!(
        unavailable.activate_connection_search_for_test(start, position(0.0, 0.0)),
        Err(CanvasInteractionError::ConnectionSearchUnavailable)
    ));
    assert_eq!(
        unavailable.presentation().phase(),
        CanvasInteractionPhase::Idle
    );
}

#[test]
fn reconnect_search_keeps_edge_identity_through_selection_and_revision_refresh() {
    let edge = EdgeId::from_u128(9);
    let intent = ConnectionGestureBinding::reconnect(
        edge_instance(edge),
        EdgeEndpoint::To,
        port_instance(source_node(), PortId::from_u128(4)),
        port_instance(source_node(), source_port()),
        NodeGraphConnectionMode::Strict,
    )
    .unwrap();
    let start = ConnectionInteractionStart::new(
        intent.clone(),
        test_graph_version(),
        CanvasPoint { x: 0.0, y: 8.0 },
        HandlePosition::Right,
        true,
    )
    .unwrap();
    let mut session = test_session(CanvasInteractionConfig::default());

    let activated = session
        .activate_connection_search_for_test(start, position(12.0, 18.0))
        .unwrap();
    let [CanvasInteractionEffect::Search(request)] = activated.effects() else {
        panic!("native reconnect activation must publish one search request");
    };
    assert_eq!(request.token.binding, intent);
    session.search_ready(request, 1).unwrap();

    let selected = session
        .select_search_result(search_selection(request))
        .unwrap();
    let [CanvasInteractionEffect::SubmitGraph { submission }] = selected.effects() else {
        panic!("reconnect search selection must submit one atomic connection mutation");
    };
    let ticket = submission.ticket();
    let (selected_binding, target, _, create_from_wire) = submission
        .connection_facts_for_test()
        .expect("selection submits a connection mutation");
    assert_eq!(selected_binding, intent);
    assert_eq!(target, target_port());
    assert!(create_from_wire);

    let snapshot = prospective_snapshot();
    let refreshed = session
        .refresh_create_from_wire_commit_from_snapshot(ticket, &snapshot)
        .unwrap();
    let [CanvasInteractionEffect::SubmitGraph { submission }] = refreshed.effects() else {
        panic!("revision refresh must retain the atomic reconnect mutation");
    };
    let (refreshed_binding, refreshed_target, validated_revision, create_from_wire) = submission
        .connection_facts_for_test()
        .expect("refresh submits a connection mutation");
    assert_eq!(submission.ticket(), ticket);
    assert_eq!(refreshed_binding, intent);
    assert_eq!(refreshed_target, target_port());
    assert_eq!(validated_revision, snapshot.version());
    assert!(create_from_wire);
}

#[test]
fn generation_exhaustion_is_reported_without_arming_an_interaction() {
    let mut session = test_session(CanvasInteractionConfig::default());
    session.next_generation = None;

    assert!(matches!(
        session.press_for_test(CanvasPressTarget::Pan, position(0.0, 0.0)),
        Err(CanvasInteractionError::IdentityExhausted)
    ));
    assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
    assert_eq!(
        session.presentation().terminal(),
        Some(InteractionTerminalReason::IdentityExhausted)
    );
}

#[test]
fn repeated_toggle_marquee_moves_reuse_the_press_time_selection_baseline() {
    let selected_node = NodeId::from_u128(21);
    let selected_edge = EdgeId::from_u128(22);
    let baseline = SelectionBaseline::from_ids_for_test([selected_node], [selected_edge], []);
    let mut session = test_session(CanvasInteractionConfig::default());
    session
        .press_for_test(
            CanvasPressTarget::Marquee {
                selection_mode: SelectionMode::Toggle,
                baseline: baseline.clone(),
            },
            position(0.0, 0.0),
        )
        .unwrap();

    let first_query = session.move_to(position(20.0, 16.0)).unwrap();
    let first = resolve_marquee_query(&mut session, &first_query, [selected_node]);
    let second_query = session.move_to(position(20.0, 16.0)).unwrap();
    let second = resolve_marquee_query(&mut session, &second_query, [selected_node]);
    let first_selection = marquee_selection_effect(&first);
    let second_selection = marquee_selection_effect(&second);

    assert_eq!(first_selection, second_selection);
    let SelectionMutation::Marquee {
        mode,
        baseline: emitted_baseline,
        ..
    } = first_selection
    else {
        panic!("marquee move must emit a marquee selection effect");
    };
    assert_eq!(*mode, SelectionMode::Toggle);
    assert_eq!(emitted_baseline, &baseline);

    let cancelled = session.cancel().unwrap();
    assert!(matches!(
        cancelled.selection_mutations_for_test(),
        [SelectionMutation::Restore(restored)] if restored == &baseline
    ));
    assert!(cancelled.damage().selection);
    assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
}

#[test]
fn marquee_auto_pan_reprojects_before_requery_and_rejects_superseded_selection() {
    for zoom in [0.5, 1.0, 2.0] {
        let transform = ViewportTransform::new(CanvasPoint::default(), zoom).unwrap();
        let baseline = SelectionBaseline::from_ids_for_test([NodeId::from_u128(90)], [], []);
        let mut session = test_session(CanvasInteractionConfig::default());
        session.test_viewport =
            ViewportSnapshot::new(session.provenance, ViewportRevision::initial(), transform);
        let sampled_viewport = session.test_viewport;
        let at = |x, y| ViewportPointerSample::new(sampled_viewport, ViewPoint::new(x, y)).unwrap();
        session
            .press_for_test(
                CanvasPressTarget::Marquee {
                    selection_mode: SelectionMode::Add,
                    baseline: baseline.clone(),
                },
                at(20.0, 20.0),
            )
            .unwrap();
        let moved = session.move_to(at(195.0, 100.0)).unwrap();
        let superseded = marquee_query_id(&moved);
        assert_eq!(moved.tick_demand(), InteractionTickDemand::WhileActive);

        let ticked = session
            .tick(1.0 / 60.0, ViewSize::new(200.0, 120.0))
            .unwrap();
        let replacement = marquee_query_id(&ticked);
        assert_ne!(replacement, superseded);
        let dispatch = ticked.into_dispatch();
        assert!(dispatch.effects.is_empty());
        assert!(dispatch.damage.viewport);
        assert!(matches!(
            dispatch.queries.as_slice(),
            [query] if matches!(
                query.kind_for_test(),
                CanvasInteractionQueryKind::NodeIntersection { .. }
            )
        ));

        let late = session
            .resolve_node_intersection_for_test(
                superseded,
                NodeIntersectionQueryOutcome::Complete {
                    nodes: vec![NodeId::from_u128(91)],
                    work: Default::default(),
                },
            )
            .unwrap();
        assert!(late.effects().is_empty());

        let current = session
            .resolve_node_intersection_for_test(
                replacement,
                NodeIntersectionQueryOutcome::Complete {
                    nodes: vec![NodeId::from_u128(92)],
                    work: Default::default(),
                },
            )
            .unwrap();
        assert!(matches!(
            current.selection_mutations_for_test(),
            [SelectionMutation::Marquee {
                nodes,
                baseline: emitted,
                ..
            }] if nodes.iter().map(|node| node.node()).eq([NodeId::from_u128(92)])
                && emitted == &baseline
        ));
        let released = session.release(at(195.0, 100.0)).unwrap();
        assert_eq!(released.tick_demand(), InteractionTickDemand::None);
    }
}

#[test]
fn connection_target_lock_obeys_invalidation_and_retirement_boundaries() {
    let mut session = test_session(CanvasInteractionConfig::default());
    session
        .press_for_test(
            CanvasPressTarget::Connection(connection_start(false)),
            position(0.0, 0.0),
        )
        .unwrap();
    let first = session.move_to(position(20.0, 0.0)).unwrap();
    session
        .resolve_connection_probe_for_test(
            connection_query_id(&first),
            complete(valid_probe(target_port())),
        )
        .unwrap();
    assert_eq!(
        drag_target_lock(&session).map(|target| target.port()),
        Some(target_port())
    );

    let graph_stale = session.move_to(position(21.0, 0.0)).unwrap();
    let restarted = session
        .restart_stale_connection_query(
            connection_query_id(&graph_stale),
            ConnectionProbeCursorStaleReason::GraphRevision,
        )
        .unwrap();
    assert!(drag_target_lock(&session).is_none());
    session
        .resolve_connection_probe_for_test(
            connection_query_id(&restarted),
            complete(ConnectionProbe::Invalid {
                version: test_graph_version(),
                target: probe_target(target_port(), CanvasPoint { x: 21.0, y: 0.0 }),
                diagnostics: Vec::new(),
            }),
        )
        .unwrap();
    assert!(
        drag_target_lock(&session).is_some(),
        "invalid targets also need stable hover"
    );

    let viewport_stale = session.move_to(position(22.0, 0.0)).unwrap();
    let retained = drag_target_lock(&session);
    let viewport_restarted = session
        .restart_stale_connection_query(
            connection_query_id(&viewport_stale),
            ConnectionProbeCursorStaleReason::ViewportTransform,
        )
        .unwrap();
    assert_eq!(drag_target_lock(&session), retained);
    session
        .resolve_connection_probe_for_test(
            connection_query_id(&viewport_restarted),
            complete(valid_probe(target_port())),
        )
        .unwrap();

    let paint_stale = session.move_to(position(23.0, 0.0)).unwrap();
    let retained = drag_target_lock(&session);
    let paint_restarted = session
        .restart_stale_connection_query(
            connection_query_id(&paint_stale),
            ConnectionProbeCursorStaleReason::PaintOrder,
        )
        .unwrap();
    assert_eq!(drag_target_lock(&session), retained);
    session
        .resolve_connection_probe_for_test(
            connection_query_id(&paint_restarted),
            complete(valid_probe(target_port())),
        )
        .unwrap();

    let geometry_stale = session.move_to(position(24.0, 0.0)).unwrap();
    session
        .restart_stale_connection_query(
            connection_query_id(&geometry_stale),
            ConnectionProbeCursorStaleReason::PortGeometry,
        )
        .unwrap();
    assert!(drag_target_lock(&session).is_none());

    let moved = session.move_to(position(25.0, 0.0)).unwrap();
    session
        .resolve_connection_probe_for_test(
            connection_query_id(&moved),
            complete(valid_probe(target_port())),
        )
        .unwrap();
    let retired = session.owner_retired(InteractionOwner::Port(port_instance(
        NodeId::from_u128(2),
        target_port(),
    )));
    assert!(drag_target_lock(&session).is_none());
    assert_eq!(retired.tick_demand(), InteractionTickDemand::WhileActive);
    session.cancel().unwrap();
    assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
}

#[test]
fn foreign_connection_start_rejection_preserves_pending_interaction_state() {
    let mut session = test_session(CanvasInteractionConfig::default());
    let pending = session.request_edge_pick(position(4.0, 5.0)).unwrap();
    let pending_query = edge_query_id(&pending);
    let before = session.presentation();
    let before_phase = before.phase();
    let before_terminal = before.terminal();
    let before_hover = before.hovered_edge();
    let before_generation = before.generation();
    let foreign = ConnectionInteractionStart::new_with_provenance(
        ViewProvenance::new(
            ViewId::new(crate::runtime::graph_runtime::RuntimeId::UNAVAILABLE, 99),
            ViewGeneration::initial(),
        ),
        test_graph_version(),
        ConnectionGestureBinding::connect_for_test(
            EdgeId::from_u128(90),
            source_port(),
            NodeGraphConnectionMode::Strict,
        ),
        EdgeEndpoint::From,
        CanvasPoint::default(),
        HandlePosition::Right,
        HandlePosition::Left,
        true,
    )
    .unwrap();

    assert!(matches!(
        session.press_for_test(
            CanvasPressTarget::Connection(foreign.clone()),
            position(0.0, 0.0),
        ),
        Err(CanvasInteractionError::ForeignInteractionFacts)
    ));
    let after_press = session.presentation();
    assert_eq!(after_press.phase(), before_phase);
    assert_eq!(after_press.terminal(), before_terminal);
    assert_eq!(after_press.hovered_edge(), before_hover);
    assert_eq!(after_press.generation(), before_generation);
    assert_eq!(
        session.pending_edge_pick.as_ref().map(|pending| pending.id),
        Some(pending_query)
    );
    assert!(matches!(
        session.activate_connection_search_for_test(foreign, position(0.0, 0.0)),
        Err(CanvasInteractionError::ForeignInteractionFacts)
    ));
    let after_search = session.presentation();
    assert_eq!(after_search.phase(), before_phase);
    assert_eq!(after_search.terminal(), before_terminal);
    assert_eq!(after_search.hovered_edge(), before_hover);
    assert_eq!(after_search.generation(), before_generation);
    assert_eq!(
        session.pending_edge_pick.as_ref().map(|pending| pending.id),
        Some(pending_query)
    );
}

#[test]
fn superseded_marquee_query_cannot_publish_selection() {
    let mut session = test_session(CanvasInteractionConfig::default());
    session
        .press_for_test(
            CanvasPressTarget::Marquee {
                selection_mode: SelectionMode::Replace,
                baseline: SelectionBaseline::default(),
            },
            position(0.0, 0.0),
        )
        .unwrap();

    let first = session.move_to(position(20.0, 16.0)).unwrap();
    let stale = marquee_query_id(&first);
    let second = session.move_to(position(30.0, 24.0)).unwrap();
    let current = marquee_query_id(&second);
    assert_ne!(stale, current);

    let late = session
        .resolve_node_intersection_for_test(
            stale,
            NodeIntersectionQueryOutcome::Complete {
                nodes: vec![NodeId::from_u128(21)],
                work: Default::default(),
            },
        )
        .unwrap();
    assert!(late.effects().is_empty());
    assert_eq!(
        session.presentation().phase(),
        CanvasInteractionPhase::Marquee
    );

    let resolved = session
        .resolve_node_intersection_for_test(
            current,
            NodeIntersectionQueryOutcome::Complete {
                nodes: vec![NodeId::from_u128(22)],
                work: Default::default(),
            },
        )
        .unwrap();
    assert!(matches!(
        resolved.selection_mutations_for_test(),
        [SelectionMutation::Marquee {
            nodes,
            ..
        }] if nodes.iter().map(|node| node.node()).eq([NodeId::from_u128(22)])
    ));
}

#[test]
fn released_marquee_waits_for_the_final_query_before_becoming_idle() {
    let mut session = test_session(CanvasInteractionConfig::default());
    session
        .press_for_test(
            CanvasPressTarget::Marquee {
                selection_mode: SelectionMode::Add,
                baseline: SelectionBaseline::default(),
            },
            position(0.0, 0.0),
        )
        .unwrap();
    let moved = session.move_to(position(20.0, 16.0)).unwrap();
    let superseded = marquee_query_id(&moved);

    let released = session.release(position(24.0, 18.0)).unwrap();
    let final_query = marquee_query_id(&released);
    assert_ne!(superseded, final_query);
    assert!(released.effects().is_empty());
    assert_eq!(
        session.presentation().phase(),
        CanvasInteractionPhase::Marquee
    );

    let late = session
        .resolve_node_intersection_for_test(
            superseded,
            NodeIntersectionQueryOutcome::Complete {
                nodes: vec![NodeId::from_u128(21)],
                work: Default::default(),
            },
        )
        .unwrap();
    assert!(late.effects().is_empty());
    assert_eq!(
        session.presentation().phase(),
        CanvasInteractionPhase::Marquee
    );

    let completed = session
        .resolve_node_intersection_for_test(
            final_query,
            NodeIntersectionQueryOutcome::Complete {
                nodes: vec![NodeId::from_u128(22)],
                work: Default::default(),
            },
        )
        .unwrap();
    assert!(matches!(
        completed.selection_mutations_for_test(),
        [SelectionMutation::Marquee { .. }]
    ));
    assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
}

#[test]
fn marquee_budget_exhaustion_never_publishes_partial_selection() {
    let mut session = test_session(CanvasInteractionConfig::default());
    session
        .press_for_test(
            CanvasPressTarget::Marquee {
                selection_mode: SelectionMode::Replace,
                baseline: SelectionBaseline::default(),
            },
            position(0.0, 0.0),
        )
        .unwrap();
    let query = session.move_to(position(20.0, 16.0)).unwrap();
    let exhausted = session
        .resolve_node_intersection_for_test(
            marquee_query_id(&query),
            NodeIntersectionQueryOutcome::BudgetExhausted {
                work: Default::default(),
            },
        )
        .unwrap();

    assert!(exhausted.effects().is_empty());
    assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
    assert_eq!(
        session.presentation().terminal(),
        Some(InteractionTerminalReason::BudgetExhausted)
    );
}

#[test]
fn node_drag_snaps_relative_intent_and_conflict_is_not_retried() {
    let config = CanvasInteractionConfig {
        grid_size_in_canvas: Some(CanvasSize {
            width: 10.0,
            height: 10.0,
        }),
        ..CanvasInteractionConfig::default()
    };
    let mut session = test_session(config);
    let first = NodeId::from_u128(10);
    let second = NodeId::from_u128(11);
    let start = node_drag_start(
        first,
        [first, second],
        CanvasPoint { x: 0.0, y: 0.0 },
        SelectionMode::Replace,
    );
    session
        .press_for_test(CanvasPressTarget::NodeDrag(start), position(0.0, 0.0))
        .unwrap();
    session.move_to(position(6.0, 14.0)).unwrap();
    assert_eq!(
        session.presentation().node_drag().unwrap().delta(),
        CanvasPoint { x: 10.0, y: 10.0 }
    );

    let released = session.release(position(6.0, 14.0)).unwrap();
    let mut effects = released.into_dispatch().effects;
    let CanvasInteractionEffect::SubmitGraph { submission } = effects.remove(0) else {
        panic!("node release must emit one graph intent");
    };
    let ticket = submission.ticket();
    let intent = submission
        .move_nodes_for_test()
        .expect("node release emits a move intent");
    assert_eq!(intent.nodes(), &[first, second]);
    assert_eq!(intent.delta(), CanvasPoint { x: 10.0, y: 10.0 });
    assert!(effects.is_empty());

    complete_with_kind(&mut session, ticket, CommandExecutionKind::Conflict).unwrap();
    let late = session.submission_outcome_unknown(ticket).unwrap();
    assert!(late.effects().is_empty());
    assert_eq!(
        session.presentation().terminal(),
        Some(InteractionTerminalReason::Conflict)
    );
}

#[test]
fn node_release_waits_for_the_final_alignment_query_without_requesting_ticks() {
    let mut session = test_session(CanvasInteractionConfig {
        alignment_snap: AlignmentSnapConfig::new(6.0, 9.0),
        ..CanvasInteractionConfig::default()
    });
    let node = NodeId::from_u128(10);
    let start = node_drag_start(node, [node], CanvasPoint::default(), SelectionMode::Replace);
    session
        .press_for_test(CanvasPressTarget::NodeDrag(start), position(0.0, 0.0))
        .unwrap();
    session.move_to(position(12.0, 0.0)).unwrap();

    let released = session.release(position(16.0, 0.0)).unwrap();
    let [query] = released.queries() else {
        panic!("release must request one final alignment result");
    };
    let CanvasInteractionQueryKind::AlignmentSnap { id, generation, .. } = query.kind_for_test()
    else {
        panic!("release must request one final alignment result");
    };
    assert!(released.effects().is_empty());
    assert_eq!(released.tick_demand(), InteractionTickDemand::None);
    assert!(
        session
            .presentation()
            .node_drag()
            .expect("release preview")
            .is_release_pending()
    );

    let committed = session
        .resolve_alignment_snap_for_test(
            *id,
            AlignmentSnapQueryOutcome::GeometryIndeterminate(AlignmentSnapWork::new(
                *generation,
                0,
            )),
        )
        .unwrap();
    assert!(matches!(
        committed.effects(),
        [CanvasInteractionEffect::SubmitGraph { .. }]
    ));
    assert_eq!(
        session.presentation().phase(),
        CanvasInteractionPhase::Committing
    );
}

#[test]
fn auto_pan_commits_viewport_before_issuing_alignment_query() {
    let mut session = test_session(CanvasInteractionConfig {
        alignment_snap: AlignmentSnapConfig::new(6.0, 9.0),
        ..CanvasInteractionConfig::default()
    });
    let node = NodeId::from_u128(10);
    let start = node_drag_start(node, [node], CanvasPoint::default(), SelectionMode::Replace);
    session
        .press_for_test(CanvasPressTarget::NodeDrag(start), position(0.0, 0.0))
        .unwrap();
    session.move_to(position(195.0, 40.0)).unwrap();

    let dispatch = session
        .tick(1.0 / 60.0, ViewSize::new(200.0, 120.0))
        .unwrap()
        .into_dispatch();
    assert!(dispatch.effects.is_empty());
    assert!(dispatch.damage.viewport);
    assert_ne!(
        session.test_viewport.revision(),
        ViewportRevision::initial()
    );
    assert!(matches!(
        dispatch.queries.as_slice(),
        [query] if matches!(
            query.kind_for_test(),
            CanvasInteractionQueryKind::AlignmentSnap { .. }
        )
    ));
}

#[test]
fn failed_auto_pan_reprojection_preserves_node_drag_state() {
    let node = NodeId::from_u128(10);
    let start = node_drag_start(node, [node], CanvasPoint::default(), SelectionMode::Replace);
    let owner = start.clicked_lifetime();
    let mut session = test_session(CanvasInteractionConfig {
        alignment_snap: AlignmentSnapConfig::new(6.0, 9.0),
        ..CanvasInteractionConfig::default()
    });
    session
        .press_for_test(CanvasPressTarget::NodeDrag(start), position(100.0, 40.0))
        .unwrap();
    session.move_to(position(195.0, 40.0)).unwrap();

    let (
        generation,
        pointer,
        raw_delta,
        delta,
        guides,
        alignment_locks,
        pending_alignment_query,
        release_pending,
    ) = match &session.state {
        InteractionState::NodeDrag(drag) => (
            drag.generation,
            drag.pointer,
            drag.raw_delta,
            drag.delta,
            Arc::clone(&drag.guides),
            drag.alignment_locks,
            drag.pending_alignment_query,
            drag.release_pending,
        ),
        state => panic!("expected node drag before failed reprojection, got {state:?}"),
    };
    let next_generation = session.next_generation;
    let next_query = session.next_query;
    let next_ticket = session.next_ticket;

    session.test_viewport = ViewportSnapshot::new(
        session.provenance,
        ViewportRevision::initial(),
        overflowing_auto_pan_transform(),
    );
    assert!(matches!(
        session.tick(0.1, ViewSize::new(200.0, 120.0)),
        Err(CanvasInteractionError::InvalidPosition)
    ));

    let InteractionState::NodeDrag(drag) = &session.state else {
        panic!("failed reprojection must preserve the node drag state");
    };
    assert_eq!(drag.generation, generation);
    assert_eq!(drag.pointer, pointer);
    assert_eq!(drag.raw_delta, raw_delta);
    assert_eq!(drag.delta, delta);
    assert_eq!(drag.guides, guides);
    assert_eq!(drag.alignment_locks, alignment_locks);
    assert_eq!(drag.pending_alignment_query, pending_alignment_query);
    assert_eq!(drag.release_pending, release_pending);
    assert_eq!(session.next_generation, next_generation);
    assert_eq!(session.next_query, next_query);
    assert_eq!(session.next_ticket, next_ticket);
    assert_eq!(
        session.presentation().phase(),
        CanvasInteractionPhase::NodeDrag
    );
    assert_eq!(session.presentation().generation(), Some(generation));
    assert!(session.uses_owner(InteractionOwner::Node(owner)));
}

#[test]
fn failed_auto_pan_reprojection_preserves_reconnect_state_and_owners() {
    let edge = EdgeId::from_u128(91);
    let start = ConnectionInteractionStart::new(
        ConnectionGestureBinding::reconnect(
            edge_instance(edge),
            EdgeEndpoint::From,
            port_instance(source_node(), source_port()),
            port_instance(NodeId::from_u128(2), target_port()),
            NodeGraphConnectionMode::Strict,
        )
        .unwrap(),
        test_graph_version(),
        CanvasPoint { x: 32.0, y: 8.0 },
        HandlePosition::Left,
        false,
    )
    .unwrap();
    let mut session = test_session(CanvasInteractionConfig::default());
    session
        .press_for_test(
            CanvasPressTarget::Connection(start.clone()),
            position(100.0, 40.0),
        )
        .unwrap();
    session.move_to(position(195.0, 40.0)).unwrap();

    let (generation, pointer, pending_query, stale_query_restarts, release_pending) =
        match &session.state {
            InteractionState::Connection(ConnectionInteraction::Dragging(drag)) => (
                drag.generation,
                drag.pointer,
                drag.pending_query,
                drag.stale_query_restarts,
                drag.release_pending,
            ),
            state => {
                panic!("expected reconnect drag before failed reprojection, got {state:?}")
            }
        };
    let next_generation = session.next_generation;
    let next_query = session.next_query;
    let next_ticket = session.next_ticket;

    session.test_viewport = ViewportSnapshot::new(
        session.provenance,
        ViewportRevision::initial(),
        overflowing_auto_pan_transform(),
    );
    assert!(matches!(
        session.tick(0.1, ViewSize::new(200.0, 120.0)),
        Err(CanvasInteractionError::InvalidPosition)
    ));

    let InteractionState::Connection(ConnectionInteraction::Dragging(drag)) = &session.state else {
        panic!("failed reprojection must preserve the reconnect drag state");
    };
    assert_eq!(drag.generation, generation);
    assert_eq!(drag.start, start);
    assert_eq!(drag.pointer, pointer);
    assert_eq!(drag.pending_query, pending_query);
    assert_eq!(drag.stale_query_restarts, stale_query_restarts);
    assert_eq!(drag.release_pending, release_pending);
    assert!(matches!(&drag.probe, ConnectionProbe::None));
    assert_eq!(session.next_generation, next_generation);
    assert_eq!(session.next_query, next_query);
    assert_eq!(session.next_ticket, next_ticket);
    assert_eq!(
        session.presentation().phase(),
        CanvasInteractionPhase::ConnectionDrag
    );
    assert_eq!(session.presentation().generation(), Some(generation));
    for owner in [
        InteractionOwner::Node(port_instance(source_node(), source_port()).owner()),
        InteractionOwner::Port(port_instance(source_node(), source_port())),
        InteractionOwner::Port(port_instance(NodeId::from_u128(2), target_port())),
        InteractionOwner::Edge(edge_instance(edge)),
    ] {
        assert!(session.uses_owner(owner));
    }
}

#[test]
fn edge_delete_requires_authoritative_permission_and_owns_one_ticket_until_settlement() {
    let edge = EdgeId::from_u128(21);
    let denied = published_edge_route(edge, false, EdgeReconnectable::Bool(true));
    let allowed = published_edge_route(edge, true, EdgeReconnectable::Bool(true));
    let mut session = test_session(CanvasInteractionConfig::default());
    session
        .press_for_test(CanvasPressTarget::Pan, position(0.0, 0.0))
        .unwrap();
    session.cancel().unwrap();

    let version = test_graph_version();
    let ignored = session.delete_edge_at_version(&denied, version).unwrap();
    assert!(ignored.effects().is_empty());
    assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
    assert_eq!(session.presentation().generation(), None);
    assert_eq!(
        session.presentation().terminal(),
        Some(InteractionTerminalReason::Cancelled)
    );

    let submitted = session.delete_edge_at_version(&allowed, version).unwrap();
    let ticket = submit_ticket(&submitted);
    let [CanvasInteractionEffect::SubmitGraph { submission }] = submitted.effects() else {
        panic!("one permitted edge delete must submit exactly one mutation");
    };
    let intent = submission
        .delete_edge_for_test()
        .expect("edge delete emits a delete-edge mutation");
    assert_eq!(ticket.0.sequence, 1);
    assert_eq!(intent.edge(), edge);
    assert_eq!(
        session.presentation().phase(),
        CanvasInteractionPhase::Committing
    );
    assert_eq!(
        session.presentation().active_edge(),
        Some(allowed.instance())
    );
    assert!(submitted.damage().edges);
    assert!(matches!(
        session.delete_edge_at_version(&allowed, version),
        Err(CanvasInteractionError::AlreadyActive)
    ));

    let settled = complete_with_kind(&mut session, ticket, CommandExecutionKind::Rejected).unwrap();
    assert!(settled.effects().is_empty());
    assert!(settled.damage().edges);
    assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
    assert_eq!(
        session.presentation().terminal(),
        Some(InteractionTerminalReason::Rejected)
    );
    let late = session.submission_outcome_unknown(ticket).unwrap();
    assert!(late.effects().is_empty());
    assert_eq!(
        session.presentation().terminal(),
        Some(InteractionTerminalReason::Rejected)
    );
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
fn retiring_an_edge_during_delete_preserves_the_ticket_for_fresh_bind_noop() {
    let edge = EdgeId::from_u128(24);
    let runtime = GraphRuntime::builder(Default::default())
        .build(reconnect_graph(edge))
        .expect("edge-delete runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("edge-delete view");
    let snapshot = runtime.snapshot().expect("edge-delete snapshot");
    let route =
        published_edge_route_for_snapshot(&snapshot, edge, true, EdgeReconnectable::Bool(true));
    let mut session = CanvasInteractionSession::attach(&view, CanvasInteractionConfig::default())
        .expect("edge-delete interaction");
    let submitted = session
        .delete_edge_at_version(&route, snapshot.version())
        .unwrap();
    let submission = take_submission(submitted);
    let ticket = submission.ticket();

    runtime
        .execute(GraphCommand::disconnect(snapshot.version(), edge))
        .into_result()
        .expect("external edge retirement executes")
        .applied()
        .expect("external edge retirement applies");

    let retired = session.owner_retired(InteractionOwner::Edge(route.instance()));
    assert!(retired.effects().is_empty());
    assert_eq!(
        session.presentation().phase(),
        CanvasInteractionPhase::Committing
    );
    assert_eq!(session.presentation().active_edge(), Some(route.instance()));

    assert!(session.bind_submission(submission).unwrap().is_none());
    session.complete_submission_without_command(ticket).unwrap();
    assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
    assert_eq!(
        session.presentation().terminal(),
        Some(InteractionTerminalReason::Noop)
    );
}

#[test]
fn active_drag_requests_bounded_auto_pan_and_retirement_cancels_late_receipts() {
    let mut session = test_session(CanvasInteractionConfig::default());
    let node = NodeId::from_u128(10);
    let start = node_drag_start(
        node,
        [node],
        CanvasPoint { x: 0.0, y: 0.0 },
        SelectionMode::Replace,
    );
    let owner = start.clicked_lifetime();
    session
        .press_for_test(CanvasPressTarget::NodeDrag(start), position(10.0, 10.0))
        .unwrap();
    session.move_to(position(5.0, 5.0)).unwrap();
    let hover_cleared = session.clear_edge_hover().unwrap();
    assert_eq!(
        hover_cleared.tick_demand(),
        InteractionTickDemand::WhileActive,
        "adapter-local hover cleanup must not cancel an active auto-pan clock",
    );
    let before = session.test_viewport;
    let tick = session.tick(0.1, ViewSize::new(200.0, 120.0)).unwrap();
    assert!(tick.effects().is_empty());
    assert!(tick.damage().viewport);
    let pan = session.test_viewport.transform().pan;
    assert!(pan.x > before.transform().pan.x && pan.y > before.transform().pan.y);
    assert!(pan.x <= 36.0 && pan.y <= 36.0);

    session.owner_retired(InteractionOwner::Node(owner));
    assert_eq!(session.presentation().phase(), CanvasInteractionPhase::Idle);
    assert_eq!(
        session.presentation().terminal(),
        Some(InteractionTerminalReason::OwnerRetired)
    );
}

#[test]
fn every_node_drag_member_owns_the_armed_dragging_and_committing_phases() {
    let clicked = NodeId::from_u128(10);
    let peer = NodeId::from_u128(11);
    let start = || {
        node_drag_start(
            clicked,
            [clicked, peer],
            CanvasPoint::default(),
            SelectionMode::Replace,
        )
    };

    let mut armed = test_session(CanvasInteractionConfig::default());
    let armed_start = start();
    let armed_peer = *armed_start
        .members()
        .iter()
        .find(|member| member.node() == peer)
        .unwrap();
    armed
        .press_for_test(CanvasPressTarget::NodeDrag(armed_start), position(0.0, 0.0))
        .unwrap();
    assert!(armed.uses_owner(InteractionOwner::Node(armed_peer)));
    armed.owner_retired(InteractionOwner::Node(armed_peer));
    assert_eq!(armed.presentation().phase(), CanvasInteractionPhase::Idle);

    let mut dragging = test_session(CanvasInteractionConfig::default());
    let dragging_start = start();
    let dragging_peer = *dragging_start
        .members()
        .iter()
        .find(|member| member.node() == peer)
        .unwrap();
    dragging
        .press_for_test(
            CanvasPressTarget::NodeDrag(dragging_start),
            position(0.0, 0.0),
        )
        .unwrap();
    dragging.move_to(position(8.0, 0.0)).unwrap();
    assert!(dragging.uses_owner(InteractionOwner::Node(dragging_peer)));
    dragging.owner_retired(InteractionOwner::Node(dragging_peer));
    assert_eq!(
        dragging.presentation().phase(),
        CanvasInteractionPhase::Idle
    );

    let mut committing = test_session(CanvasInteractionConfig::default());
    let committing_start = start();
    let committing_peer = *committing_start
        .members()
        .iter()
        .find(|member| member.node() == peer)
        .unwrap();
    committing
        .press_for_test(
            CanvasPressTarget::NodeDrag(committing_start),
            position(0.0, 0.0),
        )
        .unwrap();
    committing.move_to(position(8.0, 0.0)).unwrap();
    let released = committing.release(position(8.0, 0.0)).unwrap();
    let ticket = submit_ticket(&released);
    assert!(committing.uses_owner(InteractionOwner::Node(committing_peer)));
    committing.owner_retired(InteractionOwner::Node(committing_peer));
    assert_eq!(
        committing.presentation().phase(),
        CanvasInteractionPhase::Idle
    );
    let late = committing.submission_outcome_unknown(ticket).unwrap();
    assert!(late.effects().is_empty());
    assert_eq!(
        committing.presentation().terminal(),
        Some(InteractionTerminalReason::OwnerRetired)
    );
}

#[test]
fn unrelated_owner_retirement_preserves_active_tick_demand() {
    let mut session = active_node_drag_session();

    let update = session.owner_retired(InteractionOwner::Node(node_lifetime(NodeId::from_u128(
        999,
    ))));

    assert_eq!(
        session.presentation().phase(),
        CanvasInteractionPhase::NodeDrag
    );
    assert_eq!(update.tick_demand(), InteractionTickDemand::WhileActive);
}

#[test]
fn late_queries_preserve_active_tick_demand() {
    let mut session = active_node_drag_session();
    let late_id = InteractionQueryId(InteractionIdentity::new(session.id(), 999));

    let resolved = session
        .resolve_edge_pick_for_test(
            late_id,
            EdgePickQueryOutcome::Complete {
                pick: None,
                work: EdgePickWork::default(),
            },
        )
        .unwrap();
    assert_eq!(resolved.tick_demand(), InteractionTickDemand::WhileActive);

    let restarted = session.restart_stale_query(late_id).unwrap();
    assert_eq!(restarted.tick_demand(), InteractionTickDemand::WhileActive);
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

fn active_node_drag_session() -> CanvasInteractionSession {
    let node = NodeId::from_u128(10);
    let start = node_drag_start(
        node,
        [node],
        CanvasPoint { x: 0.0, y: 0.0 },
        SelectionMode::Replace,
    );
    let mut session = test_session(CanvasInteractionConfig::default());
    session
        .press_for_test(CanvasPressTarget::NodeDrag(start), position(10.0, 10.0))
        .unwrap();
    session.move_to(position(5.0, 5.0)).unwrap();
    session
}

fn overflowing_auto_pan_transform() -> ViewportTransform {
    ViewportTransform::new(CanvasPoint::default(), 2.0e-37)
        .expect("tiny positive finite zoom remains a valid viewport transform")
}

fn test_session(config: CanvasInteractionConfig) -> CanvasInteractionSession {
    CanvasInteractionSession::new_for_test(
        ViewId::new(crate::runtime::graph_runtime::RuntimeId::UNAVAILABLE, 0),
        ViewGeneration::initial(),
        config,
    )
    .unwrap()
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

fn complete_with_kind(
    session: &mut CanvasInteractionSession,
    ticket: InteractionTicketId,
    expected_kind: CommandExecutionKind,
) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
    use crate::runtime::commands::GraphCommand;

    let runtime = GraphRuntime::builder(Default::default())
        .build(prospective_graph())
        .expect("completion fixture runtime");
    let version = runtime
        .snapshot()
        .expect("completion fixture snapshot")
        .version();
    let command = match expected_kind {
        CommandExecutionKind::Applied | CommandExecutionKind::Conflict => {
            GraphCommand::set_node_position(version, source_node(), CanvasPoint { x: 1.0, y: 0.0 })
        }
        CommandExecutionKind::Noop => {
            GraphCommand::set_node_position(version, source_node(), CanvasPoint::default())
        }
        CommandExecutionKind::Rejected => GraphCommand::set_node_position(
            version,
            NodeId::from_u128(u128::MAX),
            CanvasPoint { x: 1.0, y: 0.0 },
        ),
        CommandExecutionKind::Failed => {
            panic!("this interaction fixture does not request transport failures")
        }
    };
    if expected_kind == CommandExecutionKind::Conflict {
        let advance = runtime.execute(GraphCommand::set_node_position(
            version,
            source_node(),
            CanvasPoint { x: 2.0, y: 0.0 },
        ));
        assert_eq!(advance.kind(), CommandExecutionKind::Applied);
    }
    let completion = runtime.execute(command);
    assert_eq!(completion.kind(), expected_kind);
    session.bind_completion_for_test(ticket, &completion);
    session.complete_submission(ticket, completion)
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

fn edge_instance(edge: EdgeId) -> EdgeInstanceKey {
    EdgeInstanceKey::new(
        crate::runtime::graph_runtime::RuntimeProvenance::new(
            crate::runtime::graph_runtime::RuntimeId::UNAVAILABLE,
        ),
        edge,
        crate::runtime::graph_runtime::EdgeIncarnation::unavailable(),
    )
}

fn node_lifetime(node: NodeId) -> NodeLifetimeKey {
    NodeLifetimeKey::unavailable(node)
}

fn port_instance(node: NodeId, port: PortId) -> PortInstanceKey {
    PortInstanceKey::unavailable(node, port)
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

fn node_drag_start(
    clicked: NodeId,
    selected: impl IntoIterator<Item = NodeId>,
    snap_origin: CanvasPoint,
    selection_mode: SelectionMode,
) -> NodeDragStart {
    let selected = selected.into_iter().collect::<Vec<_>>();
    let mut nodes = selected.clone();
    if !nodes.contains(&clicked) {
        nodes.push(clicked);
    }
    nodes.sort_unstable();
    nodes.dedup();
    let mut builder = GraphBuilder::new(GraphId::from_u128(0x4e4f_4445));
    for node in nodes {
        builder.insert_node(node, test_node(CanvasPoint::default()));
    }
    let runtime = GraphRuntime::builder(Default::default())
        .build(builder.build().expect("valid node-drag fixture graph"))
        .expect("valid node-drag fixture runtime");
    let snapshot = runtime.snapshot().expect("node-drag fixture snapshot");
    let provenance = crate::runtime::graph_runtime::RuntimeProvenance::new(
        crate::runtime::graph_runtime::RuntimeId::UNAVAILABLE,
    );
    let lifetime = |node| {
        NodeLifetimeKey::new(
            provenance,
            node,
            snapshot
                .node_incarnation(node)
                .expect("fixture node lifetime"),
        )
    };
    NodeDragStart::new(
        ViewProvenance::unavailable(),
        lifetime(clicked),
        selected.into_iter().map(lifetime),
        snap_origin,
        selection_mode,
    )
    .expect("valid node-drag start")
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

fn connection_target(allow_search: bool) -> CanvasPressTarget {
    CanvasPressTarget::Connection(connection_start(allow_search))
}

fn connection_start(allow_search: bool) -> ConnectionInteractionStart {
    ConnectionInteractionStart::new(
        ConnectionGestureBinding::connect_for_test(
            EdgeId::from_u128(9),
            source_port(),
            NodeGraphConnectionMode::Strict,
        ),
        test_graph_version(),
        CanvasPoint { x: 0.0, y: 8.0 },
        HandlePosition::Right,
        allow_search,
    )
    .unwrap()
}

fn bound_connection_start(view: &ViewSession) -> ConnectionInteractionStart {
    let drag_intent =
        port_connection_drag_intent_by_id(view, source_port()).expect("bound source port facts");
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

fn begin_bound_search_submission(
    session: &mut CanvasInteractionSession,
    view: &ViewSession,
    start: ConnectionInteractionStart,
) -> InteractionGraphSubmission {
    let position = bound_position(session, view, 0.0, 0.0);
    session
        .activate_connection_search(start, position)
        .expect("bound search activation");
    let request = session.search_request().expect("bound search request");
    session
        .search_ready(&request, 1)
        .expect("bound search ready");
    let selection = session
        .create_search_selection(&request, target_port(), prospective_creation())
        .expect("bound search selection query")
        .expect("bound search selection");
    let submitted = session
        .select_search_result(selection)
        .expect("bound search selection submit");
    take_submission(submitted)
}

fn reconnect_start(
    edge: EdgeId,
    detached: PortId,
    allow_search: bool,
) -> ConnectionInteractionStart {
    ConnectionInteractionStart::new(
        ConnectionGestureBinding::reconnect(
            edge_instance(edge),
            EdgeEndpoint::To,
            port_instance(source_node(), detached),
            port_instance(source_node(), source_port()),
            NodeGraphConnectionMode::Strict,
        )
        .unwrap(),
        test_graph_version(),
        CanvasPoint { x: 0.0, y: 8.0 },
        HandlePosition::Right,
        allow_search,
    )
    .unwrap()
}

fn canvas_marquee_target(selection_mode: SelectionMode) -> CanvasPressTarget {
    CanvasPressTarget::Canvas(CanvasPressFallback::Marquee {
        selection_mode,
        baseline: SelectionBaseline::default(),
    })
}

fn position(x: f32, y: f32) -> ViewportPointerSample {
    ViewportPointerSample::new(
        ViewportSnapshot::new(
            ViewProvenance::unavailable(),
            ViewportRevision::initial(),
            ViewportTransform::new(CanvasPoint::default(), 1.0).unwrap(),
        ),
        ViewPoint::new(x, y),
    )
    .unwrap()
}

fn interaction_position(x: f32, y: f32) -> InteractionPosition {
    InteractionPosition::from_sample(position(x, y)).unwrap()
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
    closed: bool,
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
        closed: session.closed,
    }
}

fn marquee_selection_effect(update: &CanvasInteractionUpdate) -> &SelectionMutation {
    let [selection] = update.selection_mutations_for_test() else {
        panic!("marquee move must emit one selection effect");
    };
    selection
}

fn valid_probe(port: PortId) -> ConnectionProbe {
    valid_probe_at(port, test_graph_version(), CanvasPoint { x: 24.0, y: 8.0 })
}

fn valid_probe_at(
    port: PortId,
    version: GraphVersion,
    route_anchor: CanvasPoint,
) -> ConnectionProbe {
    ConnectionProbe::Valid {
        version,
        target: probe_target(port, route_anchor),
    }
}

fn probe_target(port: PortId, route_anchor: CanvasPoint) -> ConnectionProbeTarget {
    ConnectionProbeTarget::new(
        PortInstanceKey::unavailable(NodeId::from_u128(2), port),
        route_anchor,
        HandlePosition::Left,
        0.0,
    )
}

fn begin_direct_connection_commit(session: &mut CanvasInteractionSession) -> InteractionTicketId {
    begin_direct_connection_submission(session).ticket()
}

fn begin_direct_connection_submission(
    session: &mut CanvasInteractionSession,
) -> InteractionGraphSubmission {
    session
        .press_for_test(connection_target(false), position(0.0, 0.0))
        .unwrap();
    let moved = session.move_to(position(8.0, 0.0)).unwrap();
    session
        .resolve_connection_probe_for_test(
            connection_query_id(&moved),
            complete(valid_probe(target_port())),
        )
        .unwrap();
    let released = session.release(position(20.0, 0.0)).unwrap();
    let committing = session
        .resolve_connection_probe_for_test(
            connection_query_id(&released),
            complete(valid_probe(target_port())),
        )
        .unwrap();
    take_submission(committing)
}

fn begin_search_connection_commit(session: &mut CanvasInteractionSession) -> InteractionTicketId {
    session
        .press_for_test(connection_target(true), position(0.0, 0.0))
        .unwrap();
    let moved = session.move_to(position(8.0, 0.0)).unwrap();
    session
        .resolve_connection_probe_for_test(
            connection_query_id(&moved),
            complete(ConnectionProbe::None),
        )
        .unwrap();
    let released = session.release(position(20.0, 0.0)).unwrap();
    session
        .resolve_connection_probe_for_test(
            connection_query_id(&released),
            complete(ConnectionProbe::None),
        )
        .unwrap();
    let request = session.search_request().expect("active search request");
    session.search_ready(&request, 1).unwrap();
    let committing = session
        .select_search_result(search_selection(&request))
        .unwrap();
    submit_ticket(&committing)
}

fn search_selection(request: &ConnectionSearchRequest) -> ConnectionSearchSelection {
    let snapshot = prospective_snapshot();
    ConnectionSearchSelection::from_snapshot(
        request.token.clone(),
        &snapshot,
        target_port(),
        prospective_creation(),
    )
    .expect("create-from-wire selection carries topology")
}

fn search_token_for_test() -> ConnectionSearchToken {
    let mut session = test_session(CanvasInteractionConfig::default());
    session
        .activate_connection_search_for_test(connection_start(true), position(0.0, 0.0))
        .expect("test search activation");
    session.search_request().expect("test search request").token
}

fn next_ticket_identity(ticket: InteractionTicketId) -> InteractionTicketId {
    InteractionTicketId(InteractionIdentity::new(
        ticket.session(),
        ticket.0.sequence + 1,
    ))
}

fn test_graph_version() -> GraphVersion {
    prospective_snapshot().version()
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

fn prospective_snapshot() -> GraphSnapshot {
    graph_snapshot(prospective_graph())
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

fn complete(probe: ConnectionProbe) -> ConnectionProbeQueryOutcome {
    ConnectionProbeQueryOutcome::Complete {
        probe,
        work: ConnectionProbeWork::default(),
    }
}

fn connection_query_id(update: &CanvasInteractionUpdate) -> InteractionQueryId {
    let [query] = update.queries() else {
        panic!("expected one connection probe query");
    };
    let CanvasInteractionQueryKind::ConnectionProbe { id, .. } = query.kind_for_test() else {
        panic!("expected one connection probe query");
    };
    *id
}

fn resolve_bound_connection_query(
    session: &mut CanvasInteractionSession,
    update: CanvasInteractionUpdate,
) -> CanvasInteractionUpdate {
    let dispatch = update.into_dispatch();
    assert!(dispatch.effects.is_empty());
    let mut queries = dispatch.queries.into_iter();
    let Some(query) = queries.next() else {
        panic!("expected one bound connection query");
    };
    assert!(queries.next().is_none());
    session.resolve_query(query).expect("bound probe resolves")
}

fn edge_query_id(update: &CanvasInteractionUpdate) -> InteractionQueryId {
    let [query] = update.queries() else {
        panic!("expected one edge-pick query");
    };
    let CanvasInteractionQueryKind::EdgePick { id, .. } = query.kind_for_test() else {
        panic!("expected one edge-pick query");
    };
    *id
}

fn take_opaque_query(update: CanvasInteractionUpdate) -> CanvasInteractionQuery {
    let mut queries = update.into_dispatch().queries.into_iter();
    let query = queries.next().expect("expected one opaque query");
    assert!(queries.next().is_none());
    query
}

fn marquee_query_id(update: &CanvasInteractionUpdate) -> InteractionQueryId {
    let [query] = update.queries() else {
        panic!("expected one node-intersection query");
    };
    let CanvasInteractionQueryKind::NodeIntersection { id, .. } = query.kind_for_test() else {
        panic!("expected one node-intersection query");
    };
    *id
}

fn drag_target_lock(session: &CanvasInteractionSession) -> Option<ConnectionProbeTargetIdentity> {
    match &session.state {
        InteractionState::Connection(ConnectionInteraction::Dragging(drag)) => drag.target_lock,
        state => panic!("expected active connection drag, got {state:?}"),
    }
}

fn resolve_marquee_query(
    session: &mut CanvasInteractionSession,
    update: &CanvasInteractionUpdate,
    nodes: impl IntoIterator<Item = NodeId>,
) -> CanvasInteractionUpdate {
    session
        .resolve_node_intersection_for_test(
            marquee_query_id(update),
            NodeIntersectionQueryOutcome::Complete {
                nodes: nodes.into_iter().collect(),
                work: Default::default(),
            },
        )
        .unwrap()
}

fn submit_ticket(update: &CanvasInteractionUpdate) -> InteractionTicketId {
    let [CanvasInteractionEffect::SubmitGraph { submission }] = update.effects() else {
        panic!("expected one graph submission");
    };
    submission.ticket()
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
