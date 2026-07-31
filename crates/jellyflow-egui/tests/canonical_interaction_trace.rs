use egui::{
    Color32, Event, Id, Key, Modifiers, PointerButton, Pos2, Rect, Sense, Stroke, Ui, Vec2,
};
use egui_kittest::Harness;
use jellyflow_core::{
    CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeId, EdgeKind, GraphBuilder, GraphId, Node,
    NodeGraphConnectionMode, NodeId, NodeKindKey, Port, PortCapacity, PortDirection, PortId,
    PortKey, PortKind,
};
use jellyflow_egui::{
    EguiCanvasInteraction, EguiInteractionError, EguiInteractionOutput, EguiNativeFrameHost,
};
use jellyflow_runtime::adapter::conformance::{
    ConformanceAction as TraceInput, ConformanceAdapterTransportEvidence as TraceTransportEvidence,
    ConformanceCancelReason as TraceCancelReason, ConformanceCandidate as TraceCandidate,
    ConformanceCommandCompletion as TraceCompletion,
    ConformanceCommandEvidence as TraceCommandEvidence,
    ConformanceConnectionSettlement as TraceConnectionSettlement, ConformanceEffect as TraceEffect,
    ConformanceGeometry as TraceGeometry, ConformanceGraph as TraceGraph,
    ConformanceIntent as TraceIntent, ConformanceOwnership as TraceOwnership,
    ConformancePhase as TracePhase, ConformanceScenario as CanonicalInteractionTrace,
    ConformanceScenarioId as TraceScenarioId, ConformanceSelection as TraceSelection,
    ConformanceTarget as TraceTarget, ConformanceTerminal as TraceTerminal,
    ConformanceTicket as TraceTicket, ConformanceViewStatus as TraceViewStatus,
    NormalizedObservation, canonical_scenarios, compare_observations,
    new_connection_probe_prefix_scenario,
};
use jellyflow_runtime::commands::{GraphCommand, IncidentEdgePolicy, TopologyEdit};
use jellyflow_runtime::geometry::{HandlePosition, ViewPoint};
use jellyflow_runtime::graph::{
    EdgeEndpoint, GraphRuntime, GraphRuntimeMiddleware, GraphRuntimeMiddlewareSnapshot,
};
use jellyflow_runtime::interaction::{
    CanvasInteractionConfig, CanvasInteractionEffect, CanvasInteractionError,
    CanvasInteractionPhase, CanvasPressFallback, CanvasPressTarget, ConnectionGestureBinding,
    ConnectionProbe, ConnectionSearchRequest, ConnectionSearchStatus,
    ConnectionSettlementPresentation, InteractionGraphSubmission, InteractionTerminalReason,
    InteractionTicketId, SelectionMode,
};
use jellyflow_runtime::profile::ApplyPipelineError;
use jellyflow_runtime::schema::{NodeCatalog, NodeKindSpec, NodePortSpec};
use jellyflow_runtime::view::{
    FrameRequest, GeometrySnapshot, RendererStamp, ViewCloseReason, ViewSession, ViewSessionOptions,
};

#[test]
fn egui_native_events_replay_every_canonical_interaction_trace() {
    for trace in canonical_scenarios() {
        EguiTraceRunner::for_scenario(trace.id()).replay(&trace);
    }
}

#[test]
fn egui_many_page_grant_matches_the_canonical_connection_probe_prefix() {
    let trace = connection_probe_prefix_trace();
    let mut runner = EguiTraceRunner::for_paged_probe(trace.id());

    runner.replay(&trace);

    assert!(
        runner.max_query_pages_attempted >= 2,
        "the overlapping targets must require at least two runtime query pages",
    );
    assert!(
        runner.max_query_pages_attempted <= PAGED_QUERY_PAGES_PER_FRAME,
        "one egui frame must not exceed its explicit page grant",
    );
}

const PAGED_QUERY_PAGES_PER_FRAME: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TraceQuerySchedule {
    Default,
    ManyPagesPerFrame,
}

impl TraceQuerySchedule {
    fn interaction_config(self) -> CanvasInteractionConfig {
        match self {
            Self::Default => CanvasInteractionConfig::default(),
            Self::ManyPagesPerFrame => CanvasInteractionConfig {
                query_work_budget: 1,
                ..CanvasInteractionConfig::default()
            },
        }
    }

    const fn query_pages_per_frame(self) -> usize {
        match self {
            Self::Default => 1,
            Self::ManyPagesPerFrame => PAGED_QUERY_PAGES_PER_FRAME,
        }
    }

    const fn overlaps_valid_targets(self) -> bool {
        matches!(self, Self::ManyPagesPerFrame)
    }
}

fn connection_probe_prefix_trace() -> CanonicalInteractionTrace {
    new_connection_probe_prefix_scenario()
}

struct RejectCanonicalCommand;

impl GraphRuntimeMiddleware for RejectCanonicalCommand {
    fn before_command(
        &mut self,
        _snapshot: GraphRuntimeMiddlewareSnapshot,
        _command: &GraphCommand,
    ) -> Result<(), ApplyPipelineError> {
        Err(ApplyPipelineError::Rejected {
            message: "canonical egui command rejected".to_owned(),
            diagnostics: Vec::new(),
        })
    }
}

const fn completed(completion: TraceCompletion) -> TraceCommandEvidence {
    TraceCommandEvidence::Completion(completion)
}

#[test]
fn stolen_egui_drag_ownership_cancels_the_semantic_session() {
    let mut runner = EguiTraceRunner::new();
    runner.press(TraceTarget::SourcePort);
    runner.move_pointer(runner.state().anchors.empty);
    assert_eq!(runner.observed_phase(), TracePhase::ConnectionDrag);

    runner.state_mut().steal_capture = true;
    runner.harness.step();
    let output = runner.take_output();

    assert!(output.released_native_ownership());
    assert_eq!(runner.observed_phase(), TracePhase::Idle);
    assert_eq!(
        runner.state().interaction.presentation().terminal(),
        Some(InteractionTerminalReason::Cancelled)
    );
}

#[test]
fn egui_interaction_selection_remains_local_to_its_view() {
    let mut runner = EguiTraceRunner::new();
    let untouched = runner
        .state()
        .runtime
        .attach_view(ViewSessionOptions::default())
        .expect("second view");

    assert_eq!(runner.press(TraceTarget::NodeDragHandle), TraceEffect::None);
    assert_eq!(runner.release(), TraceEffect::None);

    assert_eq!(
        runner.state().view().selection().unwrap().nodes(),
        [runner.state().ids.source_node]
    );
    assert!(untouched.selection().unwrap().nodes().is_empty());
    assert!(untouched.geometry_snapshot().unwrap().nodes().is_empty());
}

#[test]
fn release_at_a_new_position_activates_and_commits_an_armed_drag() {
    let mut runner = EguiTraceRunner::new();

    assert_eq!(runner.press(TraceTarget::NodeDragHandle), TraceEffect::None);
    assert_eq!(runner.observed_phase(), TracePhase::Armed);

    let effect = runner.release_at(runner.state().anchors.empty);

    assert_eq!(effect, TraceEffect::SubmitNodeMove);
    assert_eq!(runner.observed_phase(), TracePhase::Committing);
}

