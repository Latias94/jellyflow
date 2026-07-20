use super::*;

use jellyflow_runtime::runtime::adapter_protocol::conformance::{
    ConformanceAction, ConformanceCancelReason, ConformanceCandidate, ConformanceEffect,
    ConformanceGeometry, ConformanceOwnership, ConformancePhase, ConformanceScenario,
    ConformanceScenarioId, ConformanceSettlement, ConformanceTarget, ConformanceTerminal,
    ConformanceTicket, NormalizedObservation, canonical_scenarios, compare_observations,
};
use open_gpui::{InputEvent, MouseUpEvent};

use crate::command_journal::OpenGpuiCommandOrigin;

fn source_node() -> NodeId {
    NodeId::from_u128(10)
}

fn target_node() -> NodeId {
    NodeId::from_u128(11)
}

fn invalid_node() -> NodeId {
    NodeId::from_u128(12)
}

fn alternate_target_node() -> NodeId {
    NodeId::from_u128(13)
}

fn alternate_source_node() -> NodeId {
    NodeId::from_u128(14)
}

fn source_port() -> PortId {
    PortId::from_u128(20)
}

fn target_port() -> PortId {
    PortId::from_u128(21)
}

fn invalid_output_port() -> PortId {
    PortId::from_u128(22)
}

fn alternate_target_port() -> PortId {
    PortId::from_u128(23)
}

fn alternate_source_port() -> PortId {
    PortId::from_u128(24)
}

fn canonical_edge() -> EdgeId {
    EdgeId::from_u128(30)
}

struct CanonicalNode {
    binding: Entity<OpenGpuiNodeBinding>,
    control_hits: std::rc::Rc<std::cell::Cell<u32>>,
}

struct RejectCanonicalCommand;

impl GraphRuntimeMiddleware for RejectCanonicalCommand {
    fn before_command(
        &mut self,
        _snapshot: GraphRuntimeMiddlewareSnapshot,
        _command: &GraphCommand,
    ) -> Result<(), ApplyPipelineError> {
        Err(ApplyPipelineError::Rejected {
            message: "canonical command rejected".to_owned(),
            diagnostics: Vec::new(),
        })
    }
}

struct PanicCanonicalCommand;

impl GraphRuntimeMiddleware for PanicCanonicalCommand {
    fn before_command(
        &mut self,
        _snapshot: GraphRuntimeMiddlewareSnapshot,
        _command: &GraphCommand,
    ) -> Result<(), ApplyPipelineError> {
        panic!("canonical command transport failure")
    }
}

impl Render for CanonicalNode {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        let binding = self.binding.read(cx);
        let node_id = binding.snapshot().node_id();
        let read = binding.snapshot().read().expect("live canonical binding");
        let port = read.ports().next().expect("one canonical port");
        let direction = port.port().dir;
        let handle = binding.port_handle(port.id()).expect("canonical real port");
        let drag_handle = binding.drag_handle();
        let selector = canonical_port_selector(port.id());
        let marker = div()
            .debug_selector(move || selector.clone())
            .w(px(14.0))
            .h(px(14.0));
        let positioned = match direction {
            PortDirection::In => div().absolute().left(px(0.0)).top(px(43.0)),
            PortDirection::Out => div().absolute().right(px(0.0)).top(px(43.0)),
        };
        let node_selector = canonical_node_selector(node_id);

        let mut root = div()
            .debug_selector(move || node_selector.clone())
            .relative()
            .size_full()
            .child(positioned.child(handle.interactive(marker, |_, _, _| {})));
        if node_id == source_node() {
            let control_hits = self.control_hits.clone();
            root = root.child(
                div()
                    .debug_selector(|| "jellyflow-canonical-native-control".to_owned())
                    .absolute()
                    .left(px(16.0))
                    .top(px(12.0))
                    .w(px(64.0))
                    .h(px(24.0))
                    .on_mouse_down(MouseButton::Left, |_, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                    })
                    .on_mouse_up(MouseButton::Left, move |_, window, cx| {
                        control_hits.set(control_hits.get().saturating_add(1));
                        window.prevent_default();
                        cx.stop_propagation();
                    }),
            );
        }
        drag_handle.wrap(root)
    }
}

