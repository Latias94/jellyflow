use super::*;

use std::future::Future;
use std::pin::Pin;
use std::task::{Context as TaskContext, Poll, Waker};

use jellyflow_runtime::adapter::conformance::{
    ConformanceAction, ConformanceCancelReason, ConformanceCandidate, ConformanceCommandOutcome,
    ConformanceEffect, ConformanceGeometry, ConformanceGraph, ConformanceIntent,
    ConformanceOwnership, ConformancePhase, ConformanceScenario, ConformanceScenarioId,
    ConformanceSelection, ConformanceTarget, ConformanceTerminal, ConformanceTicket,
    ConformanceViewStatus, NormalizedObservation, canonical_scenarios, compare_observations,
    new_connection_probe_prefix_scenario,
};
use open_gpui::{InputEvent, MouseMoveEvent, MouseUpEvent};

use super::super::scene_render::open_gpui_reconnect_hit_selector;
use crate::command_journal::{OpenGpuiCommandOrigin, OpenGpuiCommandPrefixPermit};
use crate::interaction::release_pointer_capture_if_owned;

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

impl Render for CanonicalNode {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        let binding = self.binding.read(cx);
        let authoring = binding.authoring().expect("live canonical authoring");
        let node_id = authoring.node_id();
        let port = authoring.ports().next().expect("one canonical port");
        let direction = port.facts().dir;
        let handle = port.handle().clone();
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
            .child(positioned.child(handle.interactive(marker)));
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
    prefix_permit: Option<OpenGpuiCommandPrefixPermit>,
    foreign_capture: Option<HitboxId>,
    previous_capture: Option<HitboxId>,
    baseline_source_position: CanvasPoint,
    baseline_source_bounds: CanvasRect,
    query_schedule: CanonicalQuerySchedule,
    max_native_query_frames: usize,
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
    active_edge: Option<jellyflow_runtime::graph::EdgeInstanceKey>,
    terminal: Option<InteractionTerminalReason>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CanonicalQuerySchedule {
    Default,
    OnePagePerNativeFrame,
}

const PAGED_QUERY_NATIVE_FRAME_LIMIT: usize = 32;

impl CanonicalQuerySchedule {
    const fn overlaps_valid_targets(self) -> bool {
        matches!(self, Self::OnePagePerNativeFrame)
    }
}

impl CanonicalDriver {
    fn mount<'a>(
        scenario: &ConformanceScenario,
        cx: &'a mut open_gpui::TestAppContext,
    ) -> (Self, &'a mut open_gpui::VisualTestContext) {
        Self::mount_with_query_schedule(scenario, CanonicalQuerySchedule::Default, cx)
    }