#[test]
fn egui_search_retry_rejects_a_stale_cancel_request() {
    let mut runner = EguiTraceRunner::new();

    assert_eq!(runner.press(TraceTarget::SourcePort), TraceEffect::None);
    assert_eq!(
        runner.move_to_candidate(TraceCandidate::None),
        TraceEffect::ProbeConnection
    );
    assert_eq!(runner.release(), TraceEffect::OpenSearch);

    let first_request = runner
        .search_request
        .clone()
        .expect("release emits an exact search request");
    let ctx = runner.harness.ctx.clone();
    runner
        .state_mut()
        .interaction
        .search_failed(&ctx, &first_request)
        .expect("the current search request can report failure");
    assert_eq!(
        runner
            .state()
            .interaction
            .presentation()
            .connection()
            .and_then(|connection| connection.search()),
        Some(ConnectionSearchStatus::Error)
    );

    let retry = runner
        .state_mut()
        .interaction
        .retry_search(&ctx, &first_request)
        .expect("the current request starts a successor attempt");
    assert_eq!(
        runner.capture_semantic_effect(retry, false),
        TraceEffect::OpenSearch
    );
    let successor_request = runner
        .search_request
        .clone()
        .expect("retry emits its successor request");
    assert_ne!(successor_request, first_request);

    assert!(matches!(
        runner
            .state_mut()
            .interaction
            .cancel_search(&ctx, &first_request),
        Err(EguiInteractionError::Interaction(
            CanvasInteractionError::ConnectionSearchStaleGeneration
        ))
    ));
    runner
        .state_mut()
        .interaction
        .cancel_search(&ctx, &successor_request)
        .expect("only the current request can cancel its search");
    assert_eq!(
        runner.state().interaction.presentation().phase(),
        CanvasInteractionPhase::Idle
    );
    assert_eq!(
        runner.state().interaction.presentation().terminal(),
        Some(InteractionTerminalReason::Cancelled)
    );
}

#[test]
fn egui_uncertain_settlement_retries_with_a_fresh_exact_capability() {
    let mut runner = EguiTraceRunner::new();

    assert_eq!(runner.press(TraceTarget::SourcePort), TraceEffect::None);
    assert_eq!(
        runner.move_to_candidate(TraceCandidate::Valid),
        TraceEffect::ProbeConnection
    );
    assert_eq!(runner.release(), TraceEffect::SubmitConnection);

    let first_ticket = runner.ticket.expect("connection submission owns a ticket");
    let first_submission = runner
        .submission
        .take()
        .expect("connection release emits a graph submission");
    let first_command = runner
        .state_mut()
        .interaction
        .bind_submission(first_submission)
        .expect("the adapter binds the first submission")
        .expect("the first attempt emits a command");
    drop(first_command);

    let ctx = runner.harness.ctx.clone();
    runner
        .state_mut()
        .interaction
        .submission_outcome_unknown(&ctx, first_ticket)
        .expect("loss after transport admission becomes uncertain");
    runner.ticket = None;
    let first_capability = match runner
        .state()
        .interaction
        .presentation()
        .connection_settlement()
    {
        Some(ConnectionSettlementPresentation::Uncertain {
            capability,
            can_retry: false,
        }) => capability,
        settlement => panic!("expected non-retryable uncertainty, got {settlement:?}"),
    };
    assert!(matches!(
        runner
            .state_mut()
            .interaction
            .retry_connection_settlement(&ctx, first_capability),
        Err(EguiInteractionError::Interaction(
            CanvasInteractionError::ConnectionSettlementRetryNotReady
        ))
    ));

    runner.execute_external_revision("authorize uncertain retry");
    runner
        .state_mut()
        .interaction
        .reconcile_authoritative_state(&ctx)
        .expect("a later authoritative absence authorizes retry");
    assert_eq!(
        runner
            .state()
            .interaction
            .presentation()
            .connection_settlement(),
        Some(ConnectionSettlementPresentation::Uncertain {
            capability: first_capability,
            can_retry: true,
        })
    );

    let retry = runner
        .state_mut()
        .interaction
        .retry_connection_settlement(&ctx, first_capability)
        .expect("the exact capability issues a fresh attempt");
    assert_eq!(
        runner.capture_semantic_effect(retry, false),
        TraceEffect::SubmitConnection
    );
    let retry_ticket = runner.ticket.expect("retry owns a fresh ticket");
    assert_ne!(retry_ticket, first_ticket);
    let retry_submission = runner
        .submission
        .take()
        .expect("retry emits a fresh graph submission");
    let retry_command = runner
        .state_mut()
        .interaction
        .bind_submission(retry_submission)
        .expect("the adapter binds the retry submission")
        .expect("the retry emits a command");
    drop(retry_command);
    runner
        .state_mut()
        .interaction
        .submission_outcome_unknown(&ctx, retry_ticket)
        .expect("the retry owns an independent uncertain outcome");
    runner.ticket = None;
    let successor_capability = match runner
        .state()
        .interaction
        .presentation()
        .connection_settlement()
    {
        Some(ConnectionSettlementPresentation::Uncertain { capability, .. }) => capability,
        settlement => panic!("expected successor uncertainty, got {settlement:?}"),
    };
    assert_ne!(successor_capability, first_capability);

    for stale_resolution in [
        runner
            .state_mut()
            .interaction
            .retry_connection_settlement(&ctx, first_capability),
        runner
            .state_mut()
            .interaction
            .cancel_connection_settlement(&ctx, first_capability),
    ] {
        assert!(matches!(
            stale_resolution,
            Err(EguiInteractionError::Interaction(
                CanvasInteractionError::ConnectionSettlementStaleCapability
            ))
        ));
    }
    runner
        .state_mut()
        .interaction
        .cancel_connection_settlement(&ctx, successor_capability)
        .expect("only the successor capability can cancel its projection");
    assert_eq!(
        runner.state().interaction.presentation().phase(),
        CanvasInteractionPhase::Idle
    );
    assert_eq!(
        runner.state().interaction.presentation().terminal(),
        Some(InteractionTerminalReason::Cancelled)
    );
}

#[test]
fn closing_only_the_semantic_interaction_cannot_satisfy_host_closed_conformance() {
    let trace = canonical_scenarios()
        .into_iter()
        .find(|trace| trace.id() == TraceScenarioId::HostClosed)
        .expect("canonical host-close scenario");
    let mut runner = EguiTraceRunner::for_scenario(trace.id());
    let mut observations = Vec::new();
    for step in &trace.steps()[..2] {
        let effect = runner.apply(step.action());
        observations.push(runner.observation(effect));
    }

    let ctx = runner.harness.ctx.clone();
    let output = runner.state_mut().interaction.close(&ctx);
    assert!(output.released_native_ownership());
    let observation = runner.observation(TraceEffect::ReleaseNativeOwnership);
    assert_eq!(observation.geometry, TraceGeometry::Baseline);
    assert_eq!(observation.view_status, TraceViewStatus::Open);
    observations.push(observation);
    assert!(compare_observations(&trace, &observations).is_err());

    runner
        .state()
        .host
        .close()
        .expect("close the real egui host after the negative assertion");
}

struct EguiTraceRunner {
    harness: Harness<'static, EguiTraceState>,
    pointer: Pos2,
    ticket: Option<InteractionTicketId>,
    submission: Option<InteractionGraphSubmission>,
    search_request: Option<ConnectionSearchRequest>,
    baseline_revision: u64,
    excluded_revision_delta: u64,
    prefix_held: bool,
    adapter_capture: Option<Id>,
    successor_capture: Option<Id>,
    scenario: TraceScenarioId,
    baseline_source_position: CanvasPoint,
    baseline_source_bounds: CanvasRect,
    query_schedule: TraceQuerySchedule,
    max_query_pages_attempted: usize,
}

