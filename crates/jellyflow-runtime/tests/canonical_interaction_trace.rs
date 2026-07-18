#[path = "support/canonical_interaction_trace.rs"]
mod trace;

use trace::{
    CanonicalInteractionTrace, TraceCancelReason, TraceCandidate, TraceEffect, TraceInput,
    TracePhase, TraceReceipt, TraceReconnectEndpoint, TraceTarget, control_and_drag_handle_traces,
    create_from_wire_trace, edge_pick_trace, new_connection_trace, reconnect_trace,
    terminal_ownership_traces,
};

use jellyflow_core::core::{
    CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeId, EdgeKind, GraphBuilder, GraphId, Node,
    NodeId, NodeKindKey, Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
};
use jellyflow_core::interaction::NodeGraphConnectionMode;
use jellyflow_runtime::{
    CanvasInteractionConfig, CanvasInteractionEffect, CanvasInteractionPhase,
    CanvasInteractionQuery, CanvasInteractionSession, CanvasInteractionUpdate, CanvasPressTarget,
    CommandSettlement, ConnectionGestureIntent, ConnectionInteractionStart, ConnectionProbe,
    ConnectionProbeCursorStaleReason, ConnectionProbeQuery, ConnectionProbeQueryError,
    ConnectionProbeQueryOutcome, ConnectionSearchSelection, EdgeEndpoint,
    EdgePickCursorStaleReason, EdgePickQuery, EdgePickQueryError, EdgePickQueryOutcome,
    FrameRequest, GraphCommand, GraphRuntime, HandlePosition, InteractionOwner,
    InteractionPosition, InteractionTicketId, RendererStamp, SelectionBaseline, SelectionMode,
    TopologyEdit, ViewPoint, ViewRect, ViewSession, ViewSessionOptions, ViewSize,
};

#[test]
fn canonical_trace_vocabulary_is_deterministic_and_covers_terminal_receipts() {
    for receipt in [
        TraceReceipt::Applied,
        TraceReceipt::Noop,
        TraceReceipt::Rejected,
        TraceReceipt::Conflict,
    ] {
        let trace = new_connection_trace(receipt);
        assert_eq!(trace, new_connection_trace(receipt));
        assert_eq!(
            trace.steps.last().expect("terminal step").phase,
            TracePhase::Idle
        );
        assert_eq!(
            trace
                .steps
                .iter()
                .filter(|step| step.effect == TraceEffect::SubmitConnection)
                .count(),
            1,
        );

        for endpoint in [
            TraceReconnectEndpoint::Source,
            TraceReconnectEndpoint::Target,
        ] {
            let reconnect = reconnect_trace(endpoint, receipt);
            assert_eq!(
                reconnect
                    .steps
                    .iter()
                    .filter(|step| step.effect == TraceEffect::SubmitConnection)
                    .count(),
                1,
            );
        }
    }
}

#[test]
fn search_and_native_control_traces_keep_terminal_ownership_explicit() {
    let rejected = create_from_wire_trace(TraceReceipt::Rejected);
    assert_eq!(
        rejected.steps.last().expect("terminal search step").phase,
        TracePhase::Searching,
    );

    let [control, drag] = control_and_drag_handle_traces();
    assert_eq!(control.steps[0].phase, TracePhase::Idle);
    assert!(
        drag.steps
            .iter()
            .any(|step| step.phase == TracePhase::NodeDrag)
    );

    let edge = edge_pick_trace();
    assert_eq!(edge.steps[0].effect, TraceEffect::PickEdge);
    assert_eq!(
        edge.steps.last().expect("edge terminal").phase,
        TracePhase::Idle,
    );

    for trace in terminal_ownership_traces() {
        let terminal = trace.steps.last().expect("ownership terminal");
        assert_eq!(terminal.phase, TracePhase::Idle);
        assert_eq!(terminal.effect, TraceEffect::ReleaseNativeOwnership);
    }
}

#[test]
fn canonical_traces_drive_the_renderer_neutral_session_and_real_view_queries() {
    for receipt in [
        TraceReceipt::Applied,
        TraceReceipt::Noop,
        TraceReceipt::Rejected,
        TraceReceipt::Conflict,
    ] {
        TraceHarness::new().replay(&new_connection_trace(receipt));
        TraceHarness::new().replay(&create_from_wire_trace(receipt));
        for endpoint in [
            TraceReconnectEndpoint::Source,
            TraceReconnectEndpoint::Target,
        ] {
            TraceHarness::new().replay(&reconnect_trace(endpoint, receipt));
        }
    }

    for trace in control_and_drag_handle_traces() {
        TraceHarness::new().replay(&trace);
    }
    TraceHarness::new().replay(&edge_pick_trace());
    for trace in terminal_ownership_traces() {
        TraceHarness::new().replay(&trace);
    }
}

