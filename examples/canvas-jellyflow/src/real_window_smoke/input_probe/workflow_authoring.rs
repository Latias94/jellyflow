use std::collections::BTreeSet;

use jellyflow_core::{
    CanvasPoint, CanvasRect, CanvasSize, EdgeId, Graph, NodeId, PortId, TypeDesc,
};
use jellyflow_runtime::geometry::{ViewPoint, ViewRect, ViewSize};
use jellyflow_runtime::graph::GraphRevision;
use jellyflow_runtime::viewport::ViewportTransform;
use open_gpui::Keystroke;
use open_gpui_canvas_jellyflow::nodes::{
    WorkflowConditionSmokeLayout, workflow_condition_node_id, workflow_condition_smoke_layout,
};
use open_gpui_canvas_jellyflow::product_domain::condition::ConditionObservation;

use super::super::presented_frame::{SmokePortGeometry, SmokeRoute};
use super::*;

const LAYOUT_TOLERANCE: f32 = 0.5;
const SCROLL_LINES: f32 = -3.0;
const SELECT_FIRST_OPTION_CENTER_FROM_TRIGGER_TOP: f32 = 31.0;
const SELECT_OPTION_STRIDE: f32 = 36.0;
const RISK_SCORE_OPTION_INDEX: usize = 1;
const CASE_VALUE_INPUT: [&str; 3] = ["0", ".", "8"];

#[derive(Debug)]
pub(super) struct WorkflowAuthoringState {
    phase: WorkflowAuthoringPhase,
    wait_after_generation: u64,
    baseline: Option<WorkflowAuthoringBaseline>,
    stable_revision: Option<GraphRevision>,
    stable_size: Option<CanvasSize>,
    stable_samples: u8,
    typed_characters: usize,
    scroll: Option<WorkflowScrollBaseline>,
}

