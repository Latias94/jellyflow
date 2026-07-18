use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

#[cfg(test)]
use std::cell::{Cell, RefCell};

use jellyflow::core::{
    CanvasPoint, CanvasRect, CanvasSize, EdgeId, EdgeLabelAnchor, Graph, NodeId,
};
use jellyflow::runtime::runtime::geometry::{
    EdgePath, EdgePathAnchor, EdgePathSample, EdgeRouteFacts, PathCommand, resolve_edge_route_path,
};
use jellyflow::runtime::runtime::viewport::ViewportTransform;
use jellyflow::runtime::{
    CanvasInteractionPhase, EdgeEndpoint, FeedbackSubmissionError, GeometrySnapshot,
    RouteGeometryRevision, ViewPoint, ViewRect, ViewSessionError, ViewSize,
    VisibleRouteQueryRevision,
};
use open_gpui::prelude::FluentBuilder;
use open_gpui::{
    AnyElement, Context, CursorStyle, EntityId, FocusHandle, InteractiveElement, IntoElement,
    MouseButton, ParentElement, Path, PathBuilder, Pixels, StatefulInteractiveElement, Styled,
    Window, canvas, div, measured_element, point, px,
};

use crate::accessibility::{
    OPEN_GPUI_EDGE_ROLE, OPEN_GPUI_RECONNECT_HANDLE_ROLE, node_semantic_element,
    open_gpui_edge_element_id, open_gpui_interaction_element_id, open_gpui_node_element_id,
    open_gpui_node_measurement_element_id, open_gpui_reconnect_element_id,
    open_gpui_scene_element_id, open_gpui_scene_measurement_element_id, scene_semantic_element,
};
use crate::chrome::{
    OpenGpuiEdgeLabelAppearance, OpenGpuiEdgeMarkerShape, OpenGpuiEdgePaintFallbacks,
    OpenGpuiEdgePainterBatchKey, OpenGpuiEdgeStrokePattern, OpenGpuiEdgeVisualState,
};
use crate::command_journal::{OpenGpuiCommandTicketError, resolve_command_tickets};
use crate::interaction::{OpenGpuiGraphKeyListener, OpenGpuiSceneGestureKind, interaction_root};
use crate::semantic_zoom::OpenGpuiNodePresentation;

#[cfg(test)]
use crate::interaction::{
    OpenGpuiSceneGestureCancelReason, OpenGpuiSceneGestureEvent, OpenGpuiSceneGesturePhase,
};

use super::OpenGpuiSceneHost;
use super::connection::OpenGpuiConnectionPreviewPlan;
use super::diagnostics::OpenGpuiEdgeLayerOverflow;
use super::edge_layer::{
    OpenGpuiEdgePaintBatch, OpenGpuiEdgePaintItemKey, OpenGpuiEdgePaintPlanKey,
    OpenGpuiEdgePaintPlanSegment, OpenGpuiEdgePathCacheKey, OpenGpuiVisibleRouteIdentity,
};
use super::focus_navigation::{OpenGpuiGraphFocusEntry, OpenGpuiGraphFocusTarget};
use super::frame_feedback::{
    DeferredFeedbackError, DeferredFeedbackSubmission, DeferredFrameFeedback,
};
use super::node_lifecycle::suppress_host_panic;
use super::scene_gesture::selection_mode_for_modifiers;

pub(super) const OPEN_GPUI_SELECTION_RECTANGLE_SELECTOR: &str = "jellyflow-selection-rectangle";
pub(super) const OPEN_GPUI_GRID_SELECTOR: &str = "jellyflow-grid";
pub(super) const OPEN_GPUI_ALIGNMENT_GUIDE_SELECTOR: &str = "jellyflow-alignment-guide";

const OPEN_GPUI_MIN_GRID_SPACING_IN_VIEW: f32 = 12.0;
const OPEN_GPUI_MAX_GRID_LINES_PER_AXIS: usize = 512;
const OPEN_GPUI_MAJOR_GRID_INTERVAL: i64 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OpenGpuiSceneLayer {
    Edge,
    Preview,
    EdgeChrome,
    EdgeInteraction,
    Node,
    InteractionChrome,
    Overlay,
}

#[cfg(test)]
pub(super) const OPEN_GPUI_SCENE_LAYER_ORDER: [OpenGpuiSceneLayer; 7] = [
    OpenGpuiSceneLayer::Edge,
    OpenGpuiSceneLayer::Preview,
    OpenGpuiSceneLayer::EdgeChrome,
    OpenGpuiSceneLayer::EdgeInteraction,
    OpenGpuiSceneLayer::Node,
    OpenGpuiSceneLayer::InteractionChrome,
    OpenGpuiSceneLayer::Overlay,
];

#[derive(Clone)]
pub(super) struct OpenGpuiSceneWire {
    pub(super) edge: EdgeId,
    pub(super) geometry_revision: Option<RouteGeometryRevision>,
    pub(super) visual_state: OpenGpuiEdgeVisualState,
    pub(super) paint_key: OpenGpuiEdgePainterBatchKey,
    pub(super) path_stroke_width: f32,
    source_marker: Option<OpenGpuiPreparedEdgeMarker>,
    target_marker: Option<OpenGpuiPreparedEdgeMarker>,
    label: Option<OpenGpuiPreparedEdgeLabel>,
    pub(super) drag_path: Option<EdgePath>,
}

#[derive(Debug, Clone, Copy)]
struct OpenGpuiPreparedEdgeMarker {
    sample: EdgePathSample,
    shape: OpenGpuiEdgeMarkerShape,
    reverse: bool,
}

#[derive(Debug, Clone)]
struct OpenGpuiPreparedEdgeLabel {
    text: String,
    point: CanvasPoint,
    appearance: OpenGpuiEdgeLabelAppearance,
}

struct OpenGpuiPreviewPaintPlan {
    path: Option<Path<Pixels>>,
    markers: Vec<Path<Pixels>>,
}

pub(super) struct OpenGpuiSceneWires {
    pub(super) geometry: GeometrySnapshot,
    // Adapter-owned ephemeral previews compose separately and never consume published budgets.
    pub(super) published: Rc<[OpenGpuiSceneWire]>,
    published_plan_id: u64,
    ordinary_count: usize,
    pub(super) hover_overlay: Option<OpenGpuiSceneWire>,
    pub(super) connection_preview: Option<OpenGpuiConnectionPreviewPlan>,
}

#[derive(Clone, Copy, PartialEq)]
struct OpenGpuiPreparedEdgeCacheKey {
    query_bounds: ViewRect,
    query_revision: VisibleRouteQueryRevision,
    route_generation: u64,
    edge_budget: usize,
    active_edge: Option<EdgeId>,
    suppressed_edge: Option<EdgeId>,
}

#[derive(Clone)]
struct OpenGpuiPreparedEdgePlan {
    id: u64,
    published: Rc<[OpenGpuiSceneWire]>,
    ordinary_count: usize,
    indices: Rc<BTreeMap<EdgeId, usize>>,
    budget_omitted: usize,
}

#[derive(Default)]
pub(super) struct OpenGpuiPreparedEdgeCache {
    plan: Option<(OpenGpuiPreparedEdgeCacheKey, OpenGpuiPreparedEdgePlan)>,
    next_plan_id: u64,
}

impl OpenGpuiPreparedEdgeCache {
    fn get(&self, key: OpenGpuiPreparedEdgeCacheKey) -> Option<OpenGpuiPreparedEdgePlan> {
        self.plan
            .as_ref()
            .filter(|(cached, _)| *cached == key)
            .map(|(_, plan)| plan.clone())
    }

    fn insert(&mut self, key: OpenGpuiPreparedEdgeCacheKey, plan: OpenGpuiPreparedEdgePlan) {
        self.plan = Some((key, plan));
    }

    fn reserve_plan_id(&mut self) -> u64 {
        self.next_plan_id = self.next_plan_id.wrapping_add(1).max(1);
        self.next_plan_id
    }

