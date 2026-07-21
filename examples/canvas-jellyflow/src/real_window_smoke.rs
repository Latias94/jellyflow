use std::cell::RefCell;
#[cfg(any(test, feature = "real-window-smoke"))]
use std::collections::HashSet;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Write as _};
use std::process::ExitCode;
use std::rc::Rc;
use std::time::Duration;

#[cfg(any(test, feature = "real-window-smoke"))]
use jellyflow::core::{CanvasPoint, CanvasRect};
use jellyflow::core::{EdgeId, Graph, NodeId, PortId};
#[cfg(any(test, feature = "real-window-smoke"))]
use jellyflow::runtime::runtime::viewport::ViewportTransform;
#[cfg(feature = "real-window-smoke")]
use jellyflow::runtime::{GeometrySnapshot, PublishedPortOutcome};
use jellyflow::runtime::{ViewSize, WeakViewSession};
use open_gpui::{AnyWindowHandle, App, Entity};
#[cfg(any(test, feature = "real-window-smoke"))]
use open_gpui_canvas_jellyflow::blackboard::NATIVE_BLACKBOARD_WIDTH;
use open_gpui_canvas_jellyflow::connection_search::NativeGraphSurface;

#[cfg(feature = "real-window-smoke")]
mod input_probe;
#[cfg(feature = "real-window-smoke")]
use input_probe::RealWindowSmokeProbe;
mod config;
#[cfg(feature = "real-window-smoke")]
use config::SmokeCaptureState;
pub(super) use config::SmokeConfig;

const SMOKE_TIMEOUT: Duration = Duration::from_secs(15);
const SMOKE_PROBE_INTERVAL: Duration = Duration::from_millis(16);
const COMPLETE_FRAME_FEEDBACK_SUBMISSIONS: u64 = 2;
const SMOKE_MARKER: &str = "JELLYFLOW_REAL_WINDOW_SMOKE";
#[cfg(any(test, feature = "real-window-smoke"))]
const CANVAS_BACKGROUND_RGBA: [u8; 4] = [0x11, 0x13, 0x18, 0xff];
const MIN_CANVAS_NON_BACKGROUND_PIXELS: usize = 256;
const MIN_CANVAS_DISTINCT_COLORS: usize = 4;
#[cfg(any(test, feature = "real-window-smoke"))]
const NODE_SAMPLE_INSET_RATIO: f32 = 0.12;
#[cfg(any(test, feature = "real-window-smoke"))]
const MIN_NODE_NON_BACKGROUND_PIXELS: usize = 16;
#[cfg(any(test, feature = "real-window-smoke"))]
const MIN_NODE_NON_BACKGROUND_RATIO: f32 = 0.08;
const SMOKE_INPUT_EVENT_COUNT: u16 = 27;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct SmokeExpectations {
    node_ports: BTreeMap<NodeId, BTreeSet<PortId>>,
    edges: BTreeSet<EdgeId>,
}

impl SmokeExpectations {
    fn from_graph(graph: &Graph) -> Self {
        Self {
            node_ports: graph
                .nodes()
                .iter()
                .map(|(node_id, node)| (*node_id, node.ports.iter().copied().collect()))
                .collect(),
            edges: graph.edges().keys().copied().collect(),
        }
    }

    fn node_count(&self) -> usize {
        self.node_ports.len()
    }

    fn port_count(&self) -> usize {
        self.node_ports.values().map(BTreeSet::len).sum()
    }
}

#[cfg(any(test, feature = "real-window-smoke"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SmokePortState {
    Rendered { node: NodeId, valid: bool },
    Hidden,
    Culled,
    Unavailable,
}

#[cfg(any(test, feature = "real-window-smoke"))]
#[derive(Clone, Debug, PartialEq)]
struct SmokeNodeGeometryInput {
    fresh: bool,
    bounds: CanvasRect,
    ports: BTreeMap<PortId, SmokePortState>,
}

#[cfg(any(test, feature = "real-window-smoke"))]
#[derive(Clone, Debug, Default, PartialEq)]
struct SmokeGeometryInput {
    layout_revision: u64,
    nodes: BTreeMap<NodeId, SmokeNodeGeometryInput>,
    routes: BTreeSet<EdgeId>,
}

