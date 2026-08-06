mod events;
mod evidence;
mod phase;
mod planning;
mod presented_frames;
mod scenario;
mod workflow_authoring;

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use jellyflow_core::{CanvasPoint, EdgeId, PortId};
use jellyflow_runtime::geometry::{EdgePath, ViewPoint, ViewSize};
use jellyflow_runtime::graph::EdgeEndpoint;
use jellyflow_runtime::interaction::CanvasInteractionPhase;
use jellyflow_runtime::view::{PublishedPortOutcome, WeakViewSession};
use jellyflow_runtime::viewport::ViewportTransform;
use open_gpui::{
    App, Entity, FocusHandle, Keystroke, MouseButton, PinchEvent, PlatformInput, TouchPhase,
    Window, point, px,
};
use open_gpui_canvas_jellyflow::connection_search::NativeGraphSurface;
use open_gpui_canvas_jellyflow::nodes::workflow_condition_node_id;

use crate::native_app::NativeDemo;

use self::events::{key_down, key_up, mouse_down, mouse_move, mouse_up, scroll_lines};
use self::evidence::{
    RenderedFrameFence, RenderedFrameFencePoll, capture_visual_frame, connection_preview_path,
    inspect_current_frame, inspect_edge_path_frame,
};
use self::phase::ProbePhase;
use self::planning::{
    EdgePlan, NodePlan, ReconnectPlan, VisibleEdgePickPoint, auto_pan_pointer_toward,
    blank_surface_point, choose_reconnect, dragged_geometry_status, edge_owner_reveal_anchor,
    free_connection_port, geometry_fits_surface, graph_contains_edge, point_changed,
    point_inside_surface, port_owner_view_center, port_point, position_node_for_selection,
    refresh_edge_plan, reveal_canvas_points, set_zoom_centered_on_canvas_point,
    set_zoom_preserving_surface_center, settled_connection_target_point, visible_port_point,
    visible_reconnect_grip, visible_unique_edge_point,
};
use self::presented_frames::PresentedFrameRecorder;
use self::workflow_authoring::WorkflowAuthoringState;
use super::presented_frame::{SmokeFrameStage, SmokePresentedFrame};
use super::{
    INTERACTION_CAPTURE_STATES, LOD_CAPTURE_STATES, SMOKE_AUTOPAN_MIN_FRAMES, SmokeBounds,
    SmokeCaptureState, SmokeConfig, SmokeExpectations, SmokeGeometryEvidence, SmokeInputEvidence,
    SmokeInteractionEvidence, SmokeLodEvidence, SmokePaintEvidence, SmokePaintPalette,
    SmokeScenario, SmokeScenarioEvidence, SmokeZoom,
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
    reconnect_auto_pan_last: Option<ViewportTransform>,
    reconnect_auto_pan_frames: u8,
    rendered_frame_fence: RenderedFrameFence,
    release_wire_path: Option<EdgePath>,
    tab_focus_origin: Option<FocusHandle>,
    tab_focus_forward: Option<FocusHandle>,
    selection_reframed: bool,
    reconnect_selection_reframed: bool,
    search_focus_baseline: Option<FocusHandle>,
    search_render_requested: bool,
    workflow_authoring: Option<WorkflowAuthoringState>,
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
    layout_revision_before: u64,
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
            layout_revision_before: 0,
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
    demo: NativeDemo,
    config: SmokeConfig,
    phase: ProbePhase,
    wait_ticks: u16,
    geometry: Option<SmokeGeometryEvidence>,
    paint: Option<SmokePaintEvidence>,
    scenario: ScenarioProbeState,
    paint_palette: SmokePaintPalette,
    presented_frames: PresentedFrameRecorder,
}