impl EguiTraceRunner {
    fn new() -> Self {
        Self::build(None, None, TraceQuerySchedule::Default)
    }

    fn for_scenario(id: TraceScenarioId) -> Self {
        Self::build(id.command_evidence(), Some(id), TraceQuerySchedule::Default)
    }

    fn for_paged_probe(id: TraceScenarioId) -> Self {
        Self::build(
            id.command_evidence(),
            Some(id),
            TraceQuerySchedule::ManyPagesPerFrame,
        )
    }

    fn build(
        evidence: Option<TraceCommandEvidence>,
        scenario: Option<TraceScenarioId>,
        query_schedule: TraceQuerySchedule,
    ) -> Self {
        let state = EguiTraceState::for_evidence(evidence, query_schedule);
        if matches!(
            scenario,
            Some(
                TraceScenarioId::ReconnectSource(_)
                    | TraceScenarioId::ReconnectTarget(_)
                    | TraceScenarioId::ReconnectTargetNoop
                    | TraceScenarioId::ReconnectSourcePrefixWait
                    | TraceScenarioId::ReconnectTargetPrefixWait
            )
        ) {
            let edge = state
                .view()
                .edge_instance_key(state.ids.reconnect_edge)
                .expect("canonical edge identity query")
                .expect("canonical edge is current");
            state
                .view()
                .set_selection(Vec::new(), vec![edge], Vec::new())
                .expect("seed canonical egui reconnect selection");
        }
        let baseline_revision = state
            .runtime
            .snapshot()
            .expect("canonical egui baseline")
            .revision();
        let baseline_source_position = state
            .runtime
            .snapshot()
            .expect("canonical egui source baseline")
            .graph()
            .node(&state.ids.source_node)
            .expect("canonical egui source node")
            .pos;
        let pointer = state.anchors.source_pos();
        let harness = Harness::builder()
            .with_size(Vec2::new(900.0, 500.0))
            .build_ui_state(render_trace, state);
        let baseline_source_bounds = harness
            .state()
            .view()
            .geometry_snapshot()
            .expect("canonical egui geometry baseline")
            .node(harness.state().ids.source_node)
            .expect("canonical egui source geometry")
            .bounds();
        Self {
            harness,
            pointer,
            ticket: None,
            submission: None,
            search_request: None,
            baseline_revision,
            excluded_revision_delta: 0,
            prefix_held: false,
            adapter_capture: None,
            successor_capture: None,
            scenario: scenario.unwrap_or(TraceScenarioId::NewConnection(completed(
                TraceCompletion::Applied,
            ))),
            baseline_source_position,
            baseline_source_bounds,
            query_schedule,
            max_query_pages_attempted: 0,
        }
    }

    fn replay(&mut self, trace: &CanonicalInteractionTrace) {
        let mut observations = Vec::with_capacity(trace.steps().len());
        for step in trace.steps() {
            let effect = self.apply(step.action());
            observations.push(self.observation(effect));
        }
        compare_observations(trace, &observations)
            .unwrap_or_else(|error| panic!("{}: {error}", trace.display_name()));
    }

    fn apply(&mut self, input: TraceInput) -> TraceEffect {
        match input {
            TraceInput::Press(target) => self.press(target),
            TraceInput::CrossDragThreshold => {
                let output = self.move_pointer(self.state().anchors.empty);
                self.capture_semantic_effect(output, true)
            }
            TraceInput::MoveToCandidate(candidate) => self.move_to_candidate(candidate),
            TraceInput::Release => self.release(),
            TraceInput::SelectSearchCandidate => self.select_search_result(),
            TraceInput::QueuePrefix => {
                assert!(!self.prefix_held, "canonical prefix is already held");
                self.prefix_held = true;
                TraceEffect::None
            }
            TraceInput::DrainPrefix => self.drain_prefix(),
            TraceInput::AdvanceFrame => {
                self.harness.step();
                let output = self.take_output();
                self.capture_semantic_effect(output, false)
            }
            TraceInput::PublishAuthoritativeFrame => self.publish_authoritative_frame(),
            TraceInput::AdvanceAuthoritativeAbsence => self.advance_authoritative_absence(),
            TraceInput::RetryConnectionSettlement => self.retry_connection_settlement(),
            TraceInput::CancelConnectionSettlement => self.cancel_connection_settlement(),
            TraceInput::CompleteCommand(completion) => self.complete_command(completion),
            TraceInput::ReportTransportEvidence(evidence) => {
                self.report_transport_evidence(evidence)
            }
            TraceInput::LoseCapture => self.lose_capture(),
            TraceInput::ReplaceOwner => self.replace_owner(),
            TraceInput::ReleasePreviousOwner => self.release_previous_owner(),
            TraceInput::Cancel(reason) => self.cancel(reason),
            TraceInput::RetireSource => {
                let version = self.state().runtime.snapshot().unwrap().version();
                self.state()
                    .runtime
                    .execute(GraphCommand::topology(
                        version,
                        TopologyEdit::new()
                            .remove_node(self.state().ids.source_node, IncidentEdgePolicy::Reject),
                    ))
                    .into_result()
                    .expect("canonical egui source retirement executes")
                    .applied()
                    .expect("canonical egui source retirement applies");
                self.excluded_revision_delta = self.excluded_revision_delta.saturating_add(1);
                self.state_mut().response_visible = false;
                self.harness.step();
                let output = self.take_output();
                assert!(output.released_native_ownership());
                TraceEffect::ReleaseNativeOwnership
            }
            TraceInput::CloseHost => {
                let ctx = self.harness.ctx.clone();
                let state = self.state_mut();
                let output = state.interaction.close(&ctx);
                assert!(output.released_native_ownership());
                let outcome = state.host.close().expect("close canonical egui host");
                assert_eq!(outcome.reason(), ViewCloseReason::HostClosed);
                assert_eq!(
                    state.view().close_reason(),
                    Some(ViewCloseReason::HostClosed)
                );
                TraceEffect::ReleaseNativeOwnership
            }
        }
    }

    fn press(&mut self, target: TraceTarget) -> TraceEffect {
        let response = match target {
            TraceTarget::NativeControl => TraceResponse::NativeControl,
            TraceTarget::NodeDragHandle => TraceResponse::NodeDrag,
            TraceTarget::SourcePort => TraceResponse::Connection,
            TraceTarget::Edge => TraceResponse::Edge,
            TraceTarget::ReconnectSource => {
                TraceResponse::Reconnect(TraceReconnectEndpoint::Source)
            }
            TraceTarget::ReconnectTarget => {
                TraceResponse::Reconnect(TraceReconnectEndpoint::Target)
            }
        };
        self.state_mut().response = Some(response);
        self.state_mut().response_visible = true;
        self.harness.step();
        self.pointer = self.state().response_center(response);
        self.harness.hover_at(self.pointer);
        self.harness.step();
        self.harness.drag_at(self.pointer);
        self.harness.step();
        let output = self.take_output();
        let captured = self.harness.ctx.dragged_id();
        if target == TraceTarget::NativeControl {
            assert!(captured.is_none());
        } else {
            self.adapter_capture = Some(captured.expect("egui adapter owns native drag"));
        }
        self.capture_semantic_effect(output, false)
    }

    fn move_pointer(&mut self, point: ViewPoint) -> EguiInteractionOutput {
        self.pointer = Pos2::new(point.x, point.y);
        self.harness.hover_at(self.pointer);
        self.harness.step();
        self.take_output()
    }

