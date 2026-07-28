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
use jellyflow_runtime::geometry::ViewSize;
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

#[cfg(feature = "real-window-smoke")]
mod input_probe;
#[cfg(feature = "real-window-smoke")]
use input_probe::RealWindowSmokeProbe;
mod config;
use config::SmokeCaptureState;
pub(super) use config::SmokeConfig;
use config::SmokeScenario;
#[cfg(test)]
use config::{SmokeBounds, SmokeZoom};

const SMOKE_TIMEOUT: Duration = Duration::from_secs(15);
const SMOKE_PROBE_INTERVAL: Duration = Duration::from_millis(16);
const COMPLETE_FRAME_FEEDBACK_SUBMISSIONS: u64 = 2;
const SMOKE_MARKER: &str = "JELLYFLOW_REAL_WINDOW_SMOKE";
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
const MAX_DETACHED_CHROME_RUN_IN_VIEW: f32 = 8.0;
const SMOKE_INPUT_EVENT_COUNT: u16 = 36;
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

    fn port_count(&self) -> usize {
        self.node_ports.values().map(BTreeSet::len).sum()
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
    bottom_matched: bool,
    detached_max_run: usize,
    allowed_detached_run: usize,
}

#[cfg(any(test, feature = "real-window-smoke"))]
impl SmokeSelectionChromeEvidence {
    fn failure_reason(self) -> Option<&'static str> {
        if self.expected_sides != 4 || self.matched_sides != 4 || !self.bottom_matched {
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
    bounds_verified: bool,
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
    dispatched_events: u16,
    auto_pan_steering_events: u16,
    captures: BTreeSet<SmokeCaptureState>,
    node_dragged: bool,
    selection_chrome_fitted: bool,
    selection_chrome_sides: u8,
    selection_chrome_detached_run: usize,
    auto_pan_frames: u8,
    connection_auto_pan_frames: u8,
    tab_focus_moved: bool,
    tab_focus_restored: bool,
    viewport_panned: bool,
    viewport_zoomed: bool,
    connection_created: bool,
    connection_preview_wire_painted: bool,
    connection_release_frame_painted: bool,
    connection_release_continuous: bool,
    keyboard_edge_deleted: bool,
    reconnect_applied: bool,
    reconnect_preview_wire_painted: bool,
    reconnect_release_frame_painted: bool,
    reconnect_release_continuous: bool,
    search_opened: bool,
    search_focus_moved: bool,
    search_cancelled: bool,
    search_focus_restored: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct SmokeLodEvidence {
    captures: BTreeSet<SmokeCaptureState>,
    lod_continuous: bool,
    lod_max_jump_in_view: f32,
}

impl SmokeInputEvidence {
    fn failure_reason(&self, configured_scenario: SmokeScenario) -> Option<&'static str> {
        if !self.bounds_verified {
            return Some("input_window_bounds_mismatch");
        }
        match (&self.scenario, configured_scenario) {
            (SmokeScenarioEvidence::Interaction(interaction), SmokeScenario::Interaction) => {
                if interaction.dispatched_events
                    != SMOKE_INPUT_EVENT_COUNT.saturating_add(interaction.auto_pan_steering_events)
                {
                    return Some("input_dispatch_incomplete");
                }
                if !captures_complete(&interaction.captures, &INTERACTION_CAPTURE_STATES) {
                    return Some("input_target_zoom_capture_mismatch");
                }
                if interaction.auto_pan_frames < SMOKE_AUTOPAN_MIN_FRAMES {
                    return Some("input_autopan_frames_missing");
                }
                if interaction.connection_auto_pan_frames < SMOKE_AUTOPAN_MIN_FRAMES {
                    return Some("input_connection_autopan_frames_missing");
                }
                for (complete, reason) in [
                    (interaction.node_dragged, "input_node_unchanged"),
                    (
                        interaction.selection_chrome_fitted,
                        "paint_selection_chrome_not_fitted",
                    ),
                    (
                        interaction.selection_chrome_sides == 4,
                        "paint_selection_chrome_not_fitted",
                    ),
                    (interaction.tab_focus_moved, "input_tab_focus_unmoved"),
                    (
                        interaction.tab_focus_restored,
                        "input_shift_tab_focus_unrestored",
                    ),
                    (interaction.viewport_panned, "input_viewport_unpanned"),
                    (interaction.viewport_zoomed, "input_viewport_unzoomed"),
                    (interaction.connection_created, "input_connection_missing"),
                    (
                        interaction.connection_preview_wire_painted,
                        "paint_connection_preview_wire_missing",
                    ),
                    (
                        interaction.connection_release_frame_painted,
                        "paint_connection_release_frame_missing",
                    ),
                    (
                        interaction.connection_release_continuous,
                        "paint_connection_release_discontinuity",
                    ),
                    (
                        interaction.keyboard_edge_deleted,
                        "input_keyboard_delete_missing",
                    ),
                    (interaction.reconnect_applied, "input_reconnect_missing"),
                    (
                        interaction.reconnect_preview_wire_painted,
                        "paint_reconnect_preview_wire_missing",
                    ),
                    (
                        interaction.reconnect_release_frame_painted,
                        "paint_reconnect_release_frame_missing",
                    ),
                    (
                        interaction.reconnect_release_continuous,
                        "paint_reconnect_release_discontinuity",
                    ),
                    (interaction.search_opened, "input_search_missing"),
                    (interaction.search_focus_moved, "input_search_focus_missing"),
                    (interaction.search_cancelled, "input_search_not_cancelled"),
                    (
                        interaction.search_focus_restored,
                        "input_search_focus_not_restored",
                    ),
                    (
                        interaction.captures.contains(&SmokeCaptureState::Final),
                        "input_final_frame_invalid",
                    ),
                ] {
                    if !complete {
                        return Some(reason);
                    }
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

    fn interaction(&self) -> Option<&SmokeInteractionEvidence> {
        match &self.scenario {
            SmokeScenarioEvidence::Interaction(evidence) => Some(evidence),
            SmokeScenarioEvidence::LodThreshold(_) => None,
        }
    }

    fn lod(&self) -> Option<&SmokeLodEvidence> {
        match &self.scenario {
            SmokeScenarioEvidence::Interaction(_) => None,
            SmokeScenarioEvidence::LodThreshold(evidence) => Some(evidence),
        }
    }

    fn scenario_label(&self) -> &'static str {
        match self.scenario {
            SmokeScenarioEvidence::Interaction(_) => SmokeScenario::Interaction.label(),
            SmokeScenarioEvidence::LodThreshold(_) => SmokeScenario::LodThreshold.label(),
        }
    }

    fn final_frame_validated(&self) -> bool {
        match &self.scenario {
            SmokeScenarioEvidence::Interaction(evidence) => {
                evidence.captures.contains(&SmokeCaptureState::Final)
            }
            SmokeScenarioEvidence::LodThreshold(evidence) => {
                evidence.captures.contains(&SmokeCaptureState::Lod084)
            }
        }
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
    visible_expected_nodes: Option<usize>,
    active_nodes: usize,
    parked_nodes: usize,
    mounts: u64,
    restores: u64,
    evictions: u64,
    hard_retirements: u64,
    protected_instances: usize,
    protected_leases: usize,
    protected_overflows: u64,
    eligible_parked_nodes: usize,
    strong_instances: usize,
    frame_materialization_overflows: u64,
    feedback_submissions: u64,
    feedback_failures: u64,
    layout_rejections: u64,
    measurement_failures: u64,
    diagnostic: Option<SmokeHostDiagnostic>,
    host_closed: bool,
    geometry: Option<SmokeGeometryEvidence>,
    paint: Option<SmokePaintEvidence>,
    input: Option<SmokeInputEvidence>,
    native_probe_failure: Option<&'static str>,
    native_probe_phase: Option<String>,
    native_probe_wait_ticks: Option<u16>,
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
        if let Some((geometry, _)) = geometry.as_ref().zip(paint.as_ref()) {
            self.visible_expected_nodes = Some(geometry.node_regions.len());
        }
        self.geometry = geometry;
        self.paint = paint;
    }

    fn invariant_failure(&self, expectations: &SmokeExpectations) -> Option<&'static str> {
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
        if self.layout_rejections > 0 {
            return Some("layout_rejection");
        }
        if self.measurement_failures > 0 {
            return Some("measurement_failure");
        }
        if self.feedback_failures > 0 {
            return Some("feedback_failure");
        }
        if self
            .surface_size
            .is_some_and(|surface| !surface.is_positive_finite())
        {
            return Some("invalid_surface");
        }
        if self.evictions > 0 || self.hard_retirements > 0 {
            return Some("unexpected_retained_retirement");
        }
        if self.protected_overflows > 0 || self.frame_materialization_overflows > 0 {
            return Some("retained_lifecycle_overflow");
        }

        let expected_nodes = expectations.node_count();
        let retained_nodes = self.active_nodes.saturating_add(self.parked_nodes);
        if self.mounts > expected_nodes as u64
            || retained_nodes > expected_nodes
            || self.strong_instances > expected_nodes
            || self.protected_instances > self.active_nodes
            || self.eligible_parked_nodes > self.parked_nodes
        {
            return Some("unexpected_retained_lifecycle");
        }
        None
    }

    fn structural_decision(&self, expectations: &SmokeExpectations) -> SmokeProbeDecision {
        if let Some(reason) = self.invariant_failure(expectations) {
            return SmokeProbeDecision::Fail(reason);
        }

        let expected_nodes = expectations.node_count();
        let retained_nodes = self.active_nodes.saturating_add(self.parked_nodes);
        let Some(visible_nodes) = self.visible_expected_nodes else {
            return SmokeProbeDecision::Wait;
        };
        if visible_nodes > expected_nodes
            || self.active_nodes < visible_nodes
            || self.parked_nodes > expected_nodes.saturating_sub(visible_nodes)
        {
            return SmokeProbeDecision::Fail("visible_node_not_materialized");
        }

        let complete = expected_nodes > 0
            && self.surface_size.is_some_and(ViewSize::is_positive_finite)
            && retained_nodes == expected_nodes
            && self.strong_instances == expected_nodes
            && self.mounts == expected_nodes as u64
            && self.feedback_submissions >= COMPLETE_FRAME_FEEDBACK_SUBMISSIONS;
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
        expectations: &SmokeExpectations,
        config: &SmokeConfig,
    ) -> SmokeProbeDecision {
        if let Some(reason) = self.invariant_failure(expectations) {
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

fn marker_value(value: Option<impl ToString>) -> String {
    value.map_or_else(|| "missing".to_owned(), |value| value.to_string())
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
            .and_then(|observation| observation.diagnostic.as_ref())
            .map_or("none", |diagnostic| diagnostic.message.as_str());
        let geometry = observation.and_then(|observation| observation.geometry.as_ref());
        let paint = observation.and_then(|observation| observation.paint.as_ref());
        let input = observation.and_then(|observation| observation.input.as_ref());
        let native_probe_phase = observation
            .and_then(|observation| observation.native_probe_phase.as_deref())
            .unwrap_or("missing");
        let native_probe_wait_ticks =
            marker_value(observation.and_then(|observation| observation.native_probe_wait_ticks));
        let interaction = input.and_then(SmokeInputEvidence::interaction);
        let lod = input.and_then(SmokeInputEvidence::lod);
        let input_evidence = input.map_or("missing", SmokeInputEvidence::scenario_label);
        let input_dispatched_events =
            marker_value(interaction.map(|evidence| evidence.dispatched_events));
        let input_auto_pan_steering_events =
            marker_value(interaction.map(|evidence| evidence.auto_pan_steering_events));
        let input_target_zoom_events = input_dispatched_events.clone();
        let input_target_zoom_captures =
            marker_value(interaction.map(|evidence| evidence.captures.len()));
        let input_node_dragged = marker_value(interaction.map(|evidence| evidence.node_dragged));
        let input_auto_pan_frames =
            marker_value(interaction.map(|evidence| evidence.auto_pan_frames));
        let input_selection_chrome_fitted =
            marker_value(interaction.map(|evidence| evidence.selection_chrome_fitted));
        let input_selection_chrome_sides =
            marker_value(interaction.map(|evidence| evidence.selection_chrome_sides));
        let input_selection_chrome_detached_run =
            marker_value(interaction.map(|evidence| evidence.selection_chrome_detached_run));
        let input_connection_auto_pan_frames =
            marker_value(interaction.map(|evidence| evidence.connection_auto_pan_frames));
        let input_tab_focus_moved =
            marker_value(interaction.map(|evidence| evidence.tab_focus_moved));
        let input_tab_focus_restored =
            marker_value(interaction.map(|evidence| evidence.tab_focus_restored));
        let input_viewport_panned =
            marker_value(interaction.map(|evidence| evidence.viewport_panned));
        let input_viewport_zoomed =
            marker_value(interaction.map(|evidence| evidence.viewport_zoomed));
        let input_connection_created =
            marker_value(interaction.map(|evidence| evidence.connection_created));
        let input_connection_preview_wire_painted =
            marker_value(interaction.map(|evidence| evidence.connection_preview_wire_painted));
        let input_connection_release_frame_painted =
            marker_value(interaction.map(|evidence| evidence.connection_release_frame_painted));
        let input_connection_release_continuous =
            marker_value(interaction.map(|evidence| evidence.connection_release_continuous));
        let input_keyboard_edge_deleted =
            marker_value(interaction.map(|evidence| evidence.keyboard_edge_deleted));
        let input_reconnect_applied =
            marker_value(interaction.map(|evidence| evidence.reconnect_applied));
        let input_reconnect_preview_wire_painted =
            marker_value(interaction.map(|evidence| evidence.reconnect_preview_wire_painted));
        let input_reconnect_release_frame_painted =
            marker_value(interaction.map(|evidence| evidence.reconnect_release_frame_painted));
        let input_reconnect_release_continuous =
            marker_value(interaction.map(|evidence| evidence.reconnect_release_continuous));
        let input_search_opened = marker_value(interaction.map(|evidence| evidence.search_opened));
        let input_search_focus_moved =
            marker_value(interaction.map(|evidence| evidence.search_focus_moved));
        let input_search_cancelled =
            marker_value(interaction.map(|evidence| evidence.search_cancelled));
        let input_search_focus_restored =
            marker_value(interaction.map(|evidence| evidence.search_focus_restored));
        let input_final_frame_validated =
            marker_value(input.map(SmokeInputEvidence::final_frame_validated));
        let input_bounds_verified = marker_value(input.map(|evidence| evidence.bounds_verified));
        let input_lod_samples = marker_value(lod.map(|evidence| evidence.captures.len()));
        let input_lod_continuous = marker_value(lod.map(|evidence| evidence.lod_continuous));
        let input_lod_max_jump_in_view =
            marker_value(lod.map(|evidence| evidence.lod_max_jump_in_view));
        let initial_node_ids = expectations.map_or_else(
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
        let initial_port_ids = expectations.map_or_else(
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
        let initial_edge_ids = expectations.map_or_else(
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
        let current_geometry_node_ids = geometry.map_or_else(
            || "none".to_owned(),
            |geometry| {
                geometry
                    .expected_node_ids
                    .iter()
                    .map(|id| id.0.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            },
        );
        let current_geometry_port_ids = geometry.map_or_else(
            || "none".to_owned(),
            |geometry| {
                geometry
                    .expected_port_ids
                    .iter()
                    .map(|id| id.0.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            },
        );
        let current_geometry_edge_ids = geometry.map_or_else(
            || "none".to_owned(),
            |geometry| {
                geometry
                    .expected_edge_ids
                    .iter()
                    .map(|id| id.0.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            },
        );
        let geometry_node_regions = geometry.map_or_else(
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
            "{SMOKE_MARKER} status={status} demo={} bounds={} scenario={} target_zoom={} \
             reason={reason} initial_nodes={} \
             initial_ports={} initial_edges={} initial_node_ids={initial_node_ids} \
             initial_port_ids={initial_port_ids} initial_edge_ids={initial_edge_ids} \
             surface_width={surface_width} surface_height={surface_height} visible_expected_nodes={} \
             active_nodes={} parked_nodes={} mounts={} restores={} evictions={} hard_retirements={} \
             protected_instances={} protected_leases={} protected_overflows={} \
             eligible_parked_nodes={} strong_instances={} frame_materialization_overflows={} \
             feedback_submissions={} feedback_failures={} \
             layout_rejections={} measurement_failures={} geometry_layout_revision={} \
             current_geometry_nodes={} current_geometry_ports={} current_geometry_edges={} \
             current_geometry_node_ids={current_geometry_node_ids} \
             current_geometry_port_ids={current_geometry_port_ids} \
             current_geometry_edge_ids={current_geometry_edge_ids} \
             geometry_fresh_nodes={} geometry_rendered_ports={} geometry_routed_edges={} \
             geometry_node_regions={geometry_node_regions} paint_image_width={} paint_image_height={} \
             paint_canvas_width={} paint_non_background_pixels={} paint_distinct_colors={} \
             paint_nodes={paint_nodes} input_evidence={input_evidence} \
             input_dispatched_events={input_dispatched_events} \
             input_autopan_steering_events={input_auto_pan_steering_events} \
             input_target_zoom_events={input_target_zoom_events} \
             input_target_zoom_captures={input_target_zoom_captures} \
             input_node_dragged={input_node_dragged} input_autopan_frames={input_auto_pan_frames} \
             input_selection_chrome_fitted={input_selection_chrome_fitted} \
             input_selection_chrome_sides={input_selection_chrome_sides} \
             input_selection_chrome_detached_run={input_selection_chrome_detached_run} \
             input_connection_autopan_frames={input_connection_auto_pan_frames} \
             input_tab_focus_moved={input_tab_focus_moved} \
             input_tab_focus_restored={input_tab_focus_restored} \
             input_viewport_panned={input_viewport_panned} \
             input_viewport_zoomed={input_viewport_zoomed} \
             input_connection_created={input_connection_created} \
             input_connection_preview_wire_painted={input_connection_preview_wire_painted} \
             input_connection_release_frame_painted={input_connection_release_frame_painted} \
             input_connection_release_continuous={input_connection_release_continuous} \
             input_keyboard_edge_deleted={input_keyboard_edge_deleted} \
             input_reconnect_applied={input_reconnect_applied} \
             input_reconnect_preview_wire_painted={input_reconnect_preview_wire_painted} \
             input_reconnect_release_frame_painted={input_reconnect_release_frame_painted} \
             input_reconnect_release_continuous={input_reconnect_release_continuous} \
             input_search_opened={input_search_opened} \
             input_search_focus_moved={input_search_focus_moved} \
             input_search_cancelled={input_search_cancelled} \
             input_search_focus_restored={input_search_focus_restored} \
             input_final_frame_validated={input_final_frame_validated} \
             input_bounds={} input_bounds_verified={input_bounds_verified} \
             input_lod_samples={input_lod_samples} input_lod_continuous={input_lod_continuous} \
             input_lod_max_jump_in_view={input_lod_max_jump_in_view} \
             native_probe_failure={} native_probe_phase={native_probe_phase} \
             native_probe_wait_ticks={native_probe_wait_ticks} diagnostic={diagnostic:?}",
            self.demo,
            self.config.bounds().label(),
            self.config.scenario().label(),
            self.config.zoom().file_label(),
            expectations.map_or(0, SmokeExpectations::node_count),
            expectations.map_or(0, SmokeExpectations::port_count),
            expectations.map_or(0, |expected| expected.edges.len()),
            observation
                .and_then(|observation| observation.visible_expected_nodes)
                .unwrap_or(0),
            observation.map_or(0, |observation| observation.active_nodes),
            observation.map_or(0, |observation| observation.parked_nodes),
            observation.map_or(0, |observation| observation.mounts),
            observation.map_or(0, |observation| observation.restores),
            observation.map_or(0, |observation| observation.evictions),
            observation.map_or(0, |observation| observation.hard_retirements),
            observation.map_or(0, |observation| observation.protected_instances),
            observation.map_or(0, |observation| observation.protected_leases),
            observation.map_or(0, |observation| observation.protected_overflows),
            observation.map_or(0, |observation| observation.eligible_parked_nodes),
            observation.map_or(0, |observation| observation.strong_instances),
            observation.map_or(0, |observation| observation.frame_materialization_overflows),
            observation.map_or(0, |observation| observation.feedback_submissions),
            observation.map_or(0, |observation| observation.feedback_failures),
            observation.map_or(0, |observation| observation.layout_rejections),
            observation.map_or(0, |observation| observation.measurement_failures),
            geometry.map_or(0, |geometry| geometry.layout_revision),
            geometry.map_or(0, |geometry| geometry.expected_node_ids.len()),
            geometry.map_or(0, |geometry| geometry.expected_port_ids.len()),
            geometry.map_or(0, |geometry| geometry.expected_edge_ids.len()),
            geometry.map_or(0, |geometry| geometry.fresh_nodes),
            geometry.map_or(0, |geometry| geometry.rendered_ports),
            geometry.map_or(0, |geometry| geometry.routed_edges),
            paint.map_or(0, |paint| paint.image_width),
            paint.map_or(0, |paint| paint.image_height),
            paint.map_or(0, |paint| paint.canvas_width),
            paint.map_or(0, |paint| paint.non_background_pixels),
            paint.map_or(0, |paint| paint.distinct_colors),
            self.config.bounds().label(),
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
                observation.visible_expected_nodes = observation
                    .surface_size
                    .and_then(|surface| visible_expected_node_count(&view, &expectations, surface));
            }
            #[cfg(feature = "real-window-smoke")]
            {
                if !native_probe_started
                    && observation.structural_decision(&expectations) == SmokeProbeDecision::Pass
                {
                    native_probe_started = true;
                }
                if native_probe_started && observation.invariant_failure(&expectations).is_none() {
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
                                Ok(Ok(input)) => observation.input = input,
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
            let (decision, marker) = {
                let mut state = state.borrow_mut();
                #[cfg(feature = "real-window-smoke")]
                let decision = if native_probe_started {
                    state.observe_active(observation)
                } else {
                    state.observe(observation)
                };
                #[cfg(not(feature = "real-window-smoke"))]
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

#[cfg(feature = "real-window-smoke")]
fn visible_expected_node_count(
    view: &WeakViewSession,
    expectations: &SmokeExpectations,
    surface_size: ViewSize,
) -> Option<usize> {
    if !surface_size.is_positive_finite() {
        return None;
    }
    let geometry = view.geometry_snapshot().ok()?;
    let transform = view.viewport_transform().ok()?;
    expectations
        .node_ports
        .keys()
        .try_fold(0usize, |visible, node| {
            let bounds = geometry
                .node(*node)
                .filter(|node| node.is_fresh())?
                .bounds();
            let min = transform.screen_point_for_canvas(bounds.origin)?;
            let max = transform.screen_point_for_canvas(CanvasPoint {
                x: bounds.origin.x + bounds.size.width,
                y: bounds.origin.y + bounds.size.height,
            })?;
            let intersects = max.x > 0.0
                && max.y > 0.0
                && min.x < surface_size.width
                && min.y < surface_size.height;
            Some(visible.saturating_add(usize::from(intersects)))
        })
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
        let region = project_node_region(
            node.bounds,
            transform,
            surface_size,
            image_height,
            canvas_width,
        );
        let node_is_visible = match region {
            Ok(region) => {
                node_regions.insert(*node_id, region);
                true
            }
            Err("geometry_node_out_of_frame") if !require_every_node_in_frame => false,
            Err(reason) => return Err(reason),
        };
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
    if !bounds.is_positive_finite()
        || !transform.is_valid()
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
    let scale_x = canvas_width as f32 / surface_size.width;
    let scale_y = image_height as f32 / surface_size.height;
    let left = min.x * scale_x;
    let top = min.y * scale_y;
    let right = max.x * scale_x;
    let bottom = max.y * scale_y;
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
    let offset = y * row_stride + x * 4;
    let pixel = &rgba[offset..offset + 4];
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
    host.update(cx, |host, _| {
        let stats = host.stats();
        let frame_materialization_overflows = u64::from(
            host.take_unobserved_frame_materialization_overflow()
                .is_some(),
        );
        SmokeObservation {
            surface_size: host.surface_size(),
            visible_expected_nodes: None,
            active_nodes: stats.active_nodes(),
            parked_nodes: stats.parked_nodes(),
            mounts: stats.mounts(),
            restores: stats.restores(),
            evictions: stats.evictions(),
            hard_retirements: stats.hard_retirements(),
            protected_instances: stats.protected_instances(),
            protected_leases: stats.protected_leases(),
            protected_overflows: stats.protected_overflows(),
            eligible_parked_nodes: stats.eligible_parked_nodes(),
            strong_instances: stats.strong_instances(),
            frame_materialization_overflows,
            feedback_submissions: stats.feedback_submissions(),
            feedback_failures: stats.feedback_failures(),
            layout_rejections: stats.layout_rejections(),
            measurement_failures: stats.measurement_failures(),
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
        }
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
    let marker = {
        let mut state = state.borrow_mut();
        state.fail(reason);
        state.marker()
    };
    if let Some(marker) = marker {
        emit_smoke_marker(&marker);
    }
    request_shutdown();
}

pub(super) fn fail_smoke(cx: &mut App, state: &SharedSmokeState, reason: &'static str) {
    terminate_failed_smoke(state, reason, || cx.quit());
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
    use jellyflow_core::CanvasSize;
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

    #[test]
    fn clipped_port_hit_bounds_are_valid_only_when_absent_or_positive() {
        assert!(valid_optional_hit_bounds(None));
        assert!(valid_optional_hit_bounds(Some(CanvasRect {
            origin: CanvasPoint { x: 10.0, y: 20.0 },
            size: CanvasSize {
                width: 12.0,
                height: 8.0,
            },
        })));
        assert!(!valid_optional_hit_bounds(Some(CanvasRect {
            origin: CanvasPoint { x: 10.0, y: 20.0 },
            size: CanvasSize {
                width: 0.0,
                height: 8.0,
            },
        })));
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
            visible_expected_nodes: Some(expected.node_count()),
            active_nodes: expected.node_count(),
            parked_nodes: 0,
            mounts: expected.node_count() as u64,
            restores: 0,
            evictions: 0,
            hard_retirements: 0,
            protected_instances: 0,
            protected_leases: 0,
            protected_overflows: 0,
            eligible_parked_nodes: 0,
            strong_instances: expected.node_count(),
            frame_materialization_overflows: 0,
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
            native_probe_phase: None,
            native_probe_wait_ticks: None,
        }
    }

    fn complete_input_evidence() -> SmokeInputEvidence {
        SmokeInputEvidence {
            bounds_verified: true,
            scenario: SmokeScenarioEvidence::Interaction(SmokeInteractionEvidence {
                dispatched_events: SMOKE_INPUT_EVENT_COUNT,
                auto_pan_steering_events: 0,
                captures: INTERACTION_CAPTURE_STATES.into_iter().collect(),
                node_dragged: true,
                selection_chrome_fitted: true,
                selection_chrome_sides: 4,
                selection_chrome_detached_run: 0,
                auto_pan_frames: SMOKE_AUTOPAN_MIN_FRAMES,
                connection_auto_pan_frames: SMOKE_AUTOPAN_MIN_FRAMES,
                tab_focus_moved: true,
                tab_focus_restored: true,
                viewport_panned: true,
                viewport_zoomed: true,
                connection_created: true,
                connection_preview_wire_painted: true,
                connection_release_frame_painted: true,
                connection_release_continuous: true,
                keyboard_edge_deleted: true,
                reconnect_applied: true,
                reconnect_preview_wire_painted: true,
                reconnect_release_frame_painted: true,
                reconnect_release_continuous: true,
                search_opened: true,
                search_focus_moved: true,
                search_cancelled: true,
                search_focus_restored: true,
            }),
        }
    }

    fn complete_lod_input_evidence() -> SmokeInputEvidence {
        SmokeInputEvidence {
            bounds_verified: true,
            scenario: SmokeScenarioEvidence::LodThreshold(SmokeLodEvidence {
                captures: LOD_CAPTURE_STATES.into_iter().collect(),
                lod_continuous: true,
                lod_max_jump_in_view: 1.25,
            }),
        }
    }

    fn interaction_evidence_mut(
        evidence: &mut SmokeInputEvidence,
    ) -> &mut SmokeInteractionEvidence {
        match &mut evidence.scenario {
            SmokeScenarioEvidence::Interaction(interaction) => interaction,
            SmokeScenarioEvidence::LodThreshold(_) => panic!("expected interaction evidence"),
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
                bottom_matched: true,
                detached_max_run: 0,
                allowed_detached_run: 0,
            }
            .failure_reason(),
            Some("paint_selection_chrome_not_fitted"),
            "a fully visible selected node requires all four chrome sides",
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
    fn selection_chrome_probe_recognizes_each_product_theme_selection_token() {
        const WIDTH: usize = 200;
        const HEIGHT: usize = 200;
        let bounds = CanvasRect {
            origin: CanvasPoint { x: 40.0, y: 30.0 },
            size: CanvasSize {
                width: 100.0,
                height: 60.0,
            },
        };

        for theme in [
            NativeProductTheme::shader_blueprint(),
            NativeProductTheme::workflow_dify(),
        ] {
            let palette = SmokeSelectionChromePalette::from_product_theme(theme);
            let color = [
                palette.selection[0],
                palette.selection[1],
                palette.selection[2],
                0xff,
            ];
            let mut pixels = TEST_CANVAS_BACKGROUND_RGBA.repeat(WIDTH * HEIGHT);
            paint_chrome_rect(&mut pixels, WIDTH, 40, 30, 140, 90, color);

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
            .expect("product selection chrome evidence");

            assert_eq!(fitted.failure_reason(), None, "profile={:?}", theme.profile);
            assert_eq!(fitted.matched_sides, 4, "profile={:?}", theme.profile);
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
        for (edge_id, edge) in graph.edges() {
            let endpoints = expected
                .edge_nodes
                .get(edge_id)
                .expect("expected edge endpoint owners");
            assert_eq!(
                endpoints.0,
                graph.port(&edge.from).expect("source port").node
            );
            assert_eq!(endpoints.1, graph.port(&edge.to).expect("target port").node);
        }
    }

    #[test]
    fn geometry_probe_accepts_fresh_complete_flagship_geometry() {
        let evidence = geometry_evidence();

        assert_eq!(evidence.expected_node_ids.len(), 2);
        assert_eq!(evidence.fresh_nodes, 2);
        assert_eq!(evidence.expected_port_ids.len(), 4);
        assert_eq!(evidence.rendered_ports, 4);
        assert_eq!(evidence.expected_edge_ids.len(), 1);
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

        let visible = inspect_visible_geometry(
            &expected,
            &out_of_frame,
            transform(),
            surface_size(),
            IMAGE_WIDTH,
            IMAGE_HEIGHT,
            CANVAS_WIDTH,
        )
        .expect("visible-node evidence keeps complete graph geometry");
        assert_eq!(visible.fresh_nodes, expected.node_count());
        assert_eq!(visible.rendered_ports, expected.port_count());
        assert_eq!(visible.routed_edges, expected.edges.len());
        assert_eq!(
            visible.node_regions.keys().copied().collect::<Vec<_>>(),
            [node(1)]
        );
    }

    #[test]
    fn visible_geometry_accepts_culled_ports_and_routes_for_fully_offscreen_nodes() {
        let expected = expectations();
        let mut input = geometry_input();
        let offscreen = input.nodes.get_mut(&node(2)).expect("second fixture node");
        offscreen.bounds.origin.x = 1_500.0;
        offscreen.fresh = false;
        offscreen.ports.insert(port(21), SmokePortState::Culled);
        offscreen.ports.insert(port(22), SmokePortState::Culled);
        input.routes.clear();

        let visible = inspect_visible_geometry(
            &expected,
            &input,
            transform(),
            surface_size(),
            IMAGE_WIDTH,
            IMAGE_HEIGHT,
            CANVAS_WIDTH,
        )
        .expect("offscreen virtualized ports are valid visual geometry");

        assert_eq!(visible.fresh_nodes, expected.node_count() - 1);
        assert_eq!(visible.rendered_ports, expected.port_count() - 2);
        assert_eq!(visible.routed_edges, 0);
        assert_eq!(
            visible.node_regions.keys().copied().collect::<Vec<_>>(),
            [node(1)]
        );
    }

    #[test]
    fn visible_geometry_rejects_a_stale_node_that_is_still_on_screen() {
        let expected = expectations();
        let mut input = geometry_input();
        input.nodes.get_mut(&node(2)).unwrap().fresh = false;

        assert_eq!(
            inspect_visible_geometry(
                &expected,
                &input,
                transform(),
                surface_size(),
                IMAGE_WIDTH,
                IMAGE_HEIGHT,
                CANVAS_WIDTH,
            ),
            Err("geometry_stale_node")
        );
    }

    #[test]
    fn visible_geometry_accepts_a_route_when_all_node_bodies_are_offscreen() {
        let expected = expectations();
        let mut input = geometry_input();
        input
            .nodes
            .get_mut(&node(1))
            .expect("first fixture node")
            .bounds
            .origin
            .x = -1_000.0;
        input
            .nodes
            .get_mut(&node(2))
            .expect("second fixture node")
            .bounds
            .origin
            .x = 1_500.0;

        let visible = inspect_visible_geometry(
            &expected,
            &input,
            transform(),
            surface_size(),
            IMAGE_WIDTH,
            IMAGE_HEIGHT,
            CANVAS_WIDTH,
        )
        .expect("a rendered route is a valid visible graph presentation");

        assert!(visible.node_regions.is_empty());
        assert_eq!(visible.routed_edges, 1);

        let pixels = paint_all_node_regions(&visible, []);
        let paint = inspect_canvas_paint(
            IMAGE_WIDTH,
            IMAGE_HEIGHT,
            &pixels,
            &visible,
            TEST_CANVAS_BACKGROUND_RGBA,
        )
        .expect("route-only frames remain valid paint evidence");
        assert!(paint.nodes.is_empty());
        assert_eq!(paint.wires.len(), 1);
    }

    #[test]
    fn paint_probe_rejects_invalid_and_background_only_images() {
        let geometry = geometry_evidence();
        assert_eq!(
            inspect_canvas_paint(
                IMAGE_WIDTH,
                IMAGE_HEIGHT,
                &[],
                &geometry,
                TEST_CANVAS_BACKGROUND_RGBA,
            ),
            Err("paint_invalid_image")
        );

        let pixels =
            TEST_CANVAS_BACKGROUND_RGBA.repeat(IMAGE_WIDTH as usize * IMAGE_HEIGHT as usize);
        let paint = inspect_canvas_paint(
            IMAGE_WIDTH,
            IMAGE_HEIGHT,
            &pixels,
            &geometry,
            TEST_CANVAS_BACKGROUND_RGBA,
        )
        .expect("well-formed image");
        assert_eq!(paint.failure_reason(), Some("paint_blank_canvas"));
    }

    #[test]
    fn paint_palette_uses_each_product_canvas_background_exactly() {
        let shader = SmokePaintPalette::from_product_theme(NativeProductTheme::shader_blueprint());
        let workflow = SmokePaintPalette::from_product_theme(NativeProductTheme::workflow_dify());

        assert_eq!(shader.canvas_background, [0x0d, 0x12, 0x17, 0xff]);
        assert_eq!(workflow.canvas_background, [0x10, 0x13, 0x18, 0xff]);

        for palette in [shader, workflow] {
            let pixels = palette.canvas_background.repeat(4);
            assert_eq!(
                inspect_canvas_framebuffer(4, 1, 4, &pixels, palette.canvas_background),
                Ok((0, 0))
            );
        }
    }

    #[test]
    fn paint_palette_does_not_hide_another_products_background() {
        let shader = SmokePaintPalette::from_product_theme(NativeProductTheme::shader_blueprint());
        let workflow = SmokePaintPalette::from_product_theme(NativeProductTheme::workflow_dify());
        let foreground = [0x75, 0xb9, 0xd8, 0xff];
        let pixels = [
            shader.canvas_background,
            shader.canvas_background,
            workflow.canvas_background,
            foreground,
        ]
        .concat();

        assert_eq!(
            inspect_canvas_framebuffer(4, 1, 4, &pixels, shader.canvas_background),
            Ok((2, 2))
        );
        assert_eq!(
            inspect_canvas_framebuffer(4, 1, 4, &pixels, workflow.canvas_background),
            Ok((3, 2))
        );
    }

    #[test]
    fn paint_probe_accepts_independent_evidence_for_every_expected_node() {
        let expected = expectations();
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

        assert!(paint.non_background_pixels >= MIN_CANVAS_NON_BACKGROUND_PIXELS);
        assert!(paint.distinct_colors >= MIN_CANVAS_DISTINCT_COLORS);
        assert_eq!(paint.nodes.len(), expected.node_count());
        assert_eq!(paint.failure_reason(), None);
    }

    #[test]
    fn paint_probe_rejects_a_blank_expected_node_even_when_global_thresholds_pass() {
        let geometry = geometry_evidence();
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
        bootstrapping.visible_expected_nodes = None;
        bootstrapping.active_nodes = 0;
        bootstrapping.strong_instances = 0;
        bootstrapping.mounts = 0;
        bootstrapping.feedback_submissions = 1;
        assert_eq!(
            bootstrapping.decision(&expected, &SmokeConfig::default()),
            SmokeProbeDecision::Wait
        );

        let mut virtualized = complete.clone();
        virtualized.visible_expected_nodes = Some(1);
        virtualized.active_nodes = 1;
        virtualized.parked_nodes = 1;
        virtualized.eligible_parked_nodes = 1;
        assert_eq!(
            virtualized.structural_decision(&expected),
            SmokeProbeDecision::Pass
        );

        let mut visible_node_parked = virtualized;
        visible_node_parked.visible_expected_nodes = Some(2);
        assert_eq!(
            visible_node_parked.structural_decision(&expected),
            SmokeProbeDecision::Fail("visible_node_not_materialized")
        );

        let mut missing_geometry = complete;
        missing_geometry.geometry = None;
        assert_eq!(
            missing_geometry.decision(&expected, &SmokeConfig::default()),
            SmokeProbeDecision::Wait
        );
    }

    #[test]
    fn active_smoke_probe_keeps_running_through_transient_virtualization() {
        let expected = expectations();
        let mut interaction_frame = complete_observation(&expected);
        interaction_frame.visible_expected_nodes = Some(expected.node_count());
        interaction_frame.active_nodes = 1;
        interaction_frame.parked_nodes = 1;
        interaction_frame.eligible_parked_nodes = 1;

        assert_eq!(
            interaction_frame.structural_decision(&expected),
            SmokeProbeDecision::Fail("visible_node_not_materialized"),
            "bootstrap must still reject a visible node that has not materialized",
        );
        assert_eq!(
            interaction_frame.active_decision(&expected, &SmokeConfig::default()),
            SmokeProbeDecision::Pass,
            "an already-started probe must tolerate transient materialization changes",
        );
    }

    #[test]
    fn captured_frame_replaces_the_pre_advance_visible_node_count() {
        let expected = expectations();
        let mut observation = complete_observation(&expected);
        let mut geometry = observation.geometry.take().expect("geometry evidence");
        let mut paint = observation.paint.take().expect("paint evidence");
        let visible_node = node(1);
        geometry
            .node_regions
            .retain(|node, _| *node == visible_node);
        paint.nodes.retain(|node, _| *node == visible_node);
        observation.visible_expected_nodes = Some(0);

        observation.bind_captured_frame(Some(geometry), Some(paint));

        assert_eq!(observation.visible_expected_nodes, Some(1));
        assert_eq!(
            observation
                .geometry
                .as_ref()
                .map(|geometry| geometry.node_regions.len()),
            Some(1),
        );
        assert_eq!(
            observation.paint.as_ref().map(|paint| paint.nodes.len()),
            Some(1),
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
                |value| {
                    value.diagnostic = Some(SmokeHostDiagnostic {
                        kind: OpenGpuiSceneHostDiagnosticKind::FactoryFailure,
                        message: "FactoryFailure: renderer failed".to_owned(),
                    });
                },
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
                "retained node eviction",
                |value| value.evictions = 1,
                "unexpected_retained_retirement",
            ),
            (
                "retained materialization overflow",
                |value| value.frame_materialization_overflows = 1,
                "retained_lifecycle_overflow",
            ),
            (
                "incomplete input dispatch",
                |value| {
                    let mut input = complete_input_evidence();
                    interaction_evidence_mut(&mut input).dispatched_events =
                        SMOKE_INPUT_EVENT_COUNT - 1;
                    value.input = Some(input);
                },
                "input_dispatch_incomplete",
            ),
            (
                "unchanged node",
                |value| {
                    let mut input = complete_input_evidence();
                    interaction_evidence_mut(&mut input).node_dragged = false;
                    value.input = Some(input);
                },
                "input_node_unchanged",
            ),
            (
                "selection chrome did not fit published node bounds",
                |value| {
                    let mut input = complete_input_evidence();
                    interaction_evidence_mut(&mut input).selection_chrome_fitted = false;
                    value.input = Some(input);
                },
                "paint_selection_chrome_not_fitted",
            ),
            (
                "selection chrome omitted one fully visible side",
                |value| {
                    let mut input = complete_input_evidence();
                    interaction_evidence_mut(&mut input).selection_chrome_sides = 3;
                    value.input = Some(input);
                },
                "paint_selection_chrome_not_fitted",
            ),
            (
                "checkpoint not observed at target zoom",
                |value| {
                    let mut input = complete_input_evidence();
                    interaction_evidence_mut(&mut input)
                        .captures
                        .remove(&SmokeCaptureState::Final);
                    value.input = Some(input);
                },
                "input_target_zoom_capture_mismatch",
            ),
            (
                "autopan did not span enough frames",
                |value| {
                    let mut input = complete_input_evidence();
                    interaction_evidence_mut(&mut input).auto_pan_frames =
                        SMOKE_AUTOPAN_MIN_FRAMES - 1;
                    value.input = Some(input);
                },
                "input_autopan_frames_missing",
            ),
            (
                "connection autopan did not span enough frames",
                |value| {
                    let mut input = complete_input_evidence();
                    interaction_evidence_mut(&mut input).connection_auto_pan_frames =
                        SMOKE_AUTOPAN_MIN_FRAMES - 1;
                    value.input = Some(input);
                },
                "input_connection_autopan_frames_missing",
            ),
            (
                "tab focus did not return",
                |value| {
                    let mut input = complete_input_evidence();
                    interaction_evidence_mut(&mut input).tab_focus_restored = false;
                    value.input = Some(input);
                },
                "input_shift_tab_focus_unrestored",
            ),
            (
                "unzoomed viewport",
                |value| {
                    let mut input = complete_input_evidence();
                    interaction_evidence_mut(&mut input).viewport_zoomed = false;
                    value.input = Some(input);
                },
                "input_viewport_unzoomed",
            ),
            (
                "missing reconnect",
                |value| {
                    let mut input = complete_input_evidence();
                    interaction_evidence_mut(&mut input).reconnect_applied = false;
                    value.input = Some(input);
                },
                "input_reconnect_missing",
            ),
            (
                "window bounds mismatch",
                |value| {
                    let mut input = complete_input_evidence();
                    input.bounds_verified = false;
                    value.input = Some(input);
                },
                "input_window_bounds_mismatch",
            ),
        ];

        for &(label, mutate, reason) in cases {
            let mut observation = complete_observation(&expected);
            mutate(&mut observation);
            assert_eq!(
                observation.decision(&expected, &SmokeConfig::default()),
                SmokeProbeDecision::Fail(reason),
                "{label}",
            );
        }
    }

    #[test]
    fn endpoint_geometry_fallback_defers_to_visible_geometry_coverage() {
        let expected = expectations();
        let mut observation = complete_observation(&expected);
        observation.diagnostic = Some(SmokeHostDiagnostic {
            kind: OpenGpuiSceneHostDiagnosticKind::EndpointGeometry,
            message: "off-screen endpoint used an explicit fallback".to_owned(),
        });

        assert_eq!(
            observation.decision(&expected, &SmokeConfig::default()),
            SmokeProbeDecision::Pass
        );
    }

    #[test]
    fn lod_scenario_requires_three_continuous_matching_frame_samples() {
        let mut evidence = SmokeInputEvidence {
            bounds_verified: true,
            scenario: SmokeScenarioEvidence::LodThreshold(SmokeLodEvidence {
                captures: LOD_CAPTURE_STATES.into_iter().collect(),
                lod_continuous: true,
                lod_max_jump_in_view: 0.0,
            }),
        };
        assert_eq!(evidence.failure_reason(SmokeScenario::LodThreshold), None);

        let SmokeScenarioEvidence::LodThreshold(lod) = &mut evidence.scenario else {
            panic!("expected LOD evidence");
        };
        lod.captures.remove(&SmokeCaptureState::Lod084);
        assert_eq!(
            evidence.failure_reason(SmokeScenario::LodThreshold),
            Some("input_lod_samples_incomplete")
        );
        let SmokeScenarioEvidence::LodThreshold(lod) = &mut evidence.scenario else {
            panic!("expected LOD evidence");
        };
        lod.captures.insert(SmokeCaptureState::Lod084);
        lod.lod_continuous = false;
        assert_eq!(
            evidence.failure_reason(SmokeScenario::LodThreshold),
            Some("input_lod_discontinuity")
        );
    }

    #[test]
    fn typed_input_evidence_must_match_the_configured_scenario() {
        assert_eq!(
            complete_input_evidence().failure_reason(SmokeScenario::LodThreshold),
            Some("input_scenario_mismatch")
        );
        assert_eq!(
            complete_lod_input_evidence().failure_reason(SmokeScenario::Interaction),
            Some("input_scenario_mismatch")
        );
    }

    #[test]
    fn smoke_state_emits_one_marker_with_initial_geometry_and_per_node_paint() {
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
        assert!(marker.contains("bounds=compact"));
        assert!(marker.contains("scenario=interaction"));
        assert!(marker.contains("target_zoom=1"));
        assert!(marker.contains("input_evidence=interaction"));
        assert!(marker.contains("initial_nodes=2"));
        assert!(marker.contains("initial_ports=4"));
        assert!(marker.contains("initial_edges=1"));
        assert!(marker.contains("visible_expected_nodes=2"));
        assert!(marker.contains("active_nodes=2"));
        assert!(marker.contains("parked_nodes=0"));
        assert!(marker.contains("strong_instances=2"));
        assert!(marker.contains("frame_materialization_overflows=0"));
        assert!(marker.contains("initial_node_ids="));
        assert!(marker.contains("current_geometry_nodes=2"));
        assert!(marker.contains("current_geometry_ports=4"));
        assert!(marker.contains("current_geometry_edges=1"));
        assert!(marker.contains("current_geometry_node_ids="));
        assert!(marker.contains("current_geometry_port_ids="));
        assert!(marker.contains("current_geometry_edge_ids="));
        assert!(marker.contains("geometry_fresh_nodes=2"));
        assert!(marker.contains("geometry_rendered_ports=4"));
        assert!(marker.contains("geometry_routed_edges=1"));
        assert!(marker.contains("geometry_node_regions="));
        assert!(marker.contains("paint_nodes="));
        assert!(marker.contains(&format!(
            "input_dispatched_events={SMOKE_INPUT_EVENT_COUNT}"
        )));
        assert!(marker.contains("input_autopan_steering_events=0"));
        assert!(marker.contains(&format!(
            "input_target_zoom_events={SMOKE_INPUT_EVENT_COUNT}"
        )));
        assert!(marker.contains(&format!(
            "input_target_zoom_captures={SMOKE_INTERACTION_CAPTURE_COUNT}"
        )));
        assert!(marker.contains("input_node_dragged=true"));
        assert!(marker.contains(&format!("input_autopan_frames={SMOKE_AUTOPAN_MIN_FRAMES}")));
        assert!(marker.contains(&format!(
            "input_connection_autopan_frames={SMOKE_AUTOPAN_MIN_FRAMES}"
        )));
        assert!(marker.contains("input_selection_chrome_fitted=true"));
        assert!(marker.contains("input_selection_chrome_sides=4"));
        assert!(marker.contains("input_selection_chrome_detached_run=0"));
        assert!(marker.contains("input_tab_focus_moved=true"));
        assert!(marker.contains("input_tab_focus_restored=true"));
        assert!(marker.contains("input_viewport_panned=true"));
        assert!(marker.contains("input_viewport_zoomed=true"));
        assert!(marker.contains("input_connection_created=true"));
        assert!(marker.contains("input_connection_preview_wire_painted=true"));
        assert!(marker.contains("input_connection_release_frame_painted=true"));
        assert!(marker.contains("input_connection_release_continuous=true"));
        assert!(marker.contains("input_keyboard_edge_deleted=true"));
        assert!(marker.contains("input_reconnect_applied=true"));
        assert!(marker.contains("input_reconnect_preview_wire_painted=true"));
        assert!(marker.contains("input_reconnect_release_frame_painted=true"));
        assert!(marker.contains("input_reconnect_release_continuous=true"));
        assert!(marker.contains("input_search_opened=true"));
        assert!(marker.contains("input_search_focus_moved=true"));
        assert!(marker.contains("input_search_cancelled=true"));
        assert!(marker.contains("input_search_focus_restored=true"));
        assert!(marker.contains("input_final_frame_validated=true"));
        assert!(marker.contains("input_bounds_verified=true"));
        assert!(state.marker().is_none());
    }

    #[test]
    fn lod_marker_uses_configured_normal_matrix_and_typed_lod_evidence() {
        let expected = expectations();
        let config = SmokeConfig::for_test(
            SmokeBounds::Normal,
            SmokeScenario::LodThreshold,
            SmokeZoom::One,
        );
        let mut state = SmokeState::new("workflow", config);
        state.configure(expected.clone());
        let mut observation = complete_observation(&expected);
        observation.input = Some(complete_lod_input_evidence());

        assert_eq!(state.observe(observation), SmokeProbeDecision::Pass);
        let marker = state.marker().expect("completed LOD marker");

        assert!(marker.contains("bounds=normal"));
        assert!(marker.contains("scenario=lod-threshold"));
        assert!(marker.contains("target_zoom=1"));
        assert!(marker.contains("input_evidence=lod-threshold"));
        assert!(marker.contains("input_dispatched_events=missing"));
        assert!(marker.contains(&format!("input_lod_samples={SMOKE_LOD_SAMPLE_COUNT}")));
        assert!(marker.contains("input_lod_continuous=true"));
        assert!(marker.contains("input_lod_max_jump_in_view=1.25"));
        assert!(marker.contains("input_bounds=normal"));
    }

    #[test]
    fn smoke_marker_distinguishes_initial_topology_from_mutated_geometry() {
        let mut initial = expectations();
        initial.edges.insert(edge(32));
        let mut state = SmokeState::new("shader", SmokeConfig::default());
        state.configure(initial.clone());

        assert_eq!(
            state.observe(complete_observation(&initial)),
            SmokeProbeDecision::Pass
        );

        let marker = state.marker().expect("completed smoke marker");
        assert!(marker.contains("initial_edges=2"));
        assert!(marker.contains(&format!("initial_edge_ids={},{}", edge(31).0, edge(32).0)));
        assert!(marker.contains("current_geometry_edges=1"));
        assert!(marker.contains(&format!("current_geometry_edge_ids={}", edge(31).0)));
        assert!(marker.contains("geometry_routed_edges=1"));
    }

    #[test]
    fn unconfigured_expectations_failure_requests_shutdown() {
        let state = new_smoke_state("shader", SmokeConfig::default());
        let mut shutdown_requested = false;

        terminate_failed_smoke(&state, "smoke_expectations_unconfigured", || {
            shutdown_requested = true;
        });

        assert!(shutdown_requested);
        assert_eq!(
            state.borrow().outcome,
            SmokeOutcome::Failed("smoke_expectations_unconfigured")
        );
        assert!(state.borrow_mut().marker().is_none());
        assert_ne!(smoke_exit_status(state.borrow().outcome), 0);
    }

    #[test]
    fn startup_failure_emits_one_failed_marker_and_nonzero_status() {
        let config = SmokeConfig::for_test(
            SmokeBounds::Normal,
            SmokeScenario::LodThreshold,
            SmokeZoom::Two,
        );
        let mut state = SmokeState::new("workflow", config);
        state.fail("window_open");
        let marker = state.marker().expect("startup failure marker");

        assert!(marker.starts_with(SMOKE_MARKER));
        assert!(marker.contains("status=fail"));
        assert!(marker.contains("demo=workflow"));
        assert!(marker.contains("bounds=normal"));
        assert!(marker.contains("scenario=lod-threshold"));
        assert!(marker.contains("target_zoom=2"));
        assert!(marker.contains("reason=window_open"));
        assert!(marker.contains("input_evidence=missing"));
        assert!(marker.contains("input_dispatched_events=missing"));
        assert!(marker.contains("input_target_zoom_events=missing"));
        assert!(marker.contains("input_target_zoom_captures=missing"));
        assert!(marker.contains("input_bounds=normal"));
        assert!(marker.contains("input_bounds_verified=missing"));
        assert!(marker.contains("input_lod_samples=missing"));
        assert!(state.marker().is_none());
        assert_eq!(smoke_exit_status(SmokeOutcome::Passed), 0);
        assert_ne!(smoke_exit_status(SmokeOutcome::Pending), 0);
        assert_ne!(smoke_exit_status(SmokeOutcome::Failed("test_failure")), 0);
    }
}
