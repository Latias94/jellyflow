mod events;
mod evidence;
mod phase;
mod planning;
mod scenario;

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use jellyflow_core::{CanvasPoint, EdgeId, PortId};
use jellyflow_runtime::geometry::{EdgePath, ViewPoint, ViewSize};
use jellyflow_runtime::graph::EdgeEndpoint;
use jellyflow_runtime::interaction::{CanvasInteractionPhase, InteractionTerminalReason};
use jellyflow_runtime::view::{PublishedPortOutcome, WeakViewSession};
use jellyflow_runtime::viewport::ViewportTransform;
use open_gpui::{
    App, Entity, FocusHandle, MouseButton, PinchEvent, PlatformInput, TouchPhase, Window, point, px,
};
use open_gpui_canvas_jellyflow::connection_search::NativeGraphSurface;

use self::events::{key_down, key_up, mouse_down, mouse_move, mouse_up};
use self::evidence::{
    RenderedFrameFence, RenderedFrameFencePoll, capture_visual_frame, connection_preview_path,
    inspect_current_frame, inspect_edge_path_frame,
};
use self::phase::ProbePhase;
use self::planning::{
    AutoPanPointerStrategy, EdgePlan, NodePlan, ReconnectPlan, VisibleEdgePickPoint,
    auto_pan_pointer_strategy, auto_pan_pointer_toward, blank_surface_point, choose_reconnect,
    dragged_geometry_status, free_connection_port, geometry_fits_surface, graph_contains_edge,
    point_changed, point_inside_surface, port_owner_view_center, port_point,
    position_node_for_selection, refresh_edge_plan, reveal_canvas_points,
    set_zoom_centered_on_canvas_point, set_zoom_preserving_surface_center,
    settled_connection_target_point, steer_auto_pan_pointer_toward, visible_port_point,
    visible_reconnect_grip, visible_unique_edge_point,
};
use super::{
    INTERACTION_CAPTURE_STATES, LOD_CAPTURE_STATES, SMOKE_AUTOPAN_MIN_FRAMES,
    SMOKE_INPUT_EVENT_COUNT, SmokeCaptureState, SmokeConfig, SmokeExpectations,
    SmokeGeometryEvidence, SmokeInputEvidence, SmokeInteractionEvidence, SmokeLodEvidence,
    SmokePaintEvidence, SmokePaintPalette, SmokeScenario, SmokeScenarioEvidence,
};

const MAX_PHASE_WAIT_TICKS: u16 = 180;
const NODE_DRAG_X: f32 = -48.0;
const NODE_DRAG_Y: f32 = 32.0;
const TARGET_ZOOM_TOLERANCE: f32 = 0.001;

#[derive(Clone, Debug, PartialEq)]
struct LodFrameEvidence {
    zoom: f32,
    node: jellyflow_core::NodeId,
    canvas_origin: CanvasPoint,
    origin: ViewPoint,
    size: ViewSize,
    ports: BTreeMap<PortId, ViewPoint>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum LodSample {
    Lod086,
    Lod085,
    Lod084,
}

#[derive(Debug)]
enum ScenarioProbeState {
    Interaction(Box<InteractionProbeState>),
    LodThreshold(LodProbeState),
}

#[derive(Debug, Default)]
struct InteractionProbeState {
    dispatched_events: u16,
    captures: BTreeSet<SmokeCaptureState>,
    node: Option<NodePlan>,
    first_edge: Option<EdgePlan>,
    second_edge: Option<EdgePlan>,
    created_edge: Option<EdgeId>,
    reconnect: Option<ReconnectPlan>,
    edge_pick_canvas_target: Option<CanvasPoint>,
    press: Option<ViewPoint>,
    release: Option<ViewPoint>,
    viewport_before: Option<ViewportTransform>,
    auto_pan_origin: Option<ViewportTransform>,
    auto_pan_last: Option<ViewportTransform>,
    auto_pan_frames: u8,
    connection_auto_pan_last: Option<ViewportTransform>,
    connection_auto_pan_frames: u8,
    connection_auto_pan_strategy: Option<AutoPanPointerStrategy>,
    reconnect_auto_pan_last: Option<ViewportTransform>,
    reconnect_auto_pan_frames: u8,
    reconnect_auto_pan_strategy: Option<AutoPanPointerStrategy>,
    auto_pan_steering_events: u16,
    rendered_frame_fence: RenderedFrameFence,
    release_wire_path: Option<EdgePath>,
    tab_focus_origin: Option<FocusHandle>,
    tab_focus_forward: Option<FocusHandle>,
    selection_reframed: bool,
    selection_chrome_fitted: bool,
    selection_chrome_sides: u8,
    selection_chrome_detached_run: usize,
    node_dragged: bool,
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
    search_focus_baseline: Option<FocusHandle>,
    search_render_requested: bool,
}

impl InteractionProbeState {
    fn ensure_capture_pending(&self, state: SmokeCaptureState) -> Result<(), &'static str> {
        ensure_capture_pending(&self.captures, state, &INTERACTION_CAPTURE_STATES)
    }