    pub(super) fn clear(&mut self) {
        self.plan = None;
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct OpenGpuiConnectionPreviewEvidence {
    pub(super) source: CanvasPoint,
    pub(super) target: CanvasPoint,
    pub(super) suppressed_edge: Option<EdgeId>,
}

#[cfg(test)]
pub(super) struct OpenGpuiSceneEvidence {
    pub(super) last_layers: Cell<Option<[OpenGpuiSceneLayer; 7]>>,
    pub(super) last_planned_wire_count: Cell<usize>,
    pub(super) paint_passes: Cell<u64>,
    pub(super) first_paint_wire_count: Cell<Option<usize>>,
    pub(super) first_non_empty_paint_pass: Cell<Option<u64>>,
    pub(super) last_painted_wire_count: Cell<usize>,
    pub(super) last_connection_preview: Cell<Option<OpenGpuiConnectionPreviewEvidence>>,
    pub(super) last_painted_connection_preview_count: Cell<usize>,
    pub(super) painted_wire_counts: RefCell<Vec<usize>>,
    pub(super) last_scene_phase: Cell<Option<OpenGpuiSceneGesturePhase>>,
    pub(super) host_closed_scene_cancellations: Cell<u64>,
}

#[cfg(test)]
impl OpenGpuiSceneEvidence {
    pub(super) fn new() -> Self {
        Self {
            last_layers: Cell::new(None),
            last_planned_wire_count: Cell::new(0),
            paint_passes: Cell::new(0),
            first_paint_wire_count: Cell::new(None),
            first_non_empty_paint_pass: Cell::new(None),
            last_painted_wire_count: Cell::new(0),
            last_connection_preview: Cell::new(None),
            last_painted_connection_preview_count: Cell::new(0),
            painted_wire_counts: RefCell::new(Vec::new()),
            last_scene_phase: Cell::new(None),
            host_closed_scene_cancellations: Cell::new(0),
        }
    }

    fn record_plan(&self, layers: [OpenGpuiSceneLayer; 7], wire_count: usize) {
        self.last_layers.set(Some(layers));
        self.last_planned_wire_count.set(wire_count);
    }

    fn record_paint(&self, wire_count: usize) {
        let pass = self.paint_passes.get().saturating_add(1);
        self.paint_passes.set(pass);
        if self.first_paint_wire_count.get().is_none() {
            self.first_paint_wire_count.set(Some(wire_count));
        }
        if wire_count > 0 && self.first_non_empty_paint_pass.get().is_none() {
            self.first_non_empty_paint_pass.set(Some(pass));
        }
        self.last_painted_wire_count.set(wire_count);
        self.painted_wire_counts.borrow_mut().push(wire_count);
    }

    pub(super) fn record_scene_event(&self, event: &OpenGpuiSceneGestureEvent) {
        self.last_scene_phase.set(Some(event.phase()));
        if matches!(
            event.phase(),
            OpenGpuiSceneGesturePhase::Cancel(OpenGpuiSceneGestureCancelReason::HostClosed)
        ) {
            self.host_closed_scene_cancellations
                .set(self.host_closed_scene_cancellations.get().saturating_add(1));
        }
    }
}

impl OpenGpuiSceneHost {
    #[cfg(test)]
    pub(super) fn fail_next_scene_wires_for_test(&mut self, error: ViewSessionError) {
        self.scene_wires_fault = Some(error);
    }

    #[cfg(test)]
    pub(super) fn scene_wires(&mut self) -> Result<OpenGpuiSceneWires, ViewSessionError> {
        let presentation_offsets = self.interaction_node_drag_offsets();
        self.scene_wires_with_offsets(&presentation_offsets)
    }

    pub(super) fn scene_wires_with_offsets(
        &mut self,
        presentation_offsets: &BTreeMap<NodeId, CanvasPoint>,
    ) -> Result<OpenGpuiSceneWires, ViewSessionError> {
        #[cfg(test)]
        if let Some(error) = self.scene_wires_fault.take() {
            return Err(error);
        }
        let geometry = self.view.geometry_snapshot()?;
        let graph = (!presentation_offsets.is_empty())
            .then(|| self.view.graph_snapshot())
            .transpose()?;
        let connection_preview = self.connection_preview_plan(&geometry);
        let suppressed_edge = connection_preview
            .as_ref()
            .and_then(|preview| preview.suppressed_edge);
        #[cfg(test)]
        self.scene_evidence
            .last_connection_preview
            .set(connection_preview.as_ref().and_then(|preview| {
                edge_path_endpoints(&preview.path).map(|(source, target)| {
                    OpenGpuiConnectionPreviewEvidence {
                        source,
                        target,
                        suppressed_edge: preview.suppressed_edge,
                    }
                })
            }));
        let transform = self.view.viewport_transform()?;
        let query_bounds = edge_query_bounds(self.surface_size, self.viewport, transform)
            .ok_or(ViewSessionError::InvalidViewport)?;
        let route_plan =
            self.visible_routes
                .query(&self.view, query_bounds, self.visible_route_query_budget)?;
        let identities = if presentation_offsets.is_empty() {
            route_plan.routes.clone()
        } else {
            let mut identities = route_plan.routes.to_vec();
            append_dragged_incident_routes(
                &self.view,
                graph.as_ref().map(|snapshot| snapshot.graph()),
                presentation_offsets,
                &mut identities,
                self.edge_layer_budget,
            )?;
            identities.into()
        };
        let hovered = self.canvas_interaction.presentation().hovered_edge();
        let active = self.canvas_interaction.presentation().active_edge();
        let cache_key = presentation_offsets
            .is_empty()
            .then_some(OpenGpuiPreparedEdgeCacheKey {
                query_bounds,
                query_revision: route_plan.revision,
                route_generation: route_plan.generation,
                edge_budget: self.edge_layer_budget,
                active_edge: active,
                suppressed_edge,
            });
        let cached = cache_key.and_then(|key| self.prepared_edges.get(key));
        let (prepared, painter_fallback, rebuilt) = if let Some(prepared) = cached {
            (prepared, None, false)
        } else {
            let cull_margin = 4.0 / transform.zoom;
            let mut admitted = BTreeSet::new();
            let mut ordinary = Vec::new();
            let mut selected = Vec::new();
            let mut active_wires = Vec::new();
            let mut painter_fallbacks = Vec::new();
            let mut painter_fallback_count = 0usize;

            for identity in identities.iter().copied() {
                let edge = identity.edge;
                if !admitted.insert(edge) || Some(edge) == suppressed_edge {
                    continue;
                }
                let Some(route) = geometry.route(edge) else {
                    continue;
                };
                let drag_path = graph.as_ref().and_then(|graph| {
                    dragged_edge_path(graph.graph(), edge, route.facts(), |node| {
                        presentation_offsets.get(&node).copied()
                    })
                });
                let visible_path = drag_path.as_ref().unwrap_or(&route.facts().path);
                if !edge_path_intersects_viewport(visible_path, self.viewport, cull_margin) {
                    continue;
                }
                let visual_state = if active == Some(edge) {
                    OpenGpuiEdgeVisualState::Active
                } else if route.facts().interaction.selected {
                    OpenGpuiEdgeVisualState::Selected
                } else {
                    OpenGpuiEdgeVisualState::Ordinary
                };
                let paint_recipe = self.edge_painters.resolve(
                    route.facts().edge_kind,
                    visual_state,
                    &route.facts().presentation,
                    &self.graph_theme,
                );
                if paint_recipe.fallbacks().has_any() {
                    painter_fallback_count = painter_fallback_count.saturating_add(1);
                    if painter_fallbacks.len() < 16 {
                        painter_fallbacks.push((
                            route.facts().presentation.renderer_key.clone(),
                            edge_painter_fallback_message(edge, paint_recipe.fallbacks()),
                        ));
                    }
                }
                let source_marker = prepare_edge_marker(
                    visible_path,
                    EdgePathAnchor::Source,
                    paint_recipe.source_marker().shape(),
                    true,
                );
                let target_marker = prepare_edge_marker(
                    visible_path,
                    EdgePathAnchor::Target,
                    paint_recipe.target_marker().shape(),
                    false,
                );
                let label = paint_recipe.label().and_then(|label| {
                    let anchor = match label.anchor() {
                        EdgeLabelAnchor::Source => EdgePathAnchor::Progress(0.2),
                        EdgeLabelAnchor::Center => EdgePathAnchor::Center,
                        EdgeLabelAnchor::Target => EdgePathAnchor::Progress(0.8),
                    };
                    visible_path
                        .sample(anchor)
                        .ok()
                        .map(|sample| OpenGpuiPreparedEdgeLabel {
                            text: label.text().to_owned(),
                            point: sample.point,
                            appearance: label.appearance(),
                        })
                });
                let wire = OpenGpuiSceneWire {
                    edge,
                    geometry_revision: identity.geometry_revision,
                    visual_state,
                    paint_key: paint_recipe.path_batch_key(),
                    path_stroke_width: paint_recipe.path_batch_key().path().stroke_width(),
                    source_marker,
                    target_marker,
                    label,
                    drag_path,
                };
                match visual_state {
                    OpenGpuiEdgeVisualState::Ordinary => ordinary.push(wire),
                    OpenGpuiEdgeVisualState::Selected => selected.push(wire),
                    OpenGpuiEdgeVisualState::Active => active_wires.push(wire),
                    OpenGpuiEdgeVisualState::Hovered | OpenGpuiEdgeVisualState::Preview => {
                        unreachable!("prepared committed routes exclude ephemeral states")
                    }
                }
            }

            let visible = ordinary
                .len()
                .saturating_add(selected.len())
                .saturating_add(active_wires.len());
            let emphasized = selected.len().saturating_add(active_wires.len());
            if emphasized >= self.edge_layer_budget {
                ordinary.clear();
                let mut remaining = self.edge_layer_budget;
                retain_back(&mut active_wires, &mut remaining);
                retain_back(&mut selected, &mut remaining);
            } else {
                retain_back_to(&mut ordinary, self.edge_layer_budget - emphasized);
            }
            let mut published = Vec::with_capacity(self.edge_layer_budget.min(visible));
            let ordinary_count = ordinary.len();
            published.extend(ordinary);
            published.extend(selected);
            published.extend(active_wires);
            let indices = published
                .iter()
                .enumerate()
                .map(|(index, wire)| (wire.edge, index))
                .collect();
            let prepared = OpenGpuiPreparedEdgePlan {
                id: self.prepared_edges.reserve_plan_id(),
                budget_omitted: visible.saturating_sub(published.len()),
                published: published.into(),
                ordinary_count,
                indices: Rc::new(indices),
            };
            if let Some(key) = cache_key {
                self.prepared_edges.insert(key, prepared.clone());
            }
            (
                prepared,
                Some((painter_fallbacks, painter_fallback_count)),
                true,
            )
        };
        let published = prepared.published;
        let published_plan_id = prepared.id;
        let ordinary_count = prepared.ordinary_count;
        let budget_omitted = prepared.budget_omitted;
        let hover_overlay = hovered
            .filter(|edge| Some(*edge) != active && Some(*edge) != suppressed_edge)
            .and_then(|edge| {
                let index = *prepared.indices.get(&edge)?;
                let wire = published.get(index)?;
                (wire.visual_state == OpenGpuiEdgeVisualState::Ordinary).then_some((edge, wire))
            })
            .and_then(|(edge, wire)| {
                let route = geometry.route(edge)?;
                let recipe = self.edge_painters.resolve(
                    route.facts().edge_kind,
                    OpenGpuiEdgeVisualState::Hovered,
                    &route.facts().presentation,
                    &self.graph_theme,
                );
                let mut overlay = wire.clone();
                overlay.visual_state = OpenGpuiEdgeVisualState::Hovered;
                overlay.paint_key = recipe.path_batch_key();
                overlay.path_stroke_width = recipe.path_batch_key().path().stroke_width();
                Some(overlay)
            });
        let report = route_plan.report;
        let overflow = OpenGpuiEdgeLayerOverflow {
            query_entries_examined: report.work.entries_examined,
            query_candidates_matched: report.work.candidates_matched,
            geometry_overflows: report.geometry_overflows,
            query_pending: report.pending,
            query_budget_exhausted: report.budget_exhausted,
            reused_previous_plan: report.reused_previous_plan,
            budget_omitted,
        };
        self.last_edge_layer_overflow = overflow;
        if rebuilt {
            self.stats.edge_layer_plans = self.stats.edge_layer_plans.saturating_add(1);
        } else {
            self.stats.edge_route_list_reuses = self.stats.edge_route_list_reuses.saturating_add(1);
        }
        self.stats.edge_route_query_entries_examined = self
            .stats
            .edge_route_query_entries_examined
            .saturating_add(report.work.entries_examined);
        self.stats.edge_route_query_candidates_matched = self
            .stats
            .edge_route_query_candidates_matched
            .saturating_add(report.work.candidates_matched);
        self.stats.edge_route_query_pending = self
            .stats
            .edge_route_query_pending
            .saturating_add(u64::from(report.pending));
        self.stats.edge_route_query_budget_exhausted = self
            .stats
            .edge_route_query_budget_exhausted
            .saturating_add(u64::from(report.budget_exhausted));
        self.stats.edge_route_geometry_overflows = self
            .stats
            .edge_route_geometry_overflows
            .saturating_add(report.geometry_overflows as u64);
        self.stats.edge_route_previous_plan_reuses = self
            .stats
            .edge_route_previous_plan_reuses
            .saturating_add(u64::from(report.reused_previous_plan));
        if rebuilt {
            self.stats.edge_routes_planned = self
                .stats
                .edge_routes_planned
                .saturating_add(published.len() as u64);
            self.stats.edge_routes_budget_omitted = self
                .stats
                .edge_routes_budget_omitted
                .saturating_add(budget_omitted as u64);
        }
        if !overflow.is_empty() {
            self.stats.edge_layer_overflows = self.stats.edge_layer_overflows.saturating_add(1);
        }
        if let Some((fallbacks, total)) = painter_fallback {
            self.record_edge_painter_diagnostics(fallbacks, total);
        }
        Ok(OpenGpuiSceneWires {
            geometry,
            published,
            published_plan_id,
            ordinary_count,
            hover_overlay,
            connection_preview,
        })
    }

    fn render_edge_layer(
        &self,
        wires: &OpenGpuiSceneWires,
        transform: ViewportTransform,
    ) -> AnyElement {
        let ordinary = self.render_edge_path_canvas(
            wires.geometry.clone(),
            wires.published.clone(),
            0..wires.ordinary_count,
            transform,
            wires.published_plan_id,
            OpenGpuiEdgePaintPlanSegment::Ordinary,
            Some(wires.published.len()),
        );
        let overlay = wires.hover_overlay.clone().map(|wire| {
            let edge = wire.edge;
            self.render_edge_path_canvas(
                wires.geometry.clone(),
                Rc::<[OpenGpuiSceneWire]>::from(vec![wire]),
                0..1,
                transform,
                wires.published_plan_id,
                OpenGpuiEdgePaintPlanSegment::Hover(edge),
                None,
            )
        });
        let emphasized = (wires.ordinary_count < wires.published.len()).then(|| {
            self.render_edge_path_canvas(
                wires.geometry.clone(),
                wires.published.clone(),
                wires.ordinary_count..wires.published.len(),
                transform,
                wires.published_plan_id,
                OpenGpuiEdgePaintPlanSegment::Emphasized,
                None,
            )
        });
        div()
            .absolute()
            .size_full()
            .child(ordinary)
            .children(overlay)
            .children(emphasized)
            .into_any_element()
    }

    fn render_grid_layer(&self, transform: ViewportTransform) -> AnyElement {
        let Some(grid) = self.navigation.grid_size_in_canvas() else {
            return div().absolute().size_full().into_any_element();
        };
        let line_width = self.graph_theme.metrics.grid_line_width;
        let minor_color = self.graph_theme.palette.grid_minor;
        let major_color = self.graph_theme.palette.grid_major;
        div()
            .debug_selector(|| OPEN_GPUI_GRID_SELECTOR.to_owned())
            .absolute()
            .size_full()
            .child(
                canvas(
                    move |bounds, _, _| {
                        build_grid_paths(
                            CanvasSize {
                                width: bounds.size.width.as_f32(),
                                height: bounds.size.height.as_f32(),
                            },
                            grid,
                            transform,
                            line_width,
                        )
                    },
                    move |_, (minor, major), window, _| {
                        if let Some(path) = minor {
                            window.paint_path(path.clone(), minor_color);
                        }
                        if let Some(path) = major {
                            window.paint_path(path.clone(), major_color);
                        }
                    },
                )
                .absolute()
                .size_full(),
            )
            .into_any_element()
    }

    fn render_edge_path_canvas(
        &self,
        geometry: GeometrySnapshot,
        published: Rc<[OpenGpuiSceneWire]>,
        range: std::ops::Range<usize>,
        transform: ViewportTransform,
        plan_id: u64,
        segment: OpenGpuiEdgePaintPlanSegment,
        _evidence_wire_count: Option<usize>,
    ) -> AnyElement {
        #[cfg(test)]
        let paint_evidence = self.scene_evidence.clone();
        let path_cache = self.edge_paths.clone();
        let paint_path_cache = path_cache.clone();
        canvas(
            move |bounds, _, _| {
                let plan_key = OpenGpuiEdgePaintPlanKey::new(
                    plan_id,
                    segment,
                    transform.pan.x,
                    transform.pan.y,
                    transform.zoom,
                    bounds.origin.x.as_f32(),
                    bounds.origin.y.as_f32(),
                );
                path_cache
                    .borrow_mut()
                    .resolve_paint_plan(Some(plan_key), |cache| {
                        let mut batches = Vec::<OpenGpuiEdgePaintBatch>::new();
                        for wire in published[range.clone()].iter() {
                            let Some(route) = geometry.route(wire.edge) else {
                                continue;
                            };
                            let path = wire.drag_path.as_ref().unwrap_or(&route.facts().path);
                            let built = if let Some(key) =
                                edge_paint_item_key(wire, transform, bounds.origin)
                            {
                                cache.resolve_paint_item(key, || {
                                    build_scene_wire_path(
                                        path,
                                        wire.path_stroke_width,
                                        key.batch().stroke_pattern(),
                                        transform,
                                        bounds.origin,
                                    )
                                })
                            } else {
                                build_scene_wire_path(
                                    path,
                                    wire.path_stroke_width,
                                    wire.paint_key.stroke_pattern(),
                                    transform,
                                    bounds.origin,
                                )
                            };
                            let Some(path) = built else {
                                continue;
                            };
                            if let Some(batch) = batches.last_mut()
                                && batch.key == wire.paint_key
                            {
                                merge_native_paths(&mut batch.path, path);
                                batch.edge_count = batch.edge_count.saturating_add(1);
                            } else {
                                batches.push(OpenGpuiEdgePaintBatch {
                                    key: wire.paint_key,
                                    path,
                                    edge_count: 1,
                                });
                            }
                        }
                        batches
                    })
            },
            move |_, batches, window, _| {
                let edge_count = batches.iter().map(|batch| batch.edge_count).sum::<usize>();
                #[cfg(test)]
                if let Some(wire_count) = _evidence_wire_count {
                    paint_evidence.record_paint(wire_count);
                }
                paint_path_cache
                    .borrow_mut()
                    .record_native_paint(batches.len(), edge_count);
                for batch in batches.iter() {
                    window.paint_path(batch.path.clone(), batch.key.path().color());
                }
            },
        )
        .absolute()
        .size_full()
        .into_any_element()
    }

    fn render_edge_chrome_layer(
        &self,
        wires: &OpenGpuiSceneWires,
        transform: ViewportTransform,
    ) -> AnyElement {
        let mut chrome = div().absolute().size_full();
        let marker_wires = wires.published.clone();
        let marker_hover = wires.hover_overlay.clone();
        let marker_hover_edge = marker_hover.as_ref().map(|wire| wire.edge);
        let marker_diameter = self.graph_theme.metrics.edge_marker_diameter;
        let marker_plan_id = wires.published_plan_id;
        let marker_cache = self.edge_paths.clone();
        let marker_paint_cache = marker_cache.clone();
        chrome = chrome.child(
            canvas(
                move |bounds, _, _| {
                    let key = OpenGpuiEdgePaintPlanKey::new(
                        marker_plan_id,
                        OpenGpuiEdgePaintPlanSegment::Markers(marker_hover_edge),
                        transform.pan.x,
                        transform.pan.y,
                        transform.zoom,
                        bounds.origin.x.as_f32(),
                        bounds.origin.y.as_f32(),
                    );
                    marker_cache.borrow_mut().resolve_marker_plan(key, || {
                        let mut batches = Vec::<OpenGpuiEdgePaintBatch>::new();
                        for published_wire in marker_wires.iter() {
                            let wire = marker_hover
                                .as_ref()
                                .filter(|hover| hover.edge == published_wire.edge)
                                .unwrap_or(published_wire);
                            let color = wire.paint_key.path().color();
                            for marker in [wire.source_marker, wire.target_marker]
                                .into_iter()
                                .flatten()
                            {
                                let Some(center) = project_scene_point(
                                    marker.sample.point,
                                    transform,
                                    bounds.origin,
                                ) else {
                                    continue;
                                };
                                let Some(path) = build_edge_marker_path(
                                    center,
                                    marker.sample.tangent,
                                    marker.shape,
                                    marker_diameter,
                                    marker.reverse,
                                ) else {
                                    continue;
                                };
                                if let Some(batch) = batches
                                    .iter_mut()
                                    .find(|batch| batch.key.path().color() == color)
                                {
                                    merge_native_paths(&mut batch.path, path);
                                    batch.edge_count = batch.edge_count.saturating_add(1);
                                } else {
                                    batches.push(OpenGpuiEdgePaintBatch {
                                        key: wire.paint_key,
                                        path,
                                        edge_count: 1,
                                    });
                                }
                            }
                        }
                        batches
                    })
                },
                move |_, batches, window, _| {
                    let marker_count = batches.iter().map(|batch| batch.edge_count).sum::<usize>();
                    marker_paint_cache
                        .borrow_mut()
                        .record_native_marker_paint(batches.len(), marker_count);
                    for batch in batches.iter() {
                        window.paint_path(batch.path.clone(), batch.key.path().color());
                    }
                },
            )
            .absolute()
            .size_full(),
        );

        let label_metrics = self.graph_theme.metrics;
        for published_wire in wires.published.iter() {
            let wire = wires
                .hover_overlay
                .as_ref()
                .filter(|hover| hover.edge == published_wire.edge)
                .unwrap_or(published_wire);
            let Some(label) = wire.label.as_ref() else {
                continue;
            };
            let Some(position) = transform.screen_point_for_canvas(label.point) else {
                continue;
            };
            let mut label_chrome = div()
                .max_w(px(label_metrics.edge_label_max_width))
                .truncate()
                .rounded(px(label_metrics.edge_label_corner_radius))
                .pl(px(label_metrics.edge_label_horizontal_padding))
                .pr(px(label_metrics.edge_label_horizontal_padding))
                .pt(px(label_metrics.edge_label_vertical_padding))
                .pb(px(label_metrics.edge_label_vertical_padding))
                .text_size(px(label_metrics.edge_label_font_size))
                .text_color(self.graph_theme.palette.label_text)
                .bg(self.graph_theme.palette.label_background)
                .child(label.text.clone());
            if label.appearance == OpenGpuiEdgeLabelAppearance::Badge {
                label_chrome = label_chrome
                    .border_1()
                    .border_color(wire.paint_key.path().color());
            }
            let label_height = label_metrics.edge_label_font_size
                + 2.0 * label_metrics.edge_label_vertical_padding;
            chrome = chrome.child(
                div()
                    .absolute()
                    .left(px(position.x - label_metrics.edge_label_max_width / 2.0))
                    .top(px(position.y - label_height / 2.0))
                    .w(px(label_metrics.edge_label_max_width))
                    .flex()
                    .justify_center()
                    .child(label_chrome),
            );
        }

        chrome.into_any_element()
    }

    fn render_interaction_chrome_layer(
        &mut self,
        wires: &OpenGpuiSceneWires,
        transform: ViewportTransform,
        focus_entries: &mut Vec<OpenGpuiGraphFocusEntry>,
        cx: &mut Context<Self>,
    ) -> (AnyElement, AnyElement) {
        let host_id = cx.entity_id();
        let view_id = self.view.id();
        let host = cx.weak_entity();
        let scene_interactions = self.scene_interactions.clone();
        let scene_listener = Self::host_scene_listener(cx);
        let mut edge_controls = div().absolute().size_full();
        let mut reconnect_controls = div().absolute().size_full();
        for wire in wires.published.iter() {
            let Some(route) = wires.geometry.route(wire.edge) else {
                continue;
            };
            let interaction = route.facts().interaction;
            if !interaction.selectable
                && !(interaction.selected && (interaction.deletable || interaction.can_reconnect()))
            {
                continue;
            }
            let path = wire.drag_path.as_ref().unwrap_or(&route.facts().path);
            let Ok(sample) = path.sample(EdgePathAnchor::Center) else {
                continue;
            };
            let Some(position) = transform.screen_point_for_canvas(sample.point) else {
                continue;
            };
            let edge = wire.edge;
            let edge_name = route.facts().presentation.label.as_deref().map_or_else(
                || format!("Graph edge {edge:?}"),
                |label| format!("Edge {label}"),
            );
            let keyboard_host = host.clone();
            let accessibility_host = host.clone();
            let pointer_host = host.clone();
            let focus = self.graph_theme.palette.focus;
            let selected = interaction.selected;
            let deletable = interaction.deletable;
            let edge_focus = self
                .graph_focus
                .action_handle(OpenGpuiGraphFocusTarget::Edge(edge), cx);
            focus_entries.push(OpenGpuiGraphFocusEntry::new(
                OpenGpuiGraphFocusTarget::Edge(edge),
                edge_focus.clone(),
            ));
            let keyboard_focus = edge_focus.clone();
            let pointer_focus = edge_focus.clone();
            let control = div()
                .id(open_gpui_edge_element_id(host_id, view_id, edge))
                .role(OPEN_GPUI_EDGE_ROLE)
                .aria_label(if selected {
                    format!("{edge_name}, selected")
                } else {
                    edge_name.clone()
                })
                .aria_selected(selected)
                .track_focus(&edge_focus)
                .tab_stop(true)
                .absolute()
                .left(px(position.x - 10.0))
                .top(px(position.y - 10.0))
                .w(px(20.0))
                .h(px(20.0))
                .rounded_full()
                .border_1()
                .border_color(focus.opacity(0.0))
                .focus_visible(move |style| style.border_2().border_color(focus))
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    if window.default_prevented() {
                        return;
                    }
                    let Some(host) = pointer_host.upgrade() else {
                        return;
                    };
                    window.focus(&pointer_focus, cx);
                    if host.update(cx, |host, cx| {
                        host.activate_edge_action(
                            edge,
                            jellyflow::runtime::SelectionMode::Replace,
                            window,
                            cx,
                        )
                    }) {
                        window.prevent_default();
                        cx.stop_propagation();
                    }
                })
                .on_key_up(move |event, window, cx| {
                    if event.keystroke.modifiers.modified() || !keyboard_focus.is_focused(window) {
                        return;
                    }
                    let Some(host) = keyboard_host.upgrade() else {
                        return;
                    };
                    let key = event.keystroke.key.as_str();
                    let handled = if interaction.selectable
                        && (key.eq_ignore_ascii_case("enter") || key.eq_ignore_ascii_case("space"))
                    {
                        host.update(cx, |host, cx| {
                            host.activate_edge_action(
                                edge,
                                jellyflow::runtime::SelectionMode::Replace,
                                window,
                                cx,
                            )
                        })
                    } else if selected
                        && deletable
                        && (key.eq_ignore_ascii_case("delete")
                            || key.eq_ignore_ascii_case("backspace"))
                    {
                        host.update(cx, |host, cx| host.delete_edge_action(edge, window, cx))
                    } else {
                        false
                    };
                    if handled {
                        window.prevent_default();
                        cx.stop_propagation();
                    }
                })
                .when(interaction.selectable, |control| {
                    control.on_a11y_action(
                        open_gpui::accesskit::Action::Click,
                        move |_, window, cx| {
                            let Some(host) = accessibility_host.upgrade() else {
                                return;
                            };
                            if host.update(cx, |host, cx| {
                                host.activate_edge_action(
                                    edge,
                                    jellyflow::runtime::SelectionMode::Replace,
                                    window,
                                    cx,
                                )
                            }) {
                                window.prevent_default();
                                cx.stop_propagation();
                            }
                        },
                    )
                });
            edge_controls = edge_controls.child(control);

            if !selected {
                continue;
            }
            for (endpoint, anchor, allowed, endpoint_name) in [
                (
                    EdgeEndpoint::From,
                    EdgePathAnchor::Source,
                    interaction.reconnect_source,
                    "source",
                ),
                (
                    EdgeEndpoint::To,
                    EdgePathAnchor::Target,
                    interaction.reconnect_target,
                    "target",
                ),
            ] {
                if !allowed {
                    continue;
                }
                let Ok(sample) = path.sample(anchor) else {
                    continue;
                };
                let Some(position) = transform.screen_point_for_canvas(sample.point) else {
                    continue;
                };
                let coordinator = scene_interactions.clone();
                let listener = scene_listener.clone();
                let keyboard_host = host.clone();
                let accessibility_host = host.clone();
                let focus = self.graph_theme.palette.focus;
                let marker = self.graph_theme.metrics.reconnect_handle_diameter;
                let hit = self.graph_theme.metrics.reconnect_hit_diameter.max(marker);
                let target = OpenGpuiGraphFocusTarget::reconnect(edge, endpoint);
                let reconnect_focus = self.graph_focus.action_handle(target, cx);
                focus_entries.push(OpenGpuiGraphFocusEntry::new(
                    target,
                    reconnect_focus.clone(),
                ));
                let reconnect = div()
                    .id(open_gpui_reconnect_element_id(
                        host_id,
                        view_id,
                        edge,
                        endpoint_name,
                    ))
                    .role(OPEN_GPUI_RECONNECT_HANDLE_ROLE)
                    .aria_label(format!("Reconnect {endpoint_name} of {edge_name}"))
                    .track_focus(&reconnect_focus)
                    .tab_stop(true)
                    .absolute()
                    .left(px(position.x - hit * 0.5))
                    .top(px(position.y - hit * 0.5))
                    .w(px(hit))
                    .h(px(hit))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .border_1()
                    .border_color(focus.opacity(0.0))
                    .focus_visible(move |style| style.border_2().border_color(focus))
                    .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                        if coordinator.begin(
                            OpenGpuiSceneGestureKind::EdgeReconnect(edge, endpoint),
                            MouseButton::Left,
                            listener.clone(),
                            None,
                            event,
                            window,
                            cx,
                        ) {
                            window.prevent_default();
                            cx.stop_propagation();
                        }
                    })
                    .on_key_up(move |event, window, cx| {
                        if event.keystroke.modifiers.modified()
                            || (!event.keystroke.key.eq_ignore_ascii_case("enter")
                                && !event.keystroke.key.eq_ignore_ascii_case("space"))
                        {
                            return;
                        }
                        let Some(host) = keyboard_host.upgrade() else {
                            return;
                        };
                        if host.update(cx, |host, cx| {
                            host.activate_edge_reconnect_action(edge, endpoint, window, cx)
                        }) {
                            window.prevent_default();
                            cx.stop_propagation();
                        }
                    })
                    .on_a11y_action(open_gpui::accesskit::Action::Click, move |_, window, cx| {
                        let Some(host) = accessibility_host.upgrade() else {
                            return;
                        };
                        if host.update(cx, |host, cx| {
                            host.activate_edge_reconnect_action(edge, endpoint, window, cx)
                        }) {
                            window.prevent_default();
                            cx.stop_propagation();
                        }
                    })
                    .child(
                        div()
                            .w(px(marker))
                            .h(px(marker))
                            .rounded_full()
                            .border_2()
                            .border_color(self.graph_theme.palette.selection)
                            .bg(self.graph_theme.palette.canvas),
                    );
                reconnect_controls = reconnect_controls.child(reconnect);
            }
        }
        (
            edge_controls.into_any_element(),
            reconnect_controls.into_any_element(),
        )
    }

    fn render_node_layer(
        &self,
        feedback: Rc<DeferredFrameFeedback>,
        transform: ViewportTransform,
        window: &Window,
        focus_entries: &mut Vec<OpenGpuiGraphFocusEntry>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let host_id = cx.entity_id();
        let mut nodes = div().absolute().size_full();
        for pass in [
            OpenGpuiNodePresentation::Overview,
            OpenGpuiNodePresentation::Native,
        ] {
            for (key, mounted) in self.mounts.active_in_order() {
                let presentation = OpenGpuiNodePresentation::for_zoom(
                    transform.zoom,
                    mounted.activity.protected_count(),
                );
                if presentation != pass {
                    continue;
                }

                let mut bounds = mounted.bounds;
                if let Some(delta) = feedback.measurements.presentation_offset(key.node()) {
                    bounds.origin.x += delta.x;
                    bounds.origin.y += delta.y;
                }
                let origin = transform
                    .screen_point_for_canvas(bounds.origin)
                    .unwrap_or(bounds.origin);
                let screen_size = mounted
                    .semantic_zoom
                    .screen_size(transform.zoom, presentation);
                let overview_binding = mounted.binding.as_ref().map(|binding| binding.read(cx));
                let overview_drag_handle =
                    overview_binding.map(|binding| binding.drag_handle().for_host_presentation());
                let node_enabled = overview_binding
                    .map(|binding| !binding.presentation().node().disabled())
                    .unwrap_or(true);
                let node_focus = mounted.focus.clone().tab_stop(node_enabled);
                if node_enabled {
                    focus_entries.push(OpenGpuiGraphFocusEntry::new(
                        OpenGpuiGraphFocusTarget::Node {
                            node: key.node(),
                            presentation,
                        },
                        node_focus.clone(),
                    ));
                }
                if let Some(binding) = overview_binding {
                    for (port, state) in binding.presentation().ports() {
                        let Some(port_focus) = binding.port_focus_handle(port) else {
                            continue;
                        };
                        let port_focus = port_focus.clone().tab_stop(!state.disabled());
                        if !state.disabled() {
                            focus_entries.push(OpenGpuiGraphFocusEntry::new(
                                OpenGpuiGraphFocusTarget::Port {
                                    node: key.node(),
                                    port,
                                },
                                port_focus,
                            ));
                        }
                    }
                }
                let content = match presentation {
                    OpenGpuiNodePresentation::Native => mounted.view.clone().into_any_element(),
                    OpenGpuiNodePresentation::Overview => {
                        let overview = mounted.semantic_zoom.overview_element(
                            overview_binding,
                            &mounted.accessible_name,
                            mounted.selected,
                            screen_size,
                            self.graph_theme,
                        );
                        match overview_drag_handle {
                            Some(handle) => handle.wrap(overview).into_any_element(),
                            None => overview,
                        }
                    }
                };
                let keyboard_host = cx.weak_entity();
                let accessibility_host = keyboard_host.clone();
                let pointer_host = keyboard_host.clone();
                let node_for_keyboard = key.node();
                let node_for_accessibility = key.node();
                let node_for_pointer = key.node();
                let keyboard_focus = node_focus.clone();
                let pointer_focus = node_focus.clone();
                let focused = node_focus.contains_focused(window, cx);
                let dragging = self
                    .canvas_interaction
                    .presentation()
                    .node_drag()
                    .is_some_and(|drag| drag.nodes().contains(&key.node()));
                let chrome_color = if dragging || focused {
                    self.graph_theme.palette.focus
                } else if mounted.selected {
                    self.graph_theme.palette.selection
                } else {
                    self.graph_theme.palette.node_border.opacity(0.0)
                };
                let child = node_semantic_element(
                    div().id(open_gpui_node_element_id(host_id, key)),
                    mounted.accessible_name.clone(),
                    mounted.selected,
                )
                .aria_disabled(!node_enabled)
                .track_focus(&node_focus)
                .tab_stop(node_enabled)
                .absolute()
                .left(px(origin.x))
                .top(px(origin.y))
                .w(px(screen_size.width))
                .h(px(screen_size.height))
                .on_key_up(move |event, window, cx| {
                    if event.keystroke.modifiers.modified()
                        || !keyboard_focus.is_focused(window)
                        || (!event.keystroke.key.eq_ignore_ascii_case("enter")
                            && !event.keystroke.key.eq_ignore_ascii_case("space"))
                    {
                        return;
                    }
                    let Some(host) = keyboard_host.upgrade() else {
                        return;
                    };
                    if host.update(cx, |host, cx| {
                        host.activate_node_action(
                            node_for_keyboard,
                            jellyflow::runtime::SelectionMode::Replace,
                            window,
                            cx,
                        )
                    }) {
                        window.prevent_default();
                        cx.stop_propagation();
                    }
                })
                .on_a11y_action(open_gpui::accesskit::Action::Click, move |_, window, cx| {
                    let Some(host) = accessibility_host.upgrade() else {
                        return;
                    };
                    if host.update(cx, |host, cx| {
                        host.activate_node_action(
                            node_for_accessibility,
                            jellyflow::runtime::SelectionMode::Replace,
                            window,
                            cx,
                        )
                    }) {
                        window.prevent_default();
                        cx.stop_propagation();
                    }
                })
                .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                    if node_enabled && !window.default_prevented() {
                        window.focus(&pointer_focus, cx);
                        let selection_mode = selection_mode_for_modifiers(event.modifiers);
                        if let Some(host) = pointer_host.upgrade() {
                            host.update(cx, |host, cx| {
                                host.activate_node_action(
                                    node_for_pointer,
                                    selection_mode,
                                    window,
                                    cx,
                                );
                            });
                        }
                    }
                    cx.stop_propagation();
                })
                .on_scroll_wheel(|_, window, cx| {
                    if window.default_prevented() {
                        cx.stop_propagation();
                    }
                })
                .on_pinch(|_, window, cx| {
                    if window.default_prevented() {
                        cx.stop_propagation();
                    }
                })
                .child(content)
                .when(dragging || focused || mounted.selected, |node| {
                    node.child(
                        div()
                            .absolute()
                            .size_full()
                            .border_1()
                            .border_color(chrome_color),
                    )
                });
                let instance = key.clone();
                let measurements = feedback.measurements.clone();
                nodes = nodes.child(measured_element(
                    open_gpui_node_measurement_element_id(host_id, key),
                    child,
                    move |_, bounds, _, _, _| {
                        let _ = measurements.record_root(&instance, bounds);
                    },
                ));
            }
        }
        nodes.into_any_element()
    }

    fn render_preview_layer(
        &self,
        connection_preview: Option<OpenGpuiConnectionPreviewPlan>,
        geometry: GeometrySnapshot,
        transform: ViewportTransform,
    ) -> AnyElement {
        #[cfg(test)]
        self.scene_evidence
            .last_painted_connection_preview_count
            .set(0);
        let mut preview_layer = div().absolute().size_full();
        if let Some(connection_preview) = connection_preview {
            let resolved = connection_preview
                .suppressed_edge
                .and_then(|edge| geometry.route(edge))
                .map(|route| {
                    self.edge_painters.resolve(
                        route.facts().edge_kind,
                        OpenGpuiEdgeVisualState::Preview,
                        &route.facts().presentation,
                        &self.graph_theme,
                    )
                });
            let preview_style = resolved.map_or_else(
                || {
                    self.graph_theme
                        .resolve_edge_style(OpenGpuiEdgeVisualState::Preview, None)
                },
                |recipe| recipe.style(),
            );
            let stroke_pattern = resolved.map_or(OpenGpuiEdgeStrokePattern::Solid, |recipe| {
                recipe.path_batch_key().stroke_pattern()
            });
            let source_marker = resolved
                .map(|recipe| recipe.source_marker().shape())
                .unwrap_or(OpenGpuiEdgeMarkerShape::None);
            let target_marker = resolved
                .map(|recipe| recipe.target_marker().shape())
                .unwrap_or(OpenGpuiEdgeMarkerShape::None);
            let label = resolved
                .and_then(|recipe| recipe.label())
                .and_then(|label| {
                    let anchor = match label.anchor() {
                        EdgeLabelAnchor::Source => EdgePathAnchor::Progress(0.2),
                        EdgeLabelAnchor::Center => EdgePathAnchor::Center,
                        EdgeLabelAnchor::Target => EdgePathAnchor::Progress(0.8),
                    };
                    connection_preview.path.sample(anchor).ok().map(|sample| {
                        OpenGpuiPreparedEdgeLabel {
                            text: label.text().to_owned(),
                            point: sample.point,
                            appearance: label.appearance(),
                        }
                    })
                });
            let preview_path = connection_preview.path;
            let marker_diameter = self.graph_theme.metrics.edge_marker_diameter;
            #[cfg(test)]
            let paint_evidence = self.scene_evidence.clone();
            let preview = canvas(
                move |bounds, _, _| {
                    let path = build_scene_wire_path(
                        &preview_path,
                        preview_style.stroke_width(),
                        stroke_pattern,
                        transform,
                        bounds.origin,
                    );
                    let mut markers = Vec::with_capacity(2);
                    for (anchor, shape, reverse) in [
                        (EdgePathAnchor::Source, source_marker, true),
                        (EdgePathAnchor::Target, target_marker, false),
                    ] {
                        let Some(marker) =
                            prepare_edge_marker(&preview_path, anchor, shape, reverse)
                        else {
                            continue;
                        };
                        let Some(center) =
                            project_scene_point(marker.sample.point, transform, bounds.origin)
                        else {
                            continue;
                        };
                        if let Some(path) = build_edge_marker_path(
                            center,
                            marker.sample.tangent,
                            marker.shape,
                            marker_diameter,
                            marker.reverse,
                        ) {
                            markers.push(path);
                        }
                    }
                    OpenGpuiPreviewPaintPlan { path, markers }
                },
                move |_, plan, window, _| {
                    #[cfg(test)]
                    paint_evidence
                        .last_painted_connection_preview_count
                        .set(usize::from(plan.path.is_some()));
                    if let Some(path) = plan.path {
                        window.paint_path(path, preview_style.color());
                    }
                    for marker in plan.markers {
                        window.paint_path(marker, preview_style.color());
                    }
                },
            )
            .absolute()
            .size_full();
            preview_layer = preview_layer.child(preview);
            if let Some(label) = label
                && let Some(position) = transform.screen_point_for_canvas(label.point)
            {
                let metrics = self.graph_theme.metrics;
                let mut label_chrome = div()
                    .max_w(px(metrics.edge_label_max_width))
                    .truncate()
                    .rounded(px(metrics.edge_label_corner_radius))
                    .pl(px(metrics.edge_label_horizontal_padding))
                    .pr(px(metrics.edge_label_horizontal_padding))
                    .pt(px(metrics.edge_label_vertical_padding))
                    .pb(px(metrics.edge_label_vertical_padding))
                    .text_size(px(metrics.edge_label_font_size))
                    .text_color(self.graph_theme.palette.label_text)
                    .bg(self.graph_theme.palette.label_background)
                    .child(label.text);
                if label.appearance == OpenGpuiEdgeLabelAppearance::Badge {
                    label_chrome = label_chrome.border_1().border_color(preview_style.color());
                }
                let height =
                    metrics.edge_label_font_size + 2.0 * metrics.edge_label_vertical_padding;
                preview_layer = preview_layer.child(
                    div()
                        .absolute()
                        .left(px(position.x - metrics.edge_label_max_width / 2.0))
                        .top(px(position.y - height / 2.0))
                        .w(px(metrics.edge_label_max_width))
                        .flex()
                        .justify_center()
                        .child(label_chrome),
                );
            }
        }
        preview_layer.into_any_element()
    }

    fn render_overlay_layer(&self, transform: ViewportTransform) -> AnyElement {
        let mut overlay = div().absolute().size_full();
        if let Some(drag) = self.canvas_interaction.presentation().node_drag() {
            let guide_width = self.graph_theme.metrics.guide_width;
            for guide in drag.guides() {
                let line = match guide.axis() {
                    jellyflow::runtime::AlignmentGuideAxis::Vertical => {
                        let canvas_x = guide.canvas_coordinate();
                        let Some(position) = transform.screen_point_for_canvas(CanvasPoint {
                            x: canvas_x,
                            y: 0.0,
                        }) else {
                            continue;
                        };
                        div()
                            .absolute()
                            .left(px(position.x - guide_width * 0.5))
                            .top_0()
                            .w(px(guide_width))
                            .h_full()
                    }
                    jellyflow::runtime::AlignmentGuideAxis::Horizontal => {
                        let canvas_y = guide.canvas_coordinate();
                        let Some(position) = transform.screen_point_for_canvas(CanvasPoint {
                            x: 0.0,
                            y: canvas_y,
                        }) else {
                            continue;
                        };
                        div()
                            .absolute()
                            .left_0()
                            .top(px(position.y - guide_width * 0.5))
                            .w_full()
                            .h(px(guide_width))
                    }
                };
                overlay = overlay.child(
                    line.debug_selector(|| OPEN_GPUI_ALIGNMENT_GUIDE_SELECTOR.to_owned())
                        .bg(self.graph_theme.palette.guide),
                );
            }
        }
        if let Some(bounds) = self.interaction_marquee_view_bounds(transform) {
            overlay = overlay.child(
                div()
                    .debug_selector(|| OPEN_GPUI_SELECTION_RECTANGLE_SELECTOR.to_owned())
                    .absolute()
                    .left(px(bounds.origin.x))
                    .top(px(bounds.origin.y))
                    .w(px(bounds.size.width))
                    .h(px(bounds.size.height))
                    .border_1()
                    .border_color(self.graph_theme.palette.selection)
                    .bg(self.graph_theme.palette.selection.opacity(0.14)),
            );
        }
        overlay.into_any_element()
    }

    pub(super) fn render_scene(
        &mut self,
        feedback: Rc<DeferredFrameFeedback>,
        transform: ViewportTransform,
        focus_handle: FocusHandle,
        mut wires: OpenGpuiSceneWires,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let host_id = cx.entity_id();
        #[cfg(test)]
        let wire_count = wires.published.len();
        let connection_preview = wires.connection_preview.take();
        let edge_layer = div()
            .absolute()
            .size_full()
            .child(self.render_grid_layer(transform))
            .child(self.render_edge_layer(&wires, transform))
            .into_any_element();
        let preview_layer =
            self.render_preview_layer(connection_preview, wires.geometry.clone(), transform);
        let edge_chrome_layer = self.render_edge_chrome_layer(&wires, transform);
        let mut focus_entries = Vec::new();
        let node_layer =
            self.render_node_layer(feedback.clone(), transform, window, &mut focus_entries, cx);
        let (edge_interaction_layer, interaction_chrome_layer) =
            self.render_interaction_chrome_layer(&wires, transform, &mut focus_entries, cx);
        let allow_focus_return =
            self.canvas_interaction.presentation().phase() == CanvasInteractionPhase::Idle;
        self.graph_focus
            .reconcile(focus_entries, &focus_handle, allow_focus_return, window, cx);
        let overlay_layer = self.render_overlay_layer(transform);
        let edge_interaction_layer = div()
            .absolute()
            .size_full()
            .tab_index(1)
            .tab_stop(false)
            .tab_group()
            .child(edge_interaction_layer)
            .into_any_element();
        let node_layer = div()
            .absolute()
            .size_full()
            .tab_index(0)
            .tab_stop(false)
            .tab_group()
            .child(node_layer)
            .into_any_element();
        let interaction_chrome_layer = div()
            .absolute()
            .size_full()
            .tab_index(2)
            .tab_stop(false)
            .tab_group()
            .child(interaction_chrome_layer)
            .into_any_element();
        let layers = [
            (OpenGpuiSceneLayer::Edge, edge_layer),
            (OpenGpuiSceneLayer::Preview, preview_layer),
            (OpenGpuiSceneLayer::EdgeChrome, edge_chrome_layer),
            (OpenGpuiSceneLayer::EdgeInteraction, edge_interaction_layer),
            (OpenGpuiSceneLayer::Node, node_layer),
            (
                OpenGpuiSceneLayer::InteractionChrome,
                interaction_chrome_layer,
            ),
            (OpenGpuiSceneLayer::Overlay, overlay_layer),
        ];
        #[cfg(test)]
        let mut recorded_layers = [OpenGpuiSceneLayer::Overlay; 7];
        let scene_interactions = self.scene_interactions.clone();
        let selection_listener = Self::host_scene_listener(cx);
        let pan_listener = selection_listener.clone();
        let right_pan_listener = selection_listener.clone();
        let right_button_pans = self.navigation.profile().right_button_pans();
        let scene_cursor =
            if self.canvas_interaction.presentation().phase() == CanvasInteractionPhase::Pan {
                CursorStyle::ClosedHand
            } else if right_button_pans {
                CursorStyle::Crosshair
            } else {
                CursorStyle::Arrow
            };
        let host = cx.weak_entity();
        let hover_host = host.clone();
        let hover_state_host = host.clone();
        let scroll_host = host.clone();
        let pinch_host = host;
        let graph_key_listener: OpenGpuiGraphKeyListener = {
            let host = cx.weak_entity();
            Rc::new(move |event, window, cx| {
                let Some(host) = host.upgrade() else {
                    return false;
                };
                host.update(cx, |host, cx| {
                    host.handle_graph_root_key_down(event, window, cx)
                })
            })
        };
        let mut scene = scene_semantic_element(
            div().id(open_gpui_scene_element_id(host_id, self.view.id())),
            "Node graph",
        )
        .relative()
        .size_full()
        .cursor(scene_cursor)
        .bg(self.graph_theme.palette.canvas)
        .on_mouse_move(move |event, window, cx| {
            let Some(host) = hover_host.upgrade() else {
                return;
            };
            host.update(cx, |host, cx| {
                host.handle_scene_pointer_move(event, window, cx);
            });
        })
        .on_hover(move |hovered, window, cx| {
            if *hovered {
                return;
            }
            let Some(host) = hover_state_host.upgrade() else {
                return;
            };
            host.update(cx, |host, cx| {
                host.handle_scene_pointer_exit(window, cx);
            });
        })
        .on_mouse_down(MouseButton::Left, {
            let coordinator = scene_interactions.clone();
            move |event, window, cx| {
                if window.default_prevented() {
                    cx.stop_propagation();
                    return;
                }
                if coordinator.begin(
                    OpenGpuiSceneGestureKind::Selection,
                    MouseButton::Left,
                    selection_listener.clone(),
                    None,
                    event,
                    window,
                    cx,
                ) {
                    cx.stop_propagation();
                }
            }
        })
        .on_mouse_down(MouseButton::Middle, {
            let coordinator = scene_interactions.clone();
            move |event, window, cx| {
                if coordinator.begin(
                    OpenGpuiSceneGestureKind::Pan,
                    MouseButton::Middle,
                    pan_listener.clone(),
                    None,
                    event,
                    window,
                    cx,
                ) {
                    cx.stop_propagation();
                }
            }
        })
        .when(right_button_pans, |scene| {
            let coordinator = scene_interactions.clone();
            scene.on_mouse_down(MouseButton::Right, move |event, window, cx| {
                if coordinator.begin(
                    OpenGpuiSceneGestureKind::Pan,
                    MouseButton::Right,
                    right_pan_listener.clone(),
                    None,
                    event,
                    window,
                    cx,
                ) {
                    cx.stop_propagation();
                }
            })
        })
        .on_scroll_wheel(move |event, window, cx| {
            if window.default_prevented() {
                cx.stop_propagation();
                return;
            }
            let Some(host) = scroll_host.upgrade() else {
                return;
            };
            if host.update(cx, |host, cx| host.handle_scene_scroll(event, cx)) {
                window.refresh();
                cx.stop_propagation();
            }
        })
        .on_pinch(move |event, window, cx| {
            if window.default_prevented() {
                cx.stop_propagation();
                return;
            }
            let Some(host) = pinch_host.upgrade() else {
                return;
            };
            if host.update(cx, |host, cx| host.handle_scene_pinch(event, cx)) {
                window.refresh();
                cx.stop_propagation();
            }
        });
        #[cfg(test)]
        for (index, (layer, element)) in layers.into_iter().enumerate() {
            recorded_layers[index] = layer;
            scene = scene.child(element);
        }
        #[cfg(not(test))]
        for (_, element) in layers {
            scene = scene.child(element);
        }
        #[cfg(test)]
        {
            self.scene_evidence.record_plan(recorded_layers, wire_count);
        }

        let host = cx.weak_entity();
        let scene = measured_element(
            open_gpui_scene_measurement_element_id(host_id, self.view.id()),
            scene,
            move |_, bounds, _, window, cx| {
                feedback.measurements.record_scene(bounds);
                let measured_origin = bounds.origin;
                let measured_size =
                    ViewSize::new(bounds.size.width.as_f32(), bounds.size.height.as_f32());
                if !feedback.schedule_once() {
                    return;
                }
                let host = host.clone();
                let feedback = feedback.clone();
                window.defer(cx, move |window, cx| {
                    let Some(host) = host.upgrade() else {
                        return;
                    };
                    let deferred = host.update(cx, |host, _| {
                        let mut resolutions = Vec::new();
                        host.scene_origin = Some(measured_origin);
                        host.scene_transform = Some(transform);
                        let viewport_changed =
                            host.update_surface_viewport(measured_size, transform);
                        if let Some(reason) = host.close_reason() {
                            let completed = host.commands.terminate(reason);
                            host.enter_terminal(reason);
                            resolutions.extend(host.handle_completed_commands(completed));
                            return (true, resolutions, None);
                        }
                        let measurement_errors =
                            host.take_prepaint_measurement_errors(&feedback.measurements);
                        let mut request_next_frame =
                            viewport_changed || feedback.requires_follow_up_frame();
                        let mut submission_panic = None;
                        match feedback.submit(&host.view) {
                            DeferredFeedbackSubmission::Submitted(receipt) => {
                                host.stats.feedback_submissions =
                                    host.stats.feedback_submissions.saturating_add(1);
                                request_next_frame |= host.handle_feedback_receipt(&receipt);
                                let completed = match feedback.complete_commands(receipt) {
                                    Ok(completed) => Ok(completed),
                                    Err(error) => {
                                        let message = format!(
                                            "failed to pair native command receipts: {error}"
                                        );
                                        host.record_feedback_failure(None, message.clone());
                                        feedback.fail_commands(|| {
                                            OpenGpuiCommandTicketError::FramePreparation {
                                                message: message.clone(),
                                            }
                                        })
                                    }
                                };
                                resolutions.extend(host.settle_command_batch(completed));
                            }
                            DeferredFeedbackSubmission::Failed(
                                DeferredFeedbackError::Measurement(error),
                            ) => {
                                let message = error.to_string();
                                let completed = feedback.fail_commands(|| {
                                    OpenGpuiCommandTicketError::FramePreparation {
                                        message: message.clone(),
                                    }
                                });
                                resolutions.extend(host.settle_command_batch(completed));
                                host.record_measurement_failure(error);
                            }
                            DeferredFeedbackSubmission::Failed(
                                DeferredFeedbackError::Collector(error),
                            ) => {
                                let message = error.to_string();
                                let completed = feedback.fail_commands(|| {
                                    OpenGpuiCommandTicketError::FramePreparation {
                                        message: message.clone(),
                                    }
                                });
                                resolutions.extend(host.settle_command_batch(completed));
                                host.record_feedback_failure(None, error.to_string());
                            }
                            DeferredFeedbackSubmission::Failed(
                                DeferredFeedbackError::CommandBatch(error),
                            ) => {
                                let message = error.to_string();
                                let completed = feedback.fail_commands(|| {
                                    OpenGpuiCommandTicketError::FramePreparation {
                                        message: message.clone(),
                                    }
                                });
                                resolutions.extend(host.settle_command_batch(completed));
                                host.record_feedback_failure(None, message);
                            }
                            DeferredFeedbackSubmission::Failed(
                                DeferredFeedbackError::Submission(error),
                            ) => {
                                let error = Rc::new(error);
                                let completed = feedback.fail_commands(|| {
                                    OpenGpuiCommandTicketError::FeedbackSubmission(Rc::clone(
                                        &error,
                                    ))
                                });
                                resolutions.extend(host.settle_command_batch(completed));
                                match error.as_ref() {
                                    FeedbackSubmissionError::StaleFrame { .. } => {
                                        request_next_frame = true;
                                    }
                                    FeedbackSubmissionError::ReplayedFrame { .. } => {}
                                    FeedbackSubmissionError::Session(
                                        ViewSessionError::Closed { reason },
                                    ) => {
                                        let completed = host.commands.terminate(*reason);
                                        host.enter_terminal(*reason);
                                        resolutions
                                            .extend(host.handle_completed_commands(completed));
                                        request_next_frame = true;
                                    }
                                    error => {
                                        host.record_feedback_failure(None, error.to_string());
                                    }
                                }
                            }
                            DeferredFeedbackSubmission::Panicked(payload) => {
                                let message =
                                    OpenGpuiCommandTicketError::FeedbackOutcomeUnknown.to_string();
                                let completed = feedback.fail_commands(|| {
                                    OpenGpuiCommandTicketError::FeedbackOutcomeUnknown
                                });
                                resolutions.extend(host.settle_command_batch(completed));
                                host.record_feedback_failure(None, message);
                                request_next_frame = true;
                                submission_panic = Some(payload);
                            }
                        }
                        for error in measurement_errors {
                            host.record_measurement_failure(error);
                        }
                        (request_next_frame, resolutions, submission_panic)
                    });
                    let (request_next_frame, resolutions, submission_panic) = deferred;
                    suppress_host_panic(submission_panic);
                    resolve_command_tickets(resolutions);
                    if request_next_frame {
                        let host_id = host.entity_id();
                        window.on_next_frame(move |_, cx| cx.notify(host_id));
                        window.refresh();
                    }
                });
            },
        );
        interaction_root(
            open_gpui_interaction_element_id(host_id, self.view.id()),
            self.interactions.clone(),
            self.scene_interactions.clone(),
            focus_handle,
            Some(graph_key_listener),
            scene,
        )
        .into_any_element()
    }

    pub(super) fn terminal_scene(
        &self,
        host_id: EntityId,
        focus_handle: FocusHandle,
    ) -> AnyElement {
        interaction_root(
            open_gpui_interaction_element_id(host_id, self.view.id()),
            self.interactions.clone(),
            self.scene_interactions.clone(),
            focus_handle,
            None,
            scene_semantic_element(
                div().id(open_gpui_scene_element_id(host_id, self.view.id())),
                "Closed node graph",
            )
            .size_full(),
        )
        .into_any_element()
    }
}