    fn release(&mut self) -> TraceEffect {
        self.harness.event(Event::PointerButton {
            pos: self.pointer,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        });
        self.harness.step();
        let output = self.take_output();
        assert!(output.released_native_ownership());
        self.capture_semantic_effect(output, false)
    }

    fn release_at(&mut self, point: ViewPoint) -> TraceEffect {
        self.pointer = Pos2::new(point.x, point.y);
        self.harness.event(Event::PointerButton {
            pos: self.pointer,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        });
        self.harness.step();
        let output = self.take_output();
        assert!(output.released_native_ownership());
        self.capture_semantic_effect(output, false)
    }

    fn select_search_result(&mut self) -> TraceEffect {
        let ctx = self.harness.ctx.clone();
        let request = self
            .search_request
            .take()
            .expect("search effect supplies an exact request");
        let state = self.harness.state_mut();
        state
            .interaction
            .search_ready(&ctx, &request, 1)
            .expect("search result count applies");
        let selection = state
            .interaction
            .create_search_selection(&request, state.ids.created_port, trace_creation(state.ids))
            .expect("create-from-wire view remains available")
            .expect("create-from-wire selection carries topology");
        let output = state
            .interaction
            .select_search_result(&ctx, selection)
            .expect("search selection applies");
        self.capture_semantic_effect(output, false)
    }

    fn complete_command(&mut self, completion: TraceCompletion) -> TraceEffect {
        assert!(
            !self.prefix_held,
            "command transport cannot bypass a held prefix"
        );
        let ticket = self.ticket.take().expect("trace submitted one command");
        let submission = self
            .submission
            .take()
            .expect("trace captured one graph submission");
        let ctx = self.harness.ctx.clone();
        let runtime_completion = self.dispatch_submission(completion, submission);
        let output = self
            .harness
            .state_mut()
            .interaction
            .complete_submission(&ctx, ticket, runtime_completion)
            .expect("command completion settles the exact submission");
        assert!(output.effects().is_empty());
        if matches!(completion, TraceCompletion::Applied | TraceCompletion::Noop) {
            self.publish_terminal_connection_route();
        }
        match completion {
            TraceCompletion::Applied | TraceCompletion::Noop => TraceEffect::None,
            TraceCompletion::Rejected | TraceCompletion::Conflict | TraceCompletion::Failed => {
                TraceEffect::RestorePresentation
            }
        }
    }

    fn report_transport_evidence(&mut self, evidence: TraceTransportEvidence) -> TraceEffect {
        assert!(
            !self.prefix_held,
            "command transport cannot bypass a held prefix"
        );
        let ticket = self.ticket.take().expect("trace submitted one command");
        let submission = self
            .submission
            .take()
            .expect("trace captured one graph submission");
        let ctx = self.harness.ctx.clone();
        match evidence {
            TraceTransportEvidence::NotDispatched => {
                drop(submission);
                let output = self
                    .harness
                    .state_mut()
                    .interaction
                    .submission_not_dispatched(&ctx, ticket)
                    .expect("undispatched submission is reported");
                assert!(output.effects().is_empty());
                TraceEffect::RestorePresentation
            }
            TraceTransportEvidence::OutcomeUnknown => {
                let command = self
                    .harness
                    .state_mut()
                    .interaction
                    .bind_submission(submission)
                    .expect("outcome-unknown transport binds the admitted command")
                    .expect("outcome-unknown transport follows command admission");
                drop(command);
                let output = self
                    .harness
                    .state_mut()
                    .interaction
                    .submission_outcome_unknown(&ctx, ticket)
                    .expect("post-admission transport uncertainty is reported");
                assert!(output.effects().is_empty());
                TraceEffect::None
            }
        }
    }

    fn publish_terminal_connection_route(&mut self) {
        if matches!(
            self.scenario,
            TraceScenarioId::CreateFromWire(TraceCommandEvidence::Completion(
                TraceCompletion::Applied
            )) | TraceScenarioId::CreateFromWirePrefixWait
        ) || !self
            .state()
            .interaction
            .presentation()
            .connection_settlement()
            .is_some()
        {
            return;
        }
        assert_eq!(self.publish_authoritative_frame(), TraceEffect::None);
    }

    fn publish_authoritative_frame(&mut self) -> TraceEffect {
        self.harness.step();
        let output = self.take_output();
        assert!(output.effects().is_empty());
        TraceEffect::None
    }

    fn dispatch_submission(
        &mut self,
        requested: TraceCompletion,
        submission: InteractionGraphSubmission,
    ) -> jellyflow_runtime::commands::CommandExecutionCompletion {
        let emitted = self
            .state_mut()
            .interaction
            .bind_submission(submission)
            .expect("egui transport binds the emitted mutation")
            .expect("canonical emitted mutation is not a no-op");
        match requested {
            TraceCompletion::Conflict => self.execute_external_revision("conflict"),
            TraceCompletion::Applied | TraceCompletion::Noop | TraceCompletion::Rejected => {}
            TraceCompletion::Failed => {
                let foreign = GraphRuntime::builder(Default::default())
                    .build(
                        GraphBuilder::new(GraphId::from_u128(u128::MAX))
                            .build()
                            .expect("foreign failure graph is valid"),
                    )
                    .expect("foreign failure runtime builds");
                return foreign.execute(emitted);
            }
        }
        self.state().runtime.execute(emitted)
    }

    fn advance_authoritative_absence(&mut self) -> TraceEffect {
        self.execute_external_revision("uncertain-absence");
        let ctx = self.harness.ctx.clone();
        let output = self
            .harness
            .state_mut()
            .interaction
            .reconcile_authoritative_state(&ctx)
            .expect("later authoritative absence is reconciled");
        assert!(output.effects().is_empty());
        TraceEffect::None
    }

    fn retry_connection_settlement(&mut self) -> TraceEffect {
        let capability = match self
            .state()
            .interaction
            .presentation()
            .connection_settlement()
        {
            Some(ConnectionSettlementPresentation::Uncertain {
                capability,
                can_retry: true,
            }) => capability,
            settlement => panic!("expected retryable uncertainty, got {settlement:?}"),
        };
        let ctx = self.harness.ctx.clone();
        let output = self
            .harness
            .state_mut()
            .interaction
            .retry_connection_settlement(&ctx, capability)
            .expect("exact uncertainty capability starts a fresh attempt");
        self.capture_semantic_effect(output, false)
    }

    fn cancel_connection_settlement(&mut self) -> TraceEffect {
        let capability = match self
            .state()
            .interaction
            .presentation()
            .connection_settlement()
        {
            Some(ConnectionSettlementPresentation::Uncertain { capability, .. })
            | Some(ConnectionSettlementPresentation::Retryable { capability, .. }) => capability,
            settlement => panic!("expected cancellable settlement, got {settlement:?}"),
        };
        let ctx = self.harness.ctx.clone();
        let output = self
            .harness
            .state_mut()
            .interaction
            .cancel_connection_settlement(&ctx, capability)
            .expect("current settlement capability cancels its projection");
        assert!(output.effects().is_empty());
        TraceEffect::RestorePresentation
    }

    fn drain_prefix(&mut self) -> TraceEffect {
        assert!(
            self.prefix_held,
            "canonical prefix must be queued before draining"
        );
        self.execute_external_revision("prefix");
        self.prefix_held = false;
        if let Some(submission) = self.submission.take() {
            let ticket = submission.ticket();
            assert_eq!(self.ticket, Some(ticket));
            let expected = if self
                .state()
                .interaction
                .presentation()
                .connection()
                .is_some()
            {
                TraceEffect::SubmitConnection
            } else {
                TraceEffect::SubmitNodeMove
            };
            let ctx = self.harness.ctx.clone();
            let state = self.harness.state_mut();
            let output = state
                .interaction
                .resume_submission_after_prefix(&ctx, submission)
                .expect("prefix resumes through the headless session");
            assert_eq!(self.capture_semantic_effect(output, false), expected);
        }
        TraceEffect::None
    }