impl Default for WorkflowAuthoringState {
    fn default() -> Self {
        Self {
            phase: WorkflowAuthoringPhase::Reframe,
            wait_after_generation: 0,
            baseline: None,
            stable_revision: None,
            stable_size: None,
            stable_samples: 0,
            typed_characters: 0,
            scroll: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
enum WorkflowAuthoringPhase {
    #[default]
    Reframe,
    WaitLayout,
    SelectNode,
    WaitSelected,
    OpenField,
    WaitFieldOpen,
    WaitFieldCommitted,
    FocusValue,
    TypeValue,
    WaitDraft,
    AddCase,
    WaitConverged,
    Scroll,
    WaitScrolled,
    Done,
}

#[derive(Debug)]
struct WorkflowAuthoringBaseline {
    case_count: usize,
    ports: BTreeSet<PortId>,
    size: CanvasSize,
    position: CanvasPoint,
    incident_edges: BTreeSet<EdgeId>,
    viewport: ViewportTransform,
    presented_baseline: Option<(usize, ViewRect)>,
}

#[derive(Clone, Copy, Debug)]
struct WorkflowScrollBaseline {
    graph_revision: GraphRevision,
    layout_generation: u64,
    first_case_y: f32,
    connected_port: PortId,
    connected_port_anchor: CanvasPoint,
    connected_port_hit_bounds: CanvasRect,
    presented_render_sequence: u64,
    presented_connected_port_anchor: ViewPoint,
    connected_edge: EdgeId,
    connected_port_is_route_source: bool,
    presented_route_endpoint: ViewPoint,
    viewport: ViewportTransform,
    position: CanvasPoint,
}

#[derive(Clone, Copy, Debug)]
struct WorkflowConvergedFacts {
    revision: GraphRevision,
    size: CanvasSize,
    added_ports: [(PortId, CanvasPoint); 2],
}

impl RealWindowSmokeProbe {
    pub(super) fn advance_workflow_authoring(
        &mut self,
        window: &mut Window,
        cx: &mut App,
        surface: &Entity<NativeGraphSurface>,
        view: &WeakViewSession,
        surface_size: ViewSize,
    ) -> Result<Option<SmokeInputEvidence>, &'static str> {
        let phase = self.workflow_authoring()?.phase;
        match phase {
            WorkflowAuthoringPhase::Reframe => {
                let condition = workflow_condition_node_id();
                if self
                    .interaction()?
                    .node
                    .is_none_or(|plan| plan.node != condition)
                {
                    return Err("input_workflow_condition_drag_not_exercised");
                }
                self.workflow_authoring_mut()?.wait_after_generation =
                    workflow_condition_smoke_layout(cx, condition)
                        .map_or(0, WorkflowConditionSmokeLayout::generation);
                position_node_for_selection(view, condition, surface_size)?;
                window.refresh();
                self.set_workflow_authoring_phase(WorkflowAuthoringPhase::WaitLayout)?;
                Ok(None)
            }
            WorkflowAuthoringPhase::WaitLayout => {
                let condition = workflow_condition_node_id();
                let Some(layout) = workflow_condition_smoke_layout(cx, condition) else {
                    return self.wait_with_reason("input_workflow_layout_missing");
                };
                if layout.generation() <= self.workflow_authoring()?.wait_after_generation {
                    return self.wait_with_reason("input_workflow_layout_stale");
                }
                validate_control_layout(layout, surface_size)?;
                if !layout.field_enabled() {
                    return self.wait_with_reason("input_workflow_field_not_ready");
                }

                let snapshot = view
                    .graph_snapshot()
                    .map_err(|_| "input_graph_snapshot_failed")?;
                let observation = ConditionObservation::from_snapshot(&snapshot, condition)
                    .map_err(|_| "input_workflow_condition_observation_invalid")?;
                if observation.cases().len() != 2 {
                    return Err("input_workflow_condition_fixture_case_count_invalid");
                }
                let node = snapshot
                    .graph()
                    .node(&condition)
                    .ok_or("input_workflow_condition_fixture_incomplete")?;
                let Some(size) = node.size else {
                    return self.wait_with_reason("input_workflow_condition_size_missing");
                };
                let geometry = view
                    .geometry_snapshot()
                    .map_err(|_| "geometry_snapshot_read_failed")?;
                let Some(published) = geometry.node(condition).filter(|node| node.is_fresh())
                else {
                    return self.wait_with_reason("input_workflow_condition_geometry_stale");
                };
                if !sizes_close(published.bounds().size, size) {
                    return self.wait_with_reason("input_workflow_condition_size_not_converged");
                }
                let incident_edges = incident_edges(snapshot.graph(), condition)?;
                if incident_edges.is_empty() {
                    return Err("input_workflow_condition_incident_edges_missing");
                }
                if incident_edges
                    .iter()
                    .any(|edge| geometry.route(*edge).is_none())
                {
                    return self.wait_with_reason("input_workflow_condition_routes_missing");
                }
                self.workflow_authoring_mut()?.baseline = Some(WorkflowAuthoringBaseline {
                    case_count: observation.cases().len(),
                    ports: node.ports.iter().copied().collect(),
                    size,
                    position: node.pos,
                    incident_edges,
                    viewport: view
                        .viewport_transform()
                        .map_err(|_| "input_viewport_read_failed")?,
                    presented_baseline: None,
                });
                self.set_workflow_authoring_phase(WorkflowAuthoringPhase::SelectNode)?;
                Ok(None)
            }
            WorkflowAuthoringPhase::SelectNode => {
                let layout = current_condition_layout(cx)?;
                self.click_at_target_zoom(window, cx, view, rect_center(layout.header())?)?;
                self.set_workflow_authoring_phase(WorkflowAuthoringPhase::WaitSelected)?;
                Ok(None)
            }
            WorkflowAuthoringPhase::WaitSelected => {
                let condition = workflow_condition_node_id();
                let selection = view
                    .selection()
                    .map_err(|_| "input_selection_read_failed")?;
                if !selection.nodes().contains(&condition) || !interaction_is_settled(surface, cx) {
                    return self.wait_with_reason("input_workflow_condition_selection_missing");
                }
                if !self.wait_for_rendered_interaction_frame(window, cx, surface)? {
                    return self
                        .wait_with_reason("input_workflow_condition_selection_frame_missing");
                }
                let frames = self.presented_frames.frames();
                let frame = frames
                    .last()
                    .ok_or("input_workflow_presented_frame_missing")?;
                let bounds = frame
                    .node_bounds
                    .iter()
                    .find(|entry| entry.node == condition && entry.selected)
                    .ok_or("input_workflow_selected_frame_missing")?
                    .bounds;
                let baseline = self.workflow_baseline_mut()?;
                baseline.presented_baseline = Some((frames.len() - 1, bounds));
                self.set_workflow_authoring_phase(WorkflowAuthoringPhase::OpenField)?;
                Ok(None)
            }
            WorkflowAuthoringPhase::OpenField => {
                let layout = current_condition_layout(cx)?;
                self.click_at_target_zoom(window, cx, view, rect_center(layout.field())?)?;
                self.set_workflow_authoring_phase(WorkflowAuthoringPhase::WaitFieldOpen)?;
                Ok(None)
            }
            WorkflowAuthoringPhase::WaitFieldOpen => {
                if !self.wait_for_rendered_interaction_frame(window, cx, surface)? {
                    return self.wait_with_reason("input_workflow_field_overlay_frame_missing");
                }
                let layout = current_condition_layout(cx)?;
                self.click_at_target_zoom(
                    window,
                    cx,
                    view,
                    select_option_center(layout.field(), RISK_SCORE_OPTION_INDEX)?,
                )?;
                self.set_workflow_authoring_phase(WorkflowAuthoringPhase::WaitFieldCommitted)?;
                Ok(None)
            }
            WorkflowAuthoringPhase::WaitFieldCommitted => {
                if !self.wait_for_rendered_interaction_frame(window, cx, surface)? {
                    return self.wait_with_reason("input_workflow_field_commit_frame_missing");
                }
                self.set_workflow_authoring_phase(WorkflowAuthoringPhase::FocusValue)?;
                Ok(None)
            }
            WorkflowAuthoringPhase::FocusValue => {
                let layout = current_condition_layout(cx)?;
                self.click_at_target_zoom(window, cx, view, rect_center(layout.value())?)?;
                self.workflow_authoring_mut()?.typed_characters = 0;
                self.set_workflow_authoring_phase(WorkflowAuthoringPhase::TypeValue)?;
                Ok(None)
            }
            WorkflowAuthoringPhase::TypeValue => {
                let layout = current_condition_layout(cx)?;
                let state = self.workflow_authoring()?;
                if state.typed_characters > 0 && layout.generation() <= state.wait_after_generation
                {
                    return self.wait_with_reason("input_workflow_value_character_frame_missing");
                }
                let Some(character) = CASE_VALUE_INPUT.get(state.typed_characters) else {
                    self.set_workflow_authoring_phase(WorkflowAuthoringPhase::WaitDraft)?;
                    return Ok(None);
                };
                self.dispatch_keystroke_at_target_zoom(
                    window,
                    cx,
                    view,
                    Keystroke {
                        modifiers: Default::default(),
                        key: (*character).to_owned(),
                        key_char: Some((*character).to_owned()),
                    },
                )?;
                let state = self.workflow_authoring_mut()?;
                state.wait_after_generation = layout.generation();
                state.typed_characters = state.typed_characters.saturating_add(1);
                Ok(None)
            }
            WorkflowAuthoringPhase::WaitDraft => {
                let layout = current_condition_layout(cx)?;
                if layout.generation() <= self.workflow_authoring()?.wait_after_generation {
                    return self.wait_with_reason("input_workflow_value_draft_frame_missing");
                }
                if !layout.add_enabled() {
                    return self.wait_with_reason("input_workflow_add_not_ready");
                }
                self.verify_workflow_control_isolation(view)?;
                self.set_workflow_authoring_phase(WorkflowAuthoringPhase::AddCase)?;
                Ok(None)
            }
            WorkflowAuthoringPhase::AddCase => {
                let layout = current_condition_layout(cx)?;
                self.workflow_authoring_mut()?.wait_after_generation = layout.generation();
                self.click_at_target_zoom(window, cx, view, rect_center(layout.add())?)?;
                self.set_workflow_authoring_phase(WorkflowAuthoringPhase::WaitConverged)?;
                Ok(None)
            }
            WorkflowAuthoringPhase::WaitConverged => {
                let Some(facts) = self.observe_workflow_convergence(cx, view)? else {
                    return self.wait_with_reason("input_workflow_authoring_not_converged");
                };
                let stable = {
                    let state = self.workflow_authoring_mut()?;
                    let same = state.stable_revision == Some(facts.revision)
                        && state
                            .stable_size
                            .is_some_and(|size| sizes_close(size, facts.size));
                    if same {
                        state.stable_samples = state.stable_samples.saturating_add(1);
                    } else {
                        state.stable_revision = Some(facts.revision);
                        state.stable_size = Some(facts.size);
                        state.stable_samples = 1;
                    }
                    state.stable_samples >= 2
                };
                if !stable {
                    return self.wait_with_reason("input_workflow_authoring_size_unstable");
                }
                if !self.workflow_presented_span_is_ready()? {
                    return self.wait_with_reason("input_workflow_resize_frame_missing");
                }
                let presented_frames = self.presented_frames.frames();
                let Some(replacement_frame) = workflow_replacement_presented_frame(
                    &presented_frames,
                    self.workflow_baseline()?,
                    facts,
                )?
                else {
                    return self
                        .wait_with_reason("input_workflow_resize_presented_ports_not_converged");
                };
                let layout = current_condition_layout(cx)?;
                let snapshot = view
                    .graph_snapshot()
                    .map_err(|_| "input_graph_snapshot_failed")?;
                let condition = workflow_condition_node_id();
                let node = snapshot
                    .graph()
                    .node(&condition)
                    .ok_or("input_workflow_condition_fixture_incomplete")?;
                let observation = ConditionObservation::from_snapshot(&snapshot, condition)
                    .map_err(|_| "input_workflow_condition_observation_invalid")?;
                let connected_port = observation.else_branch().port_id();
                let (connected_edge, connected_port_is_route_source) =
                    connected_edge_for_port(snapshot.graph(), connected_port)?;
                let geometry = view
                    .geometry_snapshot()
                    .map_err(|_| "geometry_snapshot_read_failed")?;
                let connected_port_geometry = geometry
                    .port(connected_port)
                    .ok_or("input_workflow_scroll_port_missing")?;
                let connected_port_anchor = connected_port_geometry.route_anchor();
                let connected_port_hit_bounds = connected_port_geometry
                    .hit_bounds()
                    .ok_or("input_workflow_scroll_port_hit_bounds_missing")?;
                let Some(presented_connected_port) =
                    presented_port(replacement_frame, condition, connected_port)?
                else {
                    return self
                        .wait_with_reason("input_workflow_resize_connected_port_not_presented");
                };
                let Some(expected_presented_anchor) = replacement_frame
                    .viewport
                    .screen_point_for_canvas(connected_port_anchor)
                else {
                    return Err("input_workflow_resize_connected_port_projection_invalid");
                };
                if !view_points_close(
                    presented_connected_port.route_anchor,
                    expected_presented_anchor,
                ) {
                    return self
                        .wait_with_reason("input_workflow_resize_connected_port_not_converged");
                }
                let Some(presented_route) = presented_route(replacement_frame, connected_edge)?
                else {
                    return self.wait_with_reason("input_workflow_resize_connected_route_missing");
                };
                let presented_route_endpoint =
                    route_endpoint(presented_route, connected_port_is_route_source);
                if !view_points_close(
                    presented_route_endpoint,
                    presented_connected_port.route_anchor,
                ) {
                    return self
                        .wait_with_reason("input_workflow_resize_connected_route_not_converged");
                }
                self.workflow_authoring_mut()?.scroll = Some(WorkflowScrollBaseline {
                    graph_revision: snapshot.revision(),
                    layout_generation: layout.generation(),
                    first_case_y: layout.first_case().origin.y,
                    connected_port,
                    connected_port_anchor,
                    connected_port_hit_bounds,
                    presented_render_sequence: replacement_frame.render_sequence,
                    presented_connected_port_anchor: presented_connected_port.route_anchor,
                    connected_edge,
                    connected_port_is_route_source,
                    presented_route_endpoint,
                    viewport: view
                        .viewport_transform()
                        .map_err(|_| "input_viewport_read_failed")?,
                    position: node.pos,
                });
                self.set_workflow_authoring_phase(WorkflowAuthoringPhase::Scroll)?;
                Ok(None)
            }
            WorkflowAuthoringPhase::Scroll => {
                let layout = current_condition_layout(cx)?;
                self.dispatch_at_target_zoom(
                    window,
                    cx,
                    view,
                    scroll_lines(rect_center(layout.scroll())?, SCROLL_LINES),
                )?;
                self.set_workflow_authoring_phase(WorkflowAuthoringPhase::WaitScrolled)?;
                Ok(None)
            }
            WorkflowAuthoringPhase::WaitScrolled => {
                let scroll = self
                    .workflow_authoring()?
                    .scroll
                    .ok_or("input_workflow_scroll_baseline_missing")?;
                let layout = current_condition_layout(cx)?;
                if layout.generation() <= scroll.layout_generation
                    || layout.first_case().origin.y >= scroll.first_case_y - LAYOUT_TOLERANCE
                {
                    return self.wait_with_reason("input_workflow_scroll_not_observed");
                }
                let snapshot = view
                    .graph_snapshot()
                    .map_err(|_| "input_graph_snapshot_failed")?;
                let node = snapshot
                    .graph()
                    .node(&workflow_condition_node_id())
                    .ok_or("input_workflow_condition_fixture_incomplete")?;
                if snapshot.revision() != scroll.graph_revision || node.pos != scroll.position {
                    return Err("input_workflow_scroll_mutated_graph");
                }
                if view
                    .viewport_transform()
                    .map_err(|_| "input_viewport_read_failed")?
                    != scroll.viewport
                {
                    return Err("input_workflow_scroll_changed_viewport");
                }
                self.verify_workflow_control_isolation(view)?;
                let baseline = self.workflow_baseline()?;
                if incident_edges(snapshot.graph(), workflow_condition_node_id())?
                    != baseline.incident_edges
                {
                    return Err("input_workflow_scroll_changed_connections");
                }
                let geometry = view
                    .geometry_snapshot()
                    .map_err(|_| "geometry_snapshot_read_failed")?;
                let connected_port = geometry
                    .port(scroll.connected_port)
                    .ok_or("input_workflow_scroll_port_missing")?;
                let row_delta = layout.first_case().origin.y - scroll.first_case_y;
                let current_anchor = connected_port.route_anchor();
                let anchor_delta = current_anchor.y - scroll.connected_port_anchor.y;
                if (anchor_delta - row_delta).abs() > LAYOUT_TOLERANCE {
                    return Err("input_workflow_scroll_port_geometry_stale");
                }
                if connected_port.hit_bounds().is_some_and(|bounds| {
                    let hit_bounds_delta =
                        bounds.origin.y - scroll.connected_port_hit_bounds.origin.y;
                    (hit_bounds_delta - row_delta).abs() > LAYOUT_TOLERANCE
                }) {
                    return Err("input_workflow_scroll_port_hit_bounds_stale");
                }
                if baseline
                    .incident_edges
                    .iter()
                    .any(|edge| geometry.route(*edge).is_none())
                {
                    return self.wait_with_reason("input_workflow_scroll_routes_missing");
                }
                let presented_frames = self.presented_frames.frames();
                if !workflow_scrolled_presented_frame_is_ready(
                    &presented_frames,
                    scroll,
                    current_anchor,
                    row_delta,
                )? {
                    return self
                        .wait_with_reason("input_workflow_scroll_presented_frame_not_converged");
                }
                self.set_workflow_authoring_phase(WorkflowAuthoringPhase::Done)?;
                Ok(None)
            }
            WorkflowAuthoringPhase::Done => {
                self.presented_frames.request_final()?;
                window.refresh();
                self.transition(ProbePhase::FinalWait);
                Ok(None)
            }
        }
    }

    fn workflow_authoring(&self) -> Result<&WorkflowAuthoringState, &'static str> {
        self.interaction()?
            .workflow_authoring
            .as_ref()
            .ok_or("input_workflow_authoring_state_missing")
    }

    fn workflow_authoring_mut(&mut self) -> Result<&mut WorkflowAuthoringState, &'static str> {
        self.interaction_mut()?
            .workflow_authoring
            .as_mut()
            .ok_or("input_workflow_authoring_state_missing")
    }