fn edge_query_bounds(
    surface_size: Option<ViewSize>,
    viewport: CanvasRect,
    transform: ViewportTransform,
) -> Option<ViewRect> {
    if let Some(size) = surface_size.filter(|size| size.is_positive_finite()) {
        return Some(ViewRect::new(ViewPoint::default(), size));
    }
    let origin = transform.screen_point_for_canvas(viewport.origin)?;
    let size = ViewSize::new(
        viewport.size.width * transform.zoom,
        viewport.size.height * transform.zoom,
    );
    let bounds = ViewRect::new(ViewPoint::new(origin.x, origin.y), size);
    bounds.is_positive_finite().then_some(bounds)
}

fn append_dragged_incident_routes(
    view: &jellyflow::runtime::ViewSession,
    graph: Option<&Graph>,
    presentation_offsets: &BTreeMap<NodeId, CanvasPoint>,
    routes: &mut Vec<OpenGpuiVisibleRouteIdentity>,
    limit: usize,
) -> Result<(), ViewSessionError> {
    if routes.len() >= limit {
        return Ok(());
    }
    let Some(graph) = graph else {
        return Ok(());
    };
    let mut admitted = routes
        .iter()
        .map(|route| route.edge)
        .collect::<BTreeSet<_>>();
    for node in presentation_offsets.keys() {
        let Some(node) = graph.node(node) else {
            continue;
        };
        for port in &node.ports {
            let Some(facts) = view.port_connection_facts(*port)? else {
                continue;
            };
            for incident in facts.incident() {
                if admitted.insert(incident.edge()) {
                    routes.push(OpenGpuiVisibleRouteIdentity {
                        edge: incident.edge(),
                        geometry_revision: None,
                    });
                    if routes.len() >= limit {
                        return Ok(());
                    }
                }
            }
        }
    }
    Ok(())
}