    fn execute_external_revision(&mut self, reason: &str) {
        let snapshot = self
            .state()
            .runtime
            .snapshot()
            .expect("external egui transport snapshot");
        let command = GraphCommand::set_node_data(
            snapshot.version(),
            self.state().ids.invalid_node,
            serde_json::json!({ "external": reason }),
        );
        self.state()
            .runtime
            .execute(command)
            .into_result()
            .expect("external egui transport command executes")
            .applied()
            .expect("external egui transport command applies");
        self.excluded_revision_delta = self.excluded_revision_delta.saturating_add(1);
    }

    fn lose_capture(&mut self) -> TraceEffect {
        self.harness.event(Event::PointerGone);
        self.harness.step();
        let output = self.take_output();
        assert!(output.released_native_ownership());
        TraceEffect::ReleaseNativeOwnership
    }

    fn replace_owner(&mut self) -> TraceEffect {
        let successor = Id::new("foreign-egui-owner");
        self.successor_capture = Some(successor);
        self.state_mut().steal_capture = true;
        self.harness.step();
        let output = self.take_output();
        assert!(output.released_native_ownership());
        assert_eq!(self.harness.ctx.dragged_id(), Some(successor));
        TraceEffect::ReleaseNativeOwnership
    }

    fn release_previous_owner(&mut self) -> TraceEffect {
        let successor = self
            .successor_capture
            .expect("owner replacement installs a successor");
        let ctx = self.harness.ctx.clone();
        let state = self.harness.state_mut();
        let output = state
            .interaction
            .cancel(&ctx)
            .expect("late previous-owner release is idempotent");
        assert!(!output.released_native_ownership());
        assert_eq!(ctx.dragged_id(), Some(successor));
        TraceEffect::None
    }

    fn cancel(&mut self, reason: TraceCancelReason) -> TraceEffect {
        let output = match reason {
            TraceCancelReason::Escape => {
                self.harness.key_down(Key::Escape);
                self.harness.step();
                self.take_output()
            }
            TraceCancelReason::OutsideClick => {
                let ctx = self.harness.ctx.clone();
                let state = self.harness.state_mut();
                state
                    .interaction
                    .cancel(&ctx)
                    .expect("outside click cancels")
            }
        };
        assert!(output.released_native_ownership());
        TraceEffect::ReleaseNativeOwnership
    }

    fn assert_candidate(&self, candidate: TraceCandidate) {
        let presentation = self.state().interaction.presentation();
        let probe = presentation
            .connection()
            .expect("candidate assertion owns a connection")
            .probe();
        match (candidate, probe) {
            (TraceCandidate::None, ConnectionProbe::None) => {}
            (TraceCandidate::Invalid, ConnectionProbe::Invalid { diagnostics, .. }) => {
                assert!(!diagnostics.is_empty())
            }
            (
                TraceCandidate::Valid,
                ConnectionProbe::Deferred { .. } | ConnectionProbe::Valid { .. },
            ) => {}
            (candidate, probe) => panic!("trace candidate {candidate:?} disagrees with {probe:?}"),
        }
    }

    fn move_to_candidate(&mut self, candidate: TraceCandidate) -> TraceEffect {
        let point = match candidate {
            TraceCandidate::None => self.state().anchors.empty,
            TraceCandidate::Invalid => self.invalid_anchor(),
            TraceCandidate::Valid => self.valid_anchor(),
        };
        let output = self.move_pointer(point);
        let effect = self.capture_semantic_effect(output, true);
        self.assert_candidate(candidate);
        effect
    }

    fn capture_semantic_effect(
        &mut self,
        mut output: EguiInteractionOutput,
        query_is_probe: bool,
    ) -> TraceEffect {
        assert!(
            output.terminal_query_error().is_none(),
            "canonical egui queries must not terminate with an adapter-visible error"
        );
        let query_pages_attempted = output.attempted_query_page_count();
        self.max_query_pages_attempted = self.max_query_pages_attempted.max(query_pages_attempted);
        if self.query_schedule == TraceQuerySchedule::ManyPagesPerFrame {
            assert!(
                !output.renderer_work_pending(),
                "the many-page egui grant must settle each semantic sampling point",
            );
        }
        let attempted_query = query_pages_attempted != 0;
        let mut effects = output.take_effects().into_iter();
        if let Some(effect) = effects.next() {
            assert!(
                effects.next().is_none(),
                "one interaction transition emitted unordered effects"
            );
            match effect {
                CanvasInteractionEffect::Search(request) => {
                    self.search_request = Some(request);
                    return TraceEffect::OpenSearch;
                }
                CanvasInteractionEffect::SubmitGraph { submission } => {
                    self.ticket = Some(submission.ticket());
                    let presentation = self.state().interaction.presentation();
                    let effect = if presentation.connection().is_some() {
                        TraceEffect::SubmitConnection
                    } else if presentation.node_drag().is_some() {
                        TraceEffect::SubmitNodeMove
                    } else {
                        panic!("canonical traces only submit connection or node-move work")
                    };
                    self.submission = Some(submission);
                    return effect;
                }
            }
        }
        let presentation = self.state().interaction.presentation();
        if attempted_query && presentation.phase() == CanvasInteractionPhase::EdgeActive {
            return TraceEffect::PickEdge;
        }
        if query_is_probe && attempted_query && presentation.connection().is_some() {
            return TraceEffect::ProbeConnection;
        }
        TraceEffect::None
    }