    fn workflow_baseline(&self) -> Result<&WorkflowAuthoringBaseline, &'static str> {
        self.workflow_authoring()?
            .baseline
            .as_ref()
            .ok_or("input_workflow_authoring_baseline_missing")
    }

    fn workflow_baseline_mut(&mut self) -> Result<&mut WorkflowAuthoringBaseline, &'static str> {
        self.workflow_authoring_mut()?
            .baseline
            .as_mut()
            .ok_or("input_workflow_authoring_baseline_missing")
    }

    fn set_workflow_authoring_phase(
        &mut self,
        phase: WorkflowAuthoringPhase,
    ) -> Result<(), &'static str> {
        self.workflow_authoring_mut()?.phase = phase;
        self.wait_ticks = 0;
        Ok(())
    }

    fn verify_workflow_control_isolation(
        &self,
        view: &WeakViewSession,
    ) -> Result<(), &'static str> {
        let baseline = self.workflow_baseline()?;
        if view
            .viewport_transform()
            .map_err(|_| "input_viewport_read_failed")?
            != baseline.viewport
        {
            return Err("input_workflow_control_changed_viewport");
        }
        let selection = view
            .selection()
            .map_err(|_| "input_selection_read_failed")?;
        selection
            .nodes()
            .contains(&workflow_condition_node_id())
            .then_some(())
            .ok_or("input_workflow_control_changed_selection")
    }

    fn observe_workflow_convergence(
        &self,
        cx: &App,
        view: &WeakViewSession,
    ) -> Result<Option<WorkflowConvergedFacts>, &'static str> {
        let condition = workflow_condition_node_id();
        let baseline = self.workflow_baseline()?;
        self.verify_workflow_control_isolation(view)?;
        let snapshot = view
            .graph_snapshot()
            .map_err(|_| "input_graph_snapshot_failed")?;
        let observation = ConditionObservation::from_snapshot(&snapshot, condition)
            .map_err(|_| "input_workflow_condition_observation_invalid")?;
        if observation.cases().len() == baseline.case_count {
            return Ok(None);
        }
        if observation.cases().len() != baseline.case_count + 1 {
            return Err("input_workflow_condition_case_count_invalid");
        }
        let added = observation
            .cases()
            .last()
            .ok_or("input_workflow_condition_case_missing")?;
        if added.predicate().operand() != "risk_score"
            || added.predicate().value_type() != &TypeDesc::Float
            || added
                .predicate()
                .literal()
                .as_f64()
                .is_none_or(|value| (value - 0.8).abs() > f64::EPSILON)
        {
            return Err("input_workflow_condition_case_semantics_invalid");
        }
        let graph = snapshot.graph();
        let node = graph
            .node(&condition)
            .ok_or("input_workflow_condition_fixture_incomplete")?;
        if node.pos != baseline.position {
            return Err("input_workflow_authoring_moved_node");
        }
        let ports = node.ports.iter().copied().collect::<BTreeSet<_>>();
        let expected_added = BTreeSet::from([added.port_id(), added.value_port_id()]);
        let actual_added = ports
            .difference(&baseline.ports)
            .copied()
            .collect::<BTreeSet<_>>();
        if !baseline.ports.is_subset(&ports) || actual_added != expected_added {
            return Err("input_workflow_condition_dynamic_ports_invalid");
        }
        if incident_edges(graph, condition)? != baseline.incident_edges {
            return Err("input_workflow_authoring_changed_connections");
        }
        let Some(size) = node.size else {
            return Ok(None);
        };
        if (size.width - baseline.size.width).abs() > LAYOUT_TOLERANCE
            || size.height <= baseline.size.height + LAYOUT_TOLERANCE
        {
            return Ok(None);
        }
        let geometry = view
            .geometry_snapshot()
            .map_err(|_| "geometry_snapshot_read_failed")?;
        let Some(published) = geometry.node(condition).filter(|node| node.is_fresh()) else {
            return Ok(None);
        };
        let Some(added_route) = geometry.port(added.port_id()) else {
            return Ok(None);
        };
        let Some(added_value) = geometry.port(added.value_port_id()) else {
            return Ok(None);
        };
        if !sizes_close(published.bounds().size, size)
            || baseline
                .incident_edges
                .iter()
                .any(|edge| geometry.route(*edge).is_none())
        {
            return Ok(None);
        }
        let Some(layout) = workflow_condition_smoke_layout(cx, condition) else {
            return Ok(None);
        };
        if layout.generation() <= self.workflow_authoring()?.wait_after_generation {
            return Ok(None);
        }
        Ok(Some(WorkflowConvergedFacts {
            revision: snapshot.revision(),
            size,
            added_ports: [
                (added.port_id(), added_route.route_anchor()),
                (added.value_port_id(), added_value.route_anchor()),
            ],
        }))
    }

    fn workflow_presented_span_is_ready(&self) -> Result<bool, &'static str> {
        let baseline = self.workflow_baseline()?;
        let frames = self.presented_frames.frames();
        validate_workflow_presented_span(&frames, baseline)
    }
}