    fn mount_with_query_schedule<'a>(
        scenario: &ConformanceScenario,
        query_schedule: CanonicalQuerySchedule,
        cx: &'a mut open_gpui::TestAppContext,
    ) -> (Self, &'a mut open_gpui::VisualTestContext) {
        let with_edge = matches!(
            scenario.id(),
            ConformanceScenarioId::ReconnectSource(_)
                | ConformanceScenarioId::ReconnectTarget(_)
                | ConformanceScenarioId::ReconnectSourcePrefixWait
                | ConformanceScenarioId::ReconnectTargetPrefixWait
                | ConformanceScenarioId::EdgePick
        );
        let runtime = GraphRuntime::builder(catalog())
            .build(canonical_graph(
                with_edge,
                query_schedule.overlaps_valid_targets(),
            ))
            .expect("valid canonical graph");
        let runtime = match scenario_command_outcome(scenario.id()) {
            Some(ConformanceCommandOutcome::Rejected) => {
                runtime.with_middleware(RejectCanonicalCommand)
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
        register_test_renderer(&mut registry, move |binding, _window, cx| {
            cx.new(|_| CanonicalNode {
                binding,
                control_hits: factory_control_hits.clone(),
            })
        });
        let captured_host = std::rc::Rc::new(std::cell::RefCell::new(None));
        let scene_host = captured_host.clone();
        let scenario_id = scenario.id();
        let (_scene, cx) = cx.add_window_view(move |_, cx| {
            let host = cx.new(|_| {
                let host = OpenGpuiSceneHost::new(
                    view,
                    registry.seal(&catalog()).expect("valid canonical registry"),
                    viewport(),
                );
                if scenario_id == ConformanceScenarioId::OutsideClick {
                    host.with_connection_options(crate::host::OpenGpuiConnectionOptions {
                        allow_create_from_wire: false,
                        ..crate::host::OpenGpuiConnectionOptions::default()
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
            scenario.id(),
            ConformanceScenarioId::ReconnectSource(_)
                | ConformanceScenarioId::ReconnectTarget(_)
                | ConformanceScenarioId::ReconnectSourcePrefixWait
                | ConformanceScenarioId::ReconnectTargetPrefixWait
        ) {
            cx.read_entity(&host, |host, _| {
                set_selection_by_id(&host.view, Vec::new(), vec![canonical_edge()])
                    .expect("seed reconnect edge selection");
            });
        }
        draw(cx);
        draw(cx);
        if query_schedule == CanonicalQuerySchedule::OnePagePerNativeFrame {
            cx.update_entity(&host, |host, _| {
                let session = jellyflow_runtime::interaction::CanvasInteractionSession::attach(
                    &host.view,
                    jellyflow_runtime::interaction::CanvasInteractionConfig {
                        query_work_budget: 1,
                        ..jellyflow_runtime::interaction::CanvasInteractionConfig::default()
                    },
                )
                .expect("attach paged canonical interaction session");
                host.interaction_owner.test_replace_session(session);
            });
            cx.read_entity(&host, |host, _| {
                let geometry = host.view.geometry_snapshot().expect("paged probe geometry");
                let target = geometry
                    .port(target_port())
                    .and_then(|port| port.hit_bounds())
                    .expect("paged target hit bounds");
                let alternate = geometry
                    .port(alternate_target_port())
                    .and_then(|port| port.hit_bounds())
                    .expect("paged alternate target hit bounds");
                assert_eq!(target, alternate, "paged probe targets must overlap");
            });
        }
        let baseline_source_position = runtime
            .snapshot()
            .expect("canonical Open GPUI source baseline")
            .graph()
            .node(&source_node())
            .expect("canonical Open GPUI source node")
            .pos;
        let baseline_source_bounds = cx.read_entity(&host, |host, _| {
            host.view
                .geometry_snapshot()
                .expect("canonical Open GPUI geometry baseline")
                .node(source_node())
                .expect("canonical Open GPUI source geometry")
                .bounds()
        });

        (
            Self {
                id: scenario.id(),
                runtime,
                host,
                baseline_revision,
                excluded_revision_delta: 0,
                pointer: None,
                last_effect: ConformanceEffect::None,
                control_hits,
                prefix_permit: None,
                foreign_capture: None,
                previous_capture: None,
                baseline_source_position,
                baseline_source_bounds,
                query_schedule,
                max_native_query_frames: 0,
            },
            cx,
        )
    }

    fn perform(&mut self, action: ConformanceAction, cx: &mut open_gpui::VisualTestContext) {
        let before = self.effect_evidence(cx);
        match action {
            ConformanceAction::Press(target) => self.press(target, cx),
            ConformanceAction::CrossDragThreshold => {
                let position = if matches!(
                    self.id,
                    ConformanceScenarioId::NodeDrag(_) | ConformanceScenarioId::NodeDragPrefixWait
                ) {
                    let start = self.pointer.expect("node drag press position");
                    point(start.x + px(48.0), start.y + px(36.0))
                } else {
                    point(px(280.0), px(430.0))
                };
                self.move_pointer(position, cx);
                self.pointer = Some(position);
            }
            ConformanceAction::MoveToCandidate(candidate) => {
                let position = self.candidate_position(candidate, cx);
                self.move_pointer(position, cx);
                self.pointer = Some(position);
            }
            ConformanceAction::Release => self.release(cx),
            ConformanceAction::SelectSearchCandidate => {
                cx.update_window_entity(&self.host, |host, window, cx| {
                    let request = host
                        .connection_search_request()
                        .expect("active canonical search request");
                    let candidates = host
                        .connection_search_candidates(&request, window, cx)
                        .expect("canonical search candidates");
                    assert!(!candidates.candidates().is_empty());
                    host.select_connection_search_candidate(&request, &candidates, 0, window, cx)
                        .expect("select canonical search candidate");
                });
            }
            ConformanceAction::QueuePrefix => self.queue_prefix(cx),
            ConformanceAction::DrainPrefix => self.drain_prefix(cx),
            ConformanceAction::AdvanceFrame => {
                draw_once(cx);
                self.prepare_injected_receipt(cx);
            }
            ConformanceAction::CompleteCommand(settlement) => {
                if settlement == ConformanceCommandOutcome::NotDispatched {
                    self.settle_not_dispatched(cx);
                }
                self.settle_retained_frames(cx);
            }
            ConformanceAction::LoseCapture => self.lose_capture(cx),
            ConformanceAction::ReplaceOwner => self.replace_owner(cx),
            ConformanceAction::ReleasePreviousOwner => self.release_previous_owner(cx),
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
        let native_query_frames = self.drain_native_query_frames(cx);
        self.max_native_query_frames = self.max_native_query_frames.max(native_query_frames);
        let after = self.effect_evidence(cx);
        self.last_effect = effect_from_transition(before, after);
    }

    fn drain_native_query_frames(&self, cx: &mut open_gpui::VisualTestContext) -> usize {
        if self.query_schedule == CanonicalQuerySchedule::Default {
            return 0;
        }

        let mut frames = 0;
        while cx.read_entity(&self.host, |host, _| {
            host.interaction_owner.test_query_work_pending()
        }) {
            assert!(
                frames < PAGED_QUERY_NATIVE_FRAME_LIMIT,
                "paged canonical query did not settle within its native-frame bound",
            );
            draw_once(cx);
            frames += 1;
        }
        frames
    }

    fn move_pointer(
        &self,
        position: open_gpui::Point<open_gpui::Pixels>,
        cx: &mut open_gpui::VisualTestContext,
    ) {
        if self.query_schedule == CanonicalQuerySchedule::OnePagePerNativeFrame {
            dispatch_mouse_move(position, cx);
        } else {
            cx.simulate_mouse_move(position, Some(MouseButton::Left), Default::default());
        }
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
            ConformanceTarget::ReconnectSource => cx
                .debug_bounds(&open_gpui_reconnect_hit_selector(
                    canonical_edge(),
                    EdgeEndpoint::From,
                ))
                .expect("canonical reconnect-source grip")
                .center(),
            ConformanceTarget::ReconnectTarget => cx
                .debug_bounds(&open_gpui_reconnect_hit_selector(
                    canonical_edge(),
                    EdgeEndpoint::To,
                ))
                .expect("canonical reconnect-target grip")
                .center(),
            ConformanceTarget::Edge => self.edge_endpoint_position(
                jellyflow_runtime::geometry::EdgePathAnchor::Progress(0.35),
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
            ConformanceScenarioId::CreateFromWire(_)
                | ConformanceScenarioId::CreateFromWirePrefixWait
                | ConformanceScenarioId::EdgePick
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
                ConformanceScenarioId::ReconnectSource(_)
                | ConformanceScenarioId::ReconnectSourcePrefixWait => {
                    self.port_position(alternate_target_port(), cx)
                }
                _ => self.port_position(invalid_output_port(), cx),
            },
            ConformanceCandidate::Valid => match self.id {
                ConformanceScenarioId::ReconnectSource(_)
                | ConformanceScenarioId::ReconnectSourcePrefixWait => {
                    self.port_position(alternate_source_port(), cx)
                }
                ConformanceScenarioId::ReconnectTarget(_)
                | ConformanceScenarioId::ReconnectTargetPrefixWait => {
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
        anchor: jellyflow_runtime::geometry::EdgePathAnchor,
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

    fn queue_prefix(&mut self, cx: &mut open_gpui::VisualTestContext) {
        assert!(
            self.prefix_permit.is_none(),
            "canonical prefix is already held"
        );
        let mut barrier = cx.read_entity(&self.host, |host, _| {
            host.commands
                .capture_prefix_barrier()
                .expect("capture canonical prefix")
        });
        let mut context = TaskContext::from_waker(Waker::noop());
        let permit = match Future::poll(Pin::new(&mut barrier), &mut context) {
            Poll::Ready(Ok(permit)) => permit,
            Poll::Ready(Err(error)) => panic!("canonical prefix failed: {error}"),
            Poll::Pending => panic!("empty canonical prefix must be ready"),
        };
        self.prefix_permit = Some(permit);
    }

    fn drain_prefix(&mut self, _cx: &mut open_gpui::VisualTestContext) {
        let permit = self
            .prefix_permit
            .take()
            .expect("canonical prefix must be queued before draining");
        let snapshot = self.runtime.snapshot().expect("canonical prefix snapshot");
        permit
            .enqueue_detached(
                GraphCommand::set_node_data(
                    snapshot.version(),
                    invalid_node(),
                    serde_json::json!({ "external": "prefix" }),
                ),
                OpenGpuiCommandOrigin::Binding,
            )
            .expect("enqueue canonical prefix command");
        self.excluded_revision_delta = self.excluded_revision_delta.saturating_add(1);
    }

    fn lose_capture(&mut self, cx: &mut open_gpui::VisualTestContext) {
        cx.update(|window, _| window.release_pointer());
        draw_once(cx);
        cx.run_until_parked();
    }

    fn replace_owner(&mut self, cx: &mut open_gpui::VisualTestContext) {
        let successor = HitboxId::placeholder();
        self.previous_capture = cx.update(|window, _| {
            let previous = window.captured_hitbox();
            window.capture_pointer(successor);
            previous
        });
        assert!(self.previous_capture.is_some());
        self.foreign_capture = Some(successor);
        draw_once(cx);
        cx.run_until_parked();
    }

    fn release_previous_owner(&self, cx: &mut open_gpui::VisualTestContext) {
        let successor = self
            .foreign_capture
            .expect("owner replacement installs a successor capture");
        let previous = self
            .previous_capture
            .expect("owner replacement remembers the previous capture");
        assert!(!cx.update(|window, _| { release_pointer_capture_if_owned(previous, window) }));
        assert_eq!(
            cx.update(|window, _| window.captured_hitbox()),
            Some(successor)
        );
    }

    fn prepare_injected_receipt(&mut self, cx: &mut open_gpui::VisualTestContext) {
        let Some(settlement) = scenario_command_outcome(self.id) else {
            return;
        };
        if settlement != ConformanceCommandOutcome::Conflict {
            return;
        }

        let ticket = cx.update_entity(&self.host, |host, _| {
            assert!(
                host.interaction_owner.test_has_pending_submission(),
                "a native release must reach the command prefix boundary"
            );
            let presentation = host.interaction_presentation();
            presentation
                .connection()
                .and_then(|connection| connection.ticket())
                .or_else(|| presentation.node_drag().and_then(|drag| drag.ticket()))
                .expect("pending canonical interaction ticket")
        });
        let snapshot = self.runtime.snapshot().expect("receipt injection snapshot");
        let expected_version = snapshot.version();
        let command = match settlement {
            ConformanceCommandOutcome::Conflict => {
                let (bound_ticket, command) = cx.update_entity(&self.host, |host, _| {
                    host.bind_pending_interaction_submission_for_execution()
                        .expect("canonical conflict binds the real pending submission")
                });
                assert_eq!(bound_ticket, ticket);
                self.runtime
                    .execute(GraphCommand::set_node_data(
                        expected_version,
                        invalid_node(),
                        serde_json::json!({ "external": "conflict" }),
                    ))
                    .into_result()
                    .expect("external conflict command")
                    .applied()
                    .expect("external conflict setup applies");
                self.excluded_revision_delta = self.excluded_revision_delta.saturating_add(1);
                command.expect("canonical conflict mutation is not a no-op")
            }
            _ => unreachable!("only conflict reaches command injection"),
        };
        cx.update_entity(&self.host, |host, _| {
            host.commands
                .enqueue_detached(command, OpenGpuiCommandOrigin::Interaction { ticket })
                .expect("inject canonical command receipt");
        });
    }

    fn settle_not_dispatched(&self, cx: &mut open_gpui::VisualTestContext) {
        let ticket = cx.update_entity(&self.host, |host, _| {
            assert!(
                host.interaction_owner.test_has_pending_submission(),
                "a native release must remain pending before an undispatched settlement"
            );
            let presentation = host.interaction_presentation();
            presentation
                .connection()
                .and_then(|connection| connection.ticket())
                .or_else(|| presentation.node_drag().and_then(|drag| drag.ticket()))
                .expect("pending canonical interaction ticket")
        });
        cx.update_entity(&self.host, |host, _| {
            host.interaction_submission_not_dispatched(ticket);
        });
    }

    fn retire_source(&mut self, cx: &mut open_gpui::VisualTestContext) {
        let version = self
            .runtime
            .snapshot()
            .expect("source retirement snapshot")
            .version();
        self.runtime
            .execute(GraphCommand::topology(
                version,
                TopologyEdit::new().remove_node(source_node(), IncidentEdgePolicy::Reject),
            ))
            .into_result()
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
            ConformanceScenarioId::NewConnection(_)
            | ConformanceScenarioId::NewConnectionPrefixWait => {
                let edge = snapshot
                    .graph()
                    .edges()
                    .values()
                    .next()
                    .expect("new connection edge");
                assert_eq!((edge.from, edge.to), (source_port(), target_port()));
            }
            ConformanceScenarioId::ReconnectSource(_)
            | ConformanceScenarioId::ReconnectSourcePrefixWait => {
                let edge = snapshot
                    .graph()
                    .edge(&canonical_edge())
                    .expect("source-reconnected edge retains identity");
                assert_eq!(
                    (edge.from, edge.to),
                    (alternate_source_port(), target_port()),
                    "canonical scenario {:?}",
                    self.id,
                );
            }
            ConformanceScenarioId::ReconnectTarget(_)
            | ConformanceScenarioId::ReconnectTargetPrefixWait => {
                let edge = snapshot
                    .graph()
                    .edge(&canonical_edge())
                    .expect("target-reconnected edge retains identity");
                assert_eq!(
                    (edge.from, edge.to),
                    (source_port(), alternate_target_port()),
                    "canonical scenario {:?}",
                    self.id,
                );
            }
            ConformanceScenarioId::CreateFromWire(_)
            | ConformanceScenarioId::CreateFromWirePrefixWait => {
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
            ConformanceScenarioId::NodeDragPrefixWait => {
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
                connection_pointer: connection.map(|connection| connection.pointer_canvas()),
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
        let captured = cx.update(|window, _| window.captured_hitbox());
        let graph = self.observed_graph();
        let geometry = self.observed_geometry(graph, cx);
        cx.read_entity(&self.host, |host, _| {
            let presentation = host.interaction_presentation();
            let ticket = presentation
                .connection()
                .and_then(|connection| connection.ticket())
                .or_else(|| presentation.node_drag().and_then(|drag| drag.ticket()));
            let mut observation = NormalizedObservation::new(
                normalized_phase(&presentation),
                self.last_effect,
                match captured {
                    None => ConformanceOwnership::Released,
                    Some(owner) if Some(owner) == self.foreign_capture => {
                        ConformanceOwnership::HeldBySuccessor
                    }
                    Some(_) => ConformanceOwnership::HeldByAdapter,
                },
                if ticket.is_some() {
                    ConformanceTicket::Pending
                } else {
                    ConformanceTicket::None
                },
                self.runtime
                    .snapshot()
                    .expect("canonical graph snapshot")
                    .revision()
                    .saturating_sub(self.baseline_revision)
                    .saturating_sub(self.excluded_revision_delta),
                geometry,
            );
            observation.optimistic_presentation = ticket.is_some()
                && (presentation.connection().is_some() || presentation.node_drag().is_some());
            observation.terminal = presentation.terminal().map(normalized_terminal);
            observation.intent = presentation
                .connection_intent()
                .map_or(ConformanceIntent::None, normalized_intent);
            observation.graph = graph;
            observation.selection = match host.view.selection() {
                Ok(selection)
                    if selection.nodes().is_empty()
                        && selection.edges().is_empty()
                        && selection.groups().is_empty() =>
                {
                    ConformanceSelection::Empty
                }
                Ok(selection)
                    if selection.nodes() == [source_node()]
                        && selection.edges().is_empty()
                        && selection.groups().is_empty() =>
                {
                    ConformanceSelection::SourceNode
                }
                Ok(selection)
                    if selection.nodes().is_empty()
                        && selection.edges() == [canonical_edge()]
                        && selection.groups().is_empty() =>
                {
                    ConformanceSelection::CanonicalEdge
                }
                Ok(selection) => {
                    panic!("unexpected canonical Open GPUI selection: {selection:?}")
                }
                Err(_) => ConformanceSelection::Unavailable,
            };
            observation.view_status = match host.view.close_reason() {
                None => ConformanceViewStatus::Open,
                Some(ViewCloseReason::HostClosed) => ConformanceViewStatus::HostClosed,
                Some(ViewCloseReason::RuntimeDropped) => ConformanceViewStatus::RuntimeDropped,
            };
            observation
        })
    }

    fn observed_graph(&self) -> ConformanceGraph {
        let snapshot = self
            .runtime
            .snapshot()
            .expect("canonical Open GPUI graph remains available");
        let graph = snapshot.graph();
        if graph.node(&source_node()).is_none() {
            assert!(graph.port(&source_port()).is_none());
            return ConformanceGraph::SourceRetired;
        }
        if let Some(edge) = graph.edge(&canonical_edge()) {
            match (edge.from, edge.to) {
                (from, to) if from == alternate_source_port() && to == target_port() => {
                    return ConformanceGraph::ReconnectSource;
                }
                (from, to) if from == source_port() && to == alternate_target_port() => {
                    return ConformanceGraph::ReconnectTarget;
                }
                (from, to) if from == source_port() && to == target_port() => {}
                endpoints => {
                    panic!("unexpected canonical Open GPUI reconnect endpoints: {endpoints:?}")
                }
            }
        }
        let mut new_edges = graph
            .edges()
            .iter()
            .filter(|(edge, _)| **edge != canonical_edge());
        if let Some((_, edge)) = new_edges.next() {
            assert!(new_edges.next().is_none());
            return match (edge.from, edge.to) {
                (from, to) if from == source_port() && to == target_port() => {
                    ConformanceGraph::NewConnection
                }
                (from, to)
                    if from == source_port()
                        && graph
                            .port(&to)
                            .is_some_and(|port| port.node != target_node()) =>
                {
                    assert_eq!(graph.nodes().len(), 6);
                    ConformanceGraph::CreateFromWire
                }
                endpoints => {
                    panic!("unexpected canonical Open GPUI new edge endpoints: {endpoints:?}")
                }
            };
        }
        match graph.node(&source_node()) {
            Some(source) if source.pos != self.baseline_source_position => {
                let expected = CanvasPoint {
                    x: self.baseline_source_position.x + 48.0,
                    y: self.baseline_source_position.y + 36.0,
                };
                assert!((source.pos.x - expected.x).abs() < 0.001);
                assert!((source.pos.y - expected.y).abs() < 0.001);
                ConformanceGraph::NodeMoved
            }
            Some(_) | None => ConformanceGraph::Baseline,
        }
    }

    fn observed_geometry(
        &self,
        graph: ConformanceGraph,
        cx: &mut open_gpui::VisualTestContext,
    ) -> ConformanceGeometry {
        cx.read_entity(&self.host, |host, _| {
            let Ok(geometry) = host.view.geometry_snapshot() else {
                return ConformanceGeometry::Unavailable;
            };
            for port in [
                target_port(),
                alternate_target_port(),
                alternate_source_port(),
            ] {
                let published = geometry.port(port).unwrap_or_else(|| {
                    panic!("canonical Open GPUI geometry is missing port {port:?}")
                });
                assert!(published.hit_bounds().is_some());
            }
            match graph {
                ConformanceGraph::Baseline => {
                    if host
                        .view
                        .graph_snapshot()
                        .unwrap()
                        .graph()
                        .edge(&canonical_edge())
                        .is_some()
                    {
                        assert_route_matches(
                            &geometry,
                            canonical_edge(),
                            source_port(),
                            target_port(),
                        );
                    }
                    ConformanceGeometry::Baseline
                }
                ConformanceGraph::SourceRetired => {
                    assert!(geometry.node(source_node()).is_none());
                    assert!(geometry.port(source_port()).is_none());
                    ConformanceGeometry::SourceRetired
                }
                ConformanceGraph::NewConnection => {
                    let snapshot = host.view.graph_snapshot().unwrap();
                    let edge = snapshot
                        .graph()
                        .edges()
                        .iter()
                        .find_map(|(id, edge)| {
                            (edge.from == source_port() && edge.to == target_port()).then_some(*id)
                        })
                        .expect("canonical Open GPUI new edge");
                    assert_route_matches(&geometry, edge, source_port(), target_port());
                    ConformanceGeometry::NewConnectionRouted
                }
                ConformanceGraph::ReconnectSource => {
                    assert_route_matches(
                        &geometry,
                        canonical_edge(),
                        alternate_source_port(),
                        target_port(),
                    );
                    ConformanceGeometry::ReconnectSourceRouted
                }
                ConformanceGraph::ReconnectTarget => {
                    assert_route_matches(
                        &geometry,
                        canonical_edge(),
                        source_port(),
                        alternate_target_port(),
                    );
                    ConformanceGeometry::ReconnectTargetRouted
                }
                ConformanceGraph::CreateFromWire => {
                    let snapshot = host.view.graph_snapshot().unwrap();
                    let (edge, target) = snapshot
                        .graph()
                        .edges()
                        .iter()
                        .find_map(|(id, edge)| {
                            (edge.from == source_port() && edge.to != target_port())
                                .then_some((*id, edge.to))
                        })
                        .expect("canonical Open GPUI create-from-wire edge");
                    assert!(geometry.port(target).is_none());
                    assert!(geometry.route(edge).is_none());
                    ConformanceGeometry::CreateFromWireAwaitingLayout
                }
                ConformanceGraph::NodeMoved => {
                    let snapshot = host.view.graph_snapshot().unwrap();
                    let source_position = snapshot.graph().node(&source_node()).unwrap().pos;
                    let source_bounds = geometry.node(source_node()).unwrap().bounds();
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
                    ConformanceGeometry::NodeMoved
                }
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
        if let Some(successor) = self.foreign_capture {
            cx.update(|window, _| {
                assert_eq!(window.captured_hitbox(), Some(successor));
                window.release_pointer();
            });
        }
    }
}

fn normalized_phase(
    presentation: &jellyflow_runtime::interaction::CanvasInteractionPresentation,
) -> ConformancePhase {
    match presentation.phase() {
        CanvasInteractionPhase::Idle | CanvasInteractionPhase::Closed => ConformancePhase::Idle,
        CanvasInteractionPhase::Armed => ConformancePhase::Armed,
        CanvasInteractionPhase::NodeDrag => ConformancePhase::NodeDrag,
        CanvasInteractionPhase::ConnectionDrag
            if presentation
                .connection()
                .is_some_and(|connection| connection.probe().is_committable()) =>
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
        InteractionTerminalReason::Noop => {
            panic!("canonical Open GPUI trace does not model a no-op command")
        }
        InteractionTerminalReason::Rejected => ConformanceTerminal::Rejected,
        InteractionTerminalReason::Conflict => ConformanceTerminal::Conflict,
        InteractionTerminalReason::TransportFailure => ConformanceTerminal::NotDispatched,
        InteractionTerminalReason::Cancelled => ConformanceTerminal::Cancelled,
        InteractionTerminalReason::OwnerRetired => ConformanceTerminal::OwnerRetired,
        InteractionTerminalReason::ViewClosed => ConformanceTerminal::HostClosed,
        other => panic!("unsupported canonical terminal: {other:?}"),
    }
}

fn normalized_intent(binding: ConnectionGestureBinding) -> ConformanceIntent {
    if binding.reconnect_edge().is_none() {
        return ConformanceIntent::Connect;
    }
    match binding.fixed_endpoint() {
        EdgeEndpoint::To => ConformanceIntent::ReconnectSource,
        EdgeEndpoint::From => ConformanceIntent::ReconnectTarget,
    }
}

fn assert_route_matches(
    geometry: &jellyflow_runtime::view::GeometrySnapshot,
    edge: EdgeId,
    from: PortId,
    to: PortId,
) {
    let route = geometry
        .route(edge)
        .unwrap_or_else(|| panic!("canonical Open GPUI geometry is missing route {edge:?}"));
    let source = geometry
        .port(from)
        .expect("Open GPUI route source geometry");
    let target = geometry.port(to).expect("Open GPUI route target geometry");
    assert_eq!(route.facts().endpoints.source.point, source.route_anchor());
    assert_eq!(route.facts().endpoints.target.point, target.route_anchor());
}

fn scenario_command_outcome(id: ConformanceScenarioId) -> Option<ConformanceCommandOutcome> {
    match id {
        ConformanceScenarioId::NewConnection(settlement)
        | ConformanceScenarioId::ReconnectSource(settlement)
        | ConformanceScenarioId::ReconnectTarget(settlement)
        | ConformanceScenarioId::CreateFromWire(settlement)
        | ConformanceScenarioId::NodeDrag(settlement) => Some(settlement),
        ConformanceScenarioId::NewConnectionPrefixWait
        | ConformanceScenarioId::NewConnectionProbePrefix
        | ConformanceScenarioId::ReconnectSourcePrefixWait
        | ConformanceScenarioId::ReconnectTargetPrefixWait
        | ConformanceScenarioId::CreateFromWirePrefixWait
        | ConformanceScenarioId::NodeDragPrefixWait => Some(ConformanceCommandOutcome::Applied),
        ConformanceScenarioId::NodePressBelowThreshold
        | ConformanceScenarioId::NativeControl
        | ConformanceScenarioId::EdgePick
        | ConformanceScenarioId::CaptureLost
        | ConformanceScenarioId::OwnerReplaced
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

fn dispatch_mouse_move(
    position: open_gpui::Point<open_gpui::Pixels>,
    cx: &mut open_gpui::VisualTestContext,
) {
    cx.update(|window, cx| {
        window.dispatch_event(
            MouseMoveEvent {
                position,
                modifiers: Default::default(),
                pressed_button: Some(MouseButton::Left),
            }
            .to_platform_input(),
            cx,
        );
    });
}

fn run_canonical_scenario(scenario: &ConformanceScenario, cx: &mut open_gpui::TestAppContext) {
    run_canonical_scenario_with_query_schedule(scenario, CanonicalQuerySchedule::Default, cx);
}

fn run_canonical_scenario_with_query_schedule(
    scenario: &ConformanceScenario,
    query_schedule: CanonicalQuerySchedule,
    cx: &mut open_gpui::TestAppContext,
) {
    let (mut driver, cx) = if query_schedule == CanonicalQuerySchedule::Default {
        CanonicalDriver::mount(scenario, cx)
    } else {
        CanonicalDriver::mount_with_query_schedule(scenario, query_schedule, cx)
    };
    let mut observations = Vec::with_capacity(scenario.steps().len());
    for step in scenario.steps() {
        driver.perform(step.action(), cx);
        observations.push(driver.observe(cx));
    }
    compare_observations(scenario, &observations)
        .unwrap_or_else(|error| panic!("Open GPUI canonical mismatch: {error}"));
    if query_schedule == CanonicalQuerySchedule::Default
        && scenario_command_outcome(scenario.id()) == Some(ConformanceCommandOutcome::Applied)
    {
        driver.assert_applied_mutation();
    }
    if query_schedule == CanonicalQuerySchedule::OnePagePerNativeFrame {
        assert!(
            driver.max_native_query_frames >= 1,
            "overlapping targets must require at least one continuation frame after the first page",
        );
        assert!(driver.max_native_query_frames < PAGED_QUERY_NATIVE_FRAME_LIMIT);
    }
    driver.close(cx);
}

fn connection_probe_prefix_scenario() -> ConformanceScenario {
    new_connection_probe_prefix_scenario()
}

#[open_gpui::test]
fn canonical_scenarios_run_through_real_open_gpui_events_and_frames(
    cx: &mut open_gpui::TestAppContext,
) {
    for scenario in canonical_scenarios() {
        run_canonical_scenario(&scenario, cx);
    }
}

#[open_gpui::test]
fn one_page_open_gpui_frames_match_the_canonical_connection_probe_prefix(
    cx: &mut open_gpui::TestAppContext,
) {
    run_canonical_scenario_with_query_schedule(
        &connection_probe_prefix_scenario(),
        CanonicalQuerySchedule::OnePagePerNativeFrame,
        cx,
    );
}

fn canonical_graph(with_edge: bool, overlap_valid_targets: bool) -> jellyflow_core::Graph {
    let mut graph = GraphBuilder::new(GraphId::from_u128(0xCA11));
    let alternate_target_position = if overlap_valid_targets {
        CanvasPoint { x: 360.0, y: 80.0 }
    } else {
        CanvasPoint { x: 620.0, y: 80.0 }
    };
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
            alternate_target_position,
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
    if overlap_valid_targets {
        for index in 0..6_u128 {
            let node_id = NodeId::from_u128(1_000 + index);
            let port_id = PortId::from_u128(2_000 + index);
            graph.insert_node(node_id, node(CanvasPoint { x: 360.0, y: 80.0 }, port_id));
            graph.insert_port(port_id, port(node_id, "paged-target", PortDirection::In));
        }
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