fn retain_back<T>(items: &mut Vec<T>, remaining: &mut usize) {
    let keep = items.len().min(*remaining);
    retain_back_to(items, keep);
    *remaining -= keep;
}

fn edge_painter_fallback_message(
    edge: EdgeId,
    fallbacks: OpenGpuiEdgePaintFallbacks<'_>,
) -> String {
    let mut unknown = Vec::with_capacity(4);
    if let Some(key) = fallbacks.unknown_renderer_key() {
        unknown.push(format!("renderer={key:?}"));
    }
    if let Some(token) = fallbacks.unknown_style_token() {
        unknown.push(format!("style={token:?}"));
    }
    if let Some(key) = fallbacks.unknown_source_marker_key() {
        unknown.push(format!("source_marker={key:?}"));
    }
    if let Some(key) = fallbacks.unknown_target_marker_key() {
        unknown.push(format!("target_marker={key:?}"));
    }
    format!(
        "edge {edge:?} uses bounded native chrome fallbacks for {}",
        unknown.join(", ")
    )
}

fn retain_back_to<T>(items: &mut Vec<T>, keep: usize) {
    if items.len() > keep {
        items.drain(..items.len() - keep);
    }
}

fn dragged_edge_path(
    graph: &Graph,
    edge_id: EdgeId,
    route: &EdgeRouteFacts,
    mut node_delta: impl FnMut(NodeId) -> Option<CanvasPoint>,
) -> Option<EdgePath> {
    let edge = graph.edge(&edge_id)?;
    let source_node = graph.port(&edge.from)?.node;
    let target_node = graph.port(&edge.to)?.node;
    let source_delta = node_delta(source_node);
    let target_delta = node_delta(target_node);
    if source_delta.is_none() && target_delta.is_none() {
        return None;
    }

    let mut endpoints = route.endpoints;
    if let Some(delta) = source_delta {
        endpoints.source.point.x += delta.x;
        endpoints.source.point.y += delta.y;
    }
    if let Some(delta) = target_delta {
        endpoints.target.point.x += delta.x;
        endpoints.target.point.y += delta.y;
    }
    resolve_edge_route_path(edge, endpoints).map(|facts| facts.path)
}