#[cfg(feature = "real-window-smoke")]
impl SmokeGeometryInput {
    fn from_snapshot(expectations: &SmokeExpectations, snapshot: &GeometrySnapshot) -> Self {
        let nodes = expectations
            .node_ports
            .keys()
            .filter_map(|node_id| {
                let geometry = snapshot.node(*node_id)?;
                let ports = expectations.node_ports[node_id]
                    .iter()
                    .filter_map(|port_id| {
                        let local = geometry.port_outcomes().get(port_id)?;
                        let global = snapshot.port_outcome(*port_id)?;
                        if local != global {
                            return None;
                        }
                        let state = match local {
                            PublishedPortOutcome::Rendered(port) => SmokePortState::Rendered {
                                node: port.node(),
                                valid: port.port() == *port_id
                                    && port.visual_bounds().is_positive_finite()
                                    && port.hit_bounds().is_positive_finite()
                                    && port.route_anchor().is_finite(),
                            },
                            PublishedPortOutcome::Hidden => SmokePortState::Hidden,
                            PublishedPortOutcome::Culled => SmokePortState::Culled,
                            PublishedPortOutcome::Unavailable => SmokePortState::Unavailable,
                        };
                        Some((*port_id, state))
                    })
                    .collect();
                Some((
                    *node_id,
                    SmokeNodeGeometryInput {
                        fresh: geometry.is_fresh(),
                        bounds: geometry.bounds(),
                        ports,
                    },
                ))
            })
            .collect();
        let routes = expectations
            .edges
            .iter()
            .filter(|edge_id| snapshot.route(**edge_id).is_some())
            .copied()
            .collect();
        Self {
            layout_revision: snapshot.layout_revision(),
            nodes,
            routes,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PixelRect {
    left: usize,
    top: usize,
    right: usize,
    bottom: usize,
}

impl PixelRect {
    #[cfg(any(test, feature = "real-window-smoke"))]
    fn area(self) -> usize {
        self.right
            .saturating_sub(self.left)
            .saturating_mul(self.bottom.saturating_sub(self.top))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SmokeGeometryEvidence {
    layout_revision: u64,
    image_width: u32,
    image_height: u32,
    canvas_width: u32,
    expected_nodes: usize,
    fresh_nodes: usize,
    expected_ports: usize,
    rendered_ports: usize,
    expected_edges: usize,
    routed_edges: usize,
    node_regions: BTreeMap<NodeId, PixelRect>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SmokeNodePaintEvidence {
    sampled_pixels: usize,
    non_background_pixels: usize,
    required_non_background_pixels: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SmokePaintEvidence {
    image_width: u32,
    image_height: u32,
    canvas_width: u32,
    non_background_pixels: usize,
    distinct_colors: usize,
    nodes: BTreeMap<NodeId, SmokeNodePaintEvidence>,
}

impl SmokePaintEvidence {
    fn failure_reason(&self) -> Option<&'static str> {
        if self.non_background_pixels < MIN_CANVAS_NON_BACKGROUND_PIXELS {
            return Some("paint_blank_canvas");
        }
        if self.distinct_colors < MIN_CANVAS_DISTINCT_COLORS {
            return Some("paint_insufficient_colors");
        }
        if self
            .nodes
            .values()
            .any(|node| node.non_background_pixels < node.required_non_background_pixels)
        {
            return Some("paint_blank_node");
        }
        None
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SmokeInputEvidence {
    dispatched_events: u16,
    node_dragged: bool,
    viewport_panned: bool,
    viewport_zoomed: bool,
    connection_created: bool,
    keyboard_edge_deleted: bool,
    reconnect_applied: bool,
    search_opened: bool,
    search_focus_moved: bool,
    search_cancelled: bool,
    search_focus_restored: bool,
    final_frame_validated: bool,
    compact_layout: bool,
}

impl SmokeInputEvidence {
    fn failure_reason(self) -> Option<&'static str> {
        if self.dispatched_events != SMOKE_INPUT_EVENT_COUNT {
            return Some("input_dispatch_incomplete");
        }
        for (complete, reason) in [
            (self.node_dragged, "input_node_unchanged"),
            (self.viewport_panned, "input_viewport_unpanned"),
            (self.viewport_zoomed, "input_viewport_unzoomed"),
            (self.connection_created, "input_connection_missing"),
            (self.keyboard_edge_deleted, "input_keyboard_delete_missing"),
            (self.reconnect_applied, "input_reconnect_missing"),
            (self.search_opened, "input_search_missing"),
            (self.search_focus_moved, "input_search_focus_missing"),
            (self.search_cancelled, "input_search_not_cancelled"),
            (
                self.search_focus_restored,
                "input_search_focus_not_restored",
            ),
            (self.final_frame_validated, "input_final_frame_invalid"),
            (self.compact_layout, "input_compact_layout_missing"),
        ] {
            if !complete {
                return Some(reason);
            }
        }
        None
    }
}

#[derive(Clone, Debug, PartialEq)]
struct SmokeObservation {
    surface_size: Option<ViewSize>,
    active_nodes: usize,
    parked_nodes: usize,
    mounts: u64,
    feedback_submissions: u64,
    feedback_failures: u64,
    layout_rejections: u64,
    measurement_failures: u64,
    diagnostic: Option<String>,
    host_closed: bool,
    geometry: Option<SmokeGeometryEvidence>,
    paint: Option<SmokePaintEvidence>,
    input: Option<SmokeInputEvidence>,
    native_probe_failure: Option<&'static str>,
}

impl SmokeObservation {
    fn structural_decision(&self, expectations: &SmokeExpectations) -> SmokeProbeDecision {
        if self.host_closed {
            return SmokeProbeDecision::Fail("host_closed");
        }
        if self.diagnostic.is_some() {
            return SmokeProbeDecision::Fail("host_diagnostic");
        }
        if self.layout_rejections > 0 {
            return SmokeProbeDecision::Fail("layout_rejection");
        }
        if self.measurement_failures > 0 {
            return SmokeProbeDecision::Fail("measurement_failure");
        }
        if self.feedback_failures > 0 {
            return SmokeProbeDecision::Fail("feedback_failure");
        }
        if self
            .surface_size
            .is_some_and(|surface| !surface.is_positive_finite())
        {
            return SmokeProbeDecision::Fail("invalid_surface");
        }
        let expected_nodes = expectations.node_count();
        if self.mounts > expected_nodes as u64
            || self.active_nodes > expected_nodes
            || self.parked_nodes > 0
        {
            return SmokeProbeDecision::Fail("unexpected_retained_lifecycle");
        }

        let complete = expected_nodes > 0
            && self.surface_size.is_some_and(ViewSize::is_positive_finite)
            && self.active_nodes == expected_nodes
            && self.mounts == expected_nodes as u64
            && self.feedback_submissions >= COMPLETE_FRAME_FEEDBACK_SUBMISSIONS;
        if complete {
            SmokeProbeDecision::Pass
        } else {
            SmokeProbeDecision::Wait
        }
    }

    fn decision(&self, expectations: &SmokeExpectations) -> SmokeProbeDecision {
        match self.structural_decision(expectations) {
            SmokeProbeDecision::Wait => return SmokeProbeDecision::Wait,
            SmokeProbeDecision::Fail(reason) => return SmokeProbeDecision::Fail(reason),
            SmokeProbeDecision::Pass => {}
        }
        if let Some(reason) = self.native_probe_failure {
            return SmokeProbeDecision::Fail(reason);
        }
        if self.geometry.is_none() {
            return SmokeProbeDecision::Wait;
        }
        let Some(paint) = self.paint.as_ref() else {
            return SmokeProbeDecision::Wait;
        };
        if let Some(reason) = paint.failure_reason() {
            return SmokeProbeDecision::Fail(reason);
        }
        let Some(input) = self.input else {
            return SmokeProbeDecision::Wait;
        };
        if let Some(reason) = input.failure_reason() {
            return SmokeProbeDecision::Fail(reason);
        }
        SmokeProbeDecision::Pass
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SmokeProbeDecision {
    Wait,
    Pass,
    Fail(&'static str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SmokeOutcome {
    Pending,
    Passed,
    Failed(&'static str),
}

pub(super) struct SmokeState {
    demo: &'static str,
    config: SmokeConfig,
    expectations: Option<SmokeExpectations>,
    outcome: SmokeOutcome,
    last_observation: Option<SmokeObservation>,
    marker_emitted: bool,
}

impl SmokeState {
    fn new(demo: &'static str, config: SmokeConfig) -> Self {
        Self {
            demo,
            config,
            expectations: None,
            outcome: SmokeOutcome::Pending,
            last_observation: None,
            marker_emitted: false,
        }
    }

    fn configure(&mut self, expectations: SmokeExpectations) {
        self.expectations = Some(expectations);
    }

    pub(super) fn window_size(&self) -> (f32, f32) {
        self.config.window_size()
    }

    fn observe(&mut self, observation: SmokeObservation) -> SmokeProbeDecision {
        let decision = self.expectations.as_ref().map_or(
            SmokeProbeDecision::Fail("smoke_expectations_unconfigured"),
            |expectations| observation.decision(expectations),
        );
        self.last_observation = Some(observation);
        match decision {
            SmokeProbeDecision::Pass => self.outcome = SmokeOutcome::Passed,
            SmokeProbeDecision::Fail(reason) => self.fail(reason),
            SmokeProbeDecision::Wait => {}
        }
        decision
    }

    fn fail(&mut self, reason: &'static str) {
        if self.outcome == SmokeOutcome::Pending {
            self.outcome = SmokeOutcome::Failed(reason);
        }
    }

    fn marker(&mut self) -> Option<String> {
        if self.marker_emitted || self.outcome == SmokeOutcome::Pending {
            return None;
        }
        self.marker_emitted = true;
        let (status, reason) = match self.outcome {
            SmokeOutcome::Passed => ("pass", "complete_frame"),
            SmokeOutcome::Failed(reason) => ("fail", reason),
            SmokeOutcome::Pending => unreachable!("pending outcomes do not emit markers"),
        };
        let expectations = self.expectations.as_ref();
        let observation = self.last_observation.as_ref();
        let surface_width = observation
            .and_then(|observation| observation.surface_size)
            .map_or_else(|| "none".to_owned(), |surface| surface.width.to_string());
        let surface_height = observation
            .and_then(|observation| observation.surface_size)
            .map_or_else(|| "none".to_owned(), |surface| surface.height.to_string());
        let diagnostic = observation
            .and_then(|observation| observation.diagnostic.as_deref())
            .unwrap_or("none");
        let geometry = observation.and_then(|observation| observation.geometry.as_ref());
        let paint = observation.and_then(|observation| observation.paint.as_ref());
        let input = observation
            .and_then(|observation| observation.input)
            .unwrap_or_default();
        let expected_node_ids = expectations.map_or_else(
            || "none".to_owned(),
            |expected| {
                expected
                    .node_ports
                    .keys()
                    .map(|id| id.0.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            },
        );
        let expected_port_ids = expectations.map_or_else(
            || "none".to_owned(),
            |expected| {
                expected
                    .node_ports
                    .values()
                    .flatten()
                    .map(|id| id.0.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            },
        );
        let expected_edge_ids = expectations.map_or_else(
            || "none".to_owned(),
            |expected| {
                expected
                    .edges
                    .iter()
                    .map(|id| id.0.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            },
        );
        let geometry_nodes = geometry.map_or_else(
            || "none".to_owned(),
            |geometry| {
                geometry
                    .node_regions
                    .iter()
                    .map(|(id, region)| {
                        format!(
                            "{}:{}:{}:{}:{}",
                            id.0, region.left, region.top, region.right, region.bottom
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",")
            },
        );
        let paint_nodes = paint.map_or_else(
            || "none".to_owned(),
            |paint| {
                paint
                    .nodes
                    .iter()
                    .map(|(id, node)| {
                        format!(
                            "{}:{}/{}/{}",
                            id.0,
                            node.non_background_pixels,
                            node.required_non_background_pixels,
                            node.sampled_pixels
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",")
            },
        );
        Some(format!(
            "{SMOKE_MARKER} status={status} demo={} reason={reason} expected_nodes={} \
             expected_ports={} expected_edges={} expected_node_ids={expected_node_ids} \
             expected_port_ids={expected_port_ids} expected_edge_ids={expected_edge_ids} \
             surface_width={surface_width} surface_height={surface_height} active_nodes={} \
             parked_nodes={} mounts={} feedback_submissions={} feedback_failures={} \
             layout_rejections={} measurement_failures={} geometry_layout_revision={} \
             geometry_fresh_nodes={} geometry_rendered_ports={} geometry_routed_edges={} \
             geometry_nodes={geometry_nodes} paint_image_width={} paint_image_height={} \
             paint_canvas_width={} paint_non_background_pixels={} paint_distinct_colors={} \
             paint_nodes={paint_nodes} input_dispatched_events={} input_node_dragged={} \
             input_viewport_panned={} input_viewport_zoomed={} input_connection_created={} \
             input_keyboard_edge_deleted={} input_reconnect_applied={} input_search_opened={} \
             input_search_focus_moved={} input_search_cancelled={} \
             input_search_focus_restored={} input_final_frame_validated={} \
             input_compact_layout={} \
             native_probe_failure={} diagnostic={diagnostic:?}",
            self.demo,
            expectations.map_or(0, SmokeExpectations::node_count),
            expectations.map_or(0, SmokeExpectations::port_count),
            expectations.map_or(0, |expected| expected.edges.len()),
            observation.map_or(0, |observation| observation.active_nodes),
            observation.map_or(0, |observation| observation.parked_nodes),
            observation.map_or(0, |observation| observation.mounts),
            observation.map_or(0, |observation| observation.feedback_submissions),
            observation.map_or(0, |observation| observation.feedback_failures),
            observation.map_or(0, |observation| observation.layout_rejections),
            observation.map_or(0, |observation| observation.measurement_failures),
            geometry.map_or(0, |geometry| geometry.layout_revision),
            geometry.map_or(0, |geometry| geometry.fresh_nodes),
            geometry.map_or(0, |geometry| geometry.rendered_ports),
            geometry.map_or(0, |geometry| geometry.routed_edges),
            paint.map_or(0, |paint| paint.image_width),
            paint.map_or(0, |paint| paint.image_height),
            paint.map_or(0, |paint| paint.canvas_width),
            paint.map_or(0, |paint| paint.non_background_pixels),
            paint.map_or(0, |paint| paint.distinct_colors),
            input.dispatched_events,
            input.node_dragged,
            input.viewport_panned,
            input.viewport_zoomed,
            input.connection_created,
            input.keyboard_edge_deleted,
            input.reconnect_applied,
            input.search_opened,
            input.search_focus_moved,
            input.search_cancelled,
            input.search_focus_restored,
            input.final_frame_validated,
            input.compact_layout,
            observation
                .and_then(|observation| observation.native_probe_failure)
                .unwrap_or("none"),
        ))
    }
}

pub(super) type SharedSmokeState = Rc<RefCell<SmokeState>>;

pub(super) fn new_smoke_state(demo: &'static str, config: SmokeConfig) -> SharedSmokeState {
    Rc::new(RefCell::new(SmokeState::new(demo, config)))
}

pub(super) fn configure_smoke_document(state: &SharedSmokeState, graph: &Graph) {
    state
        .borrow_mut()
        .configure(SmokeExpectations::from_graph(graph));
}

pub(super) fn install_smoke_quit_guard(cx: &mut App, state: SharedSmokeState) {
    cx.on_app_quit(move |_| {
        let state = Rc::clone(&state);
        async move {
            let (exit_status, marker) = {
                let mut state = state.borrow_mut();
                state.fail("application_quit_before_complete");
                (smoke_exit_status(state.outcome), state.marker())
            };
            if let Some(marker) = marker {
                emit_smoke_marker(&marker);
            }
            if exit_status != 0 {
                std::process::exit(i32::from(exit_status));
            }
        }
    })
    .detach();
}

pub(super) fn start_smoke_probe(
    cx: &App,
    window_handle: AnyWindowHandle,
    graph: Entity<NativeGraphSurface>,
    view: WeakViewSession,
    state: SharedSmokeState,
) {
    let expectations = state.borrow().expectations.clone();
    let Some(expectations) = expectations else {
        fail_smoke(&state, "smoke_expectations_unconfigured");
        return;
    };
    #[cfg(feature = "real-window-smoke")]
    let (demo, config) = {
        let state = state.borrow();
        (state.demo, state.config.clone())
    };
    cx.spawn(async move |cx| {
        #[cfg(feature = "real-window-smoke")]
        let mut native_probe = RealWindowSmokeProbe::new(demo, config);
        loop {
            cx.background_executor().timer(SMOKE_PROBE_INTERVAL).await;
            let mut observation = cx.update(|cx| observe_smoke_surface(&graph, cx));
            if observation.structural_decision(&expectations) == SmokeProbeDecision::Pass {
                #[cfg(feature = "real-window-smoke")]
                match observation.surface_size {
                    Some(surface_size) => {
                        let advance = cx.update(|cx| {
                            window_handle.update(cx, |_, window, cx| {
                                native_probe.advance(
                                    window,
                                    cx,
                                    &graph,
                                    &view,
                                    &expectations,
                                    surface_size,
                                )
                            })
                        });
                        observation.geometry = native_probe.geometry().cloned();
                        observation.paint = native_probe.paint().cloned();
                        match advance {
                            Ok(Ok(input)) => observation.input = input,
                            Ok(Err(reason)) => observation.native_probe_failure = Some(reason),
                            Err(_) => {
                                observation.native_probe_failure = Some("smoke_window_unavailable");
                            }
                        }
                    }
                    None => observation.native_probe_failure = Some("invalid_surface"),
                }
                #[cfg(not(feature = "real-window-smoke"))]
                {
                    let _ = (&window_handle, &view);
                    observation.native_probe_failure = Some("smoke_feature_disabled");
                }
            }
            let (decision, marker) = {
                let mut state = state.borrow_mut();
                let decision = state.observe(observation);
                let marker = if decision == SmokeProbeDecision::Wait {
                    None
                } else {
                    state.marker()
                };
                (decision, marker)
            };
            if let Some(marker) = marker {
                emit_smoke_marker(&marker);
            }
            if decision != SmokeProbeDecision::Wait {
                cx.update(|cx| cx.quit());
                return;
            }
        }
    })
    .detach();
}

#[cfg(any(test, feature = "real-window-smoke"))]
fn canvas_pixel_width(image_width: u32, surface_size: ViewSize) -> Result<u32, &'static str> {
    if image_width == 0 || !surface_size.is_positive_finite() {
        return Err("paint_invalid_image");
    }
    let logical_window_width = surface_size.width + NATIVE_BLACKBOARD_WIDTH;
    if !logical_window_width.is_finite() || logical_window_width <= 0.0 {
        return Err("paint_invalid_image");
    }
    let width = (image_width as f64 * f64::from(surface_size.width)
        / f64::from(logical_window_width))
    .floor();
    if !width.is_finite() || width < 1.0 || width > f64::from(image_width) {
        return Err("paint_invalid_image");
    }
    Ok(width as u32)
}

#[cfg(any(test, feature = "real-window-smoke"))]
fn inspect_geometry(
    expectations: &SmokeExpectations,
    input: &SmokeGeometryInput,
    transform: ViewportTransform,
    surface_size: ViewSize,
    image_width: u32,
    image_height: u32,
    canvas_width: u32,
) -> Result<SmokeGeometryEvidence, &'static str> {
    if expectations.node_ports.is_empty()
        || !transform.is_valid()
        || !surface_size.is_positive_finite()
        || image_width == 0
        || image_height == 0
        || canvas_width == 0
        || canvas_width > image_width
    {
        return Err("geometry_invalid_probe");
    }

    let mut fresh_nodes = 0usize;
    let mut rendered_ports = 0usize;
    let mut node_regions = BTreeMap::new();
    for (node_id, expected_ports) in &expectations.node_ports {
        let node = input.nodes.get(node_id).ok_or("geometry_missing_node")?;
        if !node.fresh {
            return Err("geometry_stale_node");
        }
        fresh_nodes = fresh_nodes.saturating_add(1);
        if !node.bounds.is_positive_finite() {
            return Err("geometry_invalid_node");
        }
        let region = project_node_region(
            node.bounds,
            transform,
            surface_size,
            image_height,
            canvas_width,
        )?;
        node_regions.insert(*node_id, region);

        for port_id in expected_ports {
            match node
                .ports
                .get(port_id)
                .ok_or("geometry_missing_port_outcome")?
            {
                SmokePortState::Rendered { node: owner, valid } if *owner == *node_id && *valid => {
                    rendered_ports = rendered_ports.saturating_add(1);
                }
                SmokePortState::Rendered { .. } => return Err("geometry_invalid_port"),
                SmokePortState::Hidden => return Err("geometry_port_hidden"),
                SmokePortState::Culled => return Err("geometry_port_culled"),
                SmokePortState::Unavailable => return Err("geometry_port_unavailable"),
            }
        }
    }

    for edge_id in &expectations.edges {
        if !input.routes.contains(edge_id) {
            return Err("geometry_missing_route");
        }
    }

    Ok(SmokeGeometryEvidence {
        layout_revision: input.layout_revision,
        image_width,
        image_height,
        canvas_width,
        expected_nodes: expectations.node_count(),
        fresh_nodes,
        expected_ports: expectations.port_count(),
        rendered_ports,
        expected_edges: expectations.edges.len(),
        routed_edges: expectations.edges.len(),
        node_regions,
    })
}

#[cfg(any(test, feature = "real-window-smoke"))]
fn project_node_region(
    bounds: CanvasRect,
    transform: ViewportTransform,
    surface_size: ViewSize,
    image_height: u32,
    canvas_width: u32,
) -> Result<PixelRect, &'static str> {
    let max_canvas = CanvasPoint {
        x: bounds.origin.x + bounds.size.width,
        y: bounds.origin.y + bounds.size.height,
    };
    let min_screen = transform
        .screen_point_for_canvas(bounds.origin)
        .ok_or("geometry_invalid_node")?;
    let max_screen = transform
        .screen_point_for_canvas(max_canvas)
        .ok_or("geometry_invalid_node")?;
    let inset_x = (max_screen.x - min_screen.x) * NODE_SAMPLE_INSET_RATIO;
    let inset_y = (max_screen.y - min_screen.y) * NODE_SAMPLE_INSET_RATIO;
    let left = (min_screen.x + inset_x).max(0.0);
    let top = (min_screen.y + inset_y).max(0.0);
    let right = (max_screen.x - inset_x).min(surface_size.width);
    let bottom = (max_screen.y - inset_y).min(surface_size.height);
    if ![left, top, right, bottom].into_iter().all(f32::is_finite) || right <= left || bottom <= top
    {
        return Err("geometry_node_out_of_frame");
    }

    let scale_x = canvas_width as f64 / f64::from(surface_size.width);
    let scale_y = f64::from(image_height) / f64::from(surface_size.height);
    let left = (f64::from(left) * scale_x).floor() as usize;
    let top = (f64::from(top) * scale_y).floor() as usize;
    let right = (f64::from(right) * scale_x).ceil() as usize;
    let bottom = (f64::from(bottom) * scale_y).ceil() as usize;
    let region = PixelRect {
        left,
        top,
        right: right.min(canvas_width as usize),
        bottom: bottom.min(image_height as usize),
    };
    if region.area() == 0 {
        return Err("geometry_node_out_of_frame");
    }
    Ok(region)
}

#[cfg(any(test, feature = "real-window-smoke"))]
fn inspect_canvas_paint(
    image_width: u32,
    image_height: u32,
    rgba: &[u8],
    geometry: &SmokeGeometryEvidence,
) -> Result<SmokePaintEvidence, &'static str> {
    if geometry.image_width != image_width
        || geometry.image_height != image_height
        || geometry.canvas_width == 0
        || geometry.canvas_width > image_width
        || geometry.node_regions.len() != geometry.expected_nodes
    {
        return Err("paint_invalid_image");
    }

    let width = usize::try_from(image_width).map_err(|_| "paint_invalid_image")?;
    let height = usize::try_from(image_height).map_err(|_| "paint_invalid_image")?;
    let canvas_width = usize::try_from(geometry.canvas_width).map_err(|_| "paint_invalid_image")?;
    let row_stride = width.checked_mul(4).ok_or("paint_invalid_image")?;
    let (non_background_pixels, distinct_colors) =
        inspect_canvas_framebuffer(image_width, image_height, geometry.canvas_width, rgba)?;

    let mut nodes = BTreeMap::new();
    for (node_id, region) in &geometry.node_regions {
        if region.right > canvas_width || region.bottom > height || region.area() == 0 {
            return Err("paint_invalid_node_region");
        }
        let mut node_non_background = 0usize;
        for y in region.top..region.bottom {
            for x in region.left..region.right {
                let offset = y
                    .checked_mul(row_stride)
                    .and_then(|offset| x.checked_mul(4).and_then(|x| offset.checked_add(x)))
                    .ok_or("paint_invalid_image")?;
                if !is_background(&rgba[offset..offset + 4]) {
                    node_non_background = node_non_background.saturating_add(1);
                }
            }
        }
        let sampled_pixels = region.area();
        let ratio_threshold =
            (sampled_pixels as f64 * f64::from(MIN_NODE_NON_BACKGROUND_RATIO)).ceil() as usize;
        let required_non_background_pixels = MIN_NODE_NON_BACKGROUND_PIXELS
            .max(ratio_threshold)
            .min(sampled_pixels);
        nodes.insert(
            *node_id,
            SmokeNodePaintEvidence {
                sampled_pixels,
                non_background_pixels: node_non_background,
                required_non_background_pixels,
            },
        );
    }

    Ok(SmokePaintEvidence {
        image_width,
        image_height,
        canvas_width: geometry.canvas_width,
        non_background_pixels,
        distinct_colors,
        nodes,
    })
}

#[cfg(any(test, feature = "real-window-smoke"))]
fn inspect_canvas_framebuffer(
    image_width: u32,
    image_height: u32,
    canvas_width: u32,
    rgba: &[u8],
) -> Result<(usize, usize), &'static str> {
    let width = usize::try_from(image_width).map_err(|_| "paint_invalid_image")?;
    let height = usize::try_from(image_height).map_err(|_| "paint_invalid_image")?;
    let canvas_width = usize::try_from(canvas_width).map_err(|_| "paint_invalid_image")?;
    let expected_len = width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or("paint_invalid_image")?;
    if width == 0
        || height == 0
        || canvas_width == 0
        || canvas_width > width
        || rgba.len() != expected_len
    {
        return Err("paint_invalid_image");
    }

    let row_stride = width.checked_mul(4).ok_or("paint_invalid_image")?;
    let canvas_stride = canvas_width.checked_mul(4).ok_or("paint_invalid_image")?;
    let mut non_background_pixels = 0usize;
    let mut colors = HashSet::new();
    for row in rgba.chunks_exact(row_stride) {
        for pixel in row[..canvas_stride].chunks_exact(4) {
            if is_background(pixel) {
                continue;
            }
            non_background_pixels = non_background_pixels.saturating_add(1);
            colors.insert([pixel[0], pixel[1], pixel[2]]);
        }
    }
    Ok((non_background_pixels, colors.len()))
}

#[cfg(any(test, feature = "real-window-smoke"))]
fn is_background(pixel: &[u8]) -> bool {
    pixel[3] == 0 || pixel == CANVAS_BACKGROUND_RGBA
}

fn observe_smoke_surface(graph: &Entity<NativeGraphSurface>, cx: &App) -> SmokeObservation {
    let host = graph.read(cx).host().clone();
    let host = host.read(cx);
    let stats = host.stats();
    SmokeObservation {
        surface_size: host.surface_size(),
        active_nodes: stats.active_nodes(),
        parked_nodes: stats.parked_nodes(),
        mounts: stats.mounts(),
        feedback_submissions: stats.feedback_submissions(),
        feedback_failures: stats.feedback_failures(),
        layout_rejections: stats.layout_rejections(),
        measurement_failures: stats.measurement_failures(),
        diagnostic: host
            .last_diagnostic()
            .map(|diagnostic| format!("{:?}: {}", diagnostic.kind(), diagnostic.message())),
        host_closed: host.close_reason().is_some(),
        geometry: None,
        paint: None,
        input: None,
        native_probe_failure: None,
    }
}

pub(super) fn start_smoke_timeout(cx: &App, state: SharedSmokeState) {
    cx.spawn(async move |cx| {
        cx.background_executor().timer(SMOKE_TIMEOUT).await;
        let marker = {
            let mut state = state.borrow_mut();
            if state.outcome != SmokeOutcome::Pending {
                return;
            }
            state.fail("timeout");
            state.marker()
        };
        if let Some(marker) = marker {
            emit_smoke_marker(&marker);
        }
        cx.update(|cx| cx.quit());
    })
    .detach();
}

pub(super) fn fail_smoke(state: &SharedSmokeState, reason: &'static str) {
    let marker = {
        let mut state = state.borrow_mut();
        state.fail(reason);
        state.marker()
    };
    if let Some(marker) = marker {
        emit_smoke_marker(&marker);
    }
}

pub(super) fn finish_smoke_after_run(state: SharedSmokeState) -> ExitCode {
    let (outcome, marker) = {
        let mut state = state.borrow_mut();
        state.fail("event_loop_returned_before_complete");
        (state.outcome, state.marker())
    };
    if let Some(marker) = marker {
        emit_smoke_marker(&marker);
    }
    ExitCode::from(smoke_exit_status(outcome))
}

const fn smoke_exit_status(outcome: SmokeOutcome) -> u8 {
    match outcome {
        SmokeOutcome::Passed => 0,
        SmokeOutcome::Pending | SmokeOutcome::Failed(_) => 1,
    }
}

fn emit_smoke_marker(marker: &str) {
    eprintln!("{marker}");
    let _ = io::stderr().flush();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo_fixture::shader_demo_graph;
    use jellyflow::core::CanvasSize;
    use open_gpui_canvas_jellyflow::nodes::native_node_catalog;

    const IMAGE_WIDTH: u32 = 118;
    const IMAGE_HEIGHT: u32 = 72;
    const CANVAS_WIDTH: u32 = 90;

    fn node(value: u128) -> NodeId {
        NodeId::from_u128(value)
    }

    fn port(value: u128) -> PortId {
        PortId::from_u128(value)
    }

    fn edge(value: u128) -> EdgeId {
        EdgeId::from_u128(value)
    }

    fn expectations() -> SmokeExpectations {
        SmokeExpectations {
            node_ports: BTreeMap::from([
                (node(1), BTreeSet::from([port(11), port(12)])),
                (node(2), BTreeSet::from([port(21), port(22)])),
            ]),
            edges: BTreeSet::from([edge(31)]),
        }
    }

    fn geometry_input() -> SmokeGeometryInput {
        SmokeGeometryInput {
            layout_revision: 7,
            nodes: BTreeMap::from([
                (
                    node(1),
                    SmokeNodeGeometryInput {
                        fresh: true,
                        bounds: CanvasRect {
                            origin: CanvasPoint { x: 50.0, y: 50.0 },
                            size: CanvasSize {
                                width: 300.0,
                                height: 300.0,
                            },
                        },
                        ports: BTreeMap::from([
                            (
                                port(11),
                                SmokePortState::Rendered {
                                    node: node(1),
                                    valid: true,
                                },
                            ),
                            (
                                port(12),
                                SmokePortState::Rendered {
                                    node: node(1),
                                    valid: true,
                                },
                            ),
                        ]),
                    },
                ),
                (
                    node(2),
                    SmokeNodeGeometryInput {
                        fresh: true,
                        bounds: CanvasRect {
                            origin: CanvasPoint { x: 500.0, y: 300.0 },
                            size: CanvasSize {
                                width: 300.0,
                                height: 300.0,
                            },
                        },
                        ports: BTreeMap::from([
                            (
                                port(21),
                                SmokePortState::Rendered {
                                    node: node(2),
                                    valid: true,
                                },
                            ),
                            (
                                port(22),
                                SmokePortState::Rendered {
                                    node: node(2),
                                    valid: true,
                                },
                            ),
                        ]),
                    },
                ),
            ]),
            routes: BTreeSet::from([edge(31)]),
        }
    }

    fn surface_size() -> ViewSize {
        ViewSize::new(900.0, 720.0)
    }

    fn transform() -> ViewportTransform {
        ViewportTransform::new(CanvasPoint::default(), 1.0).expect("valid transform")
    }

    fn geometry_evidence() -> SmokeGeometryEvidence {
        let canvas_width =
            canvas_pixel_width(IMAGE_WIDTH, surface_size()).expect("canvas pixel width");
        assert_eq!(canvas_width, CANVAS_WIDTH);
        inspect_geometry(
            &expectations(),
            &geometry_input(),
            transform(),
            surface_size(),
            IMAGE_WIDTH,
            IMAGE_HEIGHT,
            canvas_width,
        )
        .expect("complete geometry")
    }

    fn paint_all_node_regions(
        geometry: &SmokeGeometryEvidence,
        regions: impl IntoIterator<Item = NodeId>,
    ) -> Vec<u8> {
        let mut pixels =
            CANVAS_BACKGROUND_RGBA.repeat(IMAGE_WIDTH as usize * IMAGE_HEIGHT as usize);
        let colors = [
            [0x42, 0x66, 0x88, 0xff],
            [0x88, 0x42, 0x66, 0xff],
            [0x66, 0x88, 0x42, 0xff],
            [0xd0, 0xd5, 0xdc, 0xff],
        ];
        let mut painted = 0usize;
        for node_id in regions {
            let region = geometry.node_regions[&node_id];
            for y in region.top..region.bottom {
                for x in region.left..region.right {
                    let offset = (y * IMAGE_WIDTH as usize + x) * 4;
                    pixels[offset..offset + 4].copy_from_slice(&colors[painted % colors.len()]);
                    painted = painted.saturating_add(1);
                }
            }
        }
        pixels
    }

    fn complete_observation(expected: &SmokeExpectations) -> SmokeObservation {
        let geometry = geometry_evidence();
        let pixels = paint_all_node_regions(&geometry, expected.node_ports.keys().copied());
        let paint = inspect_canvas_paint(IMAGE_WIDTH, IMAGE_HEIGHT, &pixels, &geometry)
            .expect("paint evidence");
        SmokeObservation {
            surface_size: Some(surface_size()),
            active_nodes: expected.node_count(),
            parked_nodes: 0,
            mounts: expected.node_count() as u64,
            feedback_submissions: COMPLETE_FRAME_FEEDBACK_SUBMISSIONS,
            feedback_failures: 0,
            layout_rejections: 0,
            measurement_failures: 0,
            diagnostic: None,
            host_closed: false,
            geometry: Some(geometry),
            paint: Some(paint),
            input: Some(complete_input_evidence()),
            native_probe_failure: None,
        }
    }

    fn complete_input_evidence() -> SmokeInputEvidence {
        SmokeInputEvidence {
            dispatched_events: SMOKE_INPUT_EVENT_COUNT,
            node_dragged: true,
            viewport_panned: true,
            viewport_zoomed: true,
            connection_created: true,
            keyboard_edge_deleted: true,
            reconnect_applied: true,
            search_opened: true,
            search_focus_moved: true,
            search_cancelled: true,
            search_focus_restored: true,
            final_frame_validated: true,
            compact_layout: true,
        }
    }

    #[test]
    fn expectations_preserve_every_stable_node_real_port_and_edge_id_from_the_document() {
        let catalog = native_node_catalog().expect("native node catalog");
        let graph = shader_demo_graph(&catalog).expect("shader demo graph");
        let expected = SmokeExpectations::from_graph(&graph);

        assert_eq!(
            expected.node_ports.keys().copied().collect::<BTreeSet<_>>(),
            graph.nodes().keys().copied().collect()
        );
        for (node_id, ports) in &expected.node_ports {
            assert_eq!(
                ports,
                &graph
                    .node(node_id)
                    .expect("expected node")
                    .ports
                    .iter()
                    .copied()
                    .collect()
            );
        }
        assert_eq!(
            expected.edges,
            graph.edges().keys().copied().collect::<BTreeSet<_>>()
        );
    }

    #[test]
    fn geometry_probe_accepts_fresh_complete_flagship_geometry() {
        let evidence = geometry_evidence();

        assert_eq!(evidence.expected_nodes, 2);
        assert_eq!(evidence.fresh_nodes, 2);
        assert_eq!(evidence.expected_ports, 4);
        assert_eq!(evidence.rendered_ports, 4);
        assert_eq!(evidence.expected_edges, 1);
        assert_eq!(evidence.routed_edges, 1);
        assert_eq!(evidence.node_regions.len(), 2);
    }

    #[test]
    fn geometry_probe_rejects_invalid_culled_stale_and_incomplete_geometry() {
        let expected = expectations();

        let mut missing_node = geometry_input();
        missing_node.nodes.remove(&node(1));
        assert_eq!(
            inspect_geometry(
                &expected,
                &missing_node,
                transform(),
                surface_size(),
                IMAGE_WIDTH,
                IMAGE_HEIGHT,
                CANVAS_WIDTH,
            ),
            Err("geometry_missing_node")
        );

        let mut invalid = geometry_input();
        invalid.nodes.get_mut(&node(1)).unwrap().bounds.size.width = f32::NAN;
        assert_eq!(
            inspect_geometry(
                &expected,
                &invalid,
                transform(),
                surface_size(),
                IMAGE_WIDTH,
                IMAGE_HEIGHT,
                CANVAS_WIDTH,
            ),
            Err("geometry_invalid_node")
        );

        let mut culled = geometry_input();
        culled
            .nodes
            .get_mut(&node(1))
            .unwrap()
            .ports
            .insert(port(11), SmokePortState::Culled);
        assert_eq!(
            inspect_geometry(
                &expected,
                &culled,
                transform(),
                surface_size(),
                IMAGE_WIDTH,
                IMAGE_HEIGHT,
                CANVAS_WIDTH,
            ),
            Err("geometry_port_culled")
        );

        let mut hidden = geometry_input();
        hidden
            .nodes
            .get_mut(&node(1))
            .unwrap()
            .ports
            .insert(port(11), SmokePortState::Hidden);
        assert_eq!(
            inspect_geometry(
                &expected,
                &hidden,
                transform(),
                surface_size(),
                IMAGE_WIDTH,
                IMAGE_HEIGHT,
                CANVAS_WIDTH,
            ),
            Err("geometry_port_hidden")
        );

        let mut unavailable = geometry_input();
        unavailable
            .nodes
            .get_mut(&node(1))
            .unwrap()
            .ports
            .insert(port(11), SmokePortState::Unavailable);
        assert_eq!(
            inspect_geometry(
                &expected,
                &unavailable,
                transform(),
                surface_size(),
                IMAGE_WIDTH,
                IMAGE_HEIGHT,
                CANVAS_WIDTH,
            ),
            Err("geometry_port_unavailable")
        );

        let mut stale = geometry_input();
        stale.nodes.get_mut(&node(1)).unwrap().fresh = false;
        assert_eq!(
            inspect_geometry(
                &expected,
                &stale,
                transform(),
                surface_size(),
                IMAGE_WIDTH,
                IMAGE_HEIGHT,
                CANVAS_WIDTH,
            ),
            Err("geometry_stale_node")
        );

        let mut missing_port = geometry_input();
        missing_port
            .nodes
            .get_mut(&node(1))
            .unwrap()
            .ports
            .remove(&port(11));
        assert_eq!(
            inspect_geometry(
                &expected,
                &missing_port,
                transform(),
                surface_size(),
                IMAGE_WIDTH,
                IMAGE_HEIGHT,
                CANVAS_WIDTH,
            ),
            Err("geometry_missing_port_outcome")
        );

        let mut missing_route = geometry_input();
        missing_route.routes.clear();
        assert_eq!(
            inspect_geometry(
                &expected,
                &missing_route,
                transform(),
                surface_size(),
                IMAGE_WIDTH,
                IMAGE_HEIGHT,
                CANVAS_WIDTH,
            ),
            Err("geometry_missing_route")
        );

        let mut out_of_frame = geometry_input();
        out_of_frame
            .nodes
            .get_mut(&node(2))
            .unwrap()
            .bounds
            .origin
            .x = 1_500.0;
        assert_eq!(
            inspect_geometry(
                &expected,
                &out_of_frame,
                transform(),
                surface_size(),
                IMAGE_WIDTH,
                IMAGE_HEIGHT,
                CANVAS_WIDTH,
            ),
            Err("geometry_node_out_of_frame")
        );
    }

    #[test]
    fn paint_probe_rejects_invalid_and_background_only_images() {
        let geometry = geometry_evidence();
        assert_eq!(
            inspect_canvas_paint(IMAGE_WIDTH, IMAGE_HEIGHT, &[], &geometry),
            Err("paint_invalid_image")
        );

        let pixels = CANVAS_BACKGROUND_RGBA.repeat(IMAGE_WIDTH as usize * IMAGE_HEIGHT as usize);
        let paint = inspect_canvas_paint(IMAGE_WIDTH, IMAGE_HEIGHT, &pixels, &geometry)
            .expect("well-formed image");
        assert_eq!(paint.failure_reason(), Some("paint_blank_canvas"));
    }

    #[test]
    fn paint_probe_accepts_independent_evidence_for_every_expected_node() {
        let expected = expectations();
        let geometry = geometry_evidence();
        let pixels = paint_all_node_regions(&geometry, expected.node_ports.keys().copied());
        let paint = inspect_canvas_paint(IMAGE_WIDTH, IMAGE_HEIGHT, &pixels, &geometry)
            .expect("paint evidence");

        assert!(paint.non_background_pixels >= MIN_CANVAS_NON_BACKGROUND_PIXELS);
        assert_eq!(paint.distinct_colors, 4);
        assert_eq!(paint.nodes.len(), expected.node_count());
        assert_eq!(paint.failure_reason(), None);
    }

    #[test]
    fn paint_probe_rejects_a_blank_expected_node_even_when_global_thresholds_pass() {
        let geometry = geometry_evidence();
        let pixels = paint_all_node_regions(&geometry, [node(1)]);
        let paint = inspect_canvas_paint(IMAGE_WIDTH, IMAGE_HEIGHT, &pixels, &geometry)
            .expect("paint evidence");

        assert!(paint.non_background_pixels >= MIN_CANVAS_NON_BACKGROUND_PIXELS);
        assert!(paint.distinct_colors >= MIN_CANVAS_DISTINCT_COLORS);
        assert_eq!(paint.nodes[&node(2)].non_background_pixels, 0);
        assert_eq!(paint.failure_reason(), Some("paint_blank_node"));
    }

    #[test]
    fn smoke_decision_waits_for_bootstrap_and_accepts_only_complete_probe_evidence() {
        let expected = expectations();
        let complete = complete_observation(&expected);
        assert_eq!(complete.decision(&expected), SmokeProbeDecision::Pass);

        let mut bootstrapping = complete.clone();
        bootstrapping.surface_size = None;
        bootstrapping.active_nodes = 0;
        bootstrapping.mounts = 0;
        bootstrapping.feedback_submissions = 1;
        assert_eq!(bootstrapping.decision(&expected), SmokeProbeDecision::Wait);

        let mut missing_geometry = complete;
        missing_geometry.geometry = None;
        assert_eq!(
            missing_geometry.decision(&expected),
            SmokeProbeDecision::Wait
        );
    }

    #[test]
    fn smoke_decision_rejects_structured_native_failures() {
        type Mutation = fn(&mut SmokeObservation);
        let expected = expectations();
        let cases: &[(&str, Mutation, &'static str)] = &[
            (
                "closed host",
                |value| value.host_closed = true,
                "host_closed",
            ),
            (
                "host diagnostic",
                |value| value.diagnostic = Some("FactoryFailure: renderer failed".to_owned()),
                "host_diagnostic",
            ),
            (
                "layout rejection",
                |value| value.layout_rejections = 1,
                "layout_rejection",
            ),
            (
                "measurement failure",
                |value| value.measurement_failures = 1,
                "measurement_failure",
            ),
            (
                "feedback failure",
                |value| value.feedback_failures = 1,
                "feedback_failure",
            ),
            (
                "invalid surface",
                |value| value.surface_size = Some(ViewSize::new(0.0, 720.0)),
                "invalid_surface",
            ),
            (
                "native probe failure",
                |value| value.native_probe_failure = Some("native_probe_failed"),
                "native_probe_failed",
            ),
            (
                "incomplete input dispatch",
                |value| {
                    let mut input = complete_input_evidence();
                    input.dispatched_events = SMOKE_INPUT_EVENT_COUNT - 1;
                    value.input = Some(input);
                },
                "input_dispatch_incomplete",
            ),
            (
                "unchanged node",
                |value| {
                    let mut input = complete_input_evidence();
                    input.node_dragged = false;
                    value.input = Some(input);
                },
                "input_node_unchanged",
            ),
            (
                "unzoomed viewport",
                |value| {
                    let mut input = complete_input_evidence();
                    input.viewport_zoomed = false;
                    value.input = Some(input);
                },
                "input_viewport_unzoomed",
            ),
            (
                "missing reconnect",
                |value| {
                    let mut input = complete_input_evidence();
                    input.reconnect_applied = false;
                    value.input = Some(input);
                },
                "input_reconnect_missing",
            ),
            (
                "non-compact window",
                |value| {
                    let mut input = complete_input_evidence();
                    input.compact_layout = false;
                    value.input = Some(input);
                },
                "input_compact_layout_missing",
            ),
        ];

        for &(label, mutate, reason) in cases {
            let mut observation = complete_observation(&expected);
            mutate(&mut observation);
            assert_eq!(
                observation.decision(&expected),
                SmokeProbeDecision::Fail(reason),
                "{label}",
            );
        }
    }

    #[test]
    fn smoke_state_emits_one_marker_with_expected_ids_geometry_and_per_node_paint() {
        let expected = expectations();
        let mut state = SmokeState::new("shader", SmokeConfig::default());
        state.configure(expected.clone());
        assert_eq!(
            state.observe(complete_observation(&expected)),
            SmokeProbeDecision::Pass
        );

        let marker = state.marker().expect("completed smoke marker");
        assert!(marker.starts_with(SMOKE_MARKER));
        assert!(marker.contains("status=pass"));
        assert!(marker.contains("demo=shader"));
        assert!(marker.contains("expected_nodes=2"));
        assert!(marker.contains("expected_ports=4"));
        assert!(marker.contains("expected_edges=1"));
        assert!(marker.contains("expected_node_ids="));
        assert!(marker.contains("geometry_fresh_nodes=2"));
        assert!(marker.contains("geometry_rendered_ports=4"));
        assert!(marker.contains("geometry_routed_edges=1"));
        assert!(marker.contains("geometry_nodes="));
        assert!(marker.contains("paint_nodes="));
        assert!(marker.contains(&format!(
            "input_dispatched_events={SMOKE_INPUT_EVENT_COUNT}"
        )));
        assert!(marker.contains("input_node_dragged=true"));
        assert!(marker.contains("input_viewport_panned=true"));
        assert!(marker.contains("input_viewport_zoomed=true"));
        assert!(marker.contains("input_connection_created=true"));
        assert!(marker.contains("input_keyboard_edge_deleted=true"));
        assert!(marker.contains("input_reconnect_applied=true"));
        assert!(marker.contains("input_search_opened=true"));
        assert!(marker.contains("input_search_focus_moved=true"));
        assert!(marker.contains("input_search_cancelled=true"));
        assert!(marker.contains("input_search_focus_restored=true"));
        assert!(marker.contains("input_final_frame_validated=true"));
        assert!(marker.contains("input_compact_layout=true"));
        assert!(state.marker().is_none());
    }

    #[test]
    fn startup_failure_emits_one_failed_marker_and_nonzero_status() {
        let mut state = SmokeState::new("workflow", SmokeConfig::default());
        state.fail("window_open");
        let marker = state.marker().expect("startup failure marker");

        assert!(marker.starts_with(SMOKE_MARKER));
        assert!(marker.contains("status=fail"));
        assert!(marker.contains("demo=workflow"));
        assert!(marker.contains("reason=window_open"));
        assert!(state.marker().is_none());
        assert_eq!(smoke_exit_status(SmokeOutcome::Passed), 0);
        assert_ne!(smoke_exit_status(SmokeOutcome::Pending), 0);
        assert_ne!(smoke_exit_status(SmokeOutcome::Failed("test_failure")), 0);
    }
}