#[test]
fn repeated_toggle_marquee_trace_is_independent_of_move_event_count() {
    let mut harness = TraceHarness::new();
    let baseline =
        SelectionBaseline::new([harness.ids.source_node], [harness.ids.reconnect_edge], []);
    let start = normalized_position(&harness.view, ViewPoint::new(600.0, 180.0));
    let end = normalized_position(&harness.view, ViewPoint::new(680.0, 260.0));
    harness
        .session
        .press(
            CanvasPressTarget::Marquee {
                selection_mode: SelectionMode::Toggle,
                baseline: baseline.clone(),
            },
            start,
        )
        .unwrap();

    let first = harness.session.move_to(end).unwrap();
    let second = harness.session.move_to(end).unwrap();
    let [CanvasInteractionEffect::Selection(first)] = first.effects() else {
        panic!("first marquee move must emit one selection effect");
    };
    let [CanvasInteractionEffect::Selection(second)] = second.effects() else {
        panic!("second marquee move must emit one selection effect");
    };

    assert_eq!(first, second);
}

#[test]
fn interaction_query_cursors_are_isolated_per_view() {
    let harness = TraceHarness::new();
    let other = harness
        ._runtime
        .attach_view(ViewSessionOptions::default())
        .unwrap();
    publish_trace_geometry(&other, harness.ids, harness.anchors);

    let intent = ConnectionGestureIntent::connect(
        EdgeId::from_u128(99),
        harness.ids.source_port,
        NodeGraphConnectionMode::Strict,
    );
    let probe_cursor = match harness
        .view
        .connection_probe(ConnectionProbeQuery::new(
            intent,
            harness.anchors.valid,
            12.0,
            0,
        ))
        .unwrap()
    {
        ConnectionProbeQueryOutcome::Incomplete { cursor, .. } => cursor,
        outcome => panic!("zero-budget probe must be resumable: {outcome:?}"),
    };
    assert!(matches!(
        other.connection_probe(ConnectionProbeQuery::resume(probe_cursor, 16)),
        Err(ConnectionProbeQueryError::StaleCursor(
            ConnectionProbeCursorStaleReason::ForeignView
        ))
    ));

    let edge_cursor = match harness
        .view
        .edge_pick(EdgePickQuery::new(ViewPoint::new(400.0, 80.0), 10.0, 0))
        .unwrap()
    {
        EdgePickQueryOutcome::Incomplete { cursor, .. } => cursor,
        outcome => panic!("zero-budget edge pick must be resumable: {outcome:?}"),
    };
    assert!(matches!(
        other.edge_pick(EdgePickQuery::resume(edge_cursor, 16)),
        Err(EdgePickQueryError::StaleCursor(
            EdgePickCursorStaleReason::ForeignView
        ))
    ));
}

#[test]
fn connection_commit_revalidation_resumes_real_view_query_before_resubmission() {
    let harness = TraceHarness::new();
    let config = CanvasInteractionConfig {
        connection_radius_in_view: 1_000.0,
        query_entry_budget: 1,
        ..CanvasInteractionConfig::default()
    };
    let mut session = CanvasInteractionSession::new(
        harness.view.id(),
        harness.view.generation().unwrap(),
        config,
    )
    .unwrap();
    let drag_intent = harness
        .view
        .port_connection_drag_intent(harness.ids.source_port)
        .unwrap()
        .expect("source port drag facts");
    let start = ConnectionInteractionStart::from_port_drag_intent(
        drag_intent,
        EdgeId::from_u128(0xA11),
        NodeGraphConnectionMode::Strict,
        CanvasPoint {
            x: harness.anchors.source.x,
            y: harness.anchors.source.y,
        },
        HandlePosition::Right,
        false,
    )
    .unwrap();
    let intent = start.intent();
    let source = normalized_position(&harness.view, harness.anchors.source);
    let target = normalized_position(&harness.view, harness.anchors.valid);
    session
        .press(CanvasPressTarget::Connection(start), source)
        .unwrap();

    let moved = session.move_to(target).unwrap();
    let (move_id, _) = take_probe(moved).expect("threshold crossing requests a probe");
    let initial_probe = harness
        .view
        .connection_probe(ConnectionProbeQuery::new(
            intent,
            harness.anchors.valid,
            config.connection_radius_in_view,
            256,
        ))
        .unwrap();
    assert!(matches!(
        &initial_probe,
        ConnectionProbeQueryOutcome::Complete {
            probe: ConnectionProbe::Valid { .. },
            ..
        }
    ));
    session
        .resolve_connection_probe(move_id, initial_probe)
        .unwrap();

    let released = session.release(target).unwrap();
    let (release_id, _) = take_probe(released).expect("release requests one fresh probe");
    let release_probe = harness
        .view
        .connection_probe(ConnectionProbeQuery::new(
            intent,
            harness.anchors.valid,
            config.connection_radius_in_view,
            256,
        ))
        .unwrap();
    let committing = session
        .resolve_connection_probe(release_id, release_probe)
        .unwrap();
    let effects = committing.into_dispatch().pre_query_effects;
    let [CanvasInteractionEffect::SubmitGraph { ticket, .. }] = effects.as_slice() else {
        panic!("release must submit one connection mutation");
    };
    let ticket = *ticket;

    let mut update = session
        .request_connection_commit_revalidation(ticket)
        .unwrap();
    let mut incomplete_pages = 0;
    let mut revalidation_id = None;
    loop {
        let (id, query) = take_probe(update).expect("pending revalidation owns one query page");
        if let Some(expected) = revalidation_id {
            assert_eq!(id, expected);
        } else {
            revalidation_id = Some(id);
        }
        let outcome = harness.view.connection_probe(query).unwrap();
        if matches!(&outcome, ConnectionProbeQueryOutcome::Incomplete { .. }) {
            incomplete_pages += 1;
        }
        update = session.resolve_connection_probe(id, outcome).unwrap();
        if !update.effects().is_empty() {
            let [
                CanvasInteractionEffect::SubmitGraph {
                    ticket: emitted_ticket,
                    ..
                },
            ] = update.effects()
            else {
                panic!("completed revalidation emits one connection mutation");
            };
            assert_eq!(*emitted_ticket, ticket);
            break;
        }
    }
    assert!(incomplete_pages > 0);
    assert_eq!(
        session.presentation().phase(),
        CanvasInteractionPhase::Committing
    );
}