fn current_condition_layout(cx: &App) -> Result<WorkflowConditionSmokeLayout, &'static str> {
    workflow_condition_smoke_layout(cx, workflow_condition_node_id())
        .ok_or("input_workflow_layout_missing")
}

fn rect_center(bounds: ViewRect) -> Result<ViewPoint, &'static str> {
    if !bounds.is_positive_finite() {
        return Err("input_workflow_control_bounds_invalid");
    }
    Ok(ViewPoint::new(
        bounds.origin.x + bounds.size.width * 0.5,
        bounds.origin.y + bounds.size.height * 0.5,
    ))
}

fn select_option_center(trigger: ViewRect, option_index: usize) -> Result<ViewPoint, &'static str> {
    if !trigger.is_positive_finite() {
        return Err("input_workflow_field_option_bounds_invalid");
    }
    // Open GPUI Select 0.2 renders Medium options in its deferred native layer at a fixed
    // 36-logical-pixel stride. The smoke owns only this concrete user click, not popup layout.
    let y = trigger.origin.y
        + SELECT_FIRST_OPTION_CENTER_FROM_TRIGGER_TOP
        + option_index as f32 * SELECT_OPTION_STRIDE;
    if !y.is_finite() {
        return Err("input_workflow_field_option_bounds_invalid");
    }
    Ok(ViewPoint::new(
        trigger.origin.x + trigger.size.width * 0.5,
        y,
    ))
}