#[cfg(test)]
pub(super) fn edge_path_endpoints(path: &EdgePath) -> Option<(CanvasPoint, CanvasPoint)> {
    let source = match path.commands.first()? {
        PathCommand::MoveTo(point) => *point,
        PathCommand::LineTo(_) | PathCommand::CubicTo { .. } => return None,
    };
    let target = match path.commands.last()? {
        PathCommand::MoveTo(point) | PathCommand::LineTo(point) => *point,
        PathCommand::CubicTo { to, .. } => *to,
    };
    Some((source, target))
}

fn edge_path_intersects_viewport(path: &EdgePath, viewport: CanvasRect, margin: f32) -> bool {
    if !viewport.is_positive_finite() || !margin.is_finite() || margin < 0.0 {
        return true;
    }
    let viewport_left = viewport.origin.x - margin;
    let viewport_top = viewport.origin.y - margin;
    let viewport_right = viewport.origin.x + viewport.size.width + margin;
    let viewport_bottom = viewport.origin.y + viewport.size.height + margin;
    if ![viewport_left, viewport_top, viewport_right, viewport_bottom]
        .into_iter()
        .all(f32::is_finite)
    {
        return true;
    }

    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    let mut include = |point: CanvasPoint| {
        if !point.is_finite() {
            return false;
        }
        min_x = min_x.min(point.x);
        min_y = min_y.min(point.y);
        max_x = max_x.max(point.x);
        max_y = max_y.max(point.y);
        true
    };
    let mut segment_count = 0_usize;
    for command in &path.commands {
        let valid = match *command {
            PathCommand::MoveTo(to) => include(to),
            PathCommand::LineTo(to) => {
                segment_count = segment_count.saturating_add(1);
                include(to)
            }
            PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => {
                segment_count = segment_count.saturating_add(1);
                include(control1) && include(control2) && include(to)
            }
        };
        if !valid {
            return true;
        }
    }
    segment_count > 0
        && max_x >= viewport_left
        && min_x <= viewport_right
        && max_y >= viewport_top
        && min_y <= viewport_bottom
}