#[test]
fn paged_connection_queries_restart_after_viewport_layout_and_graph_changes() {
    let harness = TraceHarness::new();
    let config = CanvasInteractionConfig {
        connection_radius_in_view: 1_000.0,
        query_entry_budget: 1,
        ..CanvasInteractionConfig::default()
    };
    let mut session = CanvasInteractionSession::new(
        harness.view.id(),
        harness.view.generation().unwrap(),
        config,
    )
    .unwrap();
    let drag_intent = harness
        .view
        .port_connection_drag_intent(harness.ids.source_port)
        .unwrap()
        .expect("source port drag facts");
    let start = ConnectionInteractionStart::from_port_drag_intent(
        drag_intent,
        EdgeId::from_u128(0xA12),
        NodeGraphConnectionMode::Strict,
        canvas_anchor(&harness.view, harness.anchors.source),
        HandlePosition::Right,
        false,
    )
    .unwrap();
    let source = normalized_position(&harness.view, harness.anchors.source);
    let target = normalized_position(&harness.view, harness.anchors.valid);
    session
        .press(CanvasPressTarget::Connection(start), source)
        .unwrap();

    let moved = session.move_to(target).unwrap();
    let (drag_id, drag_query) = take_probe(moved).expect("drag starts one bounded probe");
    let drag_page = harness.view.connection_probe(drag_query).unwrap();
    assert!(matches!(
        drag_page,
        ConnectionProbeQueryOutcome::Incomplete { .. }
    ));
    let drag_resume = session
        .resolve_connection_probe(drag_id, drag_page)
        .unwrap();
    let (same_drag_id, drag_resume_query) =
        take_probe(drag_resume).expect("incomplete drag probe resumes");
    assert_eq!(same_drag_id, drag_id);
    harness
        .view
        .set_viewport(CanvasPoint { x: 2.0, y: -1.0 }, 1.0)
        .unwrap();
    assert!(matches!(
        harness.view.connection_probe(drag_resume_query),
        Err(ConnectionProbeQueryError::StaleCursor(
            ConnectionProbeCursorStaleReason::ViewportTransform
        ))
    ));
    let restarted_drag = session.restart_stale_query(drag_id).unwrap();
    let (fresh_drag_id, _) = restarted_drag
        .queries()
        .first()
        .and_then(connection_probe_query_ref)
        .expect("stale drag probe restarts from page one");
    assert_ne!(fresh_drag_id, drag_id);
    assert!(drive_connection_pages(&harness.view, &mut session, restarted_drag).is_empty());
    assert_eq!(
        session.presentation().phase(),
        CanvasInteractionPhase::ConnectionDrag
    );

    let release_position = normalized_position(&harness.view, harness.anchors.valid);
    let released = session.release(release_position).unwrap();
    let (release_id, release_query) =
        take_probe(released).expect("release starts one bounded probe");
    let release_page = harness.view.connection_probe(release_query).unwrap();
    assert!(matches!(
        release_page,
        ConnectionProbeQueryOutcome::Incomplete { .. }
    ));
    let release_resume = session
        .resolve_connection_probe(release_id, release_page)
        .unwrap();
    let (_, release_resume_query) =
        take_probe(release_resume).expect("incomplete release probe resumes");
    let changed_anchors = TraceAnchors {
        valid: ViewPoint::new(harness.anchors.valid.x + 2.0, harness.anchors.valid.y),
        ..harness.anchors
    };
    publish_trace_geometry(&harness.view, harness.ids, changed_anchors);
    assert!(matches!(
        harness.view.connection_probe(release_resume_query),
        Err(ConnectionProbeQueryError::StaleCursor(
            ConnectionProbeCursorStaleReason::PortGeometry
        ))
    ));
    let restarted_release = session.restart_stale_query(release_id).unwrap();
    let (fresh_release_id, _) = restarted_release
        .queries()
        .first()
        .and_then(connection_probe_query_ref)
        .expect("stale release probe restarts from page one");
    assert_ne!(fresh_release_id, release_id);
    let release_effects = drive_connection_pages(&harness.view, &mut session, restarted_release);
    let [CanvasInteractionEffect::SubmitGraph { ticket, .. }] = release_effects.as_slice() else {
        panic!("completed release probe submits one connection mutation");
    };
    let ticket = *ticket;

    let revalidation = session
        .request_connection_commit_revalidation(ticket)
        .unwrap();
    let (revalidation_id, revalidation_query) =
        take_probe(revalidation).expect("commit starts one bounded revalidation");
    let revalidation_page = harness.view.connection_probe(revalidation_query).unwrap();
    assert!(matches!(
        revalidation_page,
        ConnectionProbeQueryOutcome::Incomplete { .. }
    ));
    let revalidation_resume = session
        .resolve_connection_probe(revalidation_id, revalidation_page)
        .unwrap();
    let (_, revalidation_resume_query) =
        take_probe(revalidation_resume).expect("incomplete revalidation resumes");
    let revision = harness._runtime.snapshot().unwrap().revision();
    harness
        ._runtime
        .execute(GraphCommand::set_node_data(
            revision,
            harness.ids.invalid_node,
            serde_json::json!({ "query_epoch": 1 }),
        ))
        .unwrap();
    publish_trace_geometry(&harness.view, harness.ids, changed_anchors);
    assert!(matches!(
        harness.view.connection_probe(revalidation_resume_query),
        Err(ConnectionProbeQueryError::StaleCursor(
            ConnectionProbeCursorStaleReason::GraphRevision
        ))
    ));
    let restarted_revalidation = session.restart_stale_query(revalidation_id).unwrap();
    let (fresh_revalidation_id, _) = restarted_revalidation
        .queries()
        .first()
        .and_then(connection_probe_query_ref)
        .expect("stale commit revalidation restarts from page one");
    assert_ne!(fresh_revalidation_id, revalidation_id);
    let revalidation_effects =
        drive_connection_pages(&harness.view, &mut session, restarted_revalidation);
    let [
        CanvasInteractionEffect::SubmitGraph {
            ticket: emitted_ticket,
            ..
        },
    ] = revalidation_effects.as_slice()
    else {
        panic!("completed revalidation resubmits one connection mutation");
    };
    assert_eq!(*emitted_ticket, ticket);
    assert_eq!(
        session.presentation().phase(),
        CanvasInteractionPhase::Committing
    );
}