    fn observed_phase(&self) -> TracePhase {
        let presentation = self.state().interaction.presentation();
        match presentation.phase() {
            CanvasInteractionPhase::Idle | CanvasInteractionPhase::Closed => TracePhase::Idle,
            CanvasInteractionPhase::Armed => TracePhase::Armed,
            CanvasInteractionPhase::NodeDrag => TracePhase::NodeDrag,
            CanvasInteractionPhase::ConnectionDrag
                if presentation
                    .connection()
                    .is_some_and(|connection| connection.probe().is_committable()) =>
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
        let presentation = self.state().interaction.presentation();
        let ownership = match self.harness.ctx.dragged_id() {
            None => TraceOwnership::Released,
            Some(owner) if self.successor_capture == Some(owner) => TraceOwnership::HeldBySuccessor,
            Some(owner) if self.adapter_capture == Some(owner) => TraceOwnership::HeldByAdapter,
            Some(owner) => panic!("unknown egui drag owner: {owner:?}"),
        };
        let ticket = if self.ticket.is_some() {
            TraceTicket::Pending
        } else {
            TraceTicket::None
        };
        let graph_revision_delta = self
            .state()
            .runtime
            .snapshot()
            .expect("egui trace runtime remains available")
            .revision()
            .saturating_sub(self.baseline_revision)
            .saturating_sub(self.excluded_revision_delta);
        let graph = self.observed_graph();
        let geometry = self.observed_geometry(graph);
        let mut observation = NormalizedObservation::new(
            self.observed_phase(),
            effect,
            ownership,
            ticket,
            graph_revision_delta,
            geometry,
        );
        observation.optimistic_presentation = presentation
            .connection()
            .is_some_and(|_| self.ticket.is_some())
            || presentation.connection_settlement().is_some()
            || (self.ticket.is_some() && presentation.node_drag().is_some());
        observation.terminal = presentation.terminal().map(trace_terminal);
        observation.connection_settlement = match presentation.connection_settlement() {
            None => TraceConnectionSettlement::None,
            Some(ConnectionSettlementPresentation::AwaitingPublication) => {
                TraceConnectionSettlement::AwaitingPublication
            }
            Some(ConnectionSettlementPresentation::Uncertain { can_retry, .. }) => {
                TraceConnectionSettlement::Uncertain {
                    retryable: can_retry,
                }
            }
            Some(ConnectionSettlementPresentation::Retryable { can_retry, .. }) => {
                TraceConnectionSettlement::Retryable {
                    retryable: can_retry,
                }
            }
        };
        observation.intent = presentation
            .connection_intent()
            .map_or(TraceIntent::None, trace_intent);
        observation.graph = graph;
        observation.selection = match self.state().view().selection() {
            Ok(selection)
                if selection.nodes().is_empty()
                    && selection.edges().is_empty()
                    && selection.groups().is_empty() =>
            {
                TraceSelection::Empty
            }
            Ok(selection)
                if selection.nodes() == [self.state().ids.source_node]
                    && selection.edges().is_empty()
                    && selection.groups().is_empty() =>
            {
                TraceSelection::SourceNode
            }
            Ok(selection)
                if selection.nodes().is_empty()
                    && selection.edges() == [self.state().ids.reconnect_edge]
                    && selection.groups().is_empty() =>
            {
                TraceSelection::CanonicalEdge
            }
            Ok(selection) => panic!("unexpected canonical egui selection: {selection:?}"),
            Err(_) => TraceSelection::Unavailable,
        };
        observation.view_status = match self.state().view().close_reason() {
            None => TraceViewStatus::Open,
            Some(ViewCloseReason::HostClosed) => TraceViewStatus::HostClosed,
            Some(ViewCloseReason::RuntimeDropped) => TraceViewStatus::RuntimeDropped,
        };
        observation
    }

    fn observed_graph(&self) -> TraceGraph {
        let ids = self.state().ids;
        let snapshot = self
            .state()
            .runtime
            .snapshot()
            .expect("egui trace graph remains available");
        let graph = snapshot.graph();
        if graph.node(&ids.source_node).is_none() {
            assert!(graph.port(&ids.source_port).is_none());
            return TraceGraph::SourceRetired;
        }
        if let Some(edge) = graph.edge(&EdgeId::from_u128(100)) {
            return match (edge.from, edge.to) {
                (from, to) if from == ids.source_port && to == ids.valid_port => {
                    TraceGraph::NewConnection
                }
                (from, to) if from == ids.source_port && to == ids.created_port => {
                    assert!(graph.node(&ids.created_node).is_some());
                    assert!(graph.port(&ids.created_port).is_some());
                    TraceGraph::CreateFromWire
                }
                endpoints => panic!("unexpected canonical egui new edge endpoints: {endpoints:?}"),
            };
        }
        let reconnect = graph
            .edge(&ids.reconnect_edge)
            .expect("canonical egui reconnect edge remains present");
        match (reconnect.from, reconnect.to) {
            (from, to) if from == ids.source_port && to == ids.reconnect_target_port => {
                TraceGraph::ReconnectSource
            }
            (from, to)
                if from == ids.reconnect_source_port && to == ids.reconnect_alternate_port =>
            {
                TraceGraph::ReconnectTarget
            }
            (from, to) if from == ids.reconnect_source_port && to == ids.reconnect_target_port => {
                let source = graph
                    .node(&ids.source_node)
                    .expect("canonical egui source node remains present");
                if source.pos == self.baseline_source_position {
                    TraceGraph::Baseline
                } else {
                    let expected = CanvasPoint {
                        x: self.baseline_source_position.x + self.state().anchors.empty.x
                            - self.state().anchors.source.x,
                        y: self.baseline_source_position.y + self.state().anchors.empty.y
                            - self.state().anchors.source.y,
                    };
                    assert!((source.pos.x - expected.x).abs() < 0.001);
                    assert!((source.pos.y - expected.y).abs() < 0.001);
                    TraceGraph::NodeMoved
                }
            }
            endpoints => panic!("unexpected canonical egui reconnect endpoints: {endpoints:?}"),
        }
    }

    fn observed_geometry(&self, graph: TraceGraph) -> TraceGeometry {
        let ids = self.state().ids;
        let Ok(geometry) = self.state().view().geometry_snapshot() else {
            return TraceGeometry::Unavailable;
        };
        for port in [
            ids.valid_port,
            ids.reconnect_source_port,
            ids.reconnect_target_port,
            ids.reconnect_alternate_port,
        ] {
            let published = geometry
                .port(port)
                .unwrap_or_else(|| panic!("canonical egui geometry is missing port {port:?}"));
            assert!(published.hit_bounds().is_some());
        }
        match graph {
            TraceGraph::Baseline => {
                self.assert_route_matches(
                    &geometry,
                    ids.reconnect_edge,
                    ids.reconnect_source_port,
                    ids.reconnect_target_port,
                );
                TraceGeometry::Baseline
            }
            TraceGraph::SourceRetired => {
                assert!(geometry.node(ids.source_node).is_none());
                assert!(geometry.port(ids.source_port).is_none());
                TraceGeometry::SourceRetired
            }
            TraceGraph::NewConnection => {
                self.assert_route_matches(
                    &geometry,
                    EdgeId::from_u128(100),
                    ids.source_port,
                    ids.valid_port,
                );
                TraceGeometry::NewConnectionRouted
            }
            TraceGraph::ReconnectSource => {
                self.assert_route_matches(
                    &geometry,
                    ids.reconnect_edge,
                    ids.source_port,
                    ids.reconnect_target_port,
                );
                TraceGeometry::ReconnectSourceRouted
            }
            TraceGraph::ReconnectTarget => {
                self.assert_route_matches(
                    &geometry,
                    ids.reconnect_edge,
                    ids.reconnect_source_port,
                    ids.reconnect_alternate_port,
                );
                TraceGeometry::ReconnectTargetRouted
            }
            TraceGraph::CreateFromWire => {
                if geometry.port(ids.created_port).is_some() {
                    self.assert_route_matches(
                        &geometry,
                        EdgeId::from_u128(100),
                        ids.source_port,
                        ids.created_port,
                    );
                    TraceGeometry::CreateFromWireRouted
                } else {
                    assert!(geometry.route(EdgeId::from_u128(100)).is_none());
                    TraceGeometry::CreateFromWireAwaitingLayout
                }
            }
            TraceGraph::NodeMoved => {
                let snapshot = self.state().runtime.snapshot().unwrap();
                let source_position = snapshot.graph().node(&ids.source_node).unwrap().pos;
                let source_bounds = geometry.node(ids.source_node).unwrap().bounds();
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
            .unwrap_or_else(|| panic!("canonical egui geometry is missing route {edge:?}"));
        let source = geometry.port(from).expect("egui route source geometry");
        let target = geometry.port(to).expect("egui route target geometry");
        assert_eq!(route.facts().endpoints.source.point, source.route_anchor());
        assert_eq!(route.facts().endpoints.target.point, target.route_anchor());
    }

    fn invalid_anchor(&self) -> ViewPoint {
        match self.scenario {
            TraceScenarioId::ReconnectSource(_) | TraceScenarioId::ReconnectSourcePrefixWait => {
                self.state().anchors.valid
            }
            _ => self.state().anchors.invalid,
        }
    }

    fn valid_anchor(&self) -> ViewPoint {
        match self.scenario {
            TraceScenarioId::ReconnectSource(_) | TraceScenarioId::ReconnectSourcePrefixWait => {
                self.state().anchors.source
            }
            TraceScenarioId::ReconnectTarget(_) | TraceScenarioId::ReconnectTargetPrefixWait => {
                self.state().anchors.reconnect_alternate
            }
            TraceScenarioId::ReconnectTargetNoop => self.state().anchors.reconnect_target,
            _ => self.state().anchors.valid,
        }
    }

    fn take_output(&mut self) -> EguiInteractionOutput {
        self.state_mut()
            .last_output
            .take()
            .expect("each egui frame emits adapter output")
    }

    fn state(&self) -> &EguiTraceState {
        self.harness.state()
    }

    fn state_mut(&mut self) -> &mut EguiTraceState {
        self.harness.state_mut()
    }
}

struct EguiTraceState {
    runtime: GraphRuntime,
    host: EguiNativeFrameHost,
    interaction: EguiCanvasInteraction,
    ids: TraceGraphIds,
    anchors: TraceAnchors,
    response: Option<TraceResponse>,
    response_visible: bool,
    steal_capture: bool,
    query_pages_per_frame: usize,
    last_output: Option<EguiInteractionOutput>,
}

impl EguiTraceState {
    fn for_evidence(
        evidence: Option<TraceCommandEvidence>,
        query_schedule: TraceQuerySchedule,
    ) -> Self {
        let ids = TraceGraphIds::new();
        let anchors = TraceAnchors::new();
        let mut builder = GraphBuilder::new(GraphId::from_u128(1));
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
                if query_schedule.overlaps_valid_targets() {
                    anchors.valid
                } else {
                    anchors.reconnect_alternate
                },
            ),
        ] {
            builder.insert_node(node, trace_node(port, anchor));
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
            .build(builder.build().expect("trace graph"))
            .expect("trace runtime");
        let runtime = match evidence {
            Some(TraceCommandEvidence::Completion(TraceCompletion::Rejected)) => {
                runtime.with_middleware(RejectCanonicalCommand)
            }
            Some(_) | None => runtime,
        };
        let view = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("trace view");
        let interaction = EguiCanvasInteraction::new(&view, query_schedule.interaction_config())
            .expect("egui interaction");
        let host = EguiNativeFrameHost::new(view);
        Self {
            runtime,
            host,
            interaction,
            ids,
            anchors,
            response: None,
            response_visible: true,
            steal_capture: false,
            query_pages_per_frame: query_schedule.query_pages_per_frame(),
            last_output: None,
        }
    }

    fn response_center(&self, response: TraceResponse) -> Pos2 {
        match response {
            TraceResponse::NativeControl | TraceResponse::NodeDrag | TraceResponse::Connection => {
                self.anchors.source_pos()
            }
            TraceResponse::Reconnect(TraceReconnectEndpoint::Source) => {
                self.anchors.reconnect_source_pos()
            }
            TraceResponse::Reconnect(TraceReconnectEndpoint::Target) => {
                self.anchors.reconnect_target_pos()
            }
            TraceResponse::Edge => Pos2::new(400.0, 80.0),
        }
    }

    fn view(&self) -> &ViewSession {
        self.host.view()
    }
}

#[derive(Clone, Copy)]
enum TraceResponse {
    NativeControl,
    NodeDrag,
    Connection,
    Reconnect(TraceReconnectEndpoint),
    Edge,
}

#[derive(Clone, Copy)]
enum TraceReconnectEndpoint {
    Source,
    Target,
}

fn render_trace(ui: &mut Ui, state: &mut EguiTraceState) {
    render_trace_native_frame(ui, state);
    let scene_rect = ui.max_rect();
    let query_pages_per_frame = state.query_pages_per_frame;
    state
        .interaction
        .begin_frame_with_query_page_budget(ui.ctx(), scene_rect, query_pages_per_frame)
        .expect("begin egui interaction frame");
    if let Some(kind) = state.response.filter(|_| state.response_visible) {
        let center = state.response_center(kind);
        let response = ui.interact(
            Rect::from_center_size(center, Vec2::splat(28.0)),
            Id::new(("canonical-egui-response", response_key(kind))),
            match kind {
                TraceResponse::NativeControl => Sense::click(),
                TraceResponse::NodeDrag
                | TraceResponse::Connection
                | TraceResponse::Reconnect(_)
                | TraceResponse::Edge => Sense::drag(),
            },
        );
        let target = if matches!(kind, TraceResponse::NativeControl) {
            CanvasPressTarget::NativeControl
        } else if response.ctx.input(|input| {
            input.pointer.button_pressed(PointerButton::Primary)
                && response.is_pointer_button_down_on()
        }) {
            press_target(state.host.view(), &state.interaction, state.ids, kind)
        } else {
            CanvasPressTarget::Pan
        };
        state
            .interaction
            .handle_response(&response, target)
            .expect("egui response maps to semantic input");
        if state.steal_capture {
            ui.ctx().set_dragged_id(Id::new("foreign-egui-owner"));
            state.steal_capture = false;
        }
    }
    paint_native_evidence(ui, state);
    let mut output = state
        .interaction
        .end_frame(ui.ctx())
        .expect("end egui interaction frame");
    let reconciliation = state
        .interaction
        .reconcile_authoritative_state(ui.ctx())
        .expect("real egui frame feedback reconciles authoritative geometry");
    output.merge(reconciliation);
    state.last_output = Some(output);
}

fn render_trace_native_frame(ui: &mut Ui, state: &EguiTraceState) {
    let ids = state.ids;
    let anchors = state.anchors;
    let overlap_valid_targets = state.query_pages_per_frame == PAGED_QUERY_PAGES_PER_FRAME;
    state
        .host
        .show(ui, trace_frame_request(), |ui, native| {
            let (port, direction, anchor) = trace_native_port(
                ids,
                anchors,
                overlap_valid_targets,
                native.frame_node().id(),
            );
            let visual = ui.interact(
                Rect::from_center_size(Pos2::new(anchor.x, anchor.y), Vec2::splat(10.0)),
                native.id("port-visual"),
                Sense::hover(),
            );
            let hit = ui.interact(
                Rect::from_center_size(Pos2::new(anchor.x, anchor.y), Vec2::splat(24.0)),
                native.id("port-hit"),
                Sense::hover(),
            );
            native
                .measure_port_with_hit(port, &visual, &hit, direction)
                .expect("canonical native port measurement");
            ui.interact(ui.max_rect(), native.id("root"), Sense::hover())
        })
        .expect("canonical real egui host submits one complete frame");
}

fn trace_native_port(
    ids: TraceGraphIds,
    anchors: TraceAnchors,
    overlap_valid_targets: bool,
    node: NodeId,
) -> (PortId, HandlePosition, ViewPoint) {
    if node == ids.source_node {
        (ids.source_port, HandlePosition::Right, anchors.source)
    } else if node == ids.invalid_node {
        (ids.invalid_port, HandlePosition::Right, anchors.invalid)
    } else if node == ids.valid_node {
        (ids.valid_port, HandlePosition::Left, anchors.valid)
    } else if node == ids.reconnect_source_node {
        (
            ids.reconnect_source_port,
            HandlePosition::Right,
            anchors.reconnect_source,
        )
    } else if node == ids.reconnect_target_node {
        (
            ids.reconnect_target_port,
            HandlePosition::Left,
            anchors.reconnect_target,
        )
    } else if node == ids.reconnect_alternate_node {
        (
            ids.reconnect_alternate_port,
            HandlePosition::Left,
            if overlap_valid_targets {
                anchors.valid
            } else {
                anchors.reconnect_alternate
            },
        )
    } else {
        (
            ids.created_port,
            HandlePosition::Left,
            ViewPoint::new(30.0, 30.0),
        )
    }
}

fn press_target(
    view: &ViewSession,
    interaction: &EguiCanvasInteraction,
    ids: TraceGraphIds,
    response: TraceResponse,
) -> CanvasPressTarget {
    match response {
        TraceResponse::NativeControl => CanvasPressTarget::NativeControl,
        TraceResponse::NodeDrag => interaction
            .node_drag_target(
                view.node_lifetime_key(ids.source_node)
                    .expect("node identity query")
                    .expect("source node is current"),
                SelectionMode::Replace,
            )
            .expect("node target query")
            .expect("source node is draggable"),
        TraceResponse::Connection => interaction
            .connection_target(
                view.port_instance_key(ids.source_port)
                    .expect("port identity query")
                    .expect("source port is current"),
                EdgeId::from_u128(100),
                NodeGraphConnectionMode::Strict,
                true,
            )
            .expect("connection target query")
            .expect("source port starts a connection"),
        TraceResponse::Reconnect(endpoint) => {
            let geometry = view.geometry_snapshot().expect("reconnect geometry");
            let route = geometry
                .route(ids.reconnect_edge)
                .expect("published reconnect route");
            let endpoint = match endpoint {
                TraceReconnectEndpoint::Source => EdgeEndpoint::From,
                TraceReconnectEndpoint::Target => EdgeEndpoint::To,
            };
            let fixed_endpoint = match endpoint {
                EdgeEndpoint::From => route.facts().endpoints.target,
                EdgeEndpoint::To => route.facts().endpoints.source,
            };
            CanvasPressTarget::Connection(
                view.connection_start_from_route(
                    route,
                    endpoint,
                    fixed_endpoint,
                    NodeGraphConnectionMode::Strict,
                    false,
                )
                .expect("reconnect facts belong to this view")
                .expect("reconnect endpoint is enabled"),
            )
        }
        TraceResponse::Edge => CanvasPressTarget::Canvas(CanvasPressFallback::Pan {
            selection_mode: SelectionMode::Replace,
        }),
    }
}

fn paint_native_evidence(ui: &Ui, state: &EguiTraceState) {
    let presentation = state.interaction.presentation();
    if let Some(connection) = presentation.connection() {
        let (center, color) = match connection.probe() {
            ConnectionProbe::Deferred { target, .. } | ConnectionProbe::Valid { target, .. } => {
                (target.route_anchor(), Color32::from_rgb(70, 190, 120))
            }
            ConnectionProbe::Invalid { target, .. } => {
                (target.route_anchor(), Color32::from_rgb(220, 85, 80))
            }
            ConnectionProbe::None => return,
        };
        ui.painter().circle_stroke(
            Pos2::new(center.x, center.y),
            9.0,
            Stroke::new(2.0_f32, color),
        );
    }
    if presentation.active_edge().is_some() {
        ui.painter().line_segment(
            [Pos2::new(360.0, 80.0), Pos2::new(440.0, 80.0)],
            Stroke::new(3.0_f32, Color32::from_rgb(85, 145, 235)),
        );
    }
}

fn trace_terminal(reason: InteractionTerminalReason) -> TraceTerminal {
    match reason {
        InteractionTerminalReason::Applied => TraceTerminal::Applied,
        InteractionTerminalReason::Noop => TraceTerminal::Noop,
        InteractionTerminalReason::Rejected => TraceTerminal::Rejected,
        InteractionTerminalReason::Conflict => TraceTerminal::Conflict,
        InteractionTerminalReason::ExecutionFailed => TraceTerminal::ExecutionFailed,
        InteractionTerminalReason::TransportFailure => TraceTerminal::TransportFailure,
        InteractionTerminalReason::Uncertain => TraceTerminal::Uncertain,
        InteractionTerminalReason::Cancelled => TraceTerminal::Cancelled,
        InteractionTerminalReason::InvalidTarget => TraceTerminal::InvalidTarget,
        InteractionTerminalReason::OwnerRetired => TraceTerminal::OwnerRetired,
        InteractionTerminalReason::ViewClosed => TraceTerminal::HostClosed,
        reason @ (InteractionTerminalReason::ProtocolFailure
        | InteractionTerminalReason::BudgetExhausted
        | InteractionTerminalReason::GeometryIndeterminate
        | InteractionTerminalReason::IdentityExhausted) => {
            panic!("canonical egui trace reached unsupported terminal reason {reason:?}")
        }
    }
}

fn trace_intent(intent: ConnectionGestureBinding) -> TraceIntent {
    let Some(_) = intent.reconnect_edge() else {
        return TraceIntent::Connect;
    };
    match intent.fixed_endpoint() {
        EdgeEndpoint::To => TraceIntent::ReconnectSource,
        EdgeEndpoint::From => TraceIntent::ReconnectTarget,
    }
}

fn response_key(response: TraceResponse) -> u8 {
    match response {
        TraceResponse::NativeControl => 0,
        TraceResponse::NodeDrag => 1,
        TraceResponse::Connection => 2,
        TraceResponse::Reconnect(TraceReconnectEndpoint::Source) => 3,
        TraceResponse::Reconnect(TraceReconnectEndpoint::Target) => 4,
        TraceResponse::Edge => 5,
    }
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

impl TraceGraphIds {
    fn new() -> Self {
        Self {
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
        }
    }
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

impl TraceAnchors {
    fn new() -> Self {
        Self {
            source: ViewPoint::new(60.0, 80.0),
            invalid: ViewPoint::new(160.0, 80.0),
            valid: ViewPoint::new(240.0, 80.0),
            reconnect_source: ViewPoint::new(360.0, 80.0),
            reconnect_target: ViewPoint::new(440.0, 80.0),
            reconnect_alternate: ViewPoint::new(540.0, 80.0),
            empty: ViewPoint::new(700.0, 300.0),
        }
    }

    fn source_pos(self) -> Pos2 {
        Pos2::new(self.source.x, self.source.y)
    }

    fn reconnect_source_pos(self) -> Pos2 {
        Pos2::new(self.reconnect_source.x, self.reconnect_source.y)
    }

    fn reconnect_target_pos(self) -> Pos2 {
        Pos2::new(self.reconnect_target.x, self.reconnect_target.y)
    }
}

fn trace_frame_request() -> FrameRequest {
    FrameRequest::new(
        CanvasRect {
            origin: CanvasPoint { x: -50.0, y: -50.0 },
            size: CanvasSize {
                width: 900.0,
                height: 500.0,
            },
        },
        RendererStamp::new("trace.node", 1),
    )
}

fn trace_node(port: PortId, anchor: ViewPoint) -> Node {
    Node {
        kind: NodeKindKey::new("trace.node"),
        kind_version: 1,
        pos: CanvasPoint {
            x: anchor.x - 30.0,
            y: anchor.y - 30.0,
        },
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

fn trace_creation(ids: TraceGraphIds) -> jellyflow_runtime::schema::NodeCreation {
    let mut catalog = NodeCatalog::new();
    catalog
        .register(
            NodeKindSpec::builder("trace.created")
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
            CanvasRect {
                origin: CanvasPoint::default(),
                size: CanvasSize {
                    width: 60.0,
                    height: 60.0,
                },
            },
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