    fn record_capture(&mut self, state: SmokeCaptureState) -> Result<(), &'static str> {
        record_capture(&mut self.captures, state, &INTERACTION_CAPTURE_STATES)
    }
}

#[derive(Debug)]
struct LodProbeState {
    current: LodSample,
    feedback_before: u64,
    node: Option<jellyflow_core::NodeId>,
    frames: BTreeMap<LodSample, LodFrameEvidence>,
    captures: BTreeSet<SmokeCaptureState>,
    continuous: bool,
    max_jump_in_view: f32,
}

impl Default for LodProbeState {
    fn default() -> Self {
        Self {
            current: LodSample::Lod086,
            feedback_before: 0,
            node: None,
            frames: BTreeMap::new(),
            captures: BTreeSet::new(),
            continuous: true,
            max_jump_in_view: 0.0,
        }
    }
}

impl LodProbeState {
    fn ensure_capture_pending(&self, state: SmokeCaptureState) -> Result<(), &'static str> {
        ensure_capture_pending(&self.captures, state, &LOD_CAPTURE_STATES)
    }

    fn record_capture(&mut self, state: SmokeCaptureState) -> Result<(), &'static str> {
        record_capture(&mut self.captures, state, &LOD_CAPTURE_STATES)
    }
}

fn ensure_capture_pending<const N: usize>(
    captures: &BTreeSet<SmokeCaptureState>,
    state: SmokeCaptureState,
    allowed: &[SmokeCaptureState; N],
) -> Result<(), &'static str> {
    if !allowed.contains(&state) {
        return Err("input_capture_scenario_mismatch");
    }
    if captures.contains(&state) {
        return Err("input_duplicate_capture");
    }
    Ok(())
}

fn record_capture<const N: usize>(
    captures: &mut BTreeSet<SmokeCaptureState>,
    state: SmokeCaptureState,
    allowed: &[SmokeCaptureState; N],
) -> Result<(), &'static str> {
    ensure_capture_pending(captures, state, allowed)?;
    captures.insert(state);
    Ok(())
}

#[derive(Debug)]
pub(super) struct RealWindowSmokeProbe {
    demo: &'static str,
    config: SmokeConfig,
    phase: ProbePhase,
    wait_ticks: u16,
    geometry: Option<SmokeGeometryEvidence>,
    paint: Option<SmokePaintEvidence>,
    scenario: ScenarioProbeState,
    paint_palette: SmokePaintPalette,
    bounds_verified: bool,
}