fn validate_control_layout(
    layout: WorkflowConditionSmokeLayout,
    surface_size: ViewSize,
) -> Result<(), &'static str> {
    for bounds in [
        layout.root(),
        layout.header(),
        layout.field(),
        layout.value(),
        layout.add(),
        layout.scroll(),
        layout.first_case(),
    ] {
        if !rect_fits_surface(bounds, surface_size) {
            return Err("input_workflow_control_outside_surface");
        }
    }
    Ok(())
}

fn rect_fits_surface(bounds: ViewRect, surface_size: ViewSize) -> bool {
    bounds.is_positive_finite()
        && surface_size.is_positive_finite()
        && bounds.origin.x >= -LAYOUT_TOLERANCE
        && bounds.origin.y >= -LAYOUT_TOLERANCE
        && bounds.origin.x + bounds.size.width <= surface_size.width + LAYOUT_TOLERANCE
        && bounds.origin.y + bounds.size.height <= surface_size.height + LAYOUT_TOLERANCE
}

fn sizes_close(left: CanvasSize, right: CanvasSize) -> bool {
    (left.width - right.width).abs() <= LAYOUT_TOLERANCE
        && (left.height - right.height).abs() <= LAYOUT_TOLERANCE
}

fn incident_edges(graph: &Graph, node: NodeId) -> Result<BTreeSet<EdgeId>, &'static str> {
    let ports = graph
        .node(&node)
        .ok_or("input_workflow_condition_fixture_incomplete")?
        .ports
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    Ok(graph
        .edges()
        .iter()
        .filter_map(|(edge_id, edge)| {
            (ports.contains(&edge.from) || ports.contains(&edge.to)).then_some(*edge_id)
        })
        .collect())
}