fn prepare_edge_marker(
    path: &EdgePath,
    anchor: EdgePathAnchor,
    shape: OpenGpuiEdgeMarkerShape,
    reverse: bool,
) -> Option<OpenGpuiPreparedEdgeMarker> {
    if shape == OpenGpuiEdgeMarkerShape::None {
        return None;
    }
    path.sample(anchor)
        .ok()
        .map(|sample| OpenGpuiPreparedEdgeMarker {
            sample,
            shape,
            reverse,
        })
}

fn build_edge_marker_path(
    center: open_gpui::Point<Pixels>,
    tangent: CanvasPoint,
    shape: OpenGpuiEdgeMarkerShape,
    diameter: f32,
    reverse: bool,
) -> Option<Path<Pixels>> {
    if shape == OpenGpuiEdgeMarkerShape::None || !diameter.is_finite() || diameter <= 0.0 {
        return None;
    }
    let direction = if reverse {
        CanvasPoint {
            x: -tangent.x,
            y: -tangent.y,
        }
    } else {
        tangent
    };
    let length = direction.x.hypot(direction.y);
    if !length.is_finite() || length <= f32::EPSILON {
        return None;
    }
    let direction = CanvasPoint {
        x: direction.x / length,
        y: direction.y / length,
    };
    let normal = CanvasPoint {
        x: -direction.y,
        y: direction.x,
    };
    let radius = diameter * 0.5;
    let at = |forward: f32, lateral: f32| {
        point(
            center.x + px(direction.x * forward + normal.x * lateral),
            center.y + px(direction.y * forward + normal.y * lateral),
        )
    };
    let points = match shape {
        OpenGpuiEdgeMarkerShape::None => return None,
        OpenGpuiEdgeMarkerShape::Circle => (0..12)
            .map(|index| {
                let angle = std::f32::consts::TAU * index as f32 / 12.0;
                point(
                    center.x + px(angle.cos() * radius),
                    center.y + px(angle.sin() * radius),
                )
            })
            .collect::<Vec<_>>(),
        OpenGpuiEdgeMarkerShape::Arrow => vec![
            at(radius, 0.0),
            at(-radius, radius * 0.72),
            at(-radius * 0.45, 0.0),
            at(-radius, -radius * 0.72),
        ],
        OpenGpuiEdgeMarkerShape::Diamond => vec![
            at(radius, 0.0),
            at(0.0, radius * 0.72),
            at(-radius, 0.0),
            at(0.0, -radius * 0.72),
        ],
    };
    let mut builder = PathBuilder::fill();
    builder.add_polygon(&points, true);
    builder.build().ok()
}