struct TraceHarness {
    _runtime: GraphRuntime,
    view: ViewSession,
    session: CanvasInteractionSession,
    ids: TraceGraphIds,
    anchors: TraceAnchors,
    pointer: InteractionPosition,
    pending_probe: Option<(
        jellyflow_runtime::InteractionQueryId,
        jellyflow_runtime::ConnectionProbeQuery,
    )>,
    ticket: Option<InteractionTicketId>,
}

#[derive(Clone, Copy)]
struct TraceGraphIds {
    source_node: NodeId,
    source_port: PortId,
    invalid_node: NodeId,
    invalid_port: PortId,
    valid_node: NodeId,
    valid_port: PortId,
    reconnect_source_node: NodeId,
    reconnect_source_port: PortId,
    reconnect_target_node: NodeId,
    reconnect_target_port: PortId,
    reconnect_alternate_node: NodeId,
    reconnect_alternate_port: PortId,
    reconnect_edge: EdgeId,
}

#[derive(Clone, Copy)]
struct TraceAnchors {
    source: ViewPoint,
    invalid: ViewPoint,
    valid: ViewPoint,
    reconnect_source: ViewPoint,
    reconnect_target: ViewPoint,
    reconnect_alternate: ViewPoint,
    empty: ViewPoint,
}