impl RealWindowSmokeProbe {
    pub(super) fn new(
        demo: &'static str,
        config: SmokeConfig,
        paint_palette: SmokePaintPalette,
    ) -> Self {
        let scenario = match config.scenario() {
            SmokeScenario::Interaction => ScenarioProbeState::Interaction(Box::default()),
            SmokeScenario::LodThreshold => {
                ScenarioProbeState::LodThreshold(LodProbeState::default())
            }
        };
        Self {
            demo,
            config,
            phase: ProbePhase::default(),
            wait_ticks: 0,
            geometry: None,
            paint: None,
            scenario,
            paint_palette,
            bounds_verified: false,
        }
    }

    pub(super) fn geometry(&self) -> Option<&SmokeGeometryEvidence> {
        self.geometry.as_ref()
    }

    pub(super) fn paint(&self) -> Option<&SmokePaintEvidence> {
        self.paint.as_ref()
    }

    pub(super) fn progress(&self) -> (String, u16) {
        (format!("{:?}", self.phase), self.wait_ticks)
    }

    fn interaction(&self) -> Result<&InteractionProbeState, &'static str> {
        match &self.scenario {
            ScenarioProbeState::Interaction(state) => Ok(state.as_ref()),
            ScenarioProbeState::LodThreshold(_) => Err("input_scenario_state_mismatch"),
        }
    }

    fn interaction_mut(&mut self) -> Result<&mut InteractionProbeState, &'static str> {
        match &mut self.scenario {
            ScenarioProbeState::Interaction(state) => Ok(state.as_mut()),
            ScenarioProbeState::LodThreshold(_) => Err("input_scenario_state_mismatch"),
        }
    }

    fn lod(&self) -> Result<&LodProbeState, &'static str> {
        match &self.scenario {
            ScenarioProbeState::Interaction(_) => Err("input_scenario_state_mismatch"),
            ScenarioProbeState::LodThreshold(state) => Ok(state),
        }
    }

    fn lod_mut(&mut self) -> Result<&mut LodProbeState, &'static str> {
        match &mut self.scenario {
            ScenarioProbeState::Interaction(_) => Err("input_scenario_state_mismatch"),
            ScenarioProbeState::LodThreshold(state) => Ok(state),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn advance(
        &mut self,
        window: &mut Window,
        cx: &mut App,
        surface: &Entity<NativeGraphSurface>,
        view: &WeakViewSession,
        expectations: &SmokeExpectations,
        surface_size: ViewSize,
    ) -> Result<Option<SmokeInputEvidence>, &'static str> {
        scenario::advance(self, window, cx, surface, view, expectations, surface_size)
    }

    fn capture_visual(
        &mut self,
        window: &mut Window,
        view: &WeakViewSession,
        surface_size: ViewSize,
    ) -> Result<(), &'static str> {
        self.verify_target_zoom(view)?;
        self.interaction()?
            .ensure_capture_pending(SmokeCaptureState::Initial)?;
        let screenshot_path = self
            .config
            .capture_path(self.demo, SmokeCaptureState::Initial);
        capture_visual_frame(
            window,
            surface_size,
            screenshot_path.as_deref(),
            self.paint_palette.canvas_background,
        )?;
        self.interaction_mut()?
            .record_capture(SmokeCaptureState::Initial)
    }

    fn capture_checkpoint(
        &mut self,
        window: &mut Window,
        view: &WeakViewSession,
        surface_size: ViewSize,
        state: SmokeCaptureState,
    ) -> Result<(), &'static str> {
        self.capture_checkpoint_omitting_edge(window, view, surface_size, state, None)
    }

    fn capture_checkpoint_omitting_edge(
        &mut self,
        window: &mut Window,
        view: &WeakViewSession,
        surface_size: ViewSize,
        state: SmokeCaptureState,
        omitted_edge: Option<EdgeId>,
    ) -> Result<(), &'static str> {
        self.verify_target_zoom(view)?;
        self.interaction()?.ensure_capture_pending(state)?;
        let graph = view
            .graph_snapshot()
            .map_err(|_| "input_graph_snapshot_failed")?;
        let expectations = SmokeExpectations::from_graph(graph.graph());
        let expectations = match omitted_edge {
            Some(edge) => expectations.without_edge(edge),
            None => expectations,
        };
        let screenshot_path = self.config.capture_path(self.demo, state);
        let selected_node = if state == SmokeCaptureState::Selected {
            Some(
                self.interaction()?
                    .node
                    .ok_or("input_node_plan_missing")?
                    .node,
            )
        } else {
            None
        };
        let (_, _, geometry, paint, selection_chrome) = inspect_current_frame(
            window,
            view,
            &expectations,
            surface_size,
            screenshot_path.as_deref(),
            selected_node,
            self.paint_palette,
        )?;
        if let Some(reason) = paint.failure_reason() {
            return Err(reason);
        }
        if let Some(chrome) = selection_chrome {
            if let Some(reason) = chrome.failure_reason() {
                return Err(reason);
            }
            let interaction = self.interaction_mut()?;
            interaction.selection_chrome_fitted = true;
            interaction.selection_chrome_sides = chrome.matched_sides;
            interaction.selection_chrome_detached_run = chrome.detached_max_run;
        }
        self.geometry = Some(geometry);
        self.paint = Some(paint);
        self.interaction_mut()?.record_capture(state)
    }

    fn plan_input(
        &mut self,
        window: &mut Window,
        view: &WeakViewSession,
        expectations: &SmokeExpectations,
        surface_size: ViewSize,
    ) -> Result<(), &'static str> {
        let (snapshot, transform, geometry, paint, _) = inspect_current_frame(
            window,
            view,
            expectations,
            surface_size,
            None,
            None,
            self.paint_palette,
        )?;
        let graph = view
            .graph_snapshot()
            .map_err(|_| "input_graph_snapshot_failed")?;
        let mut edges = expectations.edges.iter().filter_map(|edge| {
            let model = graph.graph().edge(edge)?;
            let from_anchor = snapshot
                .port_outcome(model.from)
                .and_then(PublishedPortOutcome::rendered)
                .map(|port| port.route_anchor());
            let to_anchor = snapshot
                .port_outcome(model.to)
                .and_then(PublishedPortOutcome::rendered)
                .map(|port| port.route_anchor());
            Some(EdgePlan {
                edge: *edge,
                from: model.from,
                to: model.to,
                from_anchor,
                to_anchor,
            })
        });
        let first_edge = edges.next();
        let second_edge = edges.next();
        if first_edge.is_none() || second_edge.is_none() {
            return Err("input_edge_fixture_incomplete");
        }
        let (node, bounds) = expectations
            .node_ports
            .keys()
            .filter_map(|node| {
                snapshot
                    .node(*node)
                    .filter(|geometry| geometry.is_fresh())
                    .map(|geometry| (*node, geometry.bounds()))
            })
            .min_by(|(_, left), (_, right)| left.size.width.total_cmp(&right.size.width))
            .ok_or("input_node_fixture_incomplete")?;
        let header_anchor = CanvasPoint {
            x: bounds.origin.x + bounds.size.width * 0.5,
            y: bounds.origin.y + (20.0 / transform.zoom).min(bounds.size.height * 0.5),
        };
        if !reveal_canvas_points(view, surface_size, &[header_anchor])? {
            window.refresh();
        }
        let transform = view
            .viewport_transform()
            .map_err(|_| "input_viewport_read_failed")?;
        let model = graph
            .graph()
            .node(&node)
            .ok_or("input_node_fixture_incomplete")?;
        let min = transform
            .screen_point_for_canvas(bounds.origin)
            .ok_or("input_node_projection_failed")?;
        let max = transform
            .screen_point_for_canvas(CanvasPoint {
                x: bounds.origin.x + bounds.size.width,
                y: bounds.origin.y + bounds.size.height,
            })
            .ok_or("input_node_projection_failed")?;
        let press = ViewPoint {
            x: (min.x + max.x) * 0.5,
            y: min.y + 20.0_f32.min((max.y - min.y) * 0.5),
        };
        let activation = ViewPoint {
            x: press.x + NODE_DRAG_X,
            y: press.y + NODE_DRAG_Y,
        };
        let release = ViewPoint {
            x: 12.0,
            y: activation.y.clamp(48.0, surface_size.height - 48.0),
        };
        if !point_inside_surface(press, surface_size)
            || !point_inside_surface(activation, surface_size)
            || !point_inside_surface(release, surface_size)
        {
            return Err("input_node_fixture_incomplete");
        }
        let node = NodePlan {
            node,
            original_position: model.pos,
            original_geometry_origin: bounds.origin,
            press,
            activation,
            release,
        };
        let interaction = self.interaction_mut()?;
        interaction.first_edge = first_edge;
        interaction.second_edge = second_edge;
        interaction.node = Some(node);
        self.bounds_verified = self.config.surface_matches(surface_size);
        self.geometry = Some(geometry);
        self.paint = Some(paint);
        Ok(())
    }

    fn dispatch_at_target_zoom(
        &mut self,
        window: &mut Window,
        cx: &mut App,
        view: &WeakViewSession,
        event: PlatformInput,
    ) -> Result<(), &'static str> {
        self.interaction()?;
        self.verify_target_zoom(view)?;
        window.dispatch_event(event, cx);
        window.refresh();
        let interaction = self.interaction_mut()?;
        interaction.dispatched_events = interaction.dispatched_events.saturating_add(1);
        Ok(())
    }

    fn verify_target_zoom(&self, view: &WeakViewSession) -> Result<(), &'static str> {
        let transform = view
            .viewport_transform()
            .map_err(|_| "input_viewport_read_failed")?;
        ((transform.zoom - self.config.zoom().value()).abs() <= TARGET_ZOOM_TOLERANCE)
            .then_some(())
            .ok_or("input_target_zoom_mismatch")
    }

    fn transition(&mut self, phase: ProbePhase) {
        self.phase = phase;
        self.wait_ticks = 0;
    }

    fn wait_for_rendered_interaction_frame(
        &mut self,
        window: &mut Window,
        cx: &mut App,
        surface: &Entity<NativeGraphSurface>,
    ) -> Result<bool, &'static str> {
        let poll = self
            .interaction_mut()?
            .rendered_frame_fence
            .poll(window, cx);
        if poll != RenderedFrameFencePoll::Rendered {
            surface.update(cx, |_, cx| cx.notify());
            window.refresh();
        }
        Ok(poll == RenderedFrameFencePoll::Rendered)
    }

    fn rendered_interaction_frame_pending(&self) -> Result<bool, &'static str> {
        Ok(self.interaction()?.rendered_frame_fence.is_pending())
    }

    fn edge_press_point(
        &mut self,
        window: &mut Window,
        cx: &mut App,
        surface: &Entity<NativeGraphSurface>,
        view: &WeakViewSession,
        edge: EdgeId,
        surface_size: ViewSize,
    ) -> Result<Option<ViewPoint>, &'static str> {
        let preferred = self.interaction()?.edge_pick_canvas_target;
        let outcome = visible_unique_edge_point(view, edge, surface_size, preferred)?;
        match outcome {
            VisibleEdgePickPoint::Ready(point) => {
                self.interaction_mut()?.edge_pick_canvas_target = None;
                Ok(Some(point))
            }
            VisibleEdgePickPoint::AwaitFrame(canvas_point) => {
                self.interaction_mut()?.edge_pick_canvas_target = Some(canvas_point);
                let _ = self.wait_for_rendered_interaction_frame(window, cx, surface)?;
                Ok(None)
            }
            VisibleEdgePickPoint::Unavailable => {
                surface.update(cx, |_, cx| cx.notify());
                window.refresh();
                Ok(None)
            }
        }
    }

    fn wait_or_timeout(&mut self) -> Result<Option<SmokeInputEvidence>, &'static str> {
        self.wait_with_reason(self.phase.timeout_reason())
    }

    fn wait_with_reason(
        &mut self,
        reason: &'static str,
    ) -> Result<Option<SmokeInputEvidence>, &'static str> {
        self.wait_ticks = self.wait_ticks.saturating_add(1);
        if self.wait_ticks > MAX_PHASE_WAIT_TICKS {
            Err(reason)
        } else {
            Ok(None)
        }
    }

    fn complete(&self) -> Result<Option<SmokeInputEvidence>, &'static str> {
        let scenario = match (&self.scenario, self.config.scenario()) {
            (ScenarioProbeState::Interaction(state), SmokeScenario::Interaction) => {
                if state.dispatched_events
                    != SMOKE_INPUT_EVENT_COUNT.saturating_add(state.auto_pan_steering_events)
                {
                    return Err("input_dispatch_incomplete");
                }
                SmokeScenarioEvidence::Interaction(SmokeInteractionEvidence {
                    dispatched_events: state.dispatched_events,
                    auto_pan_steering_events: state.auto_pan_steering_events,
                    captures: state.captures.clone(),
                    node_dragged: state.node_dragged,
                    selection_chrome_fitted: state.selection_chrome_fitted,
                    selection_chrome_sides: state.selection_chrome_sides,
                    selection_chrome_detached_run: state.selection_chrome_detached_run,
                    auto_pan_frames: state.auto_pan_frames,
                    connection_auto_pan_frames: state.connection_auto_pan_frames,
                    tab_focus_moved: state.tab_focus_moved,
                    tab_focus_restored: state.tab_focus_restored,
                    viewport_panned: state.viewport_panned,
                    viewport_zoomed: state.viewport_zoomed,
                    connection_created: state.connection_created,
                    connection_preview_wire_painted: state.connection_preview_wire_painted,
                    connection_release_frame_painted: state.connection_release_frame_painted,
                    connection_release_continuous: state.connection_release_continuous,
                    keyboard_edge_deleted: state.keyboard_edge_deleted,
                    reconnect_applied: state.reconnect_applied,
                    reconnect_preview_wire_painted: state.reconnect_preview_wire_painted,
                    reconnect_release_frame_painted: state.reconnect_release_frame_painted,
                    reconnect_release_continuous: state.reconnect_release_continuous,
                    search_opened: state.search_opened,
                    search_focus_moved: state.search_focus_moved,
                    search_cancelled: state.search_cancelled,
                    search_focus_restored: state.search_focus_restored,
                })
            }
            (ScenarioProbeState::LodThreshold(state), SmokeScenario::LodThreshold) => {
                SmokeScenarioEvidence::LodThreshold(SmokeLodEvidence {
                    captures: state.captures.clone(),
                    lod_continuous: state.continuous,
                    lod_max_jump_in_view: state.max_jump_in_view,
                })
            }
            _ => return Err("input_scenario_state_mismatch"),
        };
        Ok(Some(SmokeInputEvidence {
            bounds_verified: self.bounds_verified,
            scenario,
        }))
    }
}

