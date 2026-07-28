#![cfg(feature = "adapter-conformance")]

use jellyflow_runtime::adapter::conformance as trace;

use trace::{
    ConformanceAction as TraceInput, ConformanceCancelReason as TraceCancelReason,
    ConformanceCandidate as TraceCandidate, ConformanceEffect as TraceEffect,
    ConformanceGeometry as TraceGeometry, ConformanceGraph as TraceGraph,
    ConformanceIntent as TraceIntent, ConformanceOwnership as TraceOwnership,
    ConformancePhase as TracePhase, ConformanceReconnectEndpoint as TraceReconnectEndpoint,
    ConformanceScenario as CanonicalInteractionTrace, ConformanceScenarioId as TraceScenarioId,
    ConformanceSelection as TraceSelection, ConformanceSettlement as TraceReceipt,
    ConformanceTarget as TraceTarget, ConformanceTerminal as TraceTerminal,
    ConformanceTicket as TraceTicket, ConformanceViewStatus as TraceViewStatus,
    NormalizedObservation, canonical_scenarios, canonical_settlements, compare_observations,
    create_from_wire_scenario as create_from_wire_trace, edge_pick_scenario as edge_pick_trace,
    native_control_scenario as native_control_trace,
    new_connection_scenario as new_connection_trace, node_drag_scenario as node_drag_trace,
    reconnect_scenario as reconnect_trace,
    terminal_ownership_scenarios as terminal_ownership_traces,
};

use jellyflow_core::core::{
    CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeId, EdgeKind, GraphBuilder, GraphId, Node,
    NodeId, NodeKindKey, Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
};
use jellyflow_core::interaction::NodeGraphConnectionMode;
use jellyflow_runtime::commands::{
    CommandExecutionCompletion, GraphCommand, IncidentEdgePolicy, TopologyEdit,
};
use jellyflow_runtime::geometry::{HandlePosition, ViewPoint, ViewRect, ViewSize};
use jellyflow_runtime::graph::{
    EdgeEndpoint, GraphRuntime, GraphRuntimeMiddleware, GraphRuntimeMiddlewareSnapshot,
};
use jellyflow_runtime::interaction::{
    CanvasInteractionConfig, CanvasInteractionEffect, CanvasInteractionPhase,
    CanvasInteractionQueryAdvanceReceipt, CanvasInteractionQueryPageReceipt,
    CanvasInteractionSession, CanvasInteractionUpdate, CanvasPressTarget, ConnectionGestureBinding,
    ConnectionProbe, InteractionDamage, InteractionGraphSubmission, InteractionOwner,
    InteractionTerminalReason, InteractionTicketId, SelectionMode,
};
use jellyflow_runtime::profile::ApplyPipelineError;
use jellyflow_runtime::schema::{NodeCatalog, NodeCreation, NodeKindSpec, NodePortSpec};
use jellyflow_runtime::view::{
    FrameRequest, GeometrySnapshot, RendererStamp, ViewCloseReason, ViewSession, ViewSessionOptions,
};
use jellyflow_runtime::viewport::{ViewportPointerSample, ViewportTransform};

struct RejectCanonicalCommand;

impl GraphRuntimeMiddleware for RejectCanonicalCommand {
    fn before_command(
        &mut self,
        _snapshot: GraphRuntimeMiddlewareSnapshot,
        _command: &GraphCommand,
    ) -> Result<(), ApplyPipelineError> {
        Err(ApplyPipelineError::Rejected {
            message: "canonical runtime command rejected".to_owned(),
            diagnostics: Vec::new(),
        })
    }
}

