use std::cell::RefCell;
#[cfg(any(test, feature = "real-window-smoke"))]
use std::collections::HashSet;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Write as _};
use std::process::ExitCode;
use std::rc::Rc;
use std::time::Duration;

#[cfg(any(test, feature = "real-window-smoke"))]
use jellyflow_core::CanvasRect;
use jellyflow_core::{CanvasPoint, EdgeId, Graph, NodeId, PortId};
use jellyflow_open_gpui::host::OpenGpuiSceneHostDiagnosticKind;
use jellyflow_runtime::geometry::{ViewRect, ViewSize};
use jellyflow_runtime::view::WeakViewSession;
#[cfg(feature = "real-window-smoke")]
use jellyflow_runtime::view::{GeometrySnapshot, PublishedPortOutcome};
#[cfg(any(test, feature = "real-window-smoke"))]
use jellyflow_runtime::viewport::ViewportTransform;
#[cfg(any(test, feature = "real-window-smoke"))]
use open_gpui::Hsla;
use open_gpui::{AnyWindowHandle, App, Entity};
use open_gpui_canvas_jellyflow::NativeProductTheme;
#[cfg(any(test, feature = "real-window-smoke"))]
use open_gpui_canvas_jellyflow::blackboard::NATIVE_BLACKBOARD_WIDTH;
use open_gpui_canvas_jellyflow::connection_search::NativeGraphSurface;

use crate::native_app::NativeDemo;

#[cfg(feature = "real-window-smoke")]
mod input_probe;
#[cfg(feature = "real-window-smoke")]
use input_probe::RealWindowSmokeProbe;
mod config;
use config::SmokeCaptureState;
pub(super) use config::SmokeConfig;
use config::SmokeScenario;
#[cfg(any(test, feature = "real-window-smoke"))]
use config::{SmokeBounds, SmokeZoom};

const SMOKE_TIMEOUT: Duration = Duration::from_secs(15);
const SMOKE_PROBE_INTERVAL: Duration = Duration::from_millis(16);
mod presented_frame;
mod receipt;
use presented_frame::{SmokePresentedFrame, validate_presented_frames};
use receipt::{SMOKE_RECEIPT_PREFIX, SmokeRunReceiptV1};
#[cfg(test)]
const TEST_CANVAS_BACKGROUND_RGBA: [u8; 4] = [0x11, 0x13, 0x18, 0xff];
const MIN_CANVAS_NON_BACKGROUND_PIXELS: usize = 256;
const MIN_CANVAS_DISTINCT_COLORS: usize = 4;
#[cfg(any(test, feature = "real-window-smoke"))]
const WIRE_PATH_SAMPLE_COUNT: usize = 33;
#[cfg(any(test, feature = "real-window-smoke"))]
const WIRE_ENDPOINT_TOLERANCE_IN_VIEW: f32 = 3.0;
#[cfg(any(test, feature = "real-window-smoke"))]
const WIRE_PIXEL_CORRIDOR_RADIUS_IN_VIEW: f32 = 3.0;
#[cfg(any(test, feature = "real-window-smoke"))]
const MIN_WIRE_VISIBLE_SAMPLES: usize = 3;
#[cfg(any(test, feature = "real-window-smoke"))]
const MIN_WIRE_PAINTED_SAMPLE_RATIO: f32 = 0.55;
#[cfg(any(test, feature = "real-window-smoke"))]
const NODE_SAMPLE_INSET_RATIO: f32 = 0.12;
#[cfg(any(test, feature = "real-window-smoke"))]
const MIN_NODE_NON_BACKGROUND_PIXELS: usize = 16;
#[cfg(any(test, feature = "real-window-smoke"))]
const MIN_NODE_NON_BACKGROUND_RATIO: f32 = 0.08;
#[cfg(any(test, feature = "real-window-smoke"))]
const SELECTION_CHROME_COLOR_TOLERANCE: u8 = 24;
#[cfg(any(test, feature = "real-window-smoke"))]
const MIN_SELECTION_CHROME_SIDE_PIXELS: usize = 16;
#[cfg(any(test, feature = "real-window-smoke"))]
const MIN_VISIBLE_SELECTION_CHROME_SIDES: u8 = 2;
#[cfg(any(test, feature = "real-window-smoke"))]
const MAX_DETACHED_CHROME_RUN_IN_VIEW: f32 = 8.0;
const SMOKE_INTERACTION_CAPTURE_COUNT: usize = 9;
const SMOKE_AUTOPAN_MIN_FRAMES: u8 = 3;
const SMOKE_LOD_SAMPLE_COUNT: usize = 3;
const INTERACTION_CAPTURE_STATES: [SmokeCaptureState; SMOKE_INTERACTION_CAPTURE_COUNT] = [
    SmokeCaptureState::Initial,
    SmokeCaptureState::Selected,
    SmokeCaptureState::ConnectionPreview,
    SmokeCaptureState::ConnectionCommitted,
    SmokeCaptureState::ReconnectSelected,
    SmokeCaptureState::ReconnectPreview,
    SmokeCaptureState::ReconnectCommitted,
    SmokeCaptureState::SearchOpen,
    SmokeCaptureState::Final,
];
const LOD_CAPTURE_STATES: [SmokeCaptureState; SMOKE_LOD_SAMPLE_COUNT] = [
    SmokeCaptureState::Lod086,
    SmokeCaptureState::Lod085,
    SmokeCaptureState::Lod084,
];

#[cfg(any(test, feature = "real-window-smoke"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SmokeSelectionChromePalette {
    selection: [u8; 3],
    focus: [u8; 3],
}

#[cfg(any(test, feature = "real-window-smoke"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SmokePaintPalette {
    canvas_background: [u8; 4],
    selection_chrome: SmokeSelectionChromePalette,
}

#[cfg(any(test, feature = "real-window-smoke"))]
impl SmokePaintPalette {
    fn from_product_theme(theme: NativeProductTheme) -> Self {
        Self {
            canvas_background: rgba8(theme.canvas.surface),
            selection_chrome: SmokeSelectionChromePalette::from_product_theme(theme),
        }
    }
}

#[cfg(any(test, feature = "real-window-smoke"))]
impl SmokeSelectionChromePalette {
    fn from_product_theme(theme: NativeProductTheme) -> Self {
        Self {
            selection: rgb8(theme.selection.border),
            focus: rgb8(theme.selection.focus),
        }
    }

    fn matches(self, pixel: &[u8]) -> bool {
        [self.selection, self.focus].iter().any(|color| {
            pixel[..3].iter().zip(color).all(|(channel, expected)| {
                channel.abs_diff(*expected) <= SELECTION_CHROME_COLOR_TOLERANCE
            })
        })
    }
}

#[cfg(any(test, feature = "real-window-smoke"))]
fn rgb8(color: Hsla) -> [u8; 3] {
    let color = color.to_rgb();
    [
        (color.r * 255.0).round() as u8,
        (color.g * 255.0).round() as u8,
        (color.b * 255.0).round() as u8,
    ]
}