fn workflow_replacement_presented_frame<'a>(
    frames: &'a [SmokePresentedFrame],
    baseline: &WorkflowAuthoringBaseline,
    facts: WorkflowConvergedFacts,
) -> Result<Option<&'a SmokePresentedFrame>, &'static str> {
    let Some((start, _)) = baseline.presented_baseline else {
        return Err("input_workflow_presented_baseline_missing");
    };
    let Some(span) = frames.get(start..) else {
        return Ok(None);
    };
    let condition = workflow_condition_node_id();
    for frame in span.iter().rev() {
        if frame.stage != SmokeFrameStage::Interaction
            || frame.graph_revision != facts.revision
            || frame.viewport != baseline.viewport
            || frame.preview_route.is_some()
            || frame.preview_target.is_some()
            || frame.suppressed_edge.is_some()
        {
            continue;
        }
        let Some(node) = frame
            .node_bounds
            .iter()
            .find(|entry| entry.node == condition)
        else {
            continue;
        };
        if (node.bounds.size.width - facts.size.width).abs() > LAYOUT_TOLERANCE
            || (node.bounds.size.height - facts.size.height).abs() > LAYOUT_TOLERANCE
        {
            continue;
        }

        let mut ports_align = true;
        for (port_id, authoritative_anchor) in facts.added_ports {
            let Some(port) = presented_port(frame, condition, port_id)? else {
                ports_align = false;
                break;
            };
            let Some(expected_anchor) =
                frame.viewport.screen_point_for_canvas(authoritative_anchor)
            else {
                return Err("input_workflow_resize_port_projection_invalid");
            };
            if !view_points_close(port.route_anchor, expected_anchor) {
                ports_align = false;
                break;
            }
        }
        if ports_align {
            return Ok(Some(frame));
        }
    }
    Ok(None)
}

