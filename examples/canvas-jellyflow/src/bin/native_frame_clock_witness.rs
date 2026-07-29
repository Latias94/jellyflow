use std::cell::Cell;
use std::process::ExitCode;
use std::rc::Rc;
use std::time::Duration;

use jellyflow_core::CanvasPoint;
use jellyflow_open_gpui::host::{OpenGpuiSceneHost, request_native_frame};
use jellyflow_runtime::geometry::{ViewPoint, ViewSize};
use jellyflow_runtime::graph::GraphRuntime;
use jellyflow_runtime::interaction::CanvasInteractionPhase;
use jellyflow_runtime::view::{ViewSessionOptions, WeakViewSession};
use open_gpui::{
    AnyWindowHandle, App, AppContext, Bounds, Context, Entity, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, ParentElement, PlatformInput, QuitMode, Render, Styled, Task,
    Window, WindowBounds, WindowOptions, div, point, px, size,
};
use open_gpui_canvas_jellyflow::nodes::{
    NativeGraphVisualProfile, NativeNodeResources, native_node_catalog, native_node_registry,
    workflow_demo_graph, workflow_llm_node_id,
};
use open_gpui_platform::application;
use open_gpui_ui_components::ThemeMode;
use open_gpui_ui_components::gpui_adapter::init_text_input;
use open_gpui_ui_components::theme::set_active_theme_mode;

const WINDOW_WIDTH: f32 = 800.0;
const WINDOW_HEIGHT: f32 = 600.0;
const EDGE_INSET: f32 = 12.0;
const POLL_INTERVAL: Duration = Duration::from_millis(20);
const PROBE_TIMEOUT: Duration = Duration::from_secs(12);
const REQUIRED_PAN_SAMPLES: u8 = 3;
const MAX_POST_RELEASE_SETTLE_FRAMES: u8 = 16;
const MAX_POST_RELEASE_REDRAW_REQUESTS: u16 = 256;
const REQUIRED_POST_RELEASE_FRAMES: usize = 5;
const PAN_EPSILON: f32 = 0.001;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WitnessOutcome {
    Pending,
    Passed,
    Failed,
}

struct WitnessScene {
    _runtime: Rc<GraphRuntime>,
    host: Entity<OpenGpuiSceneHost>,
}

impl Render for WitnessScene {
    fn render(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        div().size_full().child(self.host.clone())
    }
}