struct CanonicalDriver {
    id: ConformanceScenarioId,
    runtime: GraphRuntime,
    host: Entity<OpenGpuiSceneHost>,
    baseline_revision: u64,
    excluded_revision_delta: u64,
    pointer: Option<open_gpui::Point<open_gpui::Pixels>>,
    last_effect: ConformanceEffect,
    control_hits: std::rc::Rc<std::cell::Cell<u32>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingMutationKind {
    Connection,
    NodeDrag,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct EffectEvidence {
    phase: CanvasInteractionPhase,
    connection_pointer: Option<CanvasPoint>,
    pending: Option<PendingMutationKind>,
    search_open: bool,
    active_edge: Option<EdgeId>,
    terminal: Option<InteractionTerminalReason>,
}

impl CanonicalDriver {
    fn mount<'a>(
        scenario: &ConformanceScenario,
        cx: &'a mut open_gpui::TestAppContext,
    ) -> (Self, &'a mut open_gpui::VisualTestContext) {
        let with_edge = matches!(
            scenario.id,
            ConformanceScenarioId::ReconnectSource(_)
                | ConformanceScenarioId::ReconnectTarget(_)
                | ConformanceScenarioId::EdgePick
        );
        let runtime = GraphRuntime::builder(catalog())
            .build(canonical_graph(with_edge))
            .expect("valid canonical graph");
        let runtime = match scenario_settlement(scenario.id) {
            Some(ConformanceSettlement::Rejected) => {
                runtime.with_middleware(RejectCanonicalCommand)
            }
            Some(ConformanceSettlement::TransportFailure) => {
                runtime.with_middleware(PanicCanonicalCommand)
            }
            _ => runtime,
        };
        let baseline_revision = runtime.snapshot().expect("baseline snapshot").revision();
        let view = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("attach canonical view");
        let control_hits = std::rc::Rc::new(std::cell::Cell::new(0));
        let factory_control_hits = control_hits.clone();
        let mut registry = OpenGpuiNodeRegistry::builder();
        registry.register(RENDERER, move |binding, _window, cx| {
            cx.new(|_| CanonicalNode {
                binding,
                control_hits: factory_control_hits.clone(),
            })
        });
        let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
        let scene_host = captured_host.clone();
        let scenario_id = scenario.id;
        let (_scene, cx) = cx.add_window_view(move |_, cx| {
            let host = cx.new(|_| {
                let host = OpenGpuiSceneHost::new(
                    view,
                    registry.seal().expect("valid canonical registry"),
                    viewport(),
                );
                if scenario_id == ConformanceScenarioId::OutsideClick {
                    host.with_connection_options(crate::OpenGpuiConnectionOptions {
                        allow_create_from_wire: false,
                        ..crate::OpenGpuiConnectionOptions::default()
                    })
                } else {
                    host
                }
            });
            *scene_host.borrow_mut() = Some(host.clone());
            InteractiveScene { host }
        });
        let host = captured_host.borrow().clone().expect("captured host");
        if matches!(
            scenario.id,
            ConformanceScenarioId::ReconnectSource(_) | ConformanceScenarioId::ReconnectTarget(_)
        ) {
            cx.read_entity(&host, |host, _| {
                host.view
                    .set_selection(Vec::new(), vec![canonical_edge()], Vec::new())
                    .expect("seed reconnect edge selection");
            });
        }
        draw(cx);
        draw(cx);

        (
            Self {
                id: scenario.id,
                runtime,
                host,
                baseline_revision,
                excluded_revision_delta: 0,
                pointer: None,
                last_effect: ConformanceEffect::None,
                control_hits,
            },
            cx,
        )
    }

    fn perform(&mut self, action: ConformanceAction, cx: &mut open_gpui::VisualTestContext) {
        let before = self.effect_evidence(cx);
        match action {
            ConformanceAction::Press(target) => self.press(target, cx),
            ConformanceAction::CrossDragThreshold => {
                let position = if matches!(self.id, ConformanceScenarioId::NodeDrag(_)) {
                    let start = self.pointer.expect("node drag press position");
                    point(start.x + px(48.0), start.y + px(36.0))
                } else {
                    point(px(280.0), px(430.0))
                };
                cx.simulate_mouse_move(position, Some(MouseButton::Left), Default::default());
                self.pointer = Some(position);
            }
            ConformanceAction::MoveToCandidate(candidate) => {
                let position = self.candidate_position(candidate, cx);
                cx.simulate_mouse_move(position, Some(MouseButton::Left), Default::default());
                self.pointer = Some(position);
            }
            ConformanceAction::Release => self.release(cx),
            ConformanceAction::SelectSearchCandidate => {
                cx.update_window_entity(&self.host, |host, window, cx| {
                    let request = host
                        .connection_search_request()
                        .expect("active canonical search request");
                    let candidates = host
                        .connection_search_candidates(request, window, cx)
                        .expect("canonical search candidates");
                    assert!(!candidates.candidates().is_empty());
                    host.select_connection_search_candidate(request, &candidates, 0, window, cx)
                        .expect("select canonical search candidate");
                });
            }
            ConformanceAction::AdvanceFrame => {
                draw_once(cx);
                self.prepare_injected_receipt(cx);
            }
            ConformanceAction::Settle(settlement) => {
                self.settle_retained_frames(cx);
                if settlement == ConformanceSettlement::Applied {
                    self.assert_applied_mutation();
                }
            }
            ConformanceAction::Cancel(ConformanceCancelReason::Escape) => {
                cx.simulate_keystrokes("escape");
            }
            ConformanceAction::Cancel(ConformanceCancelReason::OutsideClick) => {
                let position = self.pointer.expect("outside cancellation position");
                cx.simulate_mouse_up(position, MouseButton::Left, Default::default());
            }
            ConformanceAction::RetireSource => self.retire_source(cx),
            ConformanceAction::CloseHost => {
                cx.update(|window, cx| {
                    self.host
                        .update(cx, |host, cx| host.close_in_window(window, cx))
                        .expect("close active canonical host");
                });
                cx.run_until_parked();
            }
        }
        let after = self.effect_evidence(cx);
        self.last_effect = effect_from_transition(before, after);
    }

    fn press(&mut self, target: ConformanceTarget, cx: &mut open_gpui::VisualTestContext) {
        let position = match target {
            ConformanceTarget::NativeControl => {
                let before = self.control_hits.get();
                let position = cx
                    .debug_bounds("jellyflow-canonical-native-control")
                    .expect("canonical native control")
                    .center();
                cx.simulate_click(position, Default::default());
                assert_eq!(self.control_hits.get(), before.saturating_add(1));
                self.pointer = Some(position);
                return;
            }
            ConformanceTarget::SourcePort => self.port_position(source_port(), cx),
            ConformanceTarget::NodeDragHandle => cx
                .debug_bounds(&canonical_node_selector(source_node()))
                .expect("canonical node drag handle")
                .center(),
            ConformanceTarget::ReconnectSource => self.edge_endpoint_position(
                jellyflow::runtime::runtime::geometry::EdgePathAnchor::Source,
                cx,
            ),
            ConformanceTarget::ReconnectTarget => self.edge_endpoint_position(
                jellyflow::runtime::runtime::geometry::EdgePathAnchor::Target,
                cx,
            ),
            ConformanceTarget::Edge => self.edge_endpoint_position(
                jellyflow::runtime::runtime::geometry::EdgePathAnchor::Progress(0.35),
                cx,
            ),
        };
        if target == ConformanceTarget::Edge {
            cx.simulate_mouse_move(position, None, Default::default());
        }
        cx.simulate_mouse_down(position, MouseButton::Left, Default::default());
        self.pointer = Some(position);
    }

    fn release(&mut self, cx: &mut open_gpui::VisualTestContext) {
        let position = self.pointer.expect("canonical release position");
        if matches!(
            self.id,
            ConformanceScenarioId::CreateFromWire(_) | ConformanceScenarioId::EdgePick
        ) {
            cx.simulate_mouse_up(position, MouseButton::Left, Default::default());
        } else {
            dispatch_mouse_up(position, cx);
        }
    }

    fn candidate_position(
        &self,
        candidate: ConformanceCandidate,
        cx: &mut open_gpui::VisualTestContext,
    ) -> open_gpui::Point<open_gpui::Pixels> {
        match candidate {
            ConformanceCandidate::None => point(px(740.0), px(520.0)),
            ConformanceCandidate::Invalid => match self.id {
                ConformanceScenarioId::ReconnectSource(_) => {
                    self.port_position(alternate_target_port(), cx)
                }
                _ => self.port_position(invalid_output_port(), cx),
            },
            ConformanceCandidate::Valid => match self.id {
                ConformanceScenarioId::ReconnectSource(_) => {
                    self.port_position(alternate_source_port(), cx)
                }
                ConformanceScenarioId::ReconnectTarget(_) => {
                    self.port_position(alternate_target_port(), cx)
                }
                _ => self.port_position(target_port(), cx),
            },
        }
    }

    fn port_position(
        &self,
        port: PortId,
        cx: &mut open_gpui::VisualTestContext,
    ) -> open_gpui::Point<open_gpui::Pixels> {
        cx.debug_bounds(&canonical_port_selector(port))
            .unwrap_or_else(|| panic!("canonical port {port:?}"))
            .center()
    }

    fn edge_endpoint_position(
        &self,
        anchor: jellyflow::runtime::runtime::geometry::EdgePathAnchor,
        cx: &mut open_gpui::VisualTestContext,
    ) -> open_gpui::Point<open_gpui::Pixels> {
        cx.read_entity(&self.host, |host, _| {
            let geometry = host.view.geometry_snapshot().expect("edge geometry");
            let sample = geometry
                .route(canonical_edge())
                .expect("canonical edge route")
                .facts()
                .path
                .sample(anchor)
                .expect("canonical edge endpoint");
            let local = host
                .view
                .viewport_transform()
                .expect("canonical viewport")
                .screen_point_for_canvas(sample.point)
                .expect("visible edge endpoint");
            let origin = host.scene_origin.expect("canonical scene origin");
            point(origin.x + px(local.x), origin.y + px(local.y))
        })
    }

    fn settle_retained_frames(&self, cx: &mut open_gpui::VisualTestContext) {
        cx.run_until_parked();
        for _ in 0..4 {
            if cx.read_entity(&self.host, |host, _| {
                host.interaction_presentation().phase() != CanvasInteractionPhase::Committing
            }) {
                return;
            }
            draw(cx);
        }
        panic!("canonical interaction did not settle within retained-frame bound");
    }

    fn prepare_injected_receipt(&mut self, cx: &mut open_gpui::VisualTestContext) {
        let Some(settlement) = scenario_settlement(self.id) else {
            return;
        };
        if !matches!(
            settlement,
            ConformanceSettlement::Noop | ConformanceSettlement::Conflict
        ) {
            return;
        }

        let ticket = cx.update_entity(&self.host, |host, _| {
            assert!(
                host.interaction_owner.test_has_pending_submission(),
                "a native release must reach the command prefix boundary"
            );
            let presentation = host.interaction_presentation();
            let ticket = presentation
                .connection()
                .and_then(|connection| connection.ticket())
                .or_else(|| presentation.node_drag().and_then(|drag| drag.ticket()))
                .expect("pending canonical interaction ticket");
            host.discard_pending_interaction_submission();
            ticket
        });
        let snapshot = self.runtime.snapshot().expect("receipt injection snapshot");
        let expected_revision = snapshot.revision();
        let command = match settlement {
            ConformanceSettlement::Noop => GraphCommand::set_node_data(
                expected_revision,
                source_node(),
                snapshot
                    .graph()
                    .node(&source_node())
                    .expect("canonical source")
                    .data
                    .clone(),
            ),
            ConformanceSettlement::Conflict => {
                self.runtime
                    .execute(GraphCommand::set_node_data(
                        expected_revision,
                        invalid_node(),
                        serde_json::json!({ "external": "conflict" }),
                    ))
                    .expect("external conflict command")
                    .applied()
                    .expect("external conflict setup applies");
                self.excluded_revision_delta = self.excluded_revision_delta.saturating_add(1);
                GraphCommand::set_node_data(
                    expected_revision,
                    source_node(),
                    serde_json::json!({ "stale": true }),
                )
            }
            _ => unreachable!("only injected settlements reach this branch"),
        };
        cx.update_entity(&self.host, |host, _| {
            host.commands
                .enqueue_detached(command, OpenGpuiCommandOrigin::Interaction { ticket })
                .expect("inject canonical command receipt");
        });
    }

    fn retire_source(&mut self, cx: &mut open_gpui::VisualTestContext) {
        let revision = self
            .runtime
            .snapshot()
            .expect("source retirement snapshot")
            .revision();
        self.runtime
            .execute(GraphCommand::topology(
                revision,
                TopologyEdit::new().remove_node(source_node(), IncidentEdgePolicy::Reject),
            ))
            .expect("retire canonical source")
            .applied()
            .expect("source retirement applies");
        self.excluded_revision_delta = self.excluded_revision_delta.saturating_add(1);
        cx.run_until_parked();
        draw(cx);
    }

    fn assert_applied_mutation(&self) {
        let snapshot = self.runtime.snapshot().expect("applied canonical snapshot");
        match self.id {
            ConformanceScenarioId::NewConnection(_) => {
                let edge = snapshot
                    .graph()
                    .edges()
                    .values()
                    .next()
                    .expect("new connection edge");
                assert_eq!((edge.from, edge.to), (source_port(), target_port()));
            }
            ConformanceScenarioId::ReconnectSource(_) => {
                let edge = snapshot
                    .graph()
                    .edge(&canonical_edge())
                    .expect("source-reconnected edge retains identity");
                assert_eq!(
                    (edge.from, edge.to),
                    (alternate_source_port(), target_port())
                );
            }
            ConformanceScenarioId::ReconnectTarget(_) => {
                let edge = snapshot
                    .graph()
                    .edge(&canonical_edge())
                    .expect("target-reconnected edge retains identity");
                assert_eq!(
                    (edge.from, edge.to),
                    (source_port(), alternate_target_port())
                );
            }
            ConformanceScenarioId::CreateFromWire(_) => {
                assert_eq!(snapshot.graph().nodes().len(), 6);
                assert!(
                    snapshot
                        .graph()
                        .edges()
                        .values()
                        .any(|edge| { edge.from == source_port() || edge.to == source_port() })
                );
            }
            ConformanceScenarioId::NodeDrag(_) => {
                assert_ne!(
                    snapshot
                        .graph()
                        .node(&source_node())
                        .expect("dragged source node")
                        .pos,
                    CanvasPoint { x: 40.0, y: 80.0 },
                );
            }
            _ => panic!("Applied settlement is only valid for mutation scenarios"),
        }
    }

    fn effect_evidence(&self, cx: &mut open_gpui::VisualTestContext) -> EffectEvidence {
        cx.read_entity(&self.host, |host, _| {
            let presentation = host.interaction_presentation();
            let connection = presentation.connection();
            EffectEvidence {
                phase: presentation.phase(),
                connection_pointer: connection.map(|connection| connection.pointer().canvas()),
                pending: connection
                    .and_then(|connection| connection.ticket())
                    .map(|_| PendingMutationKind::Connection)
                    .or_else(|| {
                        presentation
                            .node_drag()
                            .and_then(|drag| drag.ticket())
                            .map(|_| PendingMutationKind::NodeDrag)
                    }),
                search_open: host.connection_search_request().is_some(),
                active_edge: presentation.active_edge(),
                terminal: presentation.terminal(),
            }
        })
    }

    fn observe(&self, cx: &mut open_gpui::VisualTestContext) -> NormalizedObservation {
        let captured = cx.update(|window, _| window.captured_hitbox()).is_some();
        cx.read_entity(&self.host, |host, _| {
            let presentation = host.interaction_presentation();
            let ticket = presentation
                .connection()
                .and_then(|connection| connection.ticket())
                .or_else(|| presentation.node_drag().and_then(|drag| drag.ticket()));
            NormalizedObservation {
                phase: normalized_phase(&presentation),
                effect: self.last_effect,
                terminal: presentation.terminal().map(normalized_terminal),
                ownership: if captured {
                    ConformanceOwnership::Held
                } else {
                    ConformanceOwnership::Released
                },
                ticket: if ticket.is_some() {
                    ConformanceTicket::Pending
                } else {
                    ConformanceTicket::None
                },
                graph_revision_delta: self
                    .runtime
                    .snapshot()
                    .expect("canonical graph snapshot")
                    .revision()
                    .saturating_sub(self.baseline_revision)
                    .saturating_sub(self.excluded_revision_delta),
                geometry: if host.view.geometry_snapshot().is_ok() {
                    ConformanceGeometry::Available
                } else {
                    ConformanceGeometry::Unavailable
                },
            }
        })
    }

    fn close(self, cx: &mut open_gpui::VisualTestContext) {
        if cx.read_entity(&self.host, |host, _| host.close_reason().is_none()) {
            cx.update(|window, cx| {
                self.host
                    .update(cx, |host, cx| host.close_in_window(window, cx))
                    .expect("close canonical host");
            });
        }
        cx.run_until_parked();
    }
}

fn normalized_phase(
    presentation: &jellyflow::runtime::CanvasInteractionPresentation,
) -> ConformancePhase {
    match presentation.phase() {
        CanvasInteractionPhase::Idle | CanvasInteractionPhase::Closed => ConformancePhase::Idle,
        CanvasInteractionPhase::Armed => ConformancePhase::Armed,
        CanvasInteractionPhase::NodeDrag => ConformancePhase::NodeDrag,
        CanvasInteractionPhase::ConnectionDrag
            if presentation
                .connection()
                .is_some_and(|connection| connection.probe().is_valid()) =>
        {
            ConformancePhase::Candidate
        }
        CanvasInteractionPhase::ConnectionDrag => ConformancePhase::ConnectionDrag,
        CanvasInteractionPhase::ConnectionSearch => ConformancePhase::Searching,
        CanvasInteractionPhase::Committing => ConformancePhase::Committing,
        CanvasInteractionPhase::EdgeActive => ConformancePhase::EdgeActive,
        CanvasInteractionPhase::Pan | CanvasInteractionPhase::Marquee => {
            panic!("canonical trace entered unsupported canvas gesture")
        }
    }
}

fn effect_from_transition(before: EffectEvidence, after: EffectEvidence) -> ConformanceEffect {
    if before.terminal != after.terminal {
        return match after.terminal {
            Some(
                InteractionTerminalReason::Rejected
                | InteractionTerminalReason::Conflict
                | InteractionTerminalReason::TransportFailure,
            ) => ConformanceEffect::RestorePresentation,
            Some(
                InteractionTerminalReason::Cancelled
                | InteractionTerminalReason::OwnerRetired
                | InteractionTerminalReason::ViewClosed,
            ) => ConformanceEffect::ReleaseNativeOwnership,
            _ => ConformanceEffect::None,
        };
    }
    if before.pending.is_none() && after.phase == CanvasInteractionPhase::Committing {
        return match after.pending {
            Some(PendingMutationKind::Connection) => ConformanceEffect::SubmitConnection,
            Some(PendingMutationKind::NodeDrag) => ConformanceEffect::SubmitNodeMove,
            None => ConformanceEffect::None,
        };
    }
    if !before.search_open
        && after.search_open
        && after.phase == CanvasInteractionPhase::ConnectionSearch
    {
        return ConformanceEffect::OpenSearch;
    }
    if before.active_edge != after.active_edge && after.active_edge.is_some() {
        return ConformanceEffect::PickEdge;
    }
    if matches!(
        after.phase,
        CanvasInteractionPhase::ConnectionDrag | CanvasInteractionPhase::Committing
    ) && before.connection_pointer != after.connection_pointer
        && after.connection_pointer.is_some()
    {
        return ConformanceEffect::ProbeConnection;
    }
    ConformanceEffect::None
}

fn normalized_terminal(terminal: InteractionTerminalReason) -> ConformanceTerminal {
    match terminal {
        InteractionTerminalReason::Applied => ConformanceTerminal::Applied,
        InteractionTerminalReason::Noop => ConformanceTerminal::Noop,
        InteractionTerminalReason::Rejected => ConformanceTerminal::Rejected,
        InteractionTerminalReason::Conflict => ConformanceTerminal::Conflict,
        InteractionTerminalReason::TransportFailure => ConformanceTerminal::TransportFailure,
        InteractionTerminalReason::Cancelled => ConformanceTerminal::Cancelled,
        InteractionTerminalReason::OwnerRetired => ConformanceTerminal::OwnerRetired,
        InteractionTerminalReason::ViewClosed => ConformanceTerminal::HostClosed,
        other => panic!("unsupported canonical terminal: {other:?}"),
    }
}

fn scenario_settlement(id: ConformanceScenarioId) -> Option<ConformanceSettlement> {
    match id {
        ConformanceScenarioId::NewConnection(settlement)
        | ConformanceScenarioId::ReconnectSource(settlement)
        | ConformanceScenarioId::ReconnectTarget(settlement)
        | ConformanceScenarioId::CreateFromWire(settlement)
        | ConformanceScenarioId::NodeDrag(settlement) => Some(settlement),
        ConformanceScenarioId::NativeControl
        | ConformanceScenarioId::EdgePick
        | ConformanceScenarioId::Escape
        | ConformanceScenarioId::OutsideClick
        | ConformanceScenarioId::SourceRetired
        | ConformanceScenarioId::HostClosed => None,
    }
}

fn dispatch_mouse_up(
    position: open_gpui::Point<open_gpui::Pixels>,
    cx: &mut open_gpui::VisualTestContext,
) {
    cx.update(|window, cx| {
        window.dispatch_event(
            MouseUpEvent {
                position,
                modifiers: Default::default(),
                button: MouseButton::Left,
                click_count: 1,
            }
            .to_platform_input(),
            cx,
        );
    });
}

fn run_canonical_scenario(scenario: &ConformanceScenario, cx: &mut open_gpui::TestAppContext) {
    let (mut driver, cx) = CanonicalDriver::mount(scenario, cx);
    let mut observations = Vec::with_capacity(scenario.steps.len());
    for step in &scenario.steps {
        driver.perform(step.action, cx);
        observations.push(driver.observe(cx));
    }
    compare_observations(scenario, &observations)
        .unwrap_or_else(|error| panic!("Open GPUI canonical mismatch: {error}"));
    driver.close(cx);
}

#[open_gpui::test]
fn canonical_scenarios_run_through_real_open_gpui_events_and_frames(
    cx: &mut open_gpui::TestAppContext,
) {
    for scenario in canonical_scenarios() {
        run_canonical_scenario(&scenario, cx);
    }
}

fn canonical_graph(with_edge: bool) -> jellyflow::core::Graph {
    let mut graph = GraphBuilder::new(GraphId::from_u128(0xCA11));
    for (node_id, position, port_id, key, direction) in [
        (
            source_node(),
            CanvasPoint { x: 40.0, y: 80.0 },
            source_port(),
            "source",
            PortDirection::Out,
        ),
        (
            target_node(),
            CanvasPoint { x: 360.0, y: 80.0 },
            target_port(),
            "target",
            PortDirection::In,
        ),
        (
            invalid_node(),
            CanvasPoint { x: 360.0, y: 260.0 },
            invalid_output_port(),
            "invalid-output",
            PortDirection::Out,
        ),
        (
            alternate_target_node(),
            CanvasPoint { x: 620.0, y: 80.0 },
            alternate_target_port(),
            "alternate-target",
            PortDirection::In,
        ),
        (
            alternate_source_node(),
            CanvasPoint { x: 40.0, y: 300.0 },
            alternate_source_port(),
            "alternate-source",
            PortDirection::Out,
        ),
    ] {
        graph.insert_node(node_id, node(position, port_id));
        graph.insert_port(port_id, port(node_id, key, direction));
    }
    if with_edge {
        graph.insert_edge(
            canonical_edge(),
            Edge::new(EdgeKind::Data, source_port(), target_port()),
        );
    }
    graph.build().expect("valid canonical graph")
}

fn canonical_port_selector(port: PortId) -> String {
    format!("jellyflow-canonical-port-{port:?}")
}

fn canonical_node_selector(node: NodeId) -> String {
    format!("jellyflow-canonical-node-{node:?}")
}