impl TraceHarness {
    fn new() -> Self {
        let ids = TraceGraphIds {
            source_node: NodeId::from_u128(1),
            source_port: PortId::from_u128(1),
            invalid_node: NodeId::from_u128(2),
            invalid_port: PortId::from_u128(2),
            valid_node: NodeId::from_u128(3),
            valid_port: PortId::from_u128(3),
            reconnect_source_node: NodeId::from_u128(4),
            reconnect_source_port: PortId::from_u128(4),
            reconnect_target_node: NodeId::from_u128(5),
            reconnect_target_port: PortId::from_u128(5),
            reconnect_alternate_node: NodeId::from_u128(6),
            reconnect_alternate_port: PortId::from_u128(6),
            reconnect_edge: EdgeId::from_u128(9),
        };
        let anchors = TraceAnchors {
            source: ViewPoint::new(60.0, 80.0),
            invalid: ViewPoint::new(160.0, 80.0),
            valid: ViewPoint::new(240.0, 80.0),
            reconnect_source: ViewPoint::new(360.0, 80.0),
            reconnect_target: ViewPoint::new(440.0, 80.0),
            reconnect_alternate: ViewPoint::new(540.0, 80.0),
            empty: ViewPoint::new(700.0, 300.0),
        };
        let mut builder = GraphBuilder::new(GraphId::from_u128(1));
        for (node, port, direction) in [
            (ids.source_node, ids.source_port, PortDirection::Out),
            (ids.invalid_node, ids.invalid_port, PortDirection::Out),
            (ids.valid_node, ids.valid_port, PortDirection::In),
            (
                ids.reconnect_source_node,
                ids.reconnect_source_port,
                PortDirection::Out,
            ),
            (
                ids.reconnect_target_node,
                ids.reconnect_target_port,
                PortDirection::In,
            ),
            (
                ids.reconnect_alternate_node,
                ids.reconnect_alternate_port,
                PortDirection::In,
            ),
        ] {
            builder.insert_node(node, trace_node(port));
            builder.insert_port(port, trace_port(node, direction));
        }
        builder.insert_edge(
            ids.reconnect_edge,
            Edge::new(
                EdgeKind::Data,
                ids.reconnect_source_port,
                ids.reconnect_target_port,
            ),
        );
        let runtime = GraphRuntime::builder(Default::default())
            .build(builder.build().unwrap())
            .unwrap();
        let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
        publish_trace_geometry(&view, ids, anchors);
        let session = CanvasInteractionSession::new(
            view.id(),
            view.generation().unwrap(),
            CanvasInteractionConfig::default(),
        )
        .unwrap();
        let pointer = normalized_position(&view, anchors.source);
        Self {
            _runtime: runtime,
            view,
            session,
            ids,
            anchors,
            pointer,
            pending_probe: None,
            ticket: None,
        }
    }

    fn replay(&mut self, trace: &CanonicalInteractionTrace) {
        for step in &trace.steps {
            let effect = self.apply(step.input);
            assert_eq!(
                self.observed_phase(),
                step.phase,
                "{} after {:?}",
                trace.name,
                step.input
            );
            assert_eq!(effect, step.effect, "{} after {:?}", trace.name, step.input);
        }
    }

    fn apply(&mut self, input: TraceInput) -> TraceEffect {
        match input {
            TraceInput::Press { target } => self.press(target),
            TraceInput::ThresholdCrossed => {
                self.pointer = normalized_position(&self.view, self.anchors.empty);
                let update = self.session.move_to(self.pointer).unwrap();
                self.capture_probe(update)
            }
            TraceInput::Move { .. } => {
                self.pointer = normalized_position(&self.view, self.invalid_anchor());
                let update = self.session.move_to(self.pointer).unwrap();
                self.capture_probe(update)
            }
            TraceInput::Candidate(candidate) => self.resolve_candidate(candidate),
            TraceInput::Release => self.release(),
            TraceInput::SearchSelected { .. } => {
                let generation = self.session.presentation().generation().unwrap();
                self.session.search_ready(generation, 1).unwrap();
                let update = self
                    .session
                    .select_search_result(
                        generation,
                        ConnectionSearchSelection::new(
                            self.ids.valid_port,
                            self.view.graph_snapshot().unwrap().revision(),
                            TopologyEdit::new().set_node_data(
                                self.ids.valid_node,
                                serde_json::json!({ "created_from_wire": true }),
                            ),
                        )
                        .expect("create-from-wire trace carries topology"),
                    )
                    .unwrap();
                self.capture_submit(update)
            }
            TraceInput::Receipt(receipt) => {
                let ticket = self.ticket.take().expect("trace submitted one command");
                self.session.settle(ticket, settlement(receipt)).unwrap();
                match receipt {
                    TraceReceipt::Applied | TraceReceipt::Noop => TraceEffect::None,
                    TraceReceipt::Rejected | TraceReceipt::Conflict => {
                        TraceEffect::RestorePresentation
                    }
                }
            }
            TraceInput::Cancel(reason) => {
                assert!(matches!(
                    reason,
                    TraceCancelReason::Escape | TraceCancelReason::OutsideClick
                ));
                self.session.cancel();
                TraceEffect::ReleaseNativeOwnership
            }
            TraceInput::Retire => {
                self.session
                    .owner_retired(InteractionOwner::Node(self.active_owner_node()));
                TraceEffect::ReleaseNativeOwnership
            }
            TraceInput::Close => {
                self.session.close();
                TraceEffect::ReleaseNativeOwnership
            }
        }
    }