#[derive(Clone, Copy, Debug)]
struct DragPlan {
    press: ViewPoint,
    activation: ViewPoint,
    edge: ViewPoint,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WitnessPhase {
    WaitingForLayout,
    WaitingForArmed,
    WaitingForDrag,
    ObservingNativeFrames,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct FrameObservation {
    phase: CanvasInteractionPhase,
    pan_x: f32,
    pan_y: f32,
    zoom: f32,
    callback_entries: u64,
    callback_registrations: u64,
    frame_requests: u64,
    callback_epoch: u64,
    pending_queries: usize,
    pending_submission: bool,
    ready_prefixes: usize,
    tick_scheduled: bool,
    tick_frame_requested: bool,
    query_frame_scheduled: bool,
    query_resolution_active: bool,
    native_tick_wake_active: bool,
    native_query_wake_active: bool,
}

#[derive(Clone, Copy, Debug)]
struct WitnessEvidence {
    callback_entries: u64,
    callback_registrations: u64,
    frame_requests: u64,
    pan_samples: u8,
    pan_delta_x: f32,
}

struct PostReleaseNativeFrameObserver {
    host: Entity<OpenGpuiSceneHost>,
    view: WeakViewSession,
    outcome: Rc<Cell<WitnessOutcome>>,
    active_evidence: WitnessEvidence,
    settle_frames: u8,
    observations: Vec<FrameObservation>,
    frame_driver: PostReleaseNativeFrameDriver,
}

struct PostReleaseNativeFrameDriver {
    active: Rc<Cell<bool>>,
    requests: Rc<Cell<u16>>,
    task: Option<Task<()>>,
}

enum WitnessStep {
    Continue,
    ObservationTransferred,
}

struct NativeFrameClockProbe {
    phase: WitnessPhase,
    plan: Option<DragPlan>,
}

impl NativeFrameClockProbe {
    fn new() -> Self {
        Self {
            phase: WitnessPhase::WaitingForLayout,
            plan: None,
        }
    }

    fn advance(
        &mut self,
        host: &Entity<OpenGpuiSceneHost>,
        view: &WeakViewSession,
        window: &mut Window,
        cx: &mut App,
        outcome: &Rc<Cell<WitnessOutcome>>,
    ) -> Result<WitnessStep, String> {
        match self.phase {
            WitnessPhase::WaitingForLayout => {
                let Some(plan) = drag_plan(host, view, cx)? else {
                    window.refresh();
                    return Ok(WitnessStep::Continue);
                };
                window.dispatch_event(mouse_down(plan.press), cx);
                window.refresh();
                self.plan = Some(plan);
                self.phase = WitnessPhase::WaitingForArmed;
                Ok(WitnessStep::Continue)
            }
            WitnessPhase::WaitingForArmed => {
                let phase = host.read(cx).interaction_presentation().phase();
                if phase == CanvasInteractionPhase::Armed {
                    let plan = self.plan.ok_or("drag plan disappeared after mouse down")?;
                    window.dispatch_event(mouse_move(plan.activation), cx);
                    window.refresh();
                    self.phase = WitnessPhase::WaitingForDrag;
                } else if phase != CanvasInteractionPhase::Idle {
                    return Err(format!("unexpected phase after mouse down: {phase:?}"));
                }
                Ok(WitnessStep::Continue)
            }
            WitnessPhase::WaitingForDrag => {
                let phase = host.read(cx).interaction_presentation().phase();
                if phase == CanvasInteractionPhase::NodeDrag {
                    let plan = self.plan.ok_or("drag plan disappeared after activation")?;
                    let baseline = frame_observation(host, view, cx)?;
                    window.dispatch_event(mouse_move(plan.edge), cx);
                    self.phase = WitnessPhase::ObservingNativeFrames;
                    schedule_native_frame_observer(
                        window,
                        NativeFrameObserver {
                            host: host.clone(),
                            view: view.clone(),
                            outcome: Rc::clone(outcome),
                            release: plan.edge,
                            baseline,
                            last: baseline,
                            pan_samples: 0,
                        },
                    );
                    return Ok(WitnessStep::ObservationTransferred);
                } else if phase != CanvasInteractionPhase::Armed {
                    return Err(format!("unexpected phase after drag activation: {phase:?}"));
                }
                Ok(WitnessStep::Continue)
            }
            WitnessPhase::ObservingNativeFrames => Err(
                "timer probe retained ownership after transferring to native frame callbacks"
                    .to_owned(),
            ),
        }
    }
}

struct NativeFrameObserver {
    host: Entity<OpenGpuiSceneHost>,
    view: WeakViewSession,
    outcome: Rc<Cell<WitnessOutcome>>,
    release: ViewPoint,
    baseline: FrameObservation,
    last: FrameObservation,
    pan_samples: u8,
}

impl NativeFrameObserver {
    fn observe(mut self, window: &mut Window, cx: &mut App) {
        if self.outcome.get() != WitnessOutcome::Pending {
            return;
        }
        match self.observe_current_frame(cx) {
            Ok(Some(evidence)) => {
                window.dispatch_event(mouse_up(self.release), cx);
                let frame_driver =
                    PostReleaseNativeFrameDriver::start(window, cx, Rc::clone(&self.outcome));
                schedule_post_release_native_frame(
                    window,
                    PostReleaseNativeFrameObserver {
                        host: self.host,
                        view: self.view,
                        outcome: self.outcome,
                        active_evidence: evidence,
                        settle_frames: 0,
                        observations: Vec::with_capacity(REQUIRED_POST_RELEASE_FRAMES),
                        frame_driver,
                    },
                );
            }
            Ok(None) => schedule_native_frame_observer(window, self),
            Err(reason) => {
                eprintln!("native frame clock witness failed: {reason}");
                self.outcome.set(WitnessOutcome::Failed);
                cx.quit();
            }
        }
    }

    fn observe_current_frame(&mut self, cx: &App) -> Result<Option<WitnessEvidence>, String> {
        let current = frame_observation(&self.host, &self.view, cx)?;
        if current.phase != CanvasInteractionPhase::NodeDrag {
            return Err(format!(
                "node drag ended before native frame proof: {:?}",
                current.phase
            ));
        }
        if std::env::var_os("JELLYFLOW_FRAME_WITNESS_TRACE").is_some() {
            eprintln!(
                "native frame clock witness trace: phase={:?}, pan_x={}, entries={}, \
                 registrations={}, frame_requests={}, samples={}",
                current.phase,
                current.pan_x,
                current.callback_entries,
                current.callback_registrations,
                current.frame_requests,
                self.pan_samples,
            );
        }
        if current.pan_x + PAN_EPSILON < self.last.pan_x {
            return Err(format!(
                "left-edge auto-pan reversed direction: previous={}, current={}",
                self.last.pan_x, current.pan_x
            ));
        }
        if current.callback_entries > self.last.callback_entries
            && current.pan_x > self.last.pan_x + PAN_EPSILON
        {
            self.pan_samples = self.pan_samples.saturating_add(1);
            self.last = current;
        }
        if self.pan_samples < REQUIRED_PAN_SAMPLES {
            return Ok(None);
        }
        if current.callback_entries <= self.baseline.callback_entries {
            return Err("production frame callback never entered".to_owned());
        }
        if current.callback_registrations <= self.baseline.callback_registrations {
            return Err("host did not register a production frame callback".to_owned());
        }
        if current.frame_requests <= self.baseline.frame_requests {
            return Err("host did not request successor animation frames".to_owned());
        }
        Ok(Some(WitnessEvidence {
            callback_entries: current.callback_entries,
            callback_registrations: current.callback_registrations,
            frame_requests: current.frame_requests,
            pan_samples: self.pan_samples,
            pan_delta_x: current.pan_x - self.baseline.pan_x,
        }))
    }
}

fn schedule_native_frame_observer(window: &Window, observer: NativeFrameObserver) {
    window.on_next_frame(move |window, cx| observer.observe(window, cx));
}

fn schedule_post_release_native_frame(window: &Window, observer: PostReleaseNativeFrameObserver) {
    window.on_next_frame(move |window, cx| observer.observe(window, cx));
}

impl PostReleaseNativeFrameDriver {
    fn start(window: &Window, cx: &App, outcome: Rc<Cell<WitnessOutcome>>) -> Self {
        let active = Rc::new(Cell::new(true));
        let requests = Rc::new(Cell::new(0_u16));
        let task_active = Rc::clone(&active);
        let task_requests = Rc::clone(&requests);
        let task = window.spawn(cx, async move |cx| {
            for _ in 0..MAX_POST_RELEASE_REDRAW_REQUESTS {
                cx.background_executor().timer(POLL_INTERVAL).await;
                if !task_active.get() || outcome.get() != WitnessOutcome::Pending {
                    return;
                }
                task_requests.set(task_requests.get().saturating_add(1));
                if cx.update(|window, _| request_native_frame(window)).is_err() {
                    eprintln!("native frame clock witness redraw driver lost its window");
                    task_active.set(false);
                    outcome.set(WitnessOutcome::Failed);
                    return;
                }
            }
            if task_active.replace(false) && outcome.get() == WitnessOutcome::Pending {
                eprintln!(
                    "native frame clock witness exhausted {MAX_POST_RELEASE_REDRAW_REQUESTS} \
                     post-release redraw requests"
                );
                outcome.set(WitnessOutcome::Failed);
                let _ = cx.update(|_, cx| cx.quit());
            }
        });
        Self {
            active,
            requests,
            task: Some(task),
        }
    }

    fn stop(&mut self) -> u16 {
        self.active.set(false);
        self.task = None;
        self.requests.get()
    }

    fn is_active(&self) -> bool {
        self.active.get() || self.task.is_some()
    }
}

impl PostReleaseNativeFrameObserver {
    fn observe(mut self, window: &mut Window, cx: &mut App) {
        if self.outcome.get() != WitnessOutcome::Pending {
            return;
        }
        let current = match frame_observation(&self.host, &self.view, cx) {
            Ok(observation) => observation,
            Err(reason) => return self.fail(reason, cx),
        };
        if let Some(reason) = post_release_pending_work_error(&current) {
            if !matches!(
                current.phase,
                CanvasInteractionPhase::Committing | CanvasInteractionPhase::Idle
            ) {
                return self.fail(reason, cx);
            }
            self.settle_frames = self.settle_frames.saturating_add(1);
            if self.settle_frames > MAX_POST_RELEASE_SETTLE_FRAMES {
                return self.fail(
                    format!(
                        "post-release interaction did not settle within {} native frames: {reason}",
                        MAX_POST_RELEASE_SETTLE_FRAMES
                    ),
                    cx,
                );
            }
            schedule_post_release_native_frame(window, self);
            return;
        }

        self.observations.push(current);
        if self.observations.len() < REQUIRED_POST_RELEASE_FRAMES {
            schedule_post_release_native_frame(window, self);
            return;
        }

        match validate_post_release_observations(&self.observations) {
            Ok(()) => {
                let redraw_requests = self.frame_driver.stop();
                if self.frame_driver.is_active() {
                    return self.fail("post-release redraw driver did not stop".to_owned(), cx);
                }
                emit_pass_marker(
                    self.active_evidence,
                    self.settle_frames,
                    redraw_requests,
                    &self.observations,
                );
                self.outcome.set(WitnessOutcome::Passed);
            }
            Err(reason) => {
                eprintln!("native frame clock witness failed: {reason}");
                self.outcome.set(WitnessOutcome::Failed);
            }
        }
        cx.quit();
    }

    fn fail(&mut self, reason: String, cx: &mut App) {
        self.frame_driver.stop();
        eprintln!("native frame clock witness failed: {reason}");
        self.outcome.set(WitnessOutcome::Failed);
        cx.quit();
    }
}

fn post_release_pending_work_error(observation: &FrameObservation) -> Option<String> {
    if observation.phase != CanvasInteractionPhase::Idle {
        return Some(format!(
            "interaction phase is not idle after release: {:?}",
            observation.phase
        ));
    }
    let pending = [
        (observation.pending_queries != 0, "queries"),
        (observation.pending_submission, "submission"),
        (observation.ready_prefixes != 0, "ready-prefixes"),
        (observation.tick_scheduled, "tick-token"),
        (observation.tick_frame_requested, "tick-frame-request"),
        (observation.query_frame_scheduled, "query-token"),
        (observation.query_resolution_active, "query-resolution"),
        (observation.native_tick_wake_active, "native-tick-wake"),
        (observation.native_query_wake_active, "native-query-wake"),
    ]
    .into_iter()
    .filter_map(|(present, label)| present.then_some(label))
    .collect::<Vec<_>>();
    (!pending.is_empty())
        .then(|| format!("post-release work remains pending: {}", pending.join(",")))
}

fn validate_post_release_observations(observations: &[FrameObservation]) -> Result<(), String> {
    if observations.len() != REQUIRED_POST_RELEASE_FRAMES {
        return Err(format!(
            "post-release native-frame count is {}, expected {}",
            observations.len(),
            REQUIRED_POST_RELEASE_FRAMES
        ));
    }
    for (index, observation) in observations.iter().enumerate() {
        if let Some(reason) = post_release_pending_work_error(observation) {
            return Err(format!("post-release frame {index}: {reason}"));
        }
    }
    let baseline = observations[0];
    for (index, current) in observations.iter().copied().enumerate().skip(1) {
        let viewport_stable = (current.pan_x - baseline.pan_x).abs() <= PAN_EPSILON
            && (current.pan_y - baseline.pan_y).abs() <= PAN_EPSILON
            && (current.zoom - baseline.zoom).abs() <= PAN_EPSILON;
        if current.phase != baseline.phase
            || !viewport_stable
            || current.callback_entries != baseline.callback_entries
            || current.callback_registrations != baseline.callback_registrations
            || current.frame_requests != baseline.frame_requests
            || current.callback_epoch != baseline.callback_epoch
        {
            return Err(format!(
                "post-release frame {index} changed stable state: baseline={baseline:?}, current={current:?}"
            ));
        }
    }
    Ok(())
}

fn encode_post_release_sequence(observations: &[FrameObservation]) -> String {
    observations
        .iter()
        .enumerate()
        .map(|(index, observation)| {
            format!(
                "{index}:{:?}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}",
                observation.phase,
                observation.pan_x,
                observation.pan_y,
                observation.zoom,
                observation.callback_entries,
                observation.callback_registrations,
                observation.frame_requests,
                observation.callback_epoch,
                observation.pending_queries,
                u8::from(observation.pending_submission),
                observation.ready_prefixes,
                u8::from(observation.tick_scheduled),
                u8::from(observation.tick_frame_requested),
                u8::from(observation.query_frame_scheduled),
                u8::from(observation.query_resolution_active),
                u8::from(observation.native_tick_wake_active),
                u8::from(observation.native_query_wake_active),
            )
        })
        .collect::<Vec<_>>()
        .join(";")
}

fn emit_pass_marker(
    evidence: WitnessEvidence,
    settle_frames: u8,
    redraw_requests: u16,
    observations: &[FrameObservation],
) {
    let sequence = encode_post_release_sequence(observations);
    println!(
        "JELLYFLOW_NATIVE_FRAME_CLOCK_WITNESS status=pass test_support=false \
         native_frame_request=host-native-frame post_release_stable=true \
         post_release_settle_frames={settle_frames} \
         redraw_request_count={redraw_requests} redraw_driver_active=false \
         post_release_frames={} post_release_sequence={sequence} \
         callback_entries={} callback_registrations={} frame_requests={} \
         pan_samples={} pan_delta_x={}",
        observations.len(),
        evidence.callback_entries,
        evidence.callback_registrations,
        evidence.frame_requests,
        evidence.pan_samples,
        evidence.pan_delta_x,
    );
}

fn drag_plan(
    host: &Entity<OpenGpuiSceneHost>,
    view: &WeakViewSession,
    cx: &App,
) -> Result<Option<DragPlan>, String> {
    let Some(surface) = host.read(cx).surface_size() else {
        return Ok(None);
    };
    if !surface.is_positive_finite() || surface.width <= 96.0 || surface.height <= 96.0 {
        return Ok(None);
    }
    let geometry = view
        .geometry_snapshot()
        .map_err(|error| format!("failed to read published geometry: {error}"))?;
    let Some(node) = geometry
        .node(workflow_llm_node_id())
        .filter(|geometry| geometry.is_fresh())
    else {
        return Ok(None);
    };
    let transform = view
        .viewport_transform()
        .map_err(|error| format!("failed to read viewport transform: {error}"))?;
    let bounds = node.bounds();
    let header_anchor = CanvasPoint {
        x: bounds.origin.x + bounds.size.width * 0.5,
        y: bounds.origin.y + (20.0 / transform.zoom).min(bounds.size.height * 0.5),
    };
    let press = transform
        .screen_point_for_canvas(header_anchor)
        .ok_or("failed to project the node drag handle")?;
    let activation = ViewPoint::new(press.x + 24.0, press.y + 16.0);
    let edge = ViewPoint::new(EDGE_INSET, activation.y.clamp(48.0, surface.height - 48.0));
    if [press, activation, edge]
        .into_iter()
        .all(|point| point_inside_surface(point, surface))
    {
        Ok(Some(DragPlan {
            press,
            activation,
            edge,
        }))
    } else {
        Ok(None)
    }
}

fn point_inside_surface(point: ViewPoint, surface: ViewSize) -> bool {
    point.is_finite()
        && point.x >= 0.0
        && point.y >= 0.0
        && point.x <= surface.width
        && point.y <= surface.height
}

fn tick_ordinal(id: Option<u64>) -> u64 {
    id.map_or(0, |id| id.saturating_add(1))
}

fn frame_observation(
    host: &Entity<OpenGpuiSceneHost>,
    view: &WeakViewSession,
    cx: &App,
) -> Result<FrameObservation, String> {
    let transform = view
        .viewport_transform()
        .map_err(|error| format!("failed to observe viewport transform: {error}"))?;
    let host = host.read(cx);
    let resources = host.resource_snapshot();
    Ok(FrameObservation {
        phase: host.interaction_presentation().phase(),
        pan_x: transform.pan.x,
        pan_y: transform.pan.y,
        zoom: transform.zoom,
        callback_entries: tick_ordinal(resources.interaction_entered_tick_id()),
        callback_registrations: tick_ordinal(resources.interaction_registered_tick_id()),
        frame_requests: tick_ordinal(resources.interaction_requested_tick_id()),
        callback_epoch: resources.interaction_callback_epoch(),
        pending_queries: resources.interaction_pending_queries(),
        pending_submission: resources.interaction_submission_pending(),
        ready_prefixes: resources.interaction_ready_prefixes(),
        tick_scheduled: resources.interaction_tick_scheduled(),
        tick_frame_requested: resources.interaction_tick_frame_requested(),
        query_frame_scheduled: resources.interaction_query_frame_scheduled(),
        query_resolution_active: resources.interaction_query_resolution_active(),
        native_tick_wake_active: resources.interaction_native_tick_wake_active(),
        native_query_wake_active: resources.interaction_native_query_wake_active(),
    })
}

fn native_point(view_point: ViewPoint) -> open_gpui::Point<open_gpui::Pixels> {
    point(px(view_point.x), px(view_point.y))
}

fn mouse_down(position: ViewPoint) -> PlatformInput {
    PlatformInput::MouseDown(MouseDownEvent {
        button: MouseButton::Left,
        position: native_point(position),
        ..MouseDownEvent::default()
    })
}

fn mouse_move(position: ViewPoint) -> PlatformInput {
    PlatformInput::MouseMove(MouseMoveEvent {
        position: native_point(position),
        pressed_button: Some(MouseButton::Left),
        ..MouseMoveEvent::default()
    })
}

fn mouse_up(position: ViewPoint) -> PlatformInput {
    PlatformInput::MouseUp(MouseUpEvent {
        button: MouseButton::Left,
        position: native_point(position),
        ..MouseUpEvent::default()
    })
}

fn start_probe(
    cx: &mut App,
    window_handle: AnyWindowHandle,
    host: Entity<OpenGpuiSceneHost>,
    view: WeakViewSession,
    outcome: Rc<Cell<WitnessOutcome>>,
) {
    start_probe_timeout(cx, host.clone(), view.clone(), Rc::clone(&outcome));
    cx.spawn(async move |cx| {
        let mut probe = NativeFrameClockProbe::new();
        loop {
            cx.background_executor().timer(POLL_INTERVAL).await;
            let step = cx.update(|cx| {
                window_handle.update(cx, |_, window, cx| {
                    probe.advance(&host, &view, window, cx, &outcome)
                })
            });
            match step {
                Ok(Ok(WitnessStep::Continue)) => {}
                Ok(Ok(WitnessStep::ObservationTransferred)) => return,
                Ok(Err(reason)) => {
                    eprintln!("native frame clock witness failed: {reason}");
                    outcome.set(WitnessOutcome::Failed);
                    cx.update(|cx| cx.quit());
                    return;
                }
                Err(error) => {
                    eprintln!("native frame clock witness window disappeared: {error:?}");
                    outcome.set(WitnessOutcome::Failed);
                    cx.update(|cx| cx.quit());
                    return;
                }
            }
        }
    })
    .detach();
}

fn start_probe_timeout(
    cx: &mut App,
    host: Entity<OpenGpuiSceneHost>,
    view: WeakViewSession,
    outcome: Rc<Cell<WitnessOutcome>>,
) {
    cx.spawn(async move |cx| {
        cx.background_executor().timer(PROBE_TIMEOUT).await;
        if outcome.get() != WitnessOutcome::Pending {
            return;
        }
        let diagnostic = cx.update(|cx| timeout_diagnostic(&host, &view, cx));
        eprintln!("native frame clock witness failed: {diagnostic}");
        outcome.set(WitnessOutcome::Failed);
        cx.update(|cx| cx.quit());
    })
    .detach();
}

fn timeout_diagnostic(
    host: &Entity<OpenGpuiSceneHost>,
    view: &WeakViewSession,
    cx: &App,
) -> String {
    let host = host.read(cx);
    let resources = host.resource_snapshot();
    let interaction_phase = host.interaction_presentation().phase();
    let transform = view.viewport_transform().ok();
    format!(
        "probe timeout: interaction={:?}, pan={:?}, callback_entries={}, \
         callback_registrations={}, frame_requests={}",
        interaction_phase,
        transform.map(|transform| transform.pan),
        tick_ordinal(resources.interaction_entered_tick_id()),
        tick_ordinal(resources.interaction_registered_tick_id()),
        tick_ordinal(resources.interaction_requested_tick_id()),
    )
}

fn launch(cx: &mut App, outcome: Rc<Cell<WitnessOutcome>>) -> Result<(), String> {
    set_active_theme_mode(cx, ThemeMode::Dark)
        .map_err(|error| format!("failed to initialize theme: {error:?}"))?;
    let catalog = native_node_catalog()
        .map_err(|error| format!("failed to initialize node catalog: {error:?}"))?;
    let graph = workflow_demo_graph()
        .map_err(|errors| format!("failed to initialize workflow graph: {errors:?}"))?;
    let runtime = Rc::new(
        GraphRuntime::builder(catalog.clone())
            .build(graph)
            .map_err(|error| format!("failed to initialize graph runtime: {error:?}"))?,
    );
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .map_err(|error| format!("failed to attach graph view: {error}"))?;
    let weak_view = view.downgrade();
    let visual_profile = NativeGraphVisualProfile::workflow();
    let registry = native_node_registry(
        NativeNodeResources::ready_default(cx).with_product_theme(visual_profile.product_theme()),
        &catalog,
    );
    let (graph_theme, edge_painters) = visual_profile.into_host_parts();
    let bounds = Bounds::centered(None, size(px(WINDOW_WIDTH), px(WINDOW_HEIGHT)), cx);

    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            ..Default::default()
        },
        move |window, cx| {
            let host = cx.new(|_| {
                OpenGpuiSceneHost::new_measured_surface(view, registry)
                    .with_graph_theme(graph_theme)
                    .with_edge_painter_registry(edge_painters)
            });
            start_probe(cx, window.window_handle(), host.clone(), weak_view, outcome);
            window.refresh();
            cx.new(|_| WitnessScene {
                _runtime: runtime,
                host,
            })
        },
    )
    .map_err(|error| format!("failed to open native witness window: {error:?}"))?;
    cx.activate(true);
    Ok(())
}