#[cfg(any(test, feature = "real-window-smoke"))]
fn rgba8(color: Hsla) -> [u8; 4] {
    let rgb = rgb8(color);
    let alpha = (color.a * 255.0).round() as u8;
    [rgb[0], rgb[1], rgb[2], alpha]
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct SmokeExpectations {
    node_ports: BTreeMap<NodeId, BTreeSet<PortId>>,
    edges: BTreeSet<EdgeId>,
    edge_nodes: BTreeMap<EdgeId, (NodeId, NodeId)>,
    edge_ports: BTreeMap<EdgeId, (PortId, PortId)>,
}

impl SmokeExpectations {
    fn from_graph(graph: &Graph) -> Self {
        let edge_nodes = graph
            .edges()
            .iter()
            .filter_map(|(edge_id, edge)| {
                Some((
                    *edge_id,
                    (graph.port(&edge.from)?.node, graph.port(&edge.to)?.node),
                ))
            })
            .collect();
        let edge_ports = graph
            .edges()
            .iter()
            .map(|(edge_id, edge)| (*edge_id, (edge.from, edge.to)))
            .collect();
        Self {
            node_ports: graph
                .nodes()
                .iter()
                .map(|(node_id, node)| (*node_id, node.ports.iter().copied().collect()))
                .collect(),
            edges: graph.edges().keys().copied().collect(),
            edge_nodes,
            edge_ports,
        }
    }

    fn node_count(&self) -> usize {
        self.node_ports.len()
    }

    #[cfg(feature = "real-window-smoke")]
    fn without_edge(mut self, edge: EdgeId) -> Self {
        self.edges.remove(&edge);
        self.edge_nodes.remove(&edge);
        self.edge_ports.remove(&edge);
        self
    }
}

#[cfg(any(test, feature = "real-window-smoke"))]
#[derive(Clone, Copy, Debug, PartialEq)]
enum SmokePortState {
    Rendered {
        node: NodeId,
        valid: bool,
        visual_bounds: CanvasRect,
        route_anchor: CanvasPoint,
    },
    Hidden,
    Culled,
    Unavailable,
}

#[cfg(any(test, feature = "real-window-smoke"))]
fn valid_optional_hit_bounds(bounds: Option<CanvasRect>) -> bool {
    bounds.is_none_or(CanvasRect::is_positive_finite)
}

#[cfg(any(test, feature = "real-window-smoke"))]
#[derive(Clone, Debug, PartialEq)]
struct SmokeNodeGeometryInput {
    frame_visible: bool,
    fresh: bool,
    bounds: CanvasRect,
    ports: BTreeMap<PortId, SmokePortState>,
}

#[cfg(any(test, feature = "real-window-smoke"))]
#[derive(Clone, Debug, Default, PartialEq)]
struct SmokeGeometryInput {
    layout_revision: u64,
    nodes: BTreeMap<NodeId, SmokeNodeGeometryInput>,
    routes: BTreeMap<EdgeId, SmokeRouteGeometryInput>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct SmokePathSampleInput {
    progress: f32,
    point: CanvasPoint,
}

#[cfg(any(test, feature = "real-window-smoke"))]
#[derive(Clone, Debug, PartialEq)]
struct SmokeRouteGeometryInput {
    source: CanvasPoint,
    target: CanvasPoint,
    samples: Vec<SmokePathSampleInput>,
}

#[cfg(feature = "real-window-smoke")]
impl SmokeGeometryInput {
    fn from_snapshot(
        expectations: &SmokeExpectations,
        snapshot: &GeometrySnapshot,
        graph: &Graph,
    ) -> Self {
        let nodes = expectations
            .node_ports
            .keys()
            .filter_map(|node_id| {
                let Some(geometry) = snapshot.node(*node_id) else {
                    let node = graph.node(node_id)?;
                    let bounds = node.size.and_then(|size| node.bounds_for_size(size))?;
                    return Some((
                        *node_id,
                        SmokeNodeGeometryInput {
                            frame_visible: false,
                            fresh: false,
                            bounds,
                            ports: expectations.node_ports[node_id]
                                .iter()
                                .map(|port_id| (*port_id, SmokePortState::Culled))
                                .collect(),
                        },
                    ));
                };
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
                                    && valid_optional_hit_bounds(port.hit_bounds())
                                    && port.route_anchor().is_finite(),
                                visual_bounds: port.visual_bounds(),
                                route_anchor: port.route_anchor(),
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
                        frame_visible: true,
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
            .filter_map(|edge_id| {
                let route = snapshot.route(*edge_id)?;
                let samples = (0..WIRE_PATH_SAMPLE_COUNT)
                    .map(|index| {
                        let progress = index as f32 / (WIRE_PATH_SAMPLE_COUNT - 1) as f32;
                        route
                            .facts()
                            .path
                            .sample(jellyflow_runtime::geometry::EdgePathAnchor::Progress(
                                progress,
                            ))
                            .ok()
                            .map(|sample| SmokePathSampleInput {
                                progress,
                                point: sample.point,
                            })
                    })
                    .collect::<Option<Vec<_>>>()?;
                Some((
                    *edge_id,
                    SmokeRouteGeometryInput {
                        source: route.facts().endpoints.source.point,
                        target: route.facts().endpoints.target.point,
                        samples,
                    },
                ))
            })
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PixelPoint {
    x: usize,
    y: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct SmokeEndpointGeometryEvidence {
    route_point: CanvasPoint,
    port_anchor: Option<CanvasPoint>,
    distance_in_view: Option<f32>,
    visible: bool,
    used_offscreen_fallback: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct SmokeRouteGeometryEvidence {
    source: SmokeEndpointGeometryEvidence,
    target: SmokeEndpointGeometryEvidence,
    path_samples: Vec<SmokePathSampleInput>,
    visible_path_samples: Vec<PixelPoint>,
}

impl PixelRect {
    #[cfg(any(test, feature = "real-window-smoke"))]
    fn area(self) -> usize {
        self.right
            .saturating_sub(self.left)
            .saturating_mul(self.bottom.saturating_sub(self.top))
    }
}

#[derive(Clone, Debug, PartialEq)]
struct SmokeGeometryEvidence {
    layout_revision: u64,
    image_width: u32,
    image_height: u32,
    canvas_width: u32,
    surface_size: ViewSize,
    expected_node_ids: BTreeSet<NodeId>,
    fresh_nodes: usize,
    expected_port_ids: BTreeSet<PortId>,
    rendered_ports: usize,
    expected_edge_ids: BTreeSet<EdgeId>,
    routed_edges: usize,
    node_regions: BTreeMap<NodeId, PixelRect>,
    routes: BTreeMap<EdgeId, SmokeRouteGeometryEvidence>,
    endpoint_geometry_verified: bool,
    offscreen_endpoint_fallbacks: usize,
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
    wires: BTreeMap<EdgeId, SmokeWirePaintEvidence>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SmokeWirePaintEvidence {
    visible_samples: usize,
    painted_samples: usize,
    required_painted_samples: usize,
}

impl SmokeWirePaintEvidence {
    fn failure_reason(self) -> Option<&'static str> {
        (self.painted_samples < self.required_painted_samples).then_some("paint_missing_wire")
    }
}

#[cfg(any(test, feature = "real-window-smoke"))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SmokeSelectionChromeEvidence {
    expected_sides: u8,
    matched_sides: u8,
    bottom_expected: bool,
    bottom_matched: bool,
    detached_max_run: usize,
    allowed_detached_run: usize,
}

#[cfg(any(test, feature = "real-window-smoke"))]
impl SmokeSelectionChromeEvidence {
    fn failure_reason(self) -> Option<&'static str> {
        if self.expected_sides < MIN_VISIBLE_SELECTION_CHROME_SIDES
            || self.matched_sides != self.expected_sides
            || (self.bottom_expected && !self.bottom_matched)
        {
            return Some("paint_selection_chrome_not_fitted");
        }
        if self.detached_max_run > self.allowed_detached_run {
            return Some("paint_detached_selection_chrome");
        }
        None
    }
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
        if self
            .wires
            .values()
            .any(|wire| wire.failure_reason().is_some())
        {
            return Some("paint_missing_wire");
        }
        None
    }
}

#[derive(Clone, Debug, PartialEq)]
struct SmokeInputEvidence {
    scenario: SmokeScenarioEvidence,
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(
    not(any(test, feature = "real-window-smoke")),
    allow(
        dead_code,
        reason = "scenario evidence is constructed only by the optional native smoke probe"
    )
)]
enum SmokeScenarioEvidence {
    Interaction(SmokeInteractionEvidence),
    LodThreshold(SmokeLodEvidence),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SmokeInteractionEvidence {
    captures: BTreeSet<SmokeCaptureState>,
}

#[derive(Clone, Debug, PartialEq)]
struct SmokeLodEvidence {
    captures: BTreeSet<SmokeCaptureState>,
    lod_continuous: bool,
    lod_max_jump_in_view: f32,
}

impl SmokeInputEvidence {
    fn failure_reason(&self, configured_scenario: SmokeScenario) -> Option<&'static str> {
        match (&self.scenario, configured_scenario) {
            (SmokeScenarioEvidence::Interaction(interaction), SmokeScenario::Interaction) => {
                if !captures_complete(&interaction.captures, &INTERACTION_CAPTURE_STATES) {
                    return Some("input_target_zoom_capture_mismatch");
                }
            }
            (SmokeScenarioEvidence::LodThreshold(lod), SmokeScenario::LodThreshold) => {
                if !captures_complete(&lod.captures, &LOD_CAPTURE_STATES) {
                    return Some("input_lod_samples_incomplete");
                }
                if !lod.lod_continuous {
                    return Some("input_lod_discontinuity");
                }
                if !lod.captures.contains(&SmokeCaptureState::Lod084) {
                    return Some("input_final_frame_invalid");
                }
            }
            _ => return Some("input_scenario_mismatch"),
        }
        None
    }
}

fn captures_complete<const N: usize>(
    captures: &BTreeSet<SmokeCaptureState>,
    expected: &[SmokeCaptureState; N],
) -> bool {
    captures.len() == expected.len() && expected.iter().all(|state| captures.contains(state))
}

#[derive(Clone, Debug, PartialEq)]
struct SmokeObservation {
    surface_size: Option<ViewSize>,
    diagnostic: Option<SmokeHostDiagnostic>,
    host_closed: bool,
    geometry: Option<SmokeGeometryEvidence>,
    paint: Option<SmokePaintEvidence>,
    input: Option<SmokeInputEvidence>,
    native_probe_failure: Option<&'static str>,
    native_probe_phase: Option<String>,
    native_probe_wait_ticks: Option<u16>,
    presented_frames: Vec<SmokePresentedFrame>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SmokeHostDiagnostic {
    kind: OpenGpuiSceneHostDiagnosticKind,
    message: String,
}

impl SmokeHostDiagnostic {
    fn blocks_invariant(&self, geometry: Option<&SmokeGeometryEvidence>) -> bool {
        match self.kind {
            OpenGpuiSceneHostDiagnosticKind::EndpointGeometry => {
                geometry.is_some_and(|geometry| !geometry.endpoint_geometry_verified)
            }
            _ => true,
        }
    }
}

impl SmokeObservation {
    #[cfg(any(test, feature = "real-window-smoke"))]
    fn bind_captured_frame(
        &mut self,
        geometry: Option<SmokeGeometryEvidence>,
        paint: Option<SmokePaintEvidence>,
    ) {
        self.geometry = geometry;
        self.paint = paint;
    }

    fn invariant_failure(&self) -> Option<&'static str> {
        if self.host_closed {
            return Some("host_closed");
        }
        if self
            .diagnostic
            .as_ref()
            .is_some_and(|diagnostic| diagnostic.blocks_invariant(self.geometry.as_ref()))
        {
            return Some("host_diagnostic");
        }
        if self
            .surface_size
            .is_some_and(|surface| !surface.is_positive_finite())
        {
            return Some("invalid_surface");
        }
        None
    }

    fn structural_decision(&self, expectations: &SmokeExpectations) -> SmokeProbeDecision {
        if let Some(reason) = self.invariant_failure() {
            return SmokeProbeDecision::Fail(reason);
        }
        let complete = expectations.node_count() > 0
            && self.surface_size.is_some_and(ViewSize::is_positive_finite);
        if complete {
            SmokeProbeDecision::Pass
        } else {
            SmokeProbeDecision::Wait
        }
    }

    fn decision(
        &self,
        expectations: &SmokeExpectations,
        config: &SmokeConfig,
    ) -> SmokeProbeDecision {
        match self.structural_decision(expectations) {
            SmokeProbeDecision::Wait => return SmokeProbeDecision::Wait,
            SmokeProbeDecision::Fail(reason) => return SmokeProbeDecision::Fail(reason),
            SmokeProbeDecision::Pass => {}
        }
        self.evidence_decision(config)
    }

    #[cfg(any(test, feature = "real-window-smoke"))]
    fn active_decision(
        &self,
        _expectations: &SmokeExpectations,
        config: &SmokeConfig,
    ) -> SmokeProbeDecision {
        if let Some(reason) = self.invariant_failure() {
            return SmokeProbeDecision::Fail(reason);
        }
        self.evidence_decision(config)
    }

    fn evidence_decision(&self, config: &SmokeConfig) -> SmokeProbeDecision {
        if let Some(reason) = self.native_probe_failure {
            return SmokeProbeDecision::Fail(reason);
        }
        let Some(geometry) = self.geometry.as_ref() else {
            return SmokeProbeDecision::Wait;
        };
        if !geometry.endpoint_geometry_verified {
            return SmokeProbeDecision::Fail("geometry_route_endpoint_unverified");
        }
        let Some(paint) = self.paint.as_ref() else {
            return SmokeProbeDecision::Wait;
        };
        if let Some(reason) = paint.failure_reason() {
            return SmokeProbeDecision::Fail(reason);
        }
        let Some(input) = self.input.as_ref() else {
            return SmokeProbeDecision::Wait;
        };
        if let Some(reason) = input.failure_reason(config.scenario()) {
            return SmokeProbeDecision::Fail(reason);
        }
        if let Err(reason) = validate_presented_frames(&self.presented_frames, config.scenario()) {
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
    demo: NativeDemo,
    config: SmokeConfig,
    expectations: Option<SmokeExpectations>,
    outcome: SmokeOutcome,
    last_observation: Option<SmokeObservation>,
    receipt_emitted: bool,
}

impl SmokeState {
    fn new(demo: NativeDemo, config: SmokeConfig) -> Self {
        Self {
            demo,
            config,
            expectations: None,
            outcome: SmokeOutcome::Pending,
            last_observation: None,
            receipt_emitted: false,
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
            |expectations| observation.decision(expectations, &self.config),
        );
        self.record_observation(observation, decision)
    }

    #[cfg(feature = "real-window-smoke")]
    fn observe_active(&mut self, observation: SmokeObservation) -> SmokeProbeDecision {
        let decision = self.expectations.as_ref().map_or(
            SmokeProbeDecision::Fail("smoke_expectations_unconfigured"),
            |expectations| observation.active_decision(expectations, &self.config),
        );
        self.record_observation(observation, decision)
    }

    fn record_observation(
        &mut self,
        observation: SmokeObservation,
        decision: SmokeProbeDecision,
    ) -> SmokeProbeDecision {
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

    fn receipt(&mut self) -> Option<SmokeRunReceiptV1> {
        receipt::take_run_receipt(self)
    }
}

pub(super) type SharedSmokeState = Rc<RefCell<SmokeState>>;

pub(super) fn new_smoke_state(demo: NativeDemo, config: SmokeConfig) -> SharedSmokeState {
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
            let (exit_status, receipt) = {
                let mut state = state.borrow_mut();
                state.fail("application_quit_before_complete");
                (smoke_exit_status(state.outcome), state.receipt())
            };
            if let Some(receipt) = receipt {
                emit_smoke_receipt(&receipt);
            }
            if exit_status != 0 {
                std::process::exit(i32::from(exit_status));
            }
        }
    })
    .detach();
}

pub(super) fn start_smoke_probe(
    cx: &mut App,
    window_handle: AnyWindowHandle,
    graph: Entity<NativeGraphSurface>,
    view: WeakViewSession,
    product_theme: NativeProductTheme,
    state: SharedSmokeState,
) {
    #[cfg(not(feature = "real-window-smoke"))]
    let _ = product_theme;
    let expectations = state.borrow().expectations.clone();
    let Some(expectations) = expectations else {
        fail_smoke(cx, &state, "smoke_expectations_unconfigured");
        return;
    };
    #[cfg(feature = "real-window-smoke")]
    let (demo, config) = {
        let state = state.borrow();
        (state.demo, state.config.clone())
    };
    cx.spawn(async move |cx| {
        #[cfg(feature = "real-window-smoke")]
        let mut native_probe = RealWindowSmokeProbe::new(
            demo,
            config,
            SmokePaintPalette::from_product_theme(product_theme),
        );
        #[cfg(feature = "real-window-smoke")]
        let mut native_probe_started = false;
        loop {
            cx.background_executor().timer(SMOKE_PROBE_INTERVAL).await;
            let mut observation = cx.update(|cx| observe_smoke_surface(&graph, cx));
            #[cfg(feature = "real-window-smoke")]
            {
                if !native_probe_started
                    && observation.structural_decision(&expectations) == SmokeProbeDecision::Pass
                {
                    native_probe_started = true;
                }
                if native_probe_started && observation.invariant_failure().is_none() {
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
                            let (phase, wait_ticks) = native_probe.progress();
                            observation.native_probe_phase = Some(phase);
                            observation.native_probe_wait_ticks = Some(wait_ticks);
                            observation.bind_captured_frame(
                                native_probe.geometry().cloned(),
                                native_probe.paint().cloned(),
                            );
                            match advance {
                                Ok(Ok(input)) => {
                                    if input.is_some() {
                                        observation.presented_frames =
                                            native_probe.presented_frames();
                                    }
                                    observation.input = input;
                                }
                                Ok(Err(reason)) => {
                                    observation.native_probe_failure = Some(reason);
                                }
                                Err(_) => {
                                    observation.native_probe_failure =
                                        Some("smoke_window_unavailable");
                                }
                            }
                        }
                        None => observation.native_probe_failure = Some("invalid_surface"),
                    }
                }
            }
            #[cfg(not(feature = "real-window-smoke"))]
            if observation.structural_decision(&expectations) == SmokeProbeDecision::Pass {
                let _ = (&window_handle, &view);
                observation.native_probe_failure = Some("smoke_feature_disabled");
            }
            let (decision, receipt) = {
                let mut state = state.borrow_mut();
                #[cfg(feature = "real-window-smoke")]
                let decision = if native_probe_started {
                    state.observe_active(observation)
                } else {
                    state.observe(observation)
                };
                #[cfg(not(feature = "real-window-smoke"))]
                let decision = state.observe(observation);
                let receipt = if decision == SmokeProbeDecision::Wait {
                    None
                } else {
                    state.receipt()
                };
                (decision, receipt)
            };
            if let Some(receipt) = receipt {
                emit_smoke_receipt(&receipt);
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

#[cfg(test)]
fn inspect_geometry(
    expectations: &SmokeExpectations,
    input: &SmokeGeometryInput,
    transform: ViewportTransform,
    surface_size: ViewSize,
    image_width: u32,
    image_height: u32,
    canvas_width: u32,
) -> Result<SmokeGeometryEvidence, &'static str> {
    inspect_geometry_with_coverage(
        expectations,
        input,
        transform,
        surface_size,
        image_width,
        image_height,
        canvas_width,
        true,
    )
}

#[cfg(any(test, feature = "real-window-smoke"))]
#[allow(clippy::too_many_arguments)]
fn inspect_visible_geometry(
    expectations: &SmokeExpectations,
    input: &SmokeGeometryInput,
    transform: ViewportTransform,
    surface_size: ViewSize,
    image_width: u32,
    image_height: u32,
    canvas_width: u32,
) -> Result<SmokeGeometryEvidence, &'static str> {
    inspect_geometry_with_coverage(
        expectations,
        input,
        transform,
        surface_size,
        image_width,
        image_height,
        canvas_width,
        false,
    )
}

#[cfg(any(test, feature = "real-window-smoke"))]
#[allow(clippy::too_many_arguments)]
fn inspect_geometry_with_coverage(
    expectations: &SmokeExpectations,
    input: &SmokeGeometryInput,
    transform: ViewportTransform,
    surface_size: ViewSize,
    image_width: u32,
    image_height: u32,
    canvas_width: u32,
    require_every_node_in_frame: bool,
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
        if !node.bounds.is_positive_finite() {
            return Err("geometry_invalid_node");
        }
        // Frame visibility is runtime-owned. A culled node only has document fallback bounds here,
        // which cannot reproduce fixed-scale native detail or semantic-LOD placement.
        let expected_region = if node.frame_visible {
            match project_node_region(
                node.bounds,
                transform,
                surface_size,
                image_height,
                canvas_width,
            ) {
                Ok(region) => Some(region),
                Err("geometry_node_out_of_frame") if !require_every_node_in_frame => None,
                Err(reason) => return Err(reason),
            }
        } else {
            None
        };
        let node_is_visible = expected_region.is_some();
        if let Some(region) = expected_region {
            node_regions.insert(*node_id, region);
        }
        if !node.fresh && node_is_visible {
            return Err("geometry_stale_node");
        }
        fresh_nodes = fresh_nodes.saturating_add(usize::from(node.fresh));

        for port_id in expected_ports {
            match node
                .ports
                .get(port_id)
                .ok_or("geometry_missing_port_outcome")?
            {
                SmokePortState::Rendered {
                    node: owner, valid, ..
                } if *owner == *node_id && *valid => {
                    rendered_ports = rendered_ports.saturating_add(1);
                }
                SmokePortState::Rendered { .. } => return Err("geometry_invalid_port"),
                SmokePortState::Hidden => return Err("geometry_port_hidden"),
                SmokePortState::Culled if !require_every_node_in_frame && !node_is_visible => {}
                SmokePortState::Culled => return Err("geometry_port_culled"),
                SmokePortState::Unavailable => return Err("geometry_port_unavailable"),
            }
        }
    }

    let mut routed_edges = 0usize;
    let mut routes = BTreeMap::new();
    let mut offscreen_endpoint_fallbacks = 0usize;
    for edge_id in &expectations.edges {
        if let Some(route) = input.routes.get(edge_id) {
            routed_edges = routed_edges.saturating_add(1);
            let (source_port, target_port) = expectations
                .edge_ports
                .get(edge_id)
                .copied()
                .ok_or("geometry_missing_edge_ports")?;
            let source =
                inspect_route_endpoint(route.source, source_port, input, transform, surface_size)?;
            let target =
                inspect_route_endpoint(route.target, target_port, input, transform, surface_size)?;
            offscreen_endpoint_fallbacks = offscreen_endpoint_fallbacks
                .saturating_add(usize::from(source.used_offscreen_fallback))
                .saturating_add(usize::from(target.used_offscreen_fallback));
            if route.samples.len() != WIRE_PATH_SAMPLE_COUNT
                || route
                    .samples
                    .iter()
                    .any(|sample| !sample.progress.is_finite() || !sample.point.is_finite())
            {
                return Err("geometry_invalid_route_samples");
            }
            let first = route
                .samples
                .first()
                .ok_or("geometry_invalid_route_samples")?;
            let last = route
                .samples
                .last()
                .ok_or("geometry_invalid_route_samples")?;
            if canvas_points_distance_in_view(first.point, route.source, transform)?
                > WIRE_ENDPOINT_TOLERANCE_IN_VIEW
                || canvas_points_distance_in_view(last.point, route.target, transform)?
                    > WIRE_ENDPOINT_TOLERANCE_IN_VIEW
            {
                return Err("geometry_route_path_endpoint_mismatch");
            }
            let visible_path_samples = route
                .samples
                .iter()
                .filter(|sample| sample.progress > 0.03 && sample.progress < 0.97)
                .filter_map(|sample| {
                    project_canvas_sample(
                        sample.point,
                        transform,
                        surface_size,
                        image_height,
                        canvas_width,
                    )
                })
                .collect();
            routes.insert(
                *edge_id,
                SmokeRouteGeometryEvidence {
                    source,
                    target,
                    path_samples: route.samples.clone(),
                    visible_path_samples,
                },
            );
            continue;
        }
        let Some((from, to)) = expectations.edge_nodes.get(edge_id) else {
            return Err("geometry_missing_edge_endpoints");
        };
        let edge_is_visible = node_regions.contains_key(from) && node_regions.contains_key(to);
        if require_every_node_in_frame || edge_is_visible {
            return Err("geometry_missing_route");
        }
    }
    if node_regions.is_empty() && routed_edges == 0 {
        return Err("geometry_no_visible_presentation");
    }

    Ok(SmokeGeometryEvidence {
        layout_revision: input.layout_revision,
        image_width,
        image_height,
        canvas_width,
        surface_size,
        expected_node_ids: expectations.node_ports.keys().copied().collect(),
        fresh_nodes,
        expected_port_ids: expectations
            .node_ports
            .values()
            .flatten()
            .copied()
            .collect(),
        rendered_ports,
        expected_edge_ids: expectations.edges.clone(),
        routed_edges,
        node_regions,
        routes,
        endpoint_geometry_verified: true,
        offscreen_endpoint_fallbacks,
    })
}

#[cfg(any(test, feature = "real-window-smoke"))]
fn inspect_route_endpoint(
    route_point: CanvasPoint,
    port: PortId,
    input: &SmokeGeometryInput,
    transform: ViewportTransform,
    surface_size: ViewSize,
) -> Result<SmokeEndpointGeometryEvidence, &'static str> {
    let state = input
        .nodes
        .values()
        .find_map(|node| node.ports.get(&port))
        .ok_or("geometry_missing_edge_port")?;
    match *state {
        SmokePortState::Rendered {
            route_anchor,
            visual_bounds,
            ..
        } => {
            let distance_in_view =
                canvas_points_distance_in_view(route_point, route_anchor, transform)?;
            let visible = canvas_point_is_visible(route_point, transform, surface_size)?
                || canvas_rect_intersects_surface(visual_bounds, transform, surface_size)?;
            let used_offscreen_fallback = distance_in_view > WIRE_ENDPOINT_TOLERANCE_IN_VIEW;
            if visible && used_offscreen_fallback {
                return Err("geometry_visible_route_endpoint_mismatch");
            }
            Ok(SmokeEndpointGeometryEvidence {
                route_point,
                port_anchor: Some(route_anchor),
                distance_in_view: Some(distance_in_view),
                visible,
                used_offscreen_fallback,
            })
        }
        SmokePortState::Culled => {
            let visible = canvas_point_is_visible(route_point, transform, surface_size)?;
            if visible {
                return Err("geometry_visible_route_endpoint_without_port");
            }
            Ok(SmokeEndpointGeometryEvidence {
                route_point,
                port_anchor: None,
                distance_in_view: None,
                visible: false,
                used_offscreen_fallback: true,
            })
        }
        SmokePortState::Hidden => Err("geometry_port_hidden"),
        SmokePortState::Unavailable => Err("geometry_port_unavailable"),
    }
}

#[cfg(any(test, feature = "real-window-smoke"))]
fn canvas_points_distance_in_view(
    left: CanvasPoint,
    right: CanvasPoint,
    transform: ViewportTransform,
) -> Result<f32, &'static str> {
    let left = transform
        .screen_point_for_canvas(left)
        .ok_or("geometry_invalid_route_endpoint")?;
    let right = transform
        .screen_point_for_canvas(right)
        .ok_or("geometry_invalid_route_endpoint")?;
    Ok((left.x - right.x).hypot(left.y - right.y))
}

#[cfg(any(test, feature = "real-window-smoke"))]
fn canvas_point_is_visible(
    point: CanvasPoint,
    transform: ViewportTransform,
    surface_size: ViewSize,
) -> Result<bool, &'static str> {
    let point = transform
        .screen_point_for_canvas(point)
        .ok_or("geometry_invalid_route_endpoint")?;
    Ok(point.x >= 0.0
        && point.y >= 0.0
        && point.x < surface_size.width
        && point.y < surface_size.height)
}

#[cfg(any(test, feature = "real-window-smoke"))]
fn canvas_rect_intersects_surface(
    bounds: CanvasRect,
    transform: ViewportTransform,
    surface_size: ViewSize,
) -> Result<bool, &'static str> {
    let min = transform
        .screen_point_for_canvas(bounds.origin)
        .ok_or("geometry_invalid_route_endpoint")?;
    let max = transform
        .screen_point_for_canvas(CanvasPoint {
            x: bounds.origin.x + bounds.size.width,
            y: bounds.origin.y + bounds.size.height,
        })
        .ok_or("geometry_invalid_route_endpoint")?;
    Ok(max.x > 0.0 && max.y > 0.0 && min.x < surface_size.width && min.y < surface_size.height)
}

#[cfg(any(test, feature = "real-window-smoke"))]
fn project_canvas_sample(
    point: CanvasPoint,
    transform: ViewportTransform,
    surface_size: ViewSize,
    image_height: u32,
    canvas_width: u32,
) -> Option<PixelPoint> {
    let point = transform.screen_point_for_canvas(point)?;
    if point.x < 0.0
        || point.y < 0.0
        || point.x >= surface_size.width
        || point.y >= surface_size.height
    {
        return None;
    }
    let scale_x = canvas_width as f64 / f64::from(surface_size.width);
    let scale_y = f64::from(image_height) / f64::from(surface_size.height);
    Some(PixelPoint {
        x: (f64::from(point.x) * scale_x)
            .round()
            .clamp(0.0, f64::from(canvas_width.saturating_sub(1))) as usize,
        y: (f64::from(point.y) * scale_y)
            .round()
            .clamp(0.0, f64::from(image_height.saturating_sub(1))) as usize,
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
    canvas_background: [u8; 4],
) -> Result<SmokePaintEvidence, &'static str> {
    if geometry.image_width != image_width
        || geometry.image_height != image_height
        || geometry.canvas_width == 0
        || geometry.canvas_width > image_width
        || (geometry.node_regions.is_empty() && geometry.routes.is_empty())
        || geometry.node_regions.len() > geometry.expected_node_ids.len()
    {
        return Err("paint_invalid_image");
    }

    let width = usize::try_from(image_width).map_err(|_| "paint_invalid_image")?;
    let height = usize::try_from(image_height).map_err(|_| "paint_invalid_image")?;
    let canvas_width = usize::try_from(geometry.canvas_width).map_err(|_| "paint_invalid_image")?;
    let row_stride = width.checked_mul(4).ok_or("paint_invalid_image")?;
    let (non_background_pixels, distinct_colors) = inspect_canvas_framebuffer(
        image_width,
        image_height,
        geometry.canvas_width,
        rgba,
        canvas_background,
    )?;

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
                if !is_background(&rgba[offset..offset + 4], canvas_background) {
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

    let wires = geometry
        .routes
        .iter()
        .filter(|(_, route)| route.visible_path_samples.len() >= MIN_WIRE_VISIBLE_SAMPLES)
        .map(|(edge, route)| {
            inspect_wire_corridor(
                image_width,
                image_height,
                geometry.canvas_width,
                geometry.surface_size,
                rgba,
                &route.visible_path_samples,
                canvas_background,
            )
            .map(|evidence| (*edge, evidence))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;

    Ok(SmokePaintEvidence {
        image_width,
        image_height,
        canvas_width: geometry.canvas_width,
        non_background_pixels,
        distinct_colors,
        nodes,
        wires,
    })
}

#[cfg(any(test, feature = "real-window-smoke"))]
fn inspect_wire_corridor(
    image_width: u32,
    image_height: u32,
    canvas_width: u32,
    surface_size: ViewSize,
    rgba: &[u8],
    samples: &[PixelPoint],
    canvas_background: [u8; 4],
) -> Result<SmokeWirePaintEvidence, &'static str> {
    let width = usize::try_from(image_width).map_err(|_| "paint_invalid_wire")?;
    let height = usize::try_from(image_height).map_err(|_| "paint_invalid_wire")?;
    let canvas_width = usize::try_from(canvas_width).map_err(|_| "paint_invalid_wire")?;
    let expected_len = width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or("paint_invalid_wire")?;
    if rgba.len() != expected_len
        || !surface_size.is_positive_finite()
        || canvas_width == 0
        || canvas_width > width
    {
        return Err("paint_invalid_wire");
    }
    let scale_x = canvas_width as f32 / surface_size.width;
    let scale_y = image_height as f32 / surface_size.height;
    let radius = (WIRE_PIXEL_CORRIDOR_RADIUS_IN_VIEW * scale_x.max(scale_y))
        .ceil()
        .max(2.0) as usize;
    let row_stride = width.checked_mul(4).ok_or("paint_invalid_wire")?;
    let painted_samples = samples
        .iter()
        .filter(|sample| {
            let left = sample.x.saturating_sub(radius);
            let top = sample.y.saturating_sub(radius);
            let right = sample.x.saturating_add(radius).min(canvas_width - 1);
            let bottom = sample.y.saturating_add(radius).min(height - 1);
            (top..=bottom).any(|y| {
                (left..=right).any(|x| {
                    let offset = y * row_stride + x * 4;
                    is_wire_pixel(&rgba[offset..offset + 4], canvas_background)
                })
            })
        })
        .count();
    let required_painted_samples =
        ((samples.len() as f32 * MIN_WIRE_PAINTED_SAMPLE_RATIO).ceil() as usize).max(1);
    Ok(SmokeWirePaintEvidence {
        visible_samples: samples.len(),
        painted_samples,
        required_painted_samples,
    })
}

#[cfg(any(test, feature = "real-window-smoke"))]
fn is_wire_pixel(pixel: &[u8], canvas_background: [u8; 4]) -> bool {
    if pixel.len() != 4 || pixel[3] <= 0x80 {
        return false;
    }
    let distance_squared = pixel[..3]
        .iter()
        .zip(canvas_background)
        .map(|(channel, background)| {
            let delta = i32::from(*channel) - i32::from(background);
            delta * delta
        })
        .sum::<i32>();
    distance_squared >= 45_i32.pow(2)
}

#[cfg(any(test, feature = "real-window-smoke"))]
#[allow(clippy::too_many_arguments)]
fn inspect_selection_chrome(
    bounds: CanvasRect,
    transform: ViewportTransform,
    surface_size: ViewSize,
    image_width: u32,
    image_height: u32,
    canvas_width: u32,
    palette: SmokeSelectionChromePalette,
    rgba: &[u8],
) -> Result<SmokeSelectionChromeEvidence, &'static str> {
    if !bounds.is_positive_finite() || !transform.is_valid() {
        return Err("paint_selection_chrome_invalid");
    }

    let max_canvas = CanvasPoint {
        x: bounds.origin.x + bounds.size.width,
        y: bounds.origin.y + bounds.size.height,
    };
    let min = transform
        .screen_point_for_canvas(bounds.origin)
        .ok_or("paint_selection_chrome_invalid")?;
    let max = transform
        .screen_point_for_canvas(max_canvas)
        .ok_or("paint_selection_chrome_invalid")?;
    inspect_selection_chrome_in_view(
        ViewRect::new(min, ViewSize::new(max.x - min.x, max.y - min.y)),
        surface_size,
        image_width,
        image_height,
        canvas_width,
        palette,
        rgba,
    )
}

#[cfg(any(test, feature = "real-window-smoke"))]
#[allow(clippy::too_many_arguments)]
fn inspect_selection_chrome_in_view(
    bounds: ViewRect,
    surface_size: ViewSize,
    image_width: u32,
    image_height: u32,
    canvas_width: u32,
    palette: SmokeSelectionChromePalette,
    rgba: &[u8],
) -> Result<SmokeSelectionChromeEvidence, &'static str> {
    if !bounds.is_positive_finite()
        || !surface_size.is_positive_finite()
        || image_width == 0
        || image_height == 0
        || canvas_width == 0
        || canvas_width > image_width
    {
        return Err("paint_selection_chrome_invalid");
    }
    let width = usize::try_from(image_width).map_err(|_| "paint_selection_chrome_invalid")?;
    let height = usize::try_from(image_height).map_err(|_| "paint_selection_chrome_invalid")?;
    let canvas_width =
        usize::try_from(canvas_width).map_err(|_| "paint_selection_chrome_invalid")?;
    let expected_len = width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or("paint_selection_chrome_invalid")?;
    if rgba.len() != expected_len {
        return Err("paint_selection_chrome_invalid");
    }

    let scale_x = canvas_width as f32 / surface_size.width;
    let scale_y = image_height as f32 / surface_size.height;
    let left = bounds.origin.x * scale_x;
    let top = bounds.origin.y * scale_y;
    let right = (bounds.origin.x + bounds.size.width) * scale_x;
    let bottom = (bounds.origin.y + bounds.size.height) * scale_y;
    if ![left, top, right, bottom, scale_x, scale_y]
        .into_iter()
        .all(f32::is_finite)
    {
        return Err("paint_selection_chrome_invalid");
    }

    let band = (scale_x.max(scale_y) * 2.0).ceil().max(2.0) as isize;
    let row_stride = width
        .checked_mul(4)
        .ok_or("paint_selection_chrome_invalid")?;
    let horizontal_start = left.max(0.0).ceil() as isize;
    let horizontal_end = right.min(canvas_width as f32).floor() as isize;
    let vertical_start = top.max(0.0).ceil() as isize;
    let vertical_end = bottom.min(height as f32).floor() as isize;

    let mut expected_sides = 0_u8;
    let mut matched_sides = 0_u8;
    let top_match = inspect_horizontal_chrome_side(
        rgba,
        row_stride,
        canvas_width,
        height,
        top.round() as isize,
        horizontal_start,
        horizontal_end,
        band,
        palette,
    );
    let bottom_match = inspect_horizontal_chrome_side(
        rgba,
        row_stride,
        canvas_width,
        height,
        bottom.round() as isize,
        horizontal_start,
        horizontal_end,
        band,
        palette,
    );
    let left_match = inspect_vertical_chrome_side(
        rgba,
        row_stride,
        canvas_width,
        height,
        left.round() as isize,
        vertical_start,
        vertical_end,
        band,
        palette,
    );
    let right_match = inspect_vertical_chrome_side(
        rgba,
        row_stride,
        canvas_width,
        height,
        right.round() as isize,
        vertical_start,
        vertical_end,
        band,
        palette,
    );
    for matched in [top_match, bottom_match, left_match, right_match]
        .into_iter()
        .flatten()
    {
        expected_sides = expected_sides.saturating_add(1);
        matched_sides = matched_sides.saturating_add(u8::from(matched));
    }

    let detached_start = (bottom + scale_y * 4.0).ceil().max(0.0) as usize;
    let allowed_detached_run = (MAX_DETACHED_CHROME_RUN_IN_VIEW * scale_y).ceil().max(8.0) as usize;
    let detached_max_run = [left, right]
        .into_iter()
        .filter(|x| *x >= -(band as f32) && *x <= canvas_width as f32 + band as f32)
        .map(|x| {
            longest_vertical_chrome_run(
                rgba,
                row_stride,
                canvas_width,
                height,
                x.round() as isize,
                detached_start,
                band,
                palette,
            )
        })
        .max()
        .unwrap_or(0);

    Ok(SmokeSelectionChromeEvidence {
        expected_sides,
        matched_sides,
        bottom_expected: bottom_match.is_some(),
        bottom_matched: bottom_match.unwrap_or(false),
        detached_max_run,
        allowed_detached_run,
    })
}

#[cfg(any(test, feature = "real-window-smoke"))]
#[allow(clippy::too_many_arguments)]
fn inspect_horizontal_chrome_side(
    rgba: &[u8],
    row_stride: usize,
    canvas_width: usize,
    image_height: usize,
    y: isize,
    start: isize,
    end: isize,
    band: isize,
    palette: SmokeSelectionChromePalette,
) -> Option<bool> {
    let start = start.max(0) as usize;
    let end = end.min(canvas_width as isize) as usize;
    let length = end.saturating_sub(start);
    if length < MIN_SELECTION_CHROME_SIDE_PIXELS || y < -band || y >= image_height as isize + band {
        return None;
    }
    let hits = (start..end)
        .filter(|x| {
            ((y - band).max(0)..=(y + band).min(image_height as isize - 1)).any(|sample_y| {
                selection_chrome_pixel(
                    rgba,
                    row_stride,
                    *x,
                    usize::try_from(sample_y).expect("clamped image row"),
                    palette,
                )
            })
        })
        .count();
    Some(hits >= MIN_SELECTION_CHROME_SIDE_PIXELS.max(length / 3))
}

#[cfg(any(test, feature = "real-window-smoke"))]
#[allow(clippy::too_many_arguments)]
fn inspect_vertical_chrome_side(
    rgba: &[u8],
    row_stride: usize,
    canvas_width: usize,
    image_height: usize,
    x: isize,
    start: isize,
    end: isize,
    band: isize,
    palette: SmokeSelectionChromePalette,
) -> Option<bool> {
    let start = start.max(0) as usize;
    let end = end.min(image_height as isize) as usize;
    let length = end.saturating_sub(start);
    if length < MIN_SELECTION_CHROME_SIDE_PIXELS || x < -band || x >= canvas_width as isize + band {
        return None;
    }
    let hits = (start..end)
        .filter(|y| {
            ((x - band).max(0)..=(x + band).min(canvas_width as isize - 1)).any(|sample_x| {
                selection_chrome_pixel(
                    rgba,
                    row_stride,
                    usize::try_from(sample_x).expect("clamped canvas column"),
                    *y,
                    palette,
                )
            })
        })
        .count();
    Some(hits >= MIN_SELECTION_CHROME_SIDE_PIXELS.max(length / 3))
}

#[cfg(any(test, feature = "real-window-smoke"))]
#[allow(clippy::too_many_arguments)]
fn longest_vertical_chrome_run(
    rgba: &[u8],
    row_stride: usize,
    canvas_width: usize,
    image_height: usize,
    x: isize,
    start: usize,
    band: isize,
    palette: SmokeSelectionChromePalette,
) -> usize {
    let mut longest = 0usize;
    let mut current = 0usize;
    for y in start.min(image_height)..image_height {
        let matched =
            ((x - band).max(0)..=(x + band).min(canvas_width as isize - 1)).any(|sample_x| {
                selection_chrome_pixel(
                    rgba,
                    row_stride,
                    usize::try_from(sample_x).expect("clamped canvas column"),
                    y,
                    palette,
                )
            });
        if matched {
            current = current.saturating_add(1);
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    longest
}

#[cfg(any(test, feature = "real-window-smoke"))]
fn selection_chrome_pixel(
    rgba: &[u8],
    row_stride: usize,
    x: usize,
    y: usize,
    palette: SmokeSelectionChromePalette,
) -> bool {
    let Some(offset) = y
        .checked_mul(row_stride)
        .and_then(|row| x.checked_mul(4).and_then(|column| row.checked_add(column)))
    else {
        return false;
    };
    let Some(pixel) = offset.checked_add(4).and_then(|end| rgba.get(offset..end)) else {
        return false;
    };
    pixel[3] > 0x80 && palette.matches(pixel)
}

#[cfg(any(test, feature = "real-window-smoke"))]
fn inspect_canvas_framebuffer(
    image_width: u32,
    image_height: u32,
    canvas_width: u32,
    rgba: &[u8],
    canvas_background: [u8; 4],
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
            if is_background(pixel, canvas_background) {
                continue;
            }
            non_background_pixels = non_background_pixels.saturating_add(1);
            colors.insert([pixel[0], pixel[1], pixel[2]]);
        }
    }
    Ok((non_background_pixels, colors.len()))
}

#[cfg(any(test, feature = "real-window-smoke"))]
fn is_background(pixel: &[u8], canvas_background: [u8; 4]) -> bool {
    pixel[3] == 0 || pixel == canvas_background
}

fn observe_smoke_surface(graph: &Entity<NativeGraphSurface>, cx: &mut App) -> SmokeObservation {
    let host = graph.read(cx).host().clone();
    host.update(cx, |host, _| SmokeObservation {
        surface_size: host.surface_size(),
        diagnostic: host
            .last_diagnostic()
            .map(|diagnostic| SmokeHostDiagnostic {
                kind: diagnostic.kind(),
                message: format!("{:?}: {}", diagnostic.kind(), diagnostic.message()),
            }),
        host_closed: host.close_reason().is_some(),
        geometry: None,
        paint: None,
        input: None,
        native_probe_failure: None,
        native_probe_phase: None,
        native_probe_wait_ticks: None,
        presented_frames: Vec::new(),
    })
}

pub(super) fn start_smoke_timeout(cx: &App, state: SharedSmokeState) {
    cx.spawn(async move |cx| {
        cx.background_executor().timer(SMOKE_TIMEOUT).await;
        cx.update(|cx| fail_smoke(cx, &state, "timeout"));
    })
    .detach();
}

fn terminate_failed_smoke(
    state: &SharedSmokeState,
    reason: &'static str,
    request_shutdown: impl FnOnce(),
) {
    let receipt = {
        let mut state = state.borrow_mut();
        state.fail(reason);
        state.receipt()
    };
    if let Some(receipt) = receipt {
        emit_smoke_receipt(&receipt);
    }
    request_shutdown();
}

pub(super) fn fail_smoke(cx: &mut App, state: &SharedSmokeState, reason: &'static str) {
    terminate_failed_smoke(state, reason, || cx.quit());
}

pub(super) fn finish_smoke_after_run(state: SharedSmokeState) -> ExitCode {
    let (outcome, receipt) = {
        let mut state = state.borrow_mut();
        state.fail("event_loop_returned_before_complete");
        (state.outcome, state.receipt())
    };
    if let Some(receipt) = receipt {
        emit_smoke_receipt(&receipt);
    }
    ExitCode::from(smoke_exit_status(outcome))
}

const fn smoke_exit_status(outcome: SmokeOutcome) -> u8 {
    match outcome {
        SmokeOutcome::Passed => 0,
        SmokeOutcome::Pending | SmokeOutcome::Failed(_) => 1,
    }
}

fn emit_smoke_receipt(receipt: &SmokeRunReceiptV1) {
    let payload = serde_json::to_string(receipt).expect("smoke receipt must serialize");
    eprintln!("{SMOKE_RECEIPT_PREFIX}{payload}");
    let _ = io::stderr().flush();
}

#[cfg(test)]
mod tests {
    use super::presented_frame::{
        SmokeFrameStage, SmokeNodeBounds, SmokePreviewRoute, SmokePreviewTarget, SmokeRoute,
    };
    use super::receipt::SmokeReceiptStatusV1;
    use super::*;
    use jellyflow_core::CanvasSize;
    use jellyflow_runtime::geometry::{ViewPoint, ViewRect};
    use jellyflow_runtime::interaction::CanvasInteractionPhase;

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
            edge_nodes: BTreeMap::from([(edge(31), (node(1), node(2)))]),
            edge_ports: BTreeMap::from([(edge(31), (port(11), port(21)))]),
        }
    }

    fn rendered_port(owner: NodeId, anchor: CanvasPoint) -> SmokePortState {
        SmokePortState::Rendered {
            node: owner,
            valid: true,
            visual_bounds: CanvasRect {
                origin: CanvasPoint {
                    x: anchor.x - 6.0,
                    y: anchor.y - 6.0,
                },
                size: CanvasSize {
                    width: 12.0,
                    height: 12.0,
                },
            },
            route_anchor: anchor,
        }
    }

    fn route_input(source: CanvasPoint, target: CanvasPoint) -> SmokeRouteGeometryInput {
        SmokeRouteGeometryInput {
            source,
            target,
            samples: (0..WIRE_PATH_SAMPLE_COUNT)
                .map(|index| {
                    let progress = index as f32 / (WIRE_PATH_SAMPLE_COUNT - 1) as f32;
                    SmokePathSampleInput {
                        progress,
                        point: CanvasPoint {
                            x: source.x + (target.x - source.x) * progress,
                            y: source.y + (target.y - source.y) * progress,
                        },
                    }
                })
                .collect(),
        }
    }

    fn geometry_input() -> SmokeGeometryInput {
        SmokeGeometryInput {
            layout_revision: 7,
            nodes: BTreeMap::from([
                (
                    node(1),
                    SmokeNodeGeometryInput {
                        frame_visible: true,
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
                                rendered_port(node(1), CanvasPoint { x: 350.0, y: 200.0 }),
                            ),
                            (
                                port(12),
                                rendered_port(node(1), CanvasPoint { x: 350.0, y: 260.0 }),
                            ),
                        ]),
                    },
                ),
                (
                    node(2),
                    SmokeNodeGeometryInput {
                        frame_visible: true,
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
                                rendered_port(node(2), CanvasPoint { x: 500.0, y: 400.0 }),
                            ),
                            (
                                port(22),
                                rendered_port(node(2), CanvasPoint { x: 500.0, y: 460.0 }),
                            ),
                        ]),
                    },
                ),
            ]),
            routes: BTreeMap::from([(
                edge(31),
                route_input(
                    CanvasPoint { x: 350.0, y: 200.0 },
                    CanvasPoint { x: 500.0, y: 400.0 },
                ),
            )]),
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
            TEST_CANVAS_BACKGROUND_RGBA.repeat(IMAGE_WIDTH as usize * IMAGE_HEIGHT as usize);
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
        for route in geometry.routes.values() {
            for point in &route.visible_path_samples {
                let offset = (point.y * IMAGE_WIDTH as usize + point.x) * 4;
                pixels[offset..offset + 4].copy_from_slice(&[0x59, 0xc2, 0xa6, 0xff]);
            }
        }
        pixels
    }

    fn complete_observation(expected: &SmokeExpectations) -> SmokeObservation {
        let geometry = geometry_evidence();
        let pixels = paint_all_node_regions(&geometry, expected.node_ports.keys().copied());
        let paint = inspect_canvas_paint(
            IMAGE_WIDTH,
            IMAGE_HEIGHT,
            &pixels,
            &geometry,
            TEST_CANVAS_BACKGROUND_RGBA,
        )
        .expect("paint evidence");
        SmokeObservation {
            surface_size: Some(surface_size()),
            diagnostic: None,
            host_closed: false,
            geometry: Some(geometry),
            paint: Some(paint),
            input: Some(complete_input_evidence()),
            native_probe_failure: None,
            native_probe_phase: None,
            native_probe_wait_ticks: None,
            presented_frames: interaction_presented_frames(expected),
        }
    }

    fn presented_route(edge: EdgeId) -> SmokeRoute {
        SmokeRoute {
            edge,
            source: ViewPoint::new(30.0, 40.0),
            target: ViewPoint::new(160.0, 90.0),
        }
    }

    fn presented_frame(
        render_sequence: u64,
        stage: SmokeFrameStage,
        node: NodeId,
        authoritative_edge: EdgeId,
        preview: bool,
        authoritative: bool,
        node_drag_active: bool,
    ) -> SmokePresentedFrame {
        let node_bounds = vec![SmokeNodeBounds {
            node,
            bounds: ViewRect::new(ViewPoint::new(20.0, 30.0), ViewSize::new(120.0, 80.0)),
            selected: true,
            chrome_expected: true,
            chrome_bounds: Some(ViewRect::new(
                ViewPoint::new(20.0, 30.0),
                ViewSize::new(120.0, 80.0),
            )),
        }];
        SmokePresentedFrame {
            render_sequence,
            stage,
            phase: CanvasInteractionPhase::Idle,
            graph_revision: render_sequence,
            viewport: ViewportTransform::new(CanvasPoint::default(), 1.0)
                .expect("valid fixture viewport"),
            node_drag_nodes: if node_drag_active {
                vec![node]
            } else {
                Vec::new()
            },
            node_bounds,
            port_geometries: Vec::new(),
            preview_route: preview.then(|| SmokePreviewRoute {
                source: ViewPoint::new(30.0, 40.0),
                target: ViewPoint::new(160.0, 90.0),
            }),
            preview_target: preview.then(|| SmokePreviewTarget {
                port: port(21),
                anchor: ViewPoint::new(160.0, 90.0),
            }),
            suppressed_edge: (stage == SmokeFrameStage::ReconnectPending).then(|| edge(32)),
            authoritative_routes: if authoritative {
                vec![presented_route(authoritative_edge)]
            } else {
                Vec::new()
            },
        }
    }

    fn interaction_presented_frames(expected: &SmokeExpectations) -> Vec<SmokePresentedFrame> {
        let node = *expected.node_ports.keys().next().expect("fixture node");
        let existing_edge = *expected.edges.iter().next().expect("fixture edge");
        let added_edge = edge(32);
        let mut frames = vec![
            presented_frame(
                0,
                SmokeFrameStage::Interaction,
                node,
                existing_edge,
                false,
                true,
                false,
            ),
            presented_frame(
                1,
                SmokeFrameStage::NodeDragPending,
                node,
                existing_edge,
                false,
                true,
                true,
            ),
            presented_frame(
                2,
                SmokeFrameStage::NodeDragAuthoritative,
                node,
                existing_edge,
                false,
                true,
                false,
            ),
            presented_frame(
                3,
                SmokeFrameStage::ConnectionPending,
                node,
                existing_edge,
                true,
                true,
                false,
            ),
            presented_frame(
                4,
                SmokeFrameStage::ConnectionAuthoritative,
                node,
                existing_edge,
                false,
                true,
                false,
            ),
            presented_frame(
                5,
                SmokeFrameStage::ReconnectPending,
                node,
                existing_edge,
                true,
                true,
                false,
            ),
            presented_frame(
                6,
                SmokeFrameStage::ReconnectAuthoritative,
                node,
                existing_edge,
                false,
                true,
                false,
            ),
            presented_frame(
                7,
                SmokeFrameStage::Search,
                node,
                existing_edge,
                false,
                true,
                false,
            ),
            presented_frame(
                8,
                SmokeFrameStage::Final,
                node,
                existing_edge,
                false,
                true,
                false,
            ),
        ];
        frames[4]
            .authoritative_routes
            .push(presented_route(added_edge));
        frames[6]
            .authoritative_routes
            .push(presented_route(added_edge));
        frames[7]
            .authoritative_routes
            .push(presented_route(added_edge));
        frames[8]
            .authoritative_routes
            .push(presented_route(added_edge));
        frames
    }

    fn resequence_presented_frames(frames: &mut [SmokePresentedFrame]) {
        for (sequence, frame) in frames.iter_mut().enumerate() {
            let sequence = sequence as u64;
            frame.render_sequence = sequence;
            frame.graph_revision = sequence;
        }
    }

    fn complete_input_evidence() -> SmokeInputEvidence {
        SmokeInputEvidence {
            scenario: SmokeScenarioEvidence::Interaction(SmokeInteractionEvidence {
                captures: INTERACTION_CAPTURE_STATES.into_iter().collect(),
            }),
        }
    }

    fn paint_chrome_rect(
        pixels: &mut [u8],
        width: usize,
        left: usize,
        top: usize,
        right: usize,
        bottom: usize,
        color: [u8; 4],
    ) {
        for x in left..=right {
            for y in [top, bottom] {
                let offset = (y * width + x) * 4;
                pixels[offset..offset + 4].copy_from_slice(&color);
            }
        }
        for y in top..=bottom {
            for x in [left, right] {
                let offset = (y * width + x) * 4;
                pixels[offset..offset + 4].copy_from_slice(&color);
            }
        }
    }

    #[test]
    fn selection_chrome_probe_accepts_one_fitted_outline_and_rejects_a_detached_second_frame() {
        const WIDTH: usize = 200;
        const HEIGHT: usize = 200;
        let bounds = CanvasRect {
            origin: CanvasPoint { x: 40.0, y: 30.0 },
            size: CanvasSize {
                width: 100.0,
                height: 60.0,
            },
        };
        let palette =
            SmokeSelectionChromePalette::from_product_theme(NativeProductTheme::shader_blueprint());
        let mut pixels = TEST_CANVAS_BACKGROUND_RGBA.repeat(WIDTH * HEIGHT);
        paint_chrome_rect(
            &mut pixels,
            WIDTH,
            40,
            30,
            140,
            90,
            [palette.focus[0], palette.focus[1], palette.focus[2], 0xff],
        );

        let fitted = inspect_selection_chrome(
            bounds,
            transform(),
            ViewSize::new(WIDTH as f32, HEIGHT as f32),
            WIDTH as u32,
            HEIGHT as u32,
            WIDTH as u32,
            palette,
            &pixels,
        )
        .expect("fitted selection chrome evidence");
        assert_eq!(fitted.failure_reason(), None);
        assert_eq!(fitted.matched_sides, 4);
        assert_eq!(
            SmokeSelectionChromeEvidence {
                expected_sides: 4,
                matched_sides: 3,
                bottom_expected: true,
                bottom_matched: true,
                detached_max_run: 0,
                allowed_detached_run: 0,
            }
            .failure_reason(),
            Some("paint_selection_chrome_not_fitted"),
            "a fully visible selected node requires all four chrome sides",
        );
        assert_eq!(
            SmokeSelectionChromeEvidence {
                expected_sides: 3,
                matched_sides: 3,
                bottom_expected: true,
                bottom_matched: true,
                detached_max_run: 0,
                allowed_detached_run: 0,
            }
            .failure_reason(),
            None,
            "a partially clipped node requires every visible chrome side",
        );

        paint_chrome_rect(
            &mut pixels,
            WIDTH,
            40,
            90,
            140,
            160,
            [palette.focus[0], palette.focus[1], palette.focus[2], 0xff],
        );
        let detached = inspect_selection_chrome(
            bounds,
            transform(),
            ViewSize::new(WIDTH as f32, HEIGHT as f32),
            WIDTH as u32,
            HEIGHT as u32,
            WIDTH as u32,
            palette,
            &pixels,
        )
        .expect("detached selection chrome evidence");
        assert_eq!(
            detached.failure_reason(),
            Some("paint_detached_selection_chrome")
        );
    }

    #[test]
    fn geometry_probe_rejects_stale_nodes_and_missing_routes() {
        let expected = expectations();

        let mut stale = geometry_input();
        stale.nodes.get_mut(&node(1)).expect("fixture node").fresh = false;
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
    }

    #[test]
    fn paint_probe_rejects_blank_canvas_and_blank_expected_node() {
        let geometry = geometry_evidence();
        let blank =
            TEST_CANVAS_BACKGROUND_RGBA.repeat(IMAGE_WIDTH as usize * IMAGE_HEIGHT as usize);
        let blank = inspect_canvas_paint(
            IMAGE_WIDTH,
            IMAGE_HEIGHT,
            &blank,
            &geometry,
            TEST_CANVAS_BACKGROUND_RGBA,
        )
        .expect("well-formed blank image");
        assert_eq!(blank.failure_reason(), Some("paint_blank_canvas"));

        let pixels = paint_all_node_regions(&geometry, [node(1)]);
        let paint = inspect_canvas_paint(
            IMAGE_WIDTH,
            IMAGE_HEIGHT,
            &pixels,
            &geometry,
            TEST_CANVAS_BACKGROUND_RGBA,
        )
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
        assert_eq!(
            complete.decision(&expected, &SmokeConfig::default()),
            SmokeProbeDecision::Pass
        );

        let mut bootstrapping = complete.clone();
        bootstrapping.surface_size = None;
        assert_eq!(
            bootstrapping.decision(&expected, &SmokeConfig::default()),
            SmokeProbeDecision::Wait
        );

        let mut missing_geometry = complete;
        missing_geometry.geometry = None;
        assert_eq!(
            missing_geometry.decision(&expected, &SmokeConfig::default()),
            SmokeProbeDecision::Wait
        );
    }

    #[test]
    fn rust_harness_rejects_invalid_frame_order_preview_and_drag_cohorts() {
        let expected = expectations();

        let mut gapped = interaction_presented_frames(&expected);
        gapped[1].render_sequence = 3;
        assert_eq!(
            validate_presented_frames(&gapped, SmokeScenario::Interaction),
            Err("presented_frame_order_invalid")
        );

        let mut revision_regressed = interaction_presented_frames(&expected);
        revision_regressed[2].graph_revision = 0;
        assert_eq!(
            validate_presented_frames(&revision_regressed, SmokeScenario::Interaction),
            Err("presented_frame_order_invalid")
        );

        let mut missing_preview = interaction_presented_frames(&expected);
        missing_preview[3].preview_route = None;
        assert_eq!(
            validate_presented_frames(&missing_preview, SmokeScenario::Interaction),
            Err("connection_pending_preview_missing")
        );

        let mut invalid_drag = interaction_presented_frames(&expected);
        invalid_drag[1].node_drag_nodes = vec![NodeId::from_u128(0xffff)];
        assert_eq!(
            validate_presented_frames(&invalid_drag, SmokeScenario::Interaction),
            Err("node_drag_cohort_invalid")
        );
    }

    #[test]
    fn rust_harness_rejects_node_and_route_release_jumps() {
        let expected = expectations();

        let mut node_jump = interaction_presented_frames(&expected);
        node_jump[2].node_bounds[0].bounds.origin.x += 1.0;
        node_jump[2].node_bounds[0]
            .chrome_bounds
            .as_mut()
            .expect("selected fixture chrome")
            .origin
            .x += 1.0;
        assert_eq!(
            validate_presented_frames(&node_jump, SmokeScenario::Interaction),
            Err("node_drag_release_jump")
        );

        let mut route_jump = interaction_presented_frames(&expected);
        route_jump[4].authoritative_routes[1].target.x += 1.0;
        assert_eq!(
            validate_presented_frames(&route_jump, SmokeScenario::Interaction),
            Err("connection_route_swap_not_atomic")
        );
    }

    #[test]
    fn rust_harness_allows_target_reacquisition_until_commit_begins() {
        let expected = expectations();
        let mut frames = interaction_presented_frames(&expected);
        let mut exploring = frames[3].clone();
        exploring.stage = SmokeFrameStage::ConnectionExploring;
        exploring.preview_target = None;
        let reacquired = frames[3].clone();
        frames.insert(4, exploring.clone());
        frames.insert(5, reacquired);
        resequence_presented_frames(&mut frames);
        assert_eq!(
            validate_presented_frames(&frames, SmokeScenario::Interaction),
            Ok(())
        );

        let mut committing = interaction_presented_frames(&expected);
        committing[3].phase = CanvasInteractionPhase::Committing;
        committing.insert(4, exploring);
        resequence_presented_frames(&mut committing);
        assert_eq!(
            validate_presented_frames(&committing, SmokeScenario::Interaction),
            Err("connection_commit_span_interrupted")
        );
    }

    #[test]
    fn startup_failure_emits_one_valid_failed_receipt_and_nonzero_status() {
        let config = SmokeConfig::for_test(
            SmokeBounds::Normal,
            SmokeScenario::LodThreshold,
            SmokeZoom::Two,
        );
        let mut state = SmokeState::new(NativeDemo::Workflow, config);
        state.fail("window_open");
        let receipt = state.receipt().expect("startup failure receipt");

        assert_eq!(receipt.status, SmokeReceiptStatusV1::Fail);
        assert_eq!(receipt.run_id, "lod-threshold-workflow-normal-z2");
        assert_eq!(receipt.failure.expect("failure").code, "window_open");
        assert!(receipt.artifacts.is_empty());
        assert!(state.receipt().is_none());
        assert_eq!(smoke_exit_status(SmokeOutcome::Passed), 0);
        assert_ne!(smoke_exit_status(SmokeOutcome::Pending), 0);
        assert_ne!(smoke_exit_status(SmokeOutcome::Failed("test_failure")), 0);
    }
}