fn interaction_presentation(
    surface: &Entity<NativeGraphSurface>,
    cx: &App,
) -> jellyflow_runtime::interaction::CanvasInteractionPresentation {
    let host = surface.read(cx).host().clone();
    host.read(cx).interaction_presentation()
}

fn advance_pending_interaction_frame(
    surface: &Entity<NativeGraphSurface>,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    let host = surface.read(cx).host().clone();
    host.update(cx, |host, cx| {
        host.advance_interaction_frame(Instant::now(), window, cx)
    })
}

fn interaction_is_settled(surface: &Entity<NativeGraphSurface>, cx: &App) -> bool {
    !matches!(
        interaction_presentation(surface, cx).phase(),
        CanvasInteractionPhase::Armed
            | CanvasInteractionPhase::NodeDrag
            | CanvasInteractionPhase::ConnectionDrag
            | CanvasInteractionPhase::ConnectionSearch
            | CanvasInteractionPhase::Committing
    )
}

fn connection_candidate_failure(
    surface: &Entity<NativeGraphSurface>,
    cx: &App,
    expected: PortId,
) -> Option<&'static str> {
    let presentation = interaction_presentation(surface, cx);
    match presentation.phase() {
        CanvasInteractionPhase::ConnectionDrag => {}
        CanvasInteractionPhase::Idle => {
            return Some(match presentation.terminal() {
                Some(InteractionTerminalReason::Applied) => "input_connection_drag_applied",
                Some(InteractionTerminalReason::Noop) => "input_connection_drag_noop",
                Some(InteractionTerminalReason::Rejected) => "input_connection_drag_rejected",
                Some(InteractionTerminalReason::Conflict) => "input_connection_drag_conflict",
                Some(InteractionTerminalReason::ExecutionFailed) => {
                    "input_connection_drag_execution_failed"
                }
                Some(InteractionTerminalReason::TransportFailure) => {
                    "input_connection_drag_transport_failure"
                }
                Some(InteractionTerminalReason::Uncertain) => {
                    "input_connection_drag_outcome_unknown"
                }
                Some(InteractionTerminalReason::Cancelled) => "input_connection_drag_cancelled",
                Some(InteractionTerminalReason::OwnerRetired) => {
                    "input_connection_drag_owner_retired"
                }
                Some(InteractionTerminalReason::ViewClosed) => "input_connection_drag_view_closed",
                Some(InteractionTerminalReason::BudgetExhausted) => {
                    "input_connection_drag_budget_exhausted"
                }
                Some(InteractionTerminalReason::GeometryIndeterminate) => {
                    "input_connection_drag_geometry_indeterminate"
                }
                Some(InteractionTerminalReason::IdentityExhausted) => {
                    "input_connection_drag_identity_exhausted"
                }
                None => "input_connection_drag_idle",
            });
        }
        CanvasInteractionPhase::Armed => return Some("input_connection_drag_armed"),
        CanvasInteractionPhase::EdgeActive => return Some("input_connection_edge_active"),
        _ => return Some("input_connection_drag_inactive"),
    }
    let Some(connection) = presentation.connection() else {
        return Some("input_connection_presentation_missing");
    };
    let Some(target) = connection.probe().target() else {
        return Some("input_connection_candidate_missing");
    };
    if target.port() != expected {
        return Some("input_connection_candidate_mismatch");
    }
    (!connection.probe().is_valid()).then_some("input_connection_candidate_invalid")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_evidence_rejects_duplicates_and_cross_scenario_states() {
        let mut interaction = InteractionProbeState::default();
        assert_eq!(
            interaction.record_capture(SmokeCaptureState::Initial),
            Ok(())
        );
        assert_eq!(
            interaction.record_capture(SmokeCaptureState::Initial),
            Err("input_duplicate_capture")
        );
        assert_eq!(
            interaction.record_capture(SmokeCaptureState::Lod086),
            Err("input_capture_scenario_mismatch")
        );
        assert_eq!(interaction.captures.len(), 1);

        let mut lod = LodProbeState::default();
        assert_eq!(lod.record_capture(SmokeCaptureState::Lod086), Ok(()));
        assert_eq!(
            lod.record_capture(SmokeCaptureState::Lod086),
            Err("input_duplicate_capture")
        );
        assert_eq!(
            lod.record_capture(SmokeCaptureState::Initial),
            Err("input_capture_scenario_mismatch")
        );
        assert_eq!(lod.captures.len(), 1);
    }

    #[test]
    fn safe_input_point_exits_the_auto_pan_band() {
        let surface = ViewSize::new(680.0, 640.0);

        assert!(!planning::input_point_is_safe(
            ViewPoint::new(36.0, 320.0),
            surface
        ));
        assert!(planning::input_point_is_safe(
            ViewPoint::new(48.0, 320.0),
            surface
        ));
        assert!(planning::input_point_is_safe(
            ViewPoint::new(632.0, 320.0),
            surface
        ));
        assert!(!planning::input_point_is_safe(
            ViewPoint::new(644.0, 320.0),
            surface
        ));
    }

    #[test]
    fn settled_connection_target_stops_auto_pan_without_losing_the_probe() {
        let surface = ViewSize::new(680.0, 640.0);

        assert_eq!(
            settled_connection_target_point(ViewPoint::new(646.0, 35.0), surface),
            Some(ViewPoint::new(640.0, 40.0))
        );
        assert_eq!(
            settled_connection_target_point(ViewPoint::new(664.0, 20.0), surface),
            None
        );
    }

    #[test]
    fn auto_pan_pointer_reveals_targets_on_both_axes() {
        let surface = ViewSize::new(680.0, 640.0);

        assert_eq!(
            auto_pan_pointer_toward(ViewPoint::new(900.0, 800.0), surface),
            Some(ViewPoint::new(668.0, 628.0))
        );
        assert_eq!(
            auto_pan_pointer_toward(ViewPoint::new(-120.0, -80.0), surface),
            Some(ViewPoint::new(12.0, 12.0))
        );
        assert_eq!(
            auto_pan_pointer_toward(ViewPoint::new(900.0, 320.0), surface),
            Some(ViewPoint::new(668.0, 320.0))
        );
        assert_eq!(
            auto_pan_pointer_toward(ViewPoint::new(340.0, 320.0), surface),
            Some(ViewPoint::new(668.0, 320.0))
        );
        assert_eq!(
            auto_pan_pointer_toward(ViewPoint::new(340.0, 160.0), surface),
            Some(ViewPoint::new(340.0, 12.0))
        );
    }

    #[test]
    fn auto_pan_steering_stops_the_axis_that_settles_first() {
        let surface = ViewSize::new(680.0, 640.0);

        assert_eq!(
            steer_auto_pan_pointer_toward(ViewPoint::new(-240.0, -800.0), surface),
            Some(ViewPoint::new(12.0, 12.0))
        );
        assert_eq!(
            steer_auto_pan_pointer_toward(ViewPoint::new(120.0, -500.0), surface),
            Some(ViewPoint::new(120.0, 12.0))
        );
    }

    #[test]
    fn auto_pan_steering_recovers_one_axis_after_unequal_distance_overshoot() {
        let surface = ViewSize::new(680.0, 640.0);

        assert_eq!(
            steer_auto_pan_pointer_toward(ViewPoint::new(660.0, -100.0), surface),
            Some(ViewPoint::new(668.0, 12.0))
        );
        assert_eq!(
            steer_auto_pan_pointer_toward(ViewPoint::new(100.0, 700.0), surface),
            Some(ViewPoint::new(100.0, 628.0))
        );
    }

    #[test]
    fn auto_pan_strategy_changes_as_the_target_crosses_the_surface() {
        let surface = ViewSize::new(680.0, 640.0);
        let strategy_for = |target| {
            let pointer = steer_auto_pan_pointer_toward(target, surface)
                .expect("a finite target on a valid surface must produce a pointer");
            auto_pan_pointer_strategy(pointer, surface)
                .expect("the planned pointer must produce a strategy")
        };

        let leading = strategy_for(ViewPoint::new(834.0, 20.0));
        let neutral = strategy_for(ViewPoint::new(834.0, 320.0));
        let trailing = strategy_for(ViewPoint::new(834.0, 3_974.0));

        assert_ne!(leading, neutral);
        assert_ne!(neutral, trailing);
        assert_ne!(leading, trailing);
    }
}