fn main() -> ExitCode {
    if cfg!(feature = "real-window-smoke") {
        eprintln!(
            "native frame clock witness must run without the `real-window-smoke` test-support feature"
        );
        return ExitCode::FAILURE;
    }

    let outcome = Rc::new(Cell::new(WitnessOutcome::Pending));
    let app_outcome = Rc::clone(&outcome);
    application().run(move |cx: &mut App| {
        cx.init_colors();
        init_text_input(cx);
        cx.set_quit_mode(QuitMode::LastWindowClosed);
        if let Err(error) = launch(cx, app_outcome.clone()) {
            eprintln!("native frame clock witness failed to launch: {error}");
            app_outcome.set(WitnessOutcome::Failed);
            cx.quit();
        }
    });

    match outcome.get() {
        WitnessOutcome::Passed => ExitCode::SUCCESS,
        WitnessOutcome::Pending | WitnessOutcome::Failed => ExitCode::FAILURE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quiescent_observation() -> FrameObservation {
        FrameObservation {
            phase: CanvasInteractionPhase::Idle,
            pan_x: 42.0,
            pan_y: -7.0,
            zoom: 1.25,
            callback_entries: 8,
            callback_registrations: 9,
            frame_requests: 9,
            callback_epoch: 4,
            pending_queries: 0,
            pending_submission: false,
            ready_prefixes: 0,
            tick_scheduled: false,
            tick_frame_requested: false,
            query_frame_scheduled: false,
            query_resolution_active: false,
            native_tick_wake_active: false,
            native_query_wake_active: false,
        }
    }

    fn stable_observations() -> [FrameObservation; REQUIRED_POST_RELEASE_FRAMES] {
        [quiescent_observation(); REQUIRED_POST_RELEASE_FRAMES]
    }

    #[test]
    fn five_quiescent_native_frames_are_required() {
        assert!(validate_post_release_observations(&stable_observations()).is_ok());
        assert!(
            validate_post_release_observations(&stable_observations()[..4])
                .unwrap_err()
                .contains("expected 5")
        );
    }

    #[test]
    fn callback_or_frame_request_growth_breaks_post_release_stability() {
        let mut callback_growth = stable_observations();
        callback_growth[2].callback_entries += 1;
        assert!(
            validate_post_release_observations(&callback_growth)
                .unwrap_err()
                .contains("changed stable state")
        );

        let mut request_growth = stable_observations();
        request_growth[4].frame_requests += 1;
        assert!(
            validate_post_release_observations(&request_growth)
                .unwrap_err()
                .contains("changed stable state")
        );
    }

    #[test]
    fn residual_tokens_queries_and_native_wakes_are_rejected() {
        let mut pending_tick = stable_observations();
        pending_tick[1].tick_scheduled = true;
        assert!(
            validate_post_release_observations(&pending_tick)
                .unwrap_err()
                .contains("tick-token")
        );

        let mut pending_query = stable_observations();
        pending_query[2].pending_queries = 1;
        assert!(
            validate_post_release_observations(&pending_query)
                .unwrap_err()
                .contains("queries")
        );

        let mut wake = stable_observations();
        wake[3].native_tick_wake_active = true;
        assert!(
            validate_post_release_observations(&wake)
                .unwrap_err()
                .contains("native-tick-wake")
        );
    }
}