    fn press(&mut self, target: TraceTarget) -> TraceEffect {
        match target {
            TraceTarget::NativeControl(_) => {
                self.session
                    .press(CanvasPressTarget::NativeControl, self.pointer)
                    .unwrap();
                TraceEffect::None
            }
            TraceTarget::NodeDragHandle(_) => {
                let start = self
                    .view
                    .node_drag_start(self.ids.source_node, SelectionMode::Replace)
                    .unwrap()
                    .unwrap();
                self.session
                    .press(CanvasPressTarget::NodeDrag(start), self.pointer)
                    .unwrap();
                TraceEffect::None
            }
            TraceTarget::Port(_) => {
                let drag_intent = self
                    .view
                    .port_connection_drag_intent(self.ids.source_port)
                    .unwrap()
                    .expect("source port drag facts");
                let start = ConnectionInteractionStart::from_port_drag_intent(
                    drag_intent,
                    EdgeId::from_u128(100),
                    NodeGraphConnectionMode::Strict,
                    canvas_anchor(&self.view, self.anchors.source),
                    HandlePosition::Right,
                    true,
                )
                .unwrap();
                self.session
                    .press(CanvasPressTarget::Connection(start), self.pointer)
                    .unwrap();
                TraceEffect::None
            }
            TraceTarget::ReconnectHandle { endpoint, .. } => {
                let (runtime_endpoint, fixed_anchor) = match endpoint {
                    TraceReconnectEndpoint::Source => {
                        (EdgeEndpoint::From, self.anchors.reconnect_target)
                    }
                    TraceReconnectEndpoint::Target => {
                        (EdgeEndpoint::To, self.anchors.reconnect_source)
                    }
                };
                let graph = self.view.graph_snapshot().unwrap();
                let geometry = self.view.geometry_snapshot().unwrap();
                let route = geometry
                    .route(self.ids.reconnect_edge)
                    .expect("published reconnect route");
                let start = ConnectionInteractionStart::reconnect_from_graph_route(
                    graph.graph(),
                    route,
                    runtime_endpoint,
                    NodeGraphConnectionMode::Strict,
                    false,
                )
                .expect("authoritative reconnect start");
                self.pointer = normalized_position(&self.view, fixed_anchor);
                self.session
                    .press(CanvasPressTarget::Connection(start), self.pointer)
                    .unwrap();
                TraceEffect::None
            }
            TraceTarget::Edge(_) => {
                let midpoint = ViewPoint::new(400.0, 80.0);
                let pointer = normalized_position(&self.view, midpoint);
                let dispatch = self
                    .session
                    .request_edge_pick(pointer)
                    .unwrap()
                    .into_dispatch();
                let mut queries = dispatch.queries;
                let effects = dispatch.pre_query_effects;
                assert!(effects.is_empty());
                let CanvasInteractionQuery::EdgePick { id, query } = queries.remove(0) else {
                    panic!("edge trace requested the wrong query");
                };
                let outcome = self.view.edge_pick(query).unwrap();
                let picked = match &outcome {
                    EdgePickQueryOutcome::Complete {
                        pick: Some(pick), ..
                    } => pick.edge(),
                    other => panic!("edge trace must pick the routed edge: {other:?}"),
                };
                self.session.resolve_edge_pick(id, outcome).unwrap();
                self.session
                    .press(
                        CanvasPressTarget::Edge {
                            edge: picked,
                            selection_mode: SelectionMode::Replace,
                        },
                        pointer,
                    )
                    .unwrap();
                TraceEffect::PickEdge
            }
            TraceTarget::Canvas => panic!("canonical trace has no unresolved canvas press"),
        }
    }

    fn resolve_candidate(&mut self, candidate: TraceCandidate) -> TraceEffect {
        if matches!(candidate, TraceCandidate::Valid { .. }) {
            self.pointer = normalized_position(&self.view, self.valid_anchor());
            let update = self.session.move_to(self.pointer).unwrap();
            self.capture_probe(update);
        }
        let (id, query) = self
            .pending_probe
            .take()
            .expect("candidate follows a probe");
        let outcome = self.view.connection_probe(query).unwrap();
        match (&candidate, &outcome) {
            (
                TraceCandidate::None,
                jellyflow_runtime::ConnectionProbeQueryOutcome::Complete {
                    probe: ConnectionProbe::None,
                    ..
                },
            ) => {}
            (
                TraceCandidate::Invalid { .. },
                jellyflow_runtime::ConnectionProbeQueryOutcome::Complete {
                    probe: ConnectionProbe::Invalid { .. },
                    ..
                },
            ) => {}
            (
                TraceCandidate::Valid { .. },
                jellyflow_runtime::ConnectionProbeQueryOutcome::Complete {
                    probe: ConnectionProbe::Valid { .. },
                    ..
                },
            ) => {}
            _ => panic!("trace candidate disagrees with the real view query: {outcome:?}"),
        }
        self.session.resolve_connection_probe(id, outcome).unwrap();
        TraceEffect::None
    }

    fn release(&mut self) -> TraceEffect {
        let was_connection = self.session.presentation().connection().is_some();
        let update = self.session.release(self.pointer).unwrap();
        if matches!(
            self.session.presentation().phase(),
            CanvasInteractionPhase::Committing
        ) {
            return self.capture_submit(update);
        }
        if !was_connection {
            return self.capture_submit(update);
        }
        if let Some((id, query)) = take_probe(update) {
            let outcome = self.view.connection_probe(query).unwrap();
            let terminal = self.session.resolve_connection_probe(id, outcome).unwrap();
            if terminal
                .effects()
                .iter()
                .any(|effect| matches!(effect, CanvasInteractionEffect::Search(_)))
            {
                return TraceEffect::OpenSearch;
            }
            return self.capture_submit(terminal);
        }
        TraceEffect::None
    }