fn edge_paint_item_key(
    wire: &OpenGpuiSceneWire,
    transform: ViewportTransform,
    origin: open_gpui::Point<Pixels>,
) -> Option<OpenGpuiEdgePaintItemKey> {
    if wire.drag_path.is_some() {
        return None;
    }
    let revision = wire.geometry_revision?;
    Some(OpenGpuiEdgePaintItemKey::new(
        OpenGpuiEdgePathCacheKey::new(
            wire.edge,
            revision,
            transform.pan.x,
            transform.pan.y,
            transform.zoom,
            origin.x.as_f32(),
            origin.y.as_f32(),
            wire.path_stroke_width,
            wire.paint_key.stroke_pattern(),
        ),
        wire.paint_key,
    ))
}

fn build_grid_paths(
    viewport: CanvasSize,
    grid: CanvasSize,
    transform: ViewportTransform,
    stroke_width: f32,
) -> (Option<Path<Pixels>>, Option<Path<Pixels>>) {
    if !viewport.is_positive_finite()
        || !grid.is_positive_finite()
        || !transform.is_valid()
        || !stroke_width.is_finite()
        || stroke_width <= 0.0
    {
        return (None, None);
    }
    let vertical = grid_axis_lines(transform.pan.x, grid.width, transform.zoom, viewport.width);
    let horizontal = grid_axis_lines(
        transform.pan.y,
        grid.height,
        transform.zoom,
        viewport.height,
    );
    let mut minor = PathBuilder::stroke(px(stroke_width));
    let mut major = PathBuilder::stroke(px(stroke_width));
    let mut minor_segments = 0_usize;
    let mut major_segments = 0_usize;
    for (position, is_major) in vertical {
        let (builder, count) = if is_major {
            (&mut major, &mut major_segments)
        } else {
            (&mut minor, &mut minor_segments)
        };
        builder.move_to(point(px(position), px(0.0)));
        builder.line_to(point(px(position), px(viewport.height)));
        *count = count.saturating_add(1);
    }
    for (position, is_major) in horizontal {
        let (builder, count) = if is_major {
            (&mut major, &mut major_segments)
        } else {
            (&mut minor, &mut minor_segments)
        };
        builder.move_to(point(px(0.0), px(position)));
        builder.line_to(point(px(viewport.width), px(position)));
        *count = count.saturating_add(1);
    }
    (
        (minor_segments > 0).then(|| minor.build().ok()).flatten(),
        (major_segments > 0).then(|| major.build().ok()).flatten(),
    )
}