fn workflow_scrolled_presented_frame_is_ready(
    frames: &[SmokePresentedFrame],
    baseline: WorkflowScrollBaseline,
    authoritative_anchor: CanvasPoint,
    row_delta: f32,
) -> Result<bool, &'static str> {
    for frame in frames.iter().rev() {
        if frame.render_sequence <= baseline.presented_render_sequence
            || frame.stage != SmokeFrameStage::Interaction
            || frame.graph_revision != baseline.graph_revision
            || frame.viewport != baseline.viewport
            || frame.preview_route.is_some()
            || frame.preview_target.is_some()
            || frame.suppressed_edge.is_some()
        {
            continue;
        }
        let Some(port) =
            presented_port(frame, workflow_condition_node_id(), baseline.connected_port)?
        else {
            continue;
        };
        let Some(expected_anchor) = frame.viewport.screen_point_for_canvas(authoritative_anchor)
        else {
            return Err("input_workflow_scroll_port_projection_invalid");
        };
        if !view_points_close(port.route_anchor, expected_anchor) {
            continue;
        }
        let presented_anchor_delta = ViewPoint::new(
            port.route_anchor.x - baseline.presented_connected_port_anchor.x,
            port.route_anchor.y - baseline.presented_connected_port_anchor.y,
        );
        if presented_anchor_delta.x.abs() > LAYOUT_TOLERANCE
            || (presented_anchor_delta.y - row_delta).abs() > LAYOUT_TOLERANCE
        {
            return Err("input_workflow_scroll_presented_port_delta_invalid");
        }

        let Some(route) = presented_route(frame, baseline.connected_edge)? else {
            continue;
        };
        let endpoint = route_endpoint(route, baseline.connected_port_is_route_source);
        if !view_points_close(endpoint, port.route_anchor) {
            continue;
        }
        let endpoint_delta = ViewPoint::new(
            endpoint.x - baseline.presented_route_endpoint.x,
            endpoint.y - baseline.presented_route_endpoint.y,
        );
        if endpoint_delta.x.abs() > LAYOUT_TOLERANCE
            || (endpoint_delta.y - row_delta).abs() > LAYOUT_TOLERANCE
        {
            return Err("input_workflow_scroll_presented_route_delta_invalid");
        }
        return Ok(true);
    }
    Ok(false)
}