    fn capture_probe(&mut self, update: CanvasInteractionUpdate) -> TraceEffect {
        self.pending_probe = take_probe(update);
        if self.pending_probe.is_some() {
            TraceEffect::ProbeConnection
        } else {
            TraceEffect::None
        }
    }

    fn capture_submit(&mut self, update: CanvasInteractionUpdate) -> TraceEffect {
        let effects = update.into_dispatch().pre_query_effects;
        for effect in effects {
            if let CanvasInteractionEffect::SubmitGraph { ticket, mutation } = effect {
                self.ticket = Some(ticket);
                return match mutation {
                    jellyflow_runtime::FreshGraphMutation::Connection { .. } => {
                        TraceEffect::SubmitConnection
                    }
                    jellyflow_runtime::FreshGraphMutation::MoveNodes(_) => {
                        TraceEffect::SubmitNodeMove
                    }
                    jellyflow_runtime::FreshGraphMutation::DeleteEdge(_) => {
                        panic!("canonical connection traces must not submit edge deletion")
                    }
                };
            }
        }
        TraceEffect::None
    }

    fn observed_phase(&self) -> TracePhase {
        let presentation = self.session.presentation();
        match presentation.phase() {
            CanvasInteractionPhase::Idle | CanvasInteractionPhase::Closed => TracePhase::Idle,
            CanvasInteractionPhase::Armed => TracePhase::Armed,
            CanvasInteractionPhase::NodeDrag => TracePhase::NodeDrag,
            CanvasInteractionPhase::ConnectionDrag
                if presentation
                    .connection()
                    .is_some_and(|connection| connection.probe().is_valid()) =>
            {
                TracePhase::Candidate
            }
            CanvasInteractionPhase::ConnectionDrag => TracePhase::ConnectionDrag,
            CanvasInteractionPhase::ConnectionSearch => TracePhase::Searching,
            CanvasInteractionPhase::Committing => TracePhase::Committing,
            CanvasInteractionPhase::EdgeActive => TracePhase::EdgeActive,
            CanvasInteractionPhase::Pan | CanvasInteractionPhase::Marquee => {
                panic!("canonical fixture did not request this phase")
            }
        }
    }

    fn invalid_anchor(&self) -> ViewPoint {
        match self.session.presentation().connection().map(|c| c.intent()) {
            Some(ConnectionGestureIntent::Reconnect {
                endpoint: EdgeEndpoint::From,
                ..
            }) => self.anchors.valid,
            Some(ConnectionGestureIntent::Reconnect {
                endpoint: EdgeEndpoint::To,
                ..
            })
            | Some(ConnectionGestureIntent::Connect { .. })
            | None => self.anchors.invalid,
        }
    }

    fn valid_anchor(&self) -> ViewPoint {
        match self.session.presentation().connection().map(|c| c.intent()) {
            Some(ConnectionGestureIntent::Reconnect {
                endpoint: EdgeEndpoint::From,
                ..
            }) => self.anchors.source,
            Some(ConnectionGestureIntent::Reconnect {
                endpoint: EdgeEndpoint::To,
                ..
            }) => self.anchors.reconnect_alternate,
            Some(ConnectionGestureIntent::Connect { .. }) | None => self.anchors.valid,
        }
    }

    fn active_owner_node(&self) -> NodeId {
        self.session
            .presentation()
            .connection()
            .map_or(self.ids.source_node, |connection| {
                match connection.intent() {
                    ConnectionGestureIntent::Connect { .. } => self.ids.source_node,
                    ConnectionGestureIntent::Reconnect {
                        endpoint: EdgeEndpoint::From,
                        ..
                    } => self.ids.reconnect_target_node,
                    ConnectionGestureIntent::Reconnect {
                        endpoint: EdgeEndpoint::To,
                        ..
                    } => self.ids.reconnect_source_node,
                }
            })
    }
}

fn take_probe(
    update: CanvasInteractionUpdate,
) -> Option<(
    jellyflow_runtime::InteractionQueryId,
    jellyflow_runtime::ConnectionProbeQuery,
)> {
    let queries = update.into_dispatch().queries;
    queries.into_iter().find_map(|query| match query {
        CanvasInteractionQuery::ConnectionProbe { id, query, .. } => Some((id, query)),
        CanvasInteractionQuery::AlignmentSnap { .. } | CanvasInteractionQuery::EdgePick { .. } => {
            None
        }
    })
}

fn connection_probe_query_ref(
    query: &CanvasInteractionQuery,
) -> Option<(
    jellyflow_runtime::InteractionQueryId,
    &jellyflow_runtime::ConnectionProbeQuery,
)> {
    match query {
        CanvasInteractionQuery::ConnectionProbe { id, query, .. } => Some((*id, query)),
        CanvasInteractionQuery::AlignmentSnap { .. } | CanvasInteractionQuery::EdgePick { .. } => {
            None
        }
    }
}