#[test]
fn canonical_trace_vocabulary_is_deterministic_and_covers_terminal_receipts() {
    for receipt in canonical_settlements() {
        let trace = new_connection_trace(receipt);
        assert_eq!(trace, new_connection_trace(receipt));
        assert_eq!(
            trace.steps.last().expect("terminal step").expected.phase,
            TracePhase::Idle
        );
        assert_eq!(
            trace
                .steps
                .iter()
                .filter(|step| step.expected.effect == TraceEffect::SubmitConnection)
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
                    .filter(|step| step.expected.effect == TraceEffect::SubmitConnection)
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
        rejected
            .steps
            .last()
            .expect("terminal search step")
            .expected
            .phase,
        TracePhase::Searching,
    );

    let control = native_control_trace();
    let drag = node_drag_trace(TraceReceipt::Applied);
    assert_eq!(control.steps[0].expected.phase, TracePhase::Idle);
    assert!(
        drag.steps
            .iter()
            .any(|step| step.expected.phase == TracePhase::NodeDrag)
    );

    let edge = edge_pick_trace();
    assert_eq!(edge.steps[0].expected.effect, TraceEffect::PickEdge);
    assert_eq!(
        edge.steps.last().expect("edge terminal").expected.phase,
        TracePhase::Idle,
    );

    for trace in terminal_ownership_traces() {
        let terminal = trace.steps.last().expect("ownership terminal");
        assert_eq!(terminal.expected.phase, TracePhase::Idle);
        assert_eq!(
            terminal.expected.effect,
            TraceEffect::ReleaseNativeOwnership
        );
    }
}

#[test]
fn observation_comparison_fails_for_semantic_drift_not_display_names() {
    let mut scenario = new_connection_trace(TraceReceipt::Applied);
    let observations: Vec<_> = scenario.steps.iter().map(|step| step.expected).collect();
    scenario.display_name = "renamed-for-diagnostics";
    compare_observations(&scenario, &observations).expect("display names are not identity");

    let mut divergent = observations;
    divergent[1].phase = TracePhase::Idle;
    assert!(compare_observations(&scenario, &divergent).is_err());
}

#[test]
fn canonical_traces_drive_the_renderer_neutral_session_and_real_view_queries() {
    for trace in canonical_scenarios() {
        TraceHarness::for_scenario(trace.id).replay(&trace);
    }
}

#[test]
fn final_release_sample_activates_an_armed_drag_without_a_synthetic_move() {
    let mut harness = TraceHarness::new();
    assert_eq!(
        harness.press(TraceTarget::NodeDragHandle),
        TraceEffect::None
    );
    assert_eq!(
        harness.session.presentation().phase(),
        CanvasInteractionPhase::Armed
    );

    harness.pointer = pointer_sample(&harness.view, harness.anchors.empty);

    assert_eq!(harness.release(), TraceEffect::SubmitNodeMove);
    assert_eq!(
        harness.session.presentation().phase(),
        CanvasInteractionPhase::Committing
    );
}

#[test]
fn repeated_toggle_marquee_trace_is_independent_of_move_event_count() {
    let mut harness = TraceHarness::new();
    set_selection_by_id(
        &harness.view,
        vec![harness.ids.source_node],
        vec![harness.ids.reconnect_edge],
    )
    .unwrap();
    let baseline = harness.view.selection_baseline().unwrap();
    let start = pointer_sample(&harness.view, ViewPoint::new(600.0, 180.0));
    let end = pointer_sample(&harness.view, ViewPoint::new(680.0, 260.0));
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

    let first_update = harness.session.move_to(end).unwrap();
    assert!(drive_query_pages(&mut harness.session, first_update).is_empty());
    let first = harness.view.selection().unwrap();
    let second_update = harness.session.move_to(end).unwrap();
    assert!(drive_query_pages(&mut harness.session, second_update).is_empty());
    let second = harness.view.selection().unwrap();

    assert_eq!(first, second);
}

#[test]
fn interaction_session_owns_selection_and_pan() {
    let harness = TraceHarness::new();
    set_selection_by_id(
        &harness.view,
        vec![harness.ids.source_node, harness.ids.invalid_node],
        vec![harness.ids.reconnect_edge],
    )
    .unwrap();
    let snapshot = harness.view.graph_snapshot().unwrap();
    let valid = snapshot
        .node_lifetime_key(harness.ids.valid_node)
        .expect("valid node lifetime");
    let mut session =
        CanvasInteractionSession::attach(&harness.view, CanvasInteractionConfig::default())
            .unwrap();
    let selected = session.activate_node(valid, SelectionMode::Toggle).unwrap();
    assert!(selected.effects().is_empty());
    assert!(selected.damage().selection);
    let first = harness.view.selection().unwrap();
    assert_eq!(
        first.nodes(),
        [
            harness.ids.source_node,
            harness.ids.invalid_node,
            harness.ids.valid_node,
        ]
    );
    assert_eq!(first.edges(), [harness.ids.reconnect_edge]);

    let before = harness.view.viewport_snapshot().unwrap();
    let start = pointer_sample(&harness.view, ViewPoint::new(100.0, 100.0));
    let end = pointer_sample(&harness.view, ViewPoint::new(120.0, 90.0));
    session.press(CanvasPressTarget::Pan, start).unwrap();
    let update = session.move_to(end).unwrap();
    assert!(update.damage().viewport);
    assert!(update.effects().is_empty());
    assert!(!update.query_work_pending());
    let after = harness.view.viewport_snapshot().unwrap();
    assert_ne!(after.revision(), before.revision());
    assert_eq!(after.transform().zoom, before.transform().zoom);
    assert_eq!(
        after.transform().pan.x,
        before.transform().pan.x + 20.0 / before.transform().zoom
    );
    assert_eq!(
        after.transform().pan.y,
        before.transform().pan.y - 10.0 / before.transform().zoom
    );
    session
        .release(
            harness
                .view
                .viewport_pointer_sample(ViewPoint::new(120.0, 90.0))
                .unwrap(),
        )
        .unwrap();
}

#[test]
fn runtime_owned_queries_are_session_local_single_slot_and_cancel_discards_work() {
    let harness = TraceHarness::new();
    let mut first =
        CanvasInteractionSession::attach(&harness.view, CanvasInteractionConfig::default())
            .unwrap();
    let mut second =
        CanvasInteractionSession::attach(&harness.view, CanvasInteractionConfig::default())
            .unwrap();
    let pointer = pointer_sample(&harness.view, ViewPoint::new(400.0, 80.0));

    let first_request = first.request_edge_pick(pointer).unwrap();
    assert!(first_request.query_work_pending());
    assert!(first.query_work_pending());

    let second_request = second.request_edge_pick(pointer).unwrap();
    assert!(second_request.query_work_pending());
    assert!(second.query_work_pending());

    let second_phase = second.presentation().phase();
    let (second_update, second_receipt) = advance_successful_query_page(&mut second);
    assert_eq!(second.presentation().phase(), second_phase);
    assert!(matches!(
        second_receipt.page_receipts(),
        [CanvasInteractionQueryPageReceipt::EdgePick(_)]
    ));
    assert!(!second_update.query_work_pending());
    assert!(!second.query_work_pending());
    assert!(
        first.query_work_pending(),
        "advancing another session cannot consume the first session's work"
    );

    let replacement = first.request_edge_pick(pointer).unwrap();
    assert!(replacement.query_work_pending());
    let (superseded, needs_tick, receipt, terminal_error) =
        first.advance_query_work(2).into_parts();
    assert_eq!(receipt.pages_granted(), 2);
    assert_eq!(
        receipt.pages_attempted(),
        1,
        "a replacement query supersedes the prior single-slot continuation"
    );
    assert!(matches!(
        receipt.page_receipts(),
        [CanvasInteractionQueryPageReceipt::EdgePick(_)]
    ));
    assert!(terminal_error.is_none());
    assert!(!needs_tick);
    assert!(!superseded.query_work_pending());
    assert!(!first.query_work_pending());

    let cancellable = first.request_edge_pick(pointer).unwrap();
    assert!(cancellable.query_work_pending());
    let cancelled = first.cancel().unwrap();
    assert!(!cancelled.query_work_pending());
    assert!(!first.query_work_pending());

    let (after_cancel, needs_tick, receipt, terminal_error) =
        first.advance_query_work(1).into_parts();
    assert_eq!(receipt.pages_granted(), 1);
    assert_eq!(receipt.pages_attempted(), 0);
    assert!(receipt.page_receipts().is_empty());
    assert!(terminal_error.is_none());
    assert!(!needs_tick);
    assert!(!after_cancel.query_work_pending());
    assert!(after_cancel.effects().is_empty());
    assert_eq!(after_cancel.damage(), InteractionDamage::default());
}

#[test]
fn connection_commit_revalidation_resumes_real_view_query_before_resubmission() {
    let harness = TraceHarness::new();
    let config = CanvasInteractionConfig {
        connection_acquire_radius_in_view: 1_000.0,
        connection_release_radius_in_view: 1_000.0,
        query_entry_budget: 1,
        ..CanvasInteractionConfig::default()
    };
    let mut session = CanvasInteractionSession::attach(&harness.view, config).unwrap();
    let drag_intent = harness
        .view
        .port_connection_drag_intent(port_instance(&harness.view, harness.ids.source_port))
        .expect("source port drag facts");
    let start = harness
        .view
        .connection_start_from_port_drag_intent(
            drag_intent,
            EdgeId::from_u128(0xA11),
            NodeGraphConnectionMode::Strict,
            CanvasPoint {
                x: harness.anchors.source.x,
                y: harness.anchors.source.y,
            },
            HandlePosition::Right,
            HandlePosition::Left,
            false,
        )
        .unwrap()
        .unwrap();
    let source = pointer_sample(&harness.view, harness.anchors.source);
    let target = pointer_sample(&harness.view, harness.anchors.valid);
    session
        .press(CanvasPressTarget::Connection(start), source)
        .unwrap();

    let moved = session.move_to(target).unwrap();
    assert!(
        drive_query_pages(&mut session, moved).is_empty(),
        "drag probe has no graph effect"
    );

    let released = session.release(target).unwrap();
    let mut effects = drive_query_pages(&mut session, released).into_iter();
    let Some(CanvasInteractionEffect::SubmitGraph { submission }) = effects.next() else {
        panic!("release must submit one connection mutation");
    };
    assert!(effects.next().is_none());
    let ticket = submission.ticket();

    let mut update = session.resume_submission_after_prefix(submission).unwrap();
    let mut query_pages = 0;
    loop {
        let dispatch = update.into_dispatch();
        if !dispatch.effects.is_empty() {
            let [CanvasInteractionEffect::SubmitGraph { submission }] = dispatch.effects.as_slice()
            else {
                panic!("completed revalidation emits one connection mutation");
            };
            assert_eq!(submission.ticket(), ticket);
            break;
        }
        assert!(
            dispatch.query_work_pending,
            "pending revalidation remains owned by the interaction session"
        );
        query_pages += 1;
        let (next, receipt) = advance_successful_query_page(&mut session);
        assert!(matches!(
            receipt.page_receipts(),
            [CanvasInteractionQueryPageReceipt::ConnectionProbe(_)]
        ));
        update = next;
    }
    assert!(query_pages > 1, "the bounded revalidation must paginate");
    assert_eq!(
        session.presentation().phase(),
        CanvasInteractionPhase::Committing
    );
}

#[test]
fn paged_connection_queries_restart_after_viewport_layout_and_graph_changes() {
    let harness = TraceHarness::new();
    let config = CanvasInteractionConfig {
        connection_acquire_radius_in_view: 1_000.0,
        connection_release_radius_in_view: 1_000.0,
        query_entry_budget: 1,
        ..CanvasInteractionConfig::default()
    };
    let mut session = CanvasInteractionSession::attach(&harness.view, config).unwrap();
    let drag_intent = harness
        .view
        .port_connection_drag_intent(port_instance(&harness.view, harness.ids.source_port))
        .expect("source port drag facts");
    let start = harness
        .view
        .connection_start_from_port_drag_intent(
            drag_intent,
            EdgeId::from_u128(0xA12),
            NodeGraphConnectionMode::Strict,
            canvas_anchor(&harness.view, harness.anchors.source),
            HandlePosition::Right,
            HandlePosition::Left,
            false,
        )
        .unwrap()
        .unwrap();
    let source = pointer_sample(&harness.view, harness.anchors.source);
    let target = pointer_sample(&harness.view, harness.anchors.valid);
    session
        .press(CanvasPressTarget::Connection(start), source)
        .unwrap();

    let moved = session.move_to(target).unwrap();
    assert!(moved.query_work_pending());
    let (drag_resume, drag_receipt) = advance_successful_query_page(&mut session);
    assert!(matches!(
        drag_receipt.page_receipts(),
        [CanvasInteractionQueryPageReceipt::ConnectionProbe(_)]
    ));
    assert!(drag_resume.query_work_pending());
    harness
        .view
        .set_viewport(ViewportTransform::new(CanvasPoint { x: 2.0, y: -1.0 }, 1.0).unwrap())
        .unwrap();
    let (restarted_drag, stale_drag_receipt) = advance_query_page(&mut session);
    assert!(
        stale_drag_receipt.page_receipts().is_empty(),
        "a viewport-stale cursor restarts without claiming a successful view page"
    );
    assert!(
        restarted_drag.query_work_pending(),
        "the session restarts a viewport-stale cursor from page one"
    );
    assert!(drive_query_pages(&mut session, restarted_drag).is_empty());
    assert_eq!(
        session.presentation().phase(),
        CanvasInteractionPhase::ConnectionDrag
    );

    let release_position = pointer_sample(&harness.view, harness.anchors.valid);
    let released = session.release(release_position).unwrap();
    assert!(released.query_work_pending());
    let (release_resume, release_receipt) = advance_successful_query_page(&mut session);
    assert!(matches!(
        release_receipt.page_receipts(),
        [CanvasInteractionQueryPageReceipt::ConnectionProbe(_)]
    ));
    assert!(release_resume.query_work_pending());
    let changed_anchors = TraceAnchors {
        valid: ViewPoint::new(harness.anchors.valid.x + 2.0, harness.anchors.valid.y),
        ..harness.anchors
    };
    publish_trace_geometry(&harness.view, harness.ids, changed_anchors);
    let (restarted_release, stale_release_receipt) = advance_query_page(&mut session);
    assert!(
        stale_release_receipt.page_receipts().is_empty(),
        "a geometry-stale cursor restarts without claiming a successful view page"
    );
    assert!(
        restarted_release.query_work_pending(),
        "the session restarts a geometry-stale cursor from page one"
    );
    let mut release_effects = drive_query_pages(&mut session, restarted_release).into_iter();
    let Some(CanvasInteractionEffect::SubmitGraph { submission }) = release_effects.next() else {
        panic!("completed release probe submits one connection mutation");
    };
    assert!(release_effects.next().is_none());
    let ticket = submission.ticket();

    let revalidation = session.resume_submission_after_prefix(submission).unwrap();
    assert!(revalidation.query_work_pending());
    let (revalidation_resume, revalidation_receipt) = advance_successful_query_page(&mut session);
    assert!(matches!(
        revalidation_receipt.page_receipts(),
        [CanvasInteractionQueryPageReceipt::ConnectionProbe(_)]
    ));
    assert!(revalidation_resume.query_work_pending());
    let version = harness.runtime.snapshot().unwrap().version();
    harness
        .runtime
        .execute(GraphCommand::set_node_data(
            version,
            harness.ids.invalid_node,
            serde_json::json!({ "query_epoch": 1 }),
        ))
        .into_result()
        .unwrap();
    publish_trace_geometry(&harness.view, harness.ids, changed_anchors);
    let (restarted_revalidation, stale_revalidation_receipt) = advance_query_page(&mut session);
    assert!(
        stale_revalidation_receipt.page_receipts().is_empty(),
        "a graph-stale cursor restarts without claiming a successful view page"
    );
    assert!(
        restarted_revalidation.query_work_pending(),
        "the session restarts a graph-stale cursor from page one"
    );
    let revalidation_effects = drive_query_pages(&mut session, restarted_revalidation);
    let [CanvasInteractionEffect::SubmitGraph { submission }] = revalidation_effects.as_slice()
    else {
        panic!("completed revalidation resubmits one connection mutation");
    };
    assert_eq!(submission.ticket(), ticket);
    assert_eq!(
        session.presentation().phase(),
        CanvasInteractionPhase::Committing
    );
}

struct TraceHarness {
    runtime: GraphRuntime,
    view: ViewSession,
    session: CanvasInteractionSession,
    ids: TraceGraphIds,
    anchors: TraceAnchors,
    pointer: ViewportPointerSample,
    ticket: Option<InteractionTicketId>,
    submission: Option<InteractionGraphSubmission>,
    initial_revision: u64,
    excluded_revision_delta: u64,
    prefix_held: bool,
    native_owner: TraceNativeOwner,
    scenario: TraceScenarioId,
    baseline_source_position: CanvasPoint,
    baseline_source_bounds: CanvasRect,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TraceNativeOwner {
    Released,
    Adapter,
    Successor,
}

#[derive(Clone, Copy)]
struct TraceGraphIds {
    source_node: NodeId,
    source_port: PortId,
    invalid_node: NodeId,
    invalid_port: PortId,
    valid_node: NodeId,
    valid_port: PortId,
    created_node: NodeId,
    created_port: PortId,
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
        Self::for_scenario(TraceScenarioId::NewConnection(TraceReceipt::Applied))
    }

    fn for_scenario(scenario: TraceScenarioId) -> Self {
        let ids = TraceGraphIds {
            source_node: NodeId::from_u128(1),
            source_port: PortId::from_u128(1),
            invalid_node: NodeId::from_u128(2),
            invalid_port: PortId::from_u128(2),
            valid_node: NodeId::from_u128(3),
            valid_port: PortId::from_u128(3),
            created_node: NodeId::from_u128(7),
            created_port: PortId::from_u128(7),
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
        let runtime = match scenario_settlement(scenario) {
            Some(TraceReceipt::Rejected) => runtime.with_middleware(RejectCanonicalCommand),
            Some(
                TraceReceipt::Applied
                | TraceReceipt::Noop
                | TraceReceipt::Conflict
                | TraceReceipt::TransportFailure,
            )
            | None => runtime,
        };
        let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
        publish_trace_geometry(&view, ids, anchors);
        let baseline_source_position = runtime
            .snapshot()
            .unwrap()
            .graph()
            .node(&ids.source_node)
            .unwrap()
            .pos;
        let baseline_source_bounds = view
            .geometry_snapshot()
            .unwrap()
            .node(ids.source_node)
            .unwrap()
            .bounds();
        let session =
            CanvasInteractionSession::attach(&view, CanvasInteractionConfig::default()).unwrap();
        let pointer = pointer_sample(&view, anchors.source);
        Self {
            runtime,
            view,
            session,
            ids,
            anchors,
            pointer,
            ticket: None,
            submission: None,
            initial_revision: 0,
            excluded_revision_delta: 0,
            prefix_held: false,
            native_owner: TraceNativeOwner::Released,
            scenario,
            baseline_source_position,
            baseline_source_bounds,
        }
    }

    fn replay(&mut self, trace: &CanonicalInteractionTrace) {
        if matches!(
            trace.id,
            trace::ConformanceScenarioId::ReconnectSource(_)
                | trace::ConformanceScenarioId::ReconnectTarget(_)
                | trace::ConformanceScenarioId::ReconnectSourcePrefixWait
                | trace::ConformanceScenarioId::ReconnectTargetPrefixWait
        ) {
            set_selection_by_id(&self.view, Vec::new(), vec![self.ids.reconnect_edge])
                .expect("seed canonical reconnect selection");
        }
        let mut observations = Vec::with_capacity(trace.steps.len());
        for step in &trace.steps {
            let effect = self.apply(step.action);
            observations.push(self.observation(effect));
        }
        compare_observations(trace, &observations)
            .unwrap_or_else(|error| panic!("{}: {error}", trace.display_name));
    }

    fn apply(&mut self, input: TraceInput) -> TraceEffect {
        match input {
            TraceInput::QueuePrefix => {
                assert!(!self.prefix_held, "the canonical trace holds one prefix");
                self.prefix_held = true;
                TraceEffect::None
            }
            TraceInput::DrainPrefix => {
                assert!(
                    self.prefix_held,
                    "the canonical trace queued a prefix first"
                );
                let snapshot = self.runtime.snapshot().expect("fresh trace snapshot");
                self.runtime
                    .execute(GraphCommand::set_node_data(
                        snapshot.version(),
                        self.ids.invalid_node,
                        serde_json::json!({ "external_prefix": true }),
                    ))
                    .into_result()
                    .expect("the unrelated prefix command executes")
                    .applied()
                    .expect("the unrelated prefix command mutates the graph");
                self.excluded_revision_delta += 1;
                self.prefix_held = false;
                self.resume_pending_submission();
                TraceEffect::None
            }
            TraceInput::Press(target) => {
                let effect = self.press(target);
                self.native_owner = if target == TraceTarget::NativeControl {
                    TraceNativeOwner::Released
                } else {
                    TraceNativeOwner::Adapter
                };
                effect
            }
            TraceInput::CrossDragThreshold => {
                self.pointer = pointer_sample(&self.view, self.anchors.empty);
                let update = self.session.move_to(self.pointer).unwrap();
                self.capture_probe(update)
            }
            TraceInput::MoveToCandidate(candidate) => self.resolve_candidate(candidate),
            TraceInput::Release => {
                let effect = self.release();
                self.native_owner = TraceNativeOwner::Released;
                effect
            }
            TraceInput::SelectSearchCandidate => {
                let request = self
                    .session
                    .search_request()
                    .expect("create-from-wire trace owns an active search request");
                self.session.search_ready(&request, 1).unwrap();
                let selection = self
                    .session
                    .create_search_selection(
                        &request,
                        self.ids.created_port,
                        trace_creation(self.ids),
                    )
                    .expect("create-from-wire view remains available")
                    .expect("create-from-wire trace carries topology");
                let update = self.session.select_search_result(selection).unwrap();
                self.capture_submit(update, TraceEffect::SubmitConnection)
            }
            TraceInput::AdvanceFrame => TraceEffect::None,
            TraceInput::Settle(receipt) => {
                let ticket = self.ticket.take().expect("trace submitted one command");
                let submission = self
                    .submission
                    .take()
                    .expect("trace captured one graph submission");
                match receipt {
                    TraceReceipt::Noop => panic!(
                        "canonical mutation scenarios do not synthesize no-command settlements"
                    ),
                    TraceReceipt::TransportFailure => {
                        drop(submission);
                        self.session
                            .submission_not_dispatched(ticket)
                            .expect("undispatched submission is reported");
                    }
                    TraceReceipt::Applied | TraceReceipt::Rejected | TraceReceipt::Conflict => {
                        let completion = self.dispatch_submission(receipt, submission);
                        self.session
                            .complete_submission(ticket, completion)
                            .expect("command completion settles the exact submission");
                    }
                }
                match receipt {
                    TraceReceipt::Applied | TraceReceipt::Noop => TraceEffect::None,
                    TraceReceipt::Rejected
                    | TraceReceipt::Conflict
                    | TraceReceipt::TransportFailure => TraceEffect::RestorePresentation,
                }
            }
            TraceInput::Cancel(reason) => {
                assert!(matches!(
                    reason,
                    TraceCancelReason::Escape | TraceCancelReason::OutsideClick
                ));
                self.session.cancel().unwrap();
                self.native_owner = TraceNativeOwner::Released;
                TraceEffect::ReleaseNativeOwnership
            }
            TraceInput::LoseCapture => {
                self.session.cancel().unwrap();
                self.native_owner = TraceNativeOwner::Released;
                TraceEffect::ReleaseNativeOwnership
            }
            TraceInput::ReplaceOwner => {
                self.session.cancel().unwrap();
                self.native_owner = TraceNativeOwner::Successor;
                TraceEffect::ReleaseNativeOwnership
            }
            TraceInput::ReleasePreviousOwner => {
                assert_eq!(self.native_owner, TraceNativeOwner::Successor);
                TraceEffect::None
            }
            TraceInput::RetireSource => {
                let snapshot = self.runtime.snapshot().unwrap();
                let source_owner = snapshot
                    .node_lifetime_key(self.ids.source_node)
                    .expect("canonical source has an exact lifetime");
                self.runtime
                    .execute(GraphCommand::topology(
                        snapshot.version(),
                        TopologyEdit::new()
                            .remove_node(self.ids.source_node, IncidentEdgePolicy::Reject),
                    ))
                    .into_result()
                    .unwrap()
                    .applied()
                    .expect("canonical source retirement applies");
                self.excluded_revision_delta = self.excluded_revision_delta.saturating_add(1);
                self.session
                    .owner_retired(InteractionOwner::Node(source_owner));
                self.native_owner = TraceNativeOwner::Released;
                TraceEffect::ReleaseNativeOwnership
            }
            TraceInput::CloseHost => {
                self.session.close();
                self.view.close().expect("canonical host closes its view");
                self.native_owner = TraceNativeOwner::Released;
                TraceEffect::ReleaseNativeOwnership
            }
        }
    }

    fn dispatch_submission(
        &mut self,
        requested: TraceReceipt,
        submission: InteractionGraphSubmission,
    ) -> CommandExecutionCompletion {
        let command = self
            .session
            .bind_submission(submission)
            .expect("runtime transport binds the emitted mutation")
            .expect("canonical emitted mutation is not a no-op");
        match requested {
            TraceReceipt::Conflict => self.execute_external_revision("conflict"),
            TraceReceipt::Applied | TraceReceipt::Rejected => {}
            TraceReceipt::Noop | TraceReceipt::TransportFailure => {
                unreachable!("non-dispatch receipts are handled before command binding")
            }
        }
        self.runtime.execute(command)
    }

    fn execute_external_revision(&mut self, reason: &str) {
        let snapshot = self
            .runtime
            .snapshot()
            .expect("external runtime transport snapshot");
        self.runtime
            .execute(GraphCommand::set_node_data(
                snapshot.version(),
                self.ids.invalid_node,
                serde_json::json!({ "external": reason }),
            ))
            .into_result()
            .expect("external runtime transport command executes")
            .applied()
            .expect("external runtime transport command applies");
        self.excluded_revision_delta = self.excluded_revision_delta.saturating_add(1);
    }

    fn press(&mut self, target: TraceTarget) -> TraceEffect {
        match target {
            TraceTarget::NativeControl => {
                self.session
                    .press(CanvasPressTarget::NativeControl, self.pointer)
                    .unwrap();
                TraceEffect::None
            }
            TraceTarget::NodeDragHandle => {
                let start = self
                    .view
                    .node_drag_start(
                        node_lifetime(&self.view, self.ids.source_node),
                        SelectionMode::Replace,
                    )
                    .unwrap()
                    .unwrap();
                let update = self
                    .session
                    .press(CanvasPressTarget::NodeDrag(start), self.pointer)
                    .unwrap();
                self.apply_view_local_effects(update);
                TraceEffect::None
            }
            TraceTarget::SourcePort => {
                let drag_intent = self
                    .view
                    .port_connection_drag_intent(port_instance(&self.view, self.ids.source_port))
                    .expect("source port drag facts");
                let start = self
                    .view
                    .connection_start_from_port_drag_intent(
                        drag_intent,
                        EdgeId::from_u128(100),
                        NodeGraphConnectionMode::Strict,
                        canvas_anchor(&self.view, self.anchors.source),
                        HandlePosition::Right,
                        HandlePosition::Left,
                        true,
                    )
                    .unwrap()
                    .unwrap();
                self.session
                    .press(CanvasPressTarget::Connection(start), self.pointer)
                    .unwrap();
                TraceEffect::None
            }
            endpoint @ (TraceTarget::ReconnectSource | TraceTarget::ReconnectTarget) => {
                let (runtime_endpoint, fixed_anchor) = match endpoint {
                    TraceTarget::ReconnectSource => {
                        (EdgeEndpoint::From, self.anchors.reconnect_target)
                    }
                    TraceTarget::ReconnectTarget => {
                        (EdgeEndpoint::To, self.anchors.reconnect_source)
                    }
                    _ => unreachable!(),
                };
                let geometry = self.view.geometry_snapshot().unwrap();
                let route = geometry
                    .route(self.ids.reconnect_edge)
                    .expect("published reconnect route");
                let fixed_endpoint = match runtime_endpoint {
                    EdgeEndpoint::From => route.facts().endpoints.target,
                    EdgeEndpoint::To => route.facts().endpoints.source,
                };
                let start = self
                    .view
                    .connection_start_from_route(
                        route,
                        runtime_endpoint,
                        fixed_endpoint,
                        NodeGraphConnectionMode::Strict,
                        false,
                    )
                    .unwrap()
                    .expect("authoritative reconnect start");
                self.pointer = pointer_sample(&self.view, fixed_anchor);
                self.session
                    .press(CanvasPressTarget::Connection(start), self.pointer)
                    .unwrap();
                TraceEffect::None
            }
            TraceTarget::Edge => {
                let midpoint = ViewPoint::new(400.0, 80.0);
                let pointer = pointer_sample(&self.view, midpoint);
                let dispatch = self
                    .session
                    .request_edge_pick(pointer)
                    .unwrap()
                    .into_dispatch();
                let effects = dispatch.effects;
                assert!(effects.is_empty());
                assert!(
                    dispatch.query_work_pending,
                    "edge trace leaves its pick query inside the session"
                );
                let (resolved, receipt) = advance_successful_query_page(&mut self.session);
                assert!(matches!(
                    receipt.page_receipts(),
                    [CanvasInteractionQueryPageReceipt::EdgePick(_)]
                ));
                assert!(!resolved.query_work_pending());
                assert!(resolved.effects().is_empty());
                let picked = self
                    .session
                    .presentation()
                    .hovered_edge()
                    .expect("edge trace must pick the routed edge");
                let update = self
                    .session
                    .press(
                        CanvasPressTarget::Edge {
                            instance: picked,
                            selection_mode: SelectionMode::Replace,
                        },
                        pointer,
                    )
                    .unwrap();
                self.apply_view_local_effects(update);
                TraceEffect::PickEdge
            }
        }
    }

    fn resolve_candidate(&mut self, candidate: TraceCandidate) -> TraceEffect {
        let queried = match candidate {
            TraceCandidate::None => self.session.query_work_pending(),
            TraceCandidate::Invalid => {
                self.pointer = pointer_sample(&self.view, self.invalid_anchor());
                let update = self.session.move_to(self.pointer).unwrap();
                self.capture_probe(update) == TraceEffect::ProbeConnection
            }
            TraceCandidate::Valid => {
                self.pointer = pointer_sample(&self.view, self.valid_anchor());
                let update = self.session.move_to(self.pointer).unwrap();
                self.capture_probe(update) == TraceEffect::ProbeConnection
            }
        };
        assert!(queried, "candidate action must issue a real view query");
        assert!(
            self.session.query_work_pending(),
            "candidate follows session-owned probe work"
        );
        let (update, receipt) = advance_successful_query_page(&mut self.session);
        assert!(matches!(
            receipt.page_receipts(),
            [CanvasInteractionQueryPageReceipt::ConnectionProbe(_)]
        ));
        assert!(!update.query_work_pending());
        assert!(update.effects().is_empty());
        let presentation = self.session.presentation();
        let probe = presentation
            .connection()
            .expect("candidate resolution keeps the connection drag active")
            .probe();
        match (&candidate, probe) {
            (TraceCandidate::None, ConnectionProbe::None)
            | (TraceCandidate::Invalid, ConnectionProbe::Invalid { .. })
            | (TraceCandidate::Valid, ConnectionProbe::Valid { .. }) => {}
            _ => panic!("trace candidate disagrees with the session-owned query: {probe:?}"),
        }
        TraceEffect::ProbeConnection
    }

    fn release(&mut self) -> TraceEffect {
        let was_connection = self.session.presentation().connection().is_some();
        let update = self.session.release(self.pointer).unwrap();
        if matches!(
            self.session.presentation().phase(),
            CanvasInteractionPhase::Committing
        ) {
            let effect = if was_connection {
                TraceEffect::SubmitConnection
            } else {
                TraceEffect::SubmitNodeMove
            };
            return self.capture_submit(update, effect);
        }
        if !was_connection {
            self.apply_view_local_effects(update);
            return TraceEffect::None;
        }
        let effects = drive_query_pages(&mut self.session, update);
        if effects
            .iter()
            .any(|effect| matches!(effect, CanvasInteractionEffect::Search(_)))
        {
            return TraceEffect::OpenSearch;
        }
        self.capture_submit_effects(effects, TraceEffect::SubmitConnection)
    }

    fn capture_probe(&mut self, update: CanvasInteractionUpdate) -> TraceEffect {
        let dispatch = update.into_dispatch();
        assert!(dispatch.effects.is_empty());
        assert_eq!(
            dispatch.query_work_pending,
            self.session.query_work_pending(),
            "dispatch exposes only whether the session owns query continuation work"
        );
        if dispatch.query_work_pending {
            TraceEffect::ProbeConnection
        } else {
            TraceEffect::None
        }
    }

    fn capture_submit(
        &mut self,
        update: CanvasInteractionUpdate,
        semantic_effect: TraceEffect,
    ) -> TraceEffect {
        let dispatch = update.into_dispatch();
        assert!(
            !dispatch.query_work_pending,
            "submission capture requires all session-owned queries to be resolved"
        );
        self.capture_submit_effects(dispatch.effects, semantic_effect)
    }

    fn capture_submit_effects(
        &mut self,
        effects: Vec<CanvasInteractionEffect>,
        semantic_effect: TraceEffect,
    ) -> TraceEffect {
        for effect in effects {
            if let CanvasInteractionEffect::SubmitGraph { submission } = effect {
                self.ticket = Some(submission.ticket());
                self.submission = Some(submission);
                return semantic_effect;
            }
        }
        TraceEffect::None
    }

    fn apply_view_local_effects(&self, update: CanvasInteractionUpdate) {
        let dispatch = update.into_dispatch();
        assert!(
            !dispatch.query_work_pending,
            "selection press issued a query"
        );
        assert!(
            dispatch.effects.is_empty(),
            "selection press emitted graph work"
        );
    }

    fn resume_pending_submission(&mut self) {
        let Some(submission) = self.submission.take() else {
            return;
        };
        let ticket = submission.ticket();
        assert_eq!(self.ticket, Some(ticket));
        let update = self
            .session
            .resume_submission_after_prefix(submission)
            .expect("prefix resumes through the session-owned protocol");
        let effects = drive_query_pages(&mut self.session, update);
        let mut submissions = effects.into_iter().filter_map(|effect| match effect {
            CanvasInteractionEffect::SubmitGraph { submission } => Some(submission),
            CanvasInteractionEffect::Search(_) => None,
        });
        let submission = submissions
            .next()
            .expect("prefix continuation emits a fresh submission");
        assert!(submissions.next().is_none());
        assert_eq!(submission.ticket(), ticket);
        self.submission = Some(submission);
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

    fn observation(&self, effect: TraceEffect) -> NormalizedObservation {
        let presentation = self.session.presentation();
        let phase = self.observed_phase();
        let ownership = match self.native_owner {
            TraceNativeOwner::Released => TraceOwnership::Released,
            TraceNativeOwner::Adapter => TraceOwnership::HeldByAdapter,
            TraceNativeOwner::Successor => TraceOwnership::HeldBySuccessor,
        };
        let ticket = if self.ticket.is_some() {
            TraceTicket::Pending
        } else {
            TraceTicket::None
        };
        let graph_revision_delta = self
            .runtime
            .snapshot()
            .expect("trace runtime remains available")
            .revision()
            .saturating_sub(self.initial_revision)
            .saturating_sub(self.excluded_revision_delta);
        let graph = self.observed_graph();
        let geometry = self.observed_geometry(graph);
        let mut observation = NormalizedObservation::new(
            phase,
            effect,
            ownership,
            ticket,
            graph_revision_delta,
            geometry,
        );
        observation.terminal = presentation.terminal().map(trace_terminal);
        observation.intent = presentation
            .connection_intent()
            .map_or(TraceIntent::None, trace_intent);
        observation.graph = graph;
        observation.optimistic_presentation = self.ticket.is_some()
            && (presentation.connection().is_some() || presentation.node_drag().is_some());
        observation.selection = match self.view.selection() {
            Ok(selection)
                if selection.nodes().is_empty()
                    && selection.edges().is_empty()
                    && selection.groups().is_empty() =>
            {
                TraceSelection::Empty
            }
            Ok(selection)
                if selection.nodes() == [self.ids.source_node]
                    && selection.edges().is_empty()
                    && selection.groups().is_empty() =>
            {
                TraceSelection::SourceNode
            }
            Ok(selection)
                if selection.nodes().is_empty()
                    && selection.edges() == [self.ids.reconnect_edge]
                    && selection.groups().is_empty() =>
            {
                TraceSelection::CanonicalEdge
            }
            Ok(selection) => panic!("unexpected canonical selection: {selection:?}"),
            Err(_) => TraceSelection::Unavailable,
        };
        observation.view_status = match self.view.close_reason() {
            None => TraceViewStatus::Open,
            Some(ViewCloseReason::HostClosed) => TraceViewStatus::HostClosed,
            Some(ViewCloseReason::RuntimeDropped) => TraceViewStatus::RuntimeDropped,
        };
        observation
    }

    fn observed_graph(&self) -> TraceGraph {
        let snapshot = self
            .runtime
            .snapshot()
            .expect("trace graph remains available");
        let graph = snapshot.graph();
        if graph.node(&self.ids.source_node).is_none() {
            assert!(graph.port(&self.ids.source_port).is_none());
            return TraceGraph::SourceRetired;
        }
        if let Some(edge) = graph.edge(&EdgeId::from_u128(100)) {
            return match (edge.from, edge.to) {
                (from, to) if from == self.ids.source_port && to == self.ids.valid_port => {
                    TraceGraph::NewConnection
                }
                (from, to) if from == self.ids.source_port && to == self.ids.created_port => {
                    assert!(graph.node(&self.ids.created_node).is_some());
                    assert!(graph.port(&self.ids.created_port).is_some());
                    TraceGraph::CreateFromWire
                }
                endpoints => panic!("unexpected canonical new edge endpoints: {endpoints:?}"),
            };
        }
        let reconnect = graph
            .edge(&self.ids.reconnect_edge)
            .expect("canonical reconnect edge remains present");
        match (reconnect.from, reconnect.to) {
            (from, to) if from == self.ids.source_port && to == self.ids.reconnect_target_port => {
                TraceGraph::ReconnectSource
            }
            (from, to)
                if from == self.ids.reconnect_source_port
                    && to == self.ids.reconnect_alternate_port =>
            {
                TraceGraph::ReconnectTarget
            }
            (from, to)
                if from == self.ids.reconnect_source_port
                    && to == self.ids.reconnect_target_port =>
            {
                let source = graph
                    .node(&self.ids.source_node)
                    .expect("canonical source node remains present");
                if source.pos == self.baseline_source_position {
                    TraceGraph::Baseline
                } else {
                    let expected = CanvasPoint {
                        x: self.baseline_source_position.x + self.anchors.empty.x
                            - self.anchors.source.x,
                        y: self.baseline_source_position.y + self.anchors.empty.y
                            - self.anchors.source.y,
                    };
                    assert!((source.pos.x - expected.x).abs() < 0.001);
                    assert!((source.pos.y - expected.y).abs() < 0.001);
                    TraceGraph::NodeMoved
                }
            }
            endpoints => panic!("unexpected canonical reconnect endpoints: {endpoints:?}"),
        }
    }

    fn observed_geometry(&self, graph: TraceGraph) -> TraceGeometry {
        let Ok(geometry) = self.view.geometry_snapshot() else {
            return TraceGeometry::Unavailable;
        };
        for port in [
            self.ids.valid_port,
            self.ids.reconnect_source_port,
            self.ids.reconnect_target_port,
            self.ids.reconnect_alternate_port,
        ] {
            let published = geometry
                .port(port)
                .unwrap_or_else(|| panic!("canonical geometry is missing port {port:?}"));
            assert!(published.hit_bounds().is_some());
        }
        match graph {
            TraceGraph::Baseline => {
                self.assert_route_matches(
                    &geometry,
                    self.ids.reconnect_edge,
                    self.ids.reconnect_source_port,
                    self.ids.reconnect_target_port,
                );
                TraceGeometry::Baseline
            }
            TraceGraph::SourceRetired => {
                assert!(geometry.node(self.ids.source_node).is_none());
                assert!(geometry.port(self.ids.source_port).is_none());
                TraceGeometry::SourceRetired
            }
            TraceGraph::NewConnection => {
                self.assert_route_matches(
                    &geometry,
                    EdgeId::from_u128(100),
                    self.ids.source_port,
                    self.ids.valid_port,
                );
                TraceGeometry::NewConnectionRouted
            }
            TraceGraph::ReconnectSource => {
                self.assert_route_matches(
                    &geometry,
                    self.ids.reconnect_edge,
                    self.ids.source_port,
                    self.ids.reconnect_target_port,
                );
                TraceGeometry::ReconnectSourceRouted
            }
            TraceGraph::ReconnectTarget => {
                self.assert_route_matches(
                    &geometry,
                    self.ids.reconnect_edge,
                    self.ids.reconnect_source_port,
                    self.ids.reconnect_alternate_port,
                );
                TraceGeometry::ReconnectTargetRouted
            }
            TraceGraph::CreateFromWire => {
                assert!(geometry.port(self.ids.created_port).is_none());
                assert!(geometry.route(EdgeId::from_u128(100)).is_none());
                TraceGeometry::CreateFromWireAwaitingLayout
            }
            TraceGraph::NodeMoved => {
                let graph = self.runtime.snapshot().unwrap();
                let source_position = graph.graph().node(&self.ids.source_node).unwrap().pos;
                let source_bounds = geometry.node(self.ids.source_node).unwrap().bounds();
                let dx = source_position.x - self.baseline_source_position.x;
                let dy = source_position.y - self.baseline_source_position.y;
                assert!(
                    (source_bounds.origin.x - self.baseline_source_bounds.origin.x - dx).abs()
                        < 0.001
                );
                assert!(
                    (source_bounds.origin.y - self.baseline_source_bounds.origin.y - dy).abs()
                        < 0.001
                );
                TraceGeometry::NodeMoved
            }
        }
    }

    fn assert_route_matches(
        &self,
        geometry: &GeometrySnapshot,
        edge: EdgeId,
        from: PortId,
        to: PortId,
    ) {
        let route = geometry
            .route(edge)
            .unwrap_or_else(|| panic!("canonical geometry is missing route {edge:?}"));
        let source = geometry.port(from).expect("route source geometry");
        let target = geometry.port(to).expect("route target geometry");
        assert_eq!(route.facts().endpoints.source.point, source.route_anchor());
        assert_eq!(route.facts().endpoints.target.point, target.route_anchor());
    }

    fn invalid_anchor(&self) -> ViewPoint {
        match self.scenario {
            TraceScenarioId::ReconnectSource(_) | TraceScenarioId::ReconnectSourcePrefixWait => {
                self.anchors.valid
            }
            TraceScenarioId::NewConnection(_)
            | TraceScenarioId::ReconnectTarget(_)
            | TraceScenarioId::CreateFromWire(_)
            | TraceScenarioId::NodeDrag(_)
            | TraceScenarioId::NewConnectionPrefixWait
            | TraceScenarioId::ReconnectTargetPrefixWait
            | TraceScenarioId::CreateFromWirePrefixWait
            | TraceScenarioId::NodeDragPrefixWait
            | TraceScenarioId::NodePressBelowThreshold
            | TraceScenarioId::NativeControl
            | TraceScenarioId::EdgePick
            | TraceScenarioId::CaptureLost
            | TraceScenarioId::OwnerReplaced
            | TraceScenarioId::Escape
            | TraceScenarioId::OutsideClick
            | TraceScenarioId::SourceRetired
            | TraceScenarioId::HostClosed => self.anchors.invalid,
        }
    }

    fn valid_anchor(&self) -> ViewPoint {
        match self.scenario {
            TraceScenarioId::ReconnectSource(_) | TraceScenarioId::ReconnectSourcePrefixWait => {
                self.anchors.source
            }
            TraceScenarioId::ReconnectTarget(_) | TraceScenarioId::ReconnectTargetPrefixWait => {
                self.anchors.reconnect_alternate
            }
            TraceScenarioId::NewConnection(_)
            | TraceScenarioId::CreateFromWire(_)
            | TraceScenarioId::NodeDrag(_)
            | TraceScenarioId::NewConnectionPrefixWait
            | TraceScenarioId::CreateFromWirePrefixWait
            | TraceScenarioId::NodeDragPrefixWait
            | TraceScenarioId::NodePressBelowThreshold
            | TraceScenarioId::NativeControl
            | TraceScenarioId::EdgePick
            | TraceScenarioId::CaptureLost
            | TraceScenarioId::OwnerReplaced
            | TraceScenarioId::Escape
            | TraceScenarioId::OutsideClick
            | TraceScenarioId::SourceRetired
            | TraceScenarioId::HostClosed => self.anchors.valid,
        }
    }
}

fn drive_query_pages(
    session: &mut CanvasInteractionSession,
    mut update: CanvasInteractionUpdate,
) -> Vec<CanvasInteractionEffect> {
    let mut emitted = Vec::new();
    loop {
        let dispatch = update.into_dispatch();
        emitted.extend(dispatch.effects);
        if !dispatch.query_work_pending {
            assert!(!session.query_work_pending());
            return emitted;
        }
        let (next, receipt) = advance_successful_query_page(session);
        assert_eq!(
            receipt.page_receipts().len(),
            1,
            "each successful page grant returns typed runtime proof"
        );
        update = next;
    }
}

fn advance_query_page(
    session: &mut CanvasInteractionSession,
) -> (
    CanvasInteractionUpdate,
    CanvasInteractionQueryAdvanceReceipt,
) {
    assert!(
        session.query_work_pending(),
        "a native page grant requires runtime-owned query work"
    );
    let (update, needs_tick, receipt, terminal_error) = session.advance_query_work(1).into_parts();
    assert_eq!(receipt.pages_granted(), 1);
    assert_eq!(receipt.pages_attempted(), 1);
    assert!(terminal_error.is_none());
    assert_eq!(needs_tick, update.query_work_pending());
    assert_eq!(needs_tick, session.query_work_pending());
    (update, receipt)
}

fn advance_successful_query_page(
    session: &mut CanvasInteractionSession,
) -> (
    CanvasInteractionUpdate,
    CanvasInteractionQueryAdvanceReceipt,
) {
    let (update, receipt) = advance_query_page(session);
    assert_eq!(
        receipt.page_receipts().len(),
        1,
        "a successful view page returns one typed runtime proof"
    );
    (update, receipt)
}

fn scenario_settlement(id: TraceScenarioId) -> Option<TraceReceipt> {
    match id {
        TraceScenarioId::NewConnection(settlement)
        | TraceScenarioId::ReconnectSource(settlement)
        | TraceScenarioId::ReconnectTarget(settlement)
        | TraceScenarioId::CreateFromWire(settlement)
        | TraceScenarioId::NodeDrag(settlement) => Some(settlement),
        TraceScenarioId::NewConnectionPrefixWait
        | TraceScenarioId::ReconnectSourcePrefixWait
        | TraceScenarioId::ReconnectTargetPrefixWait
        | TraceScenarioId::CreateFromWirePrefixWait
        | TraceScenarioId::NodeDragPrefixWait => Some(TraceReceipt::Applied),
        TraceScenarioId::NodePressBelowThreshold
        | TraceScenarioId::NativeControl
        | TraceScenarioId::EdgePick
        | TraceScenarioId::CaptureLost
        | TraceScenarioId::OwnerReplaced
        | TraceScenarioId::Escape
        | TraceScenarioId::OutsideClick
        | TraceScenarioId::SourceRetired
        | TraceScenarioId::HostClosed => None,
    }
}

fn trace_terminal(reason: InteractionTerminalReason) -> TraceTerminal {
    match reason {
        InteractionTerminalReason::Applied => TraceTerminal::Applied,
        InteractionTerminalReason::Noop => TraceTerminal::Noop,
        InteractionTerminalReason::Rejected => TraceTerminal::Rejected,
        InteractionTerminalReason::Conflict => TraceTerminal::Conflict,
        InteractionTerminalReason::TransportFailure => TraceTerminal::TransportFailure,
        InteractionTerminalReason::Cancelled => TraceTerminal::Cancelled,
        InteractionTerminalReason::OwnerRetired => TraceTerminal::OwnerRetired,
        InteractionTerminalReason::ViewClosed => TraceTerminal::HostClosed,
        reason @ (InteractionTerminalReason::ExecutionFailed
        | InteractionTerminalReason::Uncertain
        | InteractionTerminalReason::BudgetExhausted
        | InteractionTerminalReason::GeometryIndeterminate
        | InteractionTerminalReason::IdentityExhausted) => {
            panic!("canonical trace reached unsupported terminal reason {reason:?}")
        }
    }
}

fn trace_intent(binding: ConnectionGestureBinding) -> TraceIntent {
    match (binding.reconnect_edge(), binding.fixed_endpoint()) {
        (None, _) => TraceIntent::Connect,
        (Some(_), EdgeEndpoint::To) => TraceIntent::ReconnectSource,
        (Some(_), EdgeEndpoint::From) => TraceIntent::ReconnectTarget,
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

fn pointer_sample(view: &ViewSession, point: ViewPoint) -> ViewportPointerSample {
    view.viewport_pointer_sample(point).unwrap()
}

fn canvas_anchor(view: &ViewSession, point: ViewPoint) -> CanvasPoint {
    view.viewport_transform()
        .unwrap()
        .canvas_point_at_screen(point)
        .expect("finite canonical anchor")
}

fn node_lifetime(view: &ViewSession, node: NodeId) -> jellyflow_runtime::graph::NodeLifetimeKey {
    view.node_lifetime_key(node)
        .expect("node identity query")
        .expect("test node is current")
}

fn edge_instance(view: &ViewSession, edge: EdgeId) -> jellyflow_runtime::graph::EdgeInstanceKey {
    view.edge_instance_key(edge)
        .expect("edge identity query")
        .expect("test edge is current")
}

fn port_instance(view: &ViewSession, port: PortId) -> jellyflow_runtime::graph::PortInstanceKey {
    view.port_instance_key(port)
        .expect("port identity query")
        .expect("test port is current")
}

fn set_selection_by_id(
    view: &ViewSession,
    nodes: impl IntoIterator<Item = NodeId>,
    edges: impl IntoIterator<Item = EdgeId>,
) -> Result<(), jellyflow_runtime::view::ViewSessionError> {
    view.set_selection(
        nodes
            .into_iter()
            .map(|node| node_lifetime(view, node))
            .collect(),
        edges
            .into_iter()
            .map(|edge| edge_instance(view, edge))
            .collect(),
        Vec::new(),
    )
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

fn trace_creation(ids: TraceGraphIds) -> NodeCreation {
    let mut catalog = NodeCatalog::new();
    catalog
        .register(
            NodeKindSpec::builder("trace.created", "Created", "trace.created")
                .default_size(CanvasSize {
                    width: 60.0,
                    height: 60.0,
                })
                .port(NodePortSpec::new(
                    "port",
                    PortDirection::In,
                    PortKind::Data,
                    PortCapacity::Multi,
                ))
                .build(),
        )
        .expect("trace node kind registers");
    catalog
        .instantiate_node_with_ids(
            &NodeKindKey::new("trace.created"),
            ids.created_node,
            CanvasPoint::default(),
            [ids.created_port],
        )
        .expect("trace node creation uses exact IDs")
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