fn connected_edge_for_port(graph: &Graph, port: PortId) -> Result<(EdgeId, bool), &'static str> {
    graph
        .edges()
        .iter()
        .find_map(|(edge_id, edge)| {
            if edge.from == port {
                Some((*edge_id, true))
            } else if edge.to == port {
                Some((*edge_id, false))
            } else {
                None
            }
        })
        .ok_or("input_workflow_scroll_connected_edge_missing")
}

fn presented_port(
    frame: &SmokePresentedFrame,
    node: NodeId,
    port: PortId,
) -> Result<Option<&SmokePortGeometry>, &'static str> {
    let mut matches = frame
        .port_geometries
        .iter()
        .filter(|entry| entry.node == node && entry.port == port);
    let first = matches.next();
    if matches.next().is_some() {
        return Err("input_workflow_presented_port_duplicated");
    }
    Ok(first)
}

fn presented_route(
    frame: &SmokePresentedFrame,
    edge: EdgeId,
) -> Result<Option<&SmokeRoute>, &'static str> {
    let mut matches = frame
        .authoritative_routes
        .iter()
        .filter(|route| route.edge == edge);
    let first = matches.next();
    if matches.next().is_some() {
        return Err("input_workflow_presented_route_duplicated");
    }
    Ok(first)
}

fn route_endpoint(route: &SmokeRoute, source: bool) -> ViewPoint {
    if source { route.source } else { route.target }
}

fn view_points_close(left: ViewPoint, right: ViewPoint) -> bool {
    (left.x - right.x).abs() <= LAYOUT_TOLERANCE && (left.y - right.y).abs() <= LAYOUT_TOLERANCE
}

fn validate_workflow_presented_span(
    frames: &[SmokePresentedFrame],
    baseline: &WorkflowAuthoringBaseline,
) -> Result<bool, &'static str> {
    let Some((start, initial_bounds)) = baseline.presented_baseline else {
        return Err("input_workflow_presented_baseline_missing");
    };
    let origin = initial_bounds.origin;
    let width = initial_bounds.size.width;
    let initial_height = initial_bounds.size.height;
    let Some(span) = frames.get(start..) else {
        return Ok(false);
    };
    if span.len() < 2 {
        return Ok(false);
    }

    let condition = workflow_condition_node_id();
    let mut previous_height = initial_height;
    let mut grew = false;
    for frame in span {
        if frame.stage != SmokeFrameStage::Interaction
            || frame.viewport != baseline.viewport
            || frame.preview_route.is_some()
            || frame.preview_target.is_some()
            || frame.suppressed_edge.is_some()
        {
            return Err("input_workflow_presented_frame_not_authoritative");
        }
        let mut nodes = frame
            .node_bounds
            .iter()
            .filter(|entry| entry.node == condition);
        let node = nodes
            .next()
            .ok_or("input_workflow_presented_node_missing")?;
        if nodes.next().is_some() {
            return Err("input_workflow_presented_node_duplicated");
        }
        if !node.selected
            || (node.bounds.origin.x - origin.x).abs() > LAYOUT_TOLERANCE
            || (node.bounds.origin.y - origin.y).abs() > LAYOUT_TOLERANCE
            || (node.bounds.size.width - width).abs() > LAYOUT_TOLERANCE
            || node.bounds.size.height <= 0.0
            || node.bounds.size.height + LAYOUT_TOLERANCE < previous_height
        {
            return Err("input_workflow_presented_node_discontinuous");
        }
        grew |= node.bounds.size.height > initial_height + LAYOUT_TOLERANCE;
        previous_height = node.bounds.size.height;
        if baseline.incident_edges.iter().any(|edge| {
            !frame
                .authoritative_routes
                .iter()
                .any(|route| route.edge == *edge)
        }) {
            return Err("input_workflow_presented_route_missing");
        }
    }
    Ok(grew)
}