fn drive_connection_pages(
    view: &ViewSession,
    session: &mut CanvasInteractionSession,
    mut update: CanvasInteractionUpdate,
) -> Vec<CanvasInteractionEffect> {
    let mut emitted = Vec::new();
    loop {
        let dispatch = update.into_dispatch();
        let queries = dispatch.queries;
        let effects = dispatch.pre_query_effects;
        emitted.extend(effects);
        let mut queries = queries.into_iter();
        let Some(CanvasInteractionQuery::ConnectionProbe { id, query, .. }) = queries.next() else {
            assert!(queries.next().is_none());
            return emitted;
        };
        assert!(
            queries.next().is_none(),
            "one interaction owns one probe page"
        );
        let outcome = view.connection_probe(query).unwrap();
        update = session.resolve_connection_probe(id, outcome).unwrap();
    }
}

fn settlement(receipt: TraceReceipt) -> CommandSettlement {
    match receipt {
        TraceReceipt::Applied => CommandSettlement::Applied,
        TraceReceipt::Noop => CommandSettlement::Noop,
        TraceReceipt::Rejected => CommandSettlement::Rejected,
        TraceReceipt::Conflict => CommandSettlement::Conflict,
    }
}

fn publish_trace_geometry(view: &ViewSession, ids: TraceGraphIds, anchors: TraceAnchors) {
    let frame = view
        .frame(FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint { x: -50.0, y: -50.0 },
                size: CanvasSize {
                    width: 900.0,
                    height: 500.0,
                },
            },
            RendererStamp::new("trace.node", 1),
        ))
        .unwrap();
    let collector = frame.collector();
    for (node, port, direction, anchor) in [
        (
            ids.source_node,
            ids.source_port,
            PortDirection::Out,
            anchors.source,
        ),
        (
            ids.invalid_node,
            ids.invalid_port,
            PortDirection::Out,
            anchors.invalid,
        ),
        (
            ids.valid_node,
            ids.valid_port,
            PortDirection::In,
            anchors.valid,
        ),
        (
            ids.reconnect_source_node,
            ids.reconnect_source_port,
            PortDirection::Out,
            anchors.reconnect_source,
        ),
        (
            ids.reconnect_target_node,
            ids.reconnect_target_port,
            PortDirection::In,
            anchors.reconnect_target,
        ),
        (
            ids.reconnect_alternate_node,
            ids.reconnect_alternate_port,
            PortDirection::In,
            anchors.reconnect_alternate,
        ),
    ] {
        let frame_node = frame
            .nodes()
            .iter()
            .find(|candidate| candidate.id() == node)
            .unwrap();
        let frame_port = frame_node
            .ports()
            .iter()
            .find(|candidate| candidate.id() == port)
            .unwrap();
        let node_bounds = ViewRect::new(
            ViewPoint::new(anchor.x - 30.0, anchor.y - 30.0),
            ViewSize::new(60.0, 60.0),
        );
        let visual = ViewRect::new(
            ViewPoint::new(anchor.x - 5.0, anchor.y - 5.0),
            ViewSize::new(10.0, 10.0),
        );
        let hit = ViewRect::new(
            ViewPoint::new(anchor.x - 12.0, anchor.y - 12.0),
            ViewSize::new(24.0, 24.0),
        );
        let mut layout = frame_node.begin_layout(node_bounds);
        layout
            .port_rendered(
                frame_port.layout_target(),
                visual,
                hit,
                anchor,
                match direction {
                    PortDirection::In => HandlePosition::Left,
                    PortDirection::Out => HandlePosition::Right,
                },
            )
            .unwrap();
        collector.layout(layout.finish().unwrap()).unwrap();
    }
    view.submit_feedback(collector.finish().unwrap()).unwrap();
}

fn normalized_position(view: &ViewSession, point: ViewPoint) -> InteractionPosition {
    InteractionPosition::from_view(point, view.viewport_transform().unwrap()).unwrap()
}

fn canvas_anchor(view: &ViewSession, point: ViewPoint) -> CanvasPoint {
    view.viewport_transform()
        .unwrap()
        .canvas_point_at_screen(CanvasPoint {
            x: point.x,
            y: point.y,
        })
}

fn trace_node(port: PortId) -> Node {
    Node {
        kind: NodeKindKey::new("trace.node"),
        kind_version: 1,
        pos: CanvasPoint { x: 0.0, y: 0.0 },
        origin: None,
        selectable: None,
        draggable: None,
        connectable: None,
        deletable: None,
        parent: None,
        extent: None,
        expand_parent: None,
        size: Some(CanvasSize {
            width: 60.0,
            height: 60.0,
        }),
        hidden: false,
        collapsed: false,
        ports: vec![port],
        data: serde_json::Value::Null,
    }
}

fn trace_port(node: NodeId, direction: PortDirection) -> Port {
    Port {
        node,
        key: PortKey::new("port"),
        dir: direction,
        kind: PortKind::Data,
        capacity: PortCapacity::Multi,
        connectable: None,
        connectable_start: None,
        connectable_end: None,
        ty: None,
        data: serde_json::Value::Null,
    }
}