fn grid_axis_lines(
    pan_in_canvas: f32,
    grid_in_canvas: f32,
    zoom: f32,
    extent_in_view: f32,
) -> Vec<(f32, bool)> {
    let spacing = grid_in_canvas * zoom;
    if !spacing.is_finite() || spacing <= 0.0 || !extent_in_view.is_finite() {
        return Vec::new();
    }
    let mut stride = 1_i64;
    while spacing * (stride as f32) < OPEN_GPUI_MIN_GRID_SPACING_IN_VIEW {
        let Some(next) = stride.checked_mul(OPEN_GPUI_MAJOR_GRID_INTERVAL) else {
            return Vec::new();
        };
        stride = next;
    }
    let first_index = (-pan_in_canvas / grid_in_canvas).ceil() as i64;
    let remainder = first_index.rem_euclid(stride);
    let mut index = if remainder == 0 {
        first_index
    } else {
        first_index.saturating_add(stride - remainder)
    };
    let mut lines = Vec::new();
    while lines.len() < OPEN_GPUI_MAX_GRID_LINES_PER_AXIS {
        let position = (index as f32 * grid_in_canvas + pan_in_canvas) * zoom;
        if !position.is_finite() || position > extent_in_view {
            break;
        }
        if position >= 0.0 {
            lines.push((
                position,
                index.rem_euclid(OPEN_GPUI_MAJOR_GRID_INTERVAL) == 0,
            ));
        }
        let Some(next) = index.checked_add(stride) else {
            break;
        };
        index = next;
    }
    lines
}

fn build_scene_wire_path(
    path: &EdgePath,
    stroke_width: f32,
    stroke_pattern: OpenGpuiEdgeStrokePattern,
    transform: ViewportTransform,
    scene_origin: open_gpui::Point<Pixels>,
) -> Option<Path<Pixels>> {
    let mut builder = match stroke_pattern {
        OpenGpuiEdgeStrokePattern::Solid => PathBuilder::stroke(px(stroke_width)),
        OpenGpuiEdgeStrokePattern::Dashed => {
            PathBuilder::stroke(px(stroke_width)).dash_array(&[px(6.0), px(4.0)])
        }
        OpenGpuiEdgeStrokePattern::Dotted => {
            PathBuilder::stroke(px(stroke_width)).dash_array(&[px(1.0), px(3.0)])
        }
    };
    let mut segment_count = 0_usize;
    for command in &path.commands {
        match *command {
            PathCommand::MoveTo(to) => {
                builder.move_to(project_scene_point(to, transform, scene_origin)?);
            }
            PathCommand::LineTo(to) => {
                builder.line_to(project_scene_point(to, transform, scene_origin)?);
                segment_count = segment_count.saturating_add(1);
            }
            PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => {
                builder.cubic_bezier_to(
                    project_scene_point(to, transform, scene_origin)?,
                    project_scene_point(control1, transform, scene_origin)?,
                    project_scene_point(control2, transform, scene_origin)?,
                );
                segment_count = segment_count.saturating_add(1);
            }
        }
    }
    if segment_count == 0 {
        return None;
    }
    builder.build().ok()
}

fn merge_native_paths(batch: &mut Path<Pixels>, mut path: Path<Pixels>) {
    batch.bounds = batch.bounds.union(&path.bounds);
    batch.vertices.append(&mut path.vertices);
}

fn project_scene_point(
    canvas_point: CanvasPoint,
    transform: ViewportTransform,
    scene_origin: open_gpui::Point<Pixels>,
) -> Option<open_gpui::Point<Pixels>> {
    let screen = transform.screen_point_for_canvas(canvas_point)?;
    Some(point(
        scene_origin.x + px(screen.x),
        scene_origin.y + px(screen.y),
    ))
}

#[cfg(test)]
mod navigation_render_tests {
    use super::*;

    #[test]
    fn grid_lines_preserve_major_intervals_and_thin_at_low_zoom() {
        let normal = grid_axis_lines(0.0, 20.0, 1.0, 100.0);
        assert_eq!(
            normal,
            vec![
                (0.0, true),
                (20.0, false),
                (40.0, false),
                (60.0, false),
                (80.0, true),
                (100.0, false),
            ]
        );

        let overview = grid_axis_lines(0.0, 20.0, 0.1, 100.0);
        assert_eq!(
            overview,
            vec![(0.0, true), (32.0, true), (64.0, true), (96.0, true)]
        );
        assert!(overview.len() < normal.len());
    }

    #[test]
    fn grid_line_generation_is_bounded_per_axis() {
        let lines = grid_axis_lines(0.0, 20.0, 100.0, 2_000_000.0);
        assert_eq!(lines.len(), OPEN_GPUI_MAX_GRID_LINES_PER_AXIS);
    }
}