impl RealWindowSmokeProbe {
    pub(super) fn new(
        demo: NativeDemo,
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
            presented_frames: PresentedFrameRecorder::default(),
        }
    }

    pub(super) fn geometry(&self) -> Option<&SmokeGeometryEvidence> {
        self.geometry.as_ref()
    }

    pub(super) fn paint(&self) -> Option<&SmokePaintEvidence> {
        self.paint.as_ref()
    }

    pub(super) fn presented_frames(&self) -> Vec<SmokePresentedFrame> {
        self.presented_frames.frames()
    }

    pub(super) fn poll_lod_presented_frame(
        &self,
        surface: &Entity<NativeGraphSurface>,
        cx: &mut App,
        expected_zoom: f32,
    ) -> Result<bool, &'static str> {
        let host = surface.read(cx).host().clone();
        let scenes = host.update(cx, |host, _| host.drain_smoke_presented_scenes());
        self.presented_frames.poll_lod_frames(scenes, expected_zoom)
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
        if let Some(reason) = self.presented_frames.failure() {
            return Err(reason);
        }
        let outcome =
            scenario::advance(self, window, cx, surface, view, expectations, surface_size);
        if self.presented_frames.is_active() && matches!(&outcome, Ok(None)) {
            jellyflow_open_gpui::host::request_native_test_frame(window);
        }
        outcome
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
        if let Some(chrome) = selection_chrome
            && let Some(reason) = chrome.failure_reason()
        {
            return Err(reason);
        }
        self.geometry = Some(geometry);
        self.paint = Some(paint);
        self.interaction_mut()?.record_capture(state)?;
        Ok(())
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
        let (node, bounds) = if self.demo == NativeDemo::Workflow {
            let node = workflow_condition_node_id();
            let bounds = snapshot
                .node(node)
                .filter(|geometry| geometry.is_fresh())
                .map(|geometry| geometry.bounds())
                .ok_or("input_workflow_condition_fixture_incomplete")?;
            (node, bounds)
        } else {
            expectations
                .node_ports
                .keys()
                .filter_map(|node| {
                    snapshot
                        .node(*node)
                        .filter(|geometry| geometry.is_fresh())
                        .map(|geometry| (*node, geometry.bounds()))
                })
                .min_by(|(_, left), (_, right)| left.size.width.total_cmp(&right.size.width))
                .ok_or("input_node_fixture_incomplete")?
        };
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
            x: if self.demo == NativeDemo::Workflow {
                surface_size.width - 12.0
            } else {
                12.0
            },
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
        Ok(())
    }

    fn click_at_target_zoom(
        &mut self,
        window: &mut Window,
        cx: &mut App,
        view: &WeakViewSession,
        point: ViewPoint,
    ) -> Result<(), &'static str> {
        self.dispatch_at_target_zoom(window, cx, view, mouse_down(MouseButton::Left, point))?;
        self.dispatch_at_target_zoom(window, cx, view, mouse_up(MouseButton::Left, point))
    }

    fn dispatch_keystroke_at_target_zoom(
        &mut self,
        window: &mut Window,
        cx: &mut App,
        view: &WeakViewSession,
        keystroke: Keystroke,
    ) -> Result<(), &'static str> {
        self.interaction()?;
        self.verify_target_zoom(view)?;
        if !window.dispatch_keystroke(keystroke, cx) {
            return Err("input_workflow_keystroke_unhandled");
        }
        window.refresh();
        Ok(())
    }

    fn should_run_workflow_authoring(&self) -> bool {
        self.demo == NativeDemo::Workflow
            && self.config.bounds() == SmokeBounds::Compact
            && self.config.scenario() == SmokeScenario::Interaction
            && self.config.zoom() == SmokeZoom::One
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
                SmokeScenarioEvidence::Interaction(SmokeInteractionEvidence {
                    captures: state.captures.clone(),
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
        Ok(Some(SmokeInputEvidence { scenario }))
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
            return Some(if presentation.terminal().is_some() {
                "input_connection_drag_terminated"
            } else {
                "input_connection_drag_idle"
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
    (!connection.probe().is_committable()).then_some("input_connection_candidate_invalid")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_pan_planning_respects_safe_settled_and_directional_bounds() {
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

        assert_eq!(
            settled_connection_target_point(ViewPoint::new(646.0, 35.0), surface),
            Some(ViewPoint::new(640.0, 40.0))
        );
        assert_eq!(
            settled_connection_target_point(ViewPoint::new(664.0, 20.0), surface),
            None
        );

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
}
