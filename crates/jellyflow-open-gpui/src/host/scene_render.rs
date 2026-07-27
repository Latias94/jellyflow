use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

#[cfg(test)]
use std::cell::{Cell, RefCell};

#[cfg(test)]
use jellyflow_core::EdgeKind;
use jellyflow_core::{CanvasPoint, CanvasRect, CanvasSize, EdgeId, EdgeLabelAnchor, Graph, NodeId};
use jellyflow_runtime::geometry::{
    EdgeFlattenOptions, EdgePath, EdgePathAnchor, EdgePathSample, EdgePosition, EdgeRouteFacts,
    HandlePosition, PathCommand, flatten_edge_path, resolve_edge_route_path,
};
use jellyflow_runtime::viewport::ViewportTransform;
use jellyflow_runtime::{
    geometry::ViewPoint, geometry::ViewRect, geometry::ViewSize, graph::EdgeEndpoint,
    graph::EdgeInstanceKey, graph::GraphRevision, interaction::CanvasInteractionPhase,
    view::FeedbackSubmissionError, view::GeometrySnapshot, view::RouteGeometryRevision,
    view::ViewSessionError, view::VisibleRouteQueryRevision,
};
use lyon::math::{Point as LyonPoint, point as lyon_point};
use lyon::path::Path as LyonPath;
use lyon::tessellation::geometry_builder::{
    GeometryBuilder, GeometryBuilderError, StrokeGeometryBuilder,
};
use lyon::tessellation::{
    LineCap, LineJoin, StrokeOptions, StrokeTessellator, StrokeVertex, VertexId,
};
use open_gpui::prelude::FluentBuilder;
use open_gpui::{
    AnyElement, Context, CursorStyle, EntityId, FocusHandle, InteractiveElement, IntoElement,
    MouseButton, ParentElement, Path, PathBuilder, Pixels, StatefulInteractiveElement, Styled,
    Window, canvas, div, measured_element, point, px,
};

use crate::accessibility::{
    OPEN_GPUI_RECONNECT_HANDLE_ROLE, node_semantic_element, open_gpui_edge_element_id,
    open_gpui_interaction_element_id, open_gpui_node_element_id,
    open_gpui_node_measurement_element_id, open_gpui_reconnect_element_id,
    open_gpui_scene_element_id, open_gpui_scene_measurement_element_id, scene_semantic_element,
};
use crate::chrome::{
    OpenGpuiEdgeLabelAppearance, OpenGpuiEdgeMarkerShape, OpenGpuiEdgePaintFallbacks,
    OpenGpuiEdgePainterBatchKey, OpenGpuiEdgeStrokePattern, OpenGpuiEdgeVisualFacts,
};
use crate::command_journal::{
    OpenGpuiCommandTicketError, OpenGpuiPendingCommandResolution, resolve_command_tickets,
};
use crate::interaction::{
    OpenGpuiGraphKeyDownListener, OpenGpuiGraphKeyUpListener, OpenGpuiSceneGestureKind,
    interaction_root,
};
use crate::semantic_zoom::{OpenGpuiNodePresentation, overview_node_corner_radius};
use crate::theme::OpenGpuiGraphStyleSlot;

#[cfg(test)]
use crate::interaction::{
    OpenGpuiSceneGestureCancelReason, OpenGpuiSceneGestureEvent, OpenGpuiSceneGesturePhase,
};

use super::OpenGpuiSceneHost;
use super::connection::OpenGpuiConnectionPreviewPlan;
#[cfg(test)]
use super::connection::OpenGpuiConnectionPreviewProbeOutcome;
use super::diagnostics::OpenGpuiEdgeLayerOverflow;
use super::edge_layer::{
    OpenGpuiEdgePaintPlanKey, OpenGpuiEdgePaintPlanSegment, OpenGpuiEdgePathCacheKey,
    OpenGpuiVisibleRouteIdentity,
};
use super::edge_semantics::{
    OPEN_GPUI_EDGE_SEMANTIC_SIZE_IN_VIEW, OpenGpuiEdgeActivation, OpenGpuiEdgeSemanticElement,
    open_gpui_edge_semantic_selector,
};
use super::endpoint_resolver::{
    PublishedEndpointDiagnostic, PublishedEndpointResolver, PublishedRouteEndpointIdentity,
    ResolvedEndpoint, outward_tangent,
};
use super::focus_navigation::{OpenGpuiGraphFocusEntry, OpenGpuiGraphFocusTarget};
use super::frame_feedback::{OpenGpuiFrameTransaction, OpenGpuiFrameTransactionSubmission};
use super::node_lifecycle::suppress_host_panic;

pub(super) const OPEN_GPUI_SELECTION_RECTANGLE_SELECTOR: &str = "jellyflow-selection-rectangle";
pub(super) const OPEN_GPUI_GRID_SELECTOR: &str = "jellyflow-grid";
pub(super) const OPEN_GPUI_ALIGNMENT_GUIDE_SELECTOR: &str = "jellyflow-alignment-guide";

struct OpenGpuiPrepaintFrameOutcome {
    request_next_frame: bool,
    resolutions: Vec<OpenGpuiPendingCommandResolution>,
    submission_panic: Option<Box<dyn std::any::Any + Send + 'static>>,
    terminal_cleanup_pending: bool,
}

impl OpenGpuiPrepaintFrameOutcome {
    fn idle() -> Self {
        Self {
            request_next_frame: false,
            resolutions: Vec::new(),
            submission_panic: None,
            terminal_cleanup_pending: false,
        }
    }
}

pub(super) fn open_gpui_node_chrome_selector(node: NodeId) -> String {
    format!("jellyflow-node-chrome-{node:?}")
}

pub(super) fn open_gpui_reconnect_hit_selector(edge: EdgeId, endpoint: EdgeEndpoint) -> String {
    format!("jellyflow-reconnect-hit-{edge:?}-{endpoint:?}")
}

pub(super) fn open_gpui_reconnect_chrome_selector(edge: EdgeId, endpoint: EdgeEndpoint) -> String {
    format!("jellyflow-reconnect-chrome-{edge:?}-{endpoint:?}")
}

const OPEN_GPUI_MIN_GRID_SPACING_IN_VIEW: f32 = 12.0;
const OPEN_GPUI_MAX_GRID_LINES_PER_AXIS: usize = 512;
const OPEN_GPUI_MAJOR_GRID_INTERVAL: i64 = 4;
const OPEN_GPUI_EDGE_PATH_FLATTEN_MAX_SEGMENTS: usize = 4_096;
const OPEN_GPUI_EDGE_PREVIEW_PATH_VERTEX_BUDGET: usize = 65_536;
const OPEN_GPUI_MIN_STROKE_VERTICES_PER_SEGMENT: usize = 6;
/// Separates the reconnect grip from native port chrome without scaling with canvas zoom.
pub(super) const OPEN_GPUI_RECONNECT_GRIP_OFFSET_IN_VIEW: f32 = 12.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OpenGpuiSceneLayer {
    Edge,
    EdgeChrome,
    Preview,
    EdgeInteraction,
    Node,
    InteractionChrome,
    Overlay,
}

#[cfg(test)]
pub(super) const OPEN_GPUI_SCENE_LAYER_ORDER: [OpenGpuiSceneLayer; 7] = [
    OpenGpuiSceneLayer::Edge,
    OpenGpuiSceneLayer::EdgeChrome,
    OpenGpuiSceneLayer::Preview,
    OpenGpuiSceneLayer::EdgeInteraction,
    OpenGpuiSceneLayer::Node,
    OpenGpuiSceneLayer::InteractionChrome,
    OpenGpuiSceneLayer::Overlay,
];

#[derive(Clone)]
pub(super) struct OpenGpuiSceneWire {
    pub(super) edge: EdgeId,
    pub(super) geometry_revision: Option<RouteGeometryRevision>,
    pub(super) style_slot: Option<OpenGpuiGraphStyleSlot>,
    pub(super) visual_facts: OpenGpuiEdgeVisualFacts,
    pub(super) base_paint_key: OpenGpuiEdgePainterBatchKey,
    pub(super) halo_paint_key: Option<OpenGpuiEdgePainterBatchKey>,
    pub(super) source_endpoint: ResolvedEndpoint,
    pub(super) target_endpoint: ResolvedEndpoint,
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
    base_path: Option<Path<Pixels>>,
    halo_path: Option<Path<Pixels>>,
    markers: Vec<Path<Pixels>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OpenGpuiSceneWirePaintLayer {
    Base,
    Halo,
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
    graph_revision: GraphRevision,
    route_generation: u64,
    geometry_layout_revision: u64,
    edge_budget: usize,
    active_edge: Option<EdgeInstanceKey>,
    suppressed_edge: Option<EdgeInstanceKey>,
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
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct OpenGpuiConnectionPreviewPaintEvidence {
    pub(super) edge_kind: EdgeKind,
    pub(super) probe: OpenGpuiConnectionPreviewProbeOutcome,
    pub(super) style_slot: Option<crate::theme::OpenGpuiGraphStyleSlot>,
    pub(super) paint_key: OpenGpuiEdgePainterBatchKey,
    pub(super) halo_paint_key: Option<OpenGpuiEdgePainterBatchKey>,
    pub(super) stroke_width: f32,
    pub(super) halo_stroke_width: Option<f32>,
    pub(super) source_marker: OpenGpuiEdgeMarkerShape,
    pub(super) target_marker: OpenGpuiEdgeMarkerShape,
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
    pub(super) last_connection_preview_paint: Cell<Option<OpenGpuiConnectionPreviewPaintEvidence>>,
    pub(super) last_painted_connection_preview_count: Cell<usize>,
    pub(super) painted_wire_counts: RefCell<Vec<usize>>,
    #[cfg(test)]
    pending_painted_plan_id: Cell<Option<u64>>,
    #[cfg(test)]
    pending_painted_wire_count: Cell<usize>,
    pub(super) last_scene_phase: Cell<Option<OpenGpuiSceneGesturePhase>>,
    pub(super) last_scene_kind: RefCell<Option<OpenGpuiSceneGestureKind>>,
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
            last_connection_preview_paint: Cell::new(None),
            last_painted_connection_preview_count: Cell::new(0),
            painted_wire_counts: RefCell::new(Vec::new()),
            #[cfg(test)]
            pending_painted_plan_id: Cell::new(None),
            #[cfg(test)]
            pending_painted_wire_count: Cell::new(0),
            last_scene_phase: Cell::new(None),
            last_scene_kind: RefCell::new(None),
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

    #[cfg(test)]
    fn record_plan_paint(&self, plan_id: u64, edge_count: usize, final_segment: bool) {
        if self.pending_painted_plan_id.get() != Some(plan_id) {
            self.pending_painted_plan_id.set(Some(plan_id));
            self.pending_painted_wire_count.set(0);
        }
        self.pending_painted_wire_count.set(
            self.pending_painted_wire_count
                .get()
                .saturating_add(edge_count),
        );
        if final_segment {
            self.record_paint(self.pending_painted_wire_count.replace(0));
            self.pending_painted_plan_id.set(None);
        }
    }

    pub(super) fn record_scene_event(&self, event: &OpenGpuiSceneGestureEvent) {
        self.last_scene_phase.set(Some(event.phase()));
        *self.last_scene_kind.borrow_mut() = Some(event.kind().clone());
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
        let graph = self.view.graph_snapshot()?;
        let connection_preview = self.connection_preview_plan();
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
                        suppressed_edge: preview.suppressed_edge.map(EdgeInstanceKey::edge),
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
                Some(graph.graph()),
                presentation_offsets,
                &mut identities,
                self.edge_layer_budget,
            )?;
            identities.into()
        };
        let hovered = self.interaction_owner.presentation().hovered_edge();
        let active = self.interaction_owner.presentation().active_edge();
        let cache_key = presentation_offsets
            .is_empty()
            .then_some(OpenGpuiPreparedEdgeCacheKey {
                query_bounds,
                query_revision: route_plan.revision,
                graph_revision: graph.revision(),
                route_generation: route_plan.generation,
                geometry_layout_revision: geometry.layout_revision(),
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
            let mut endpoint_diagnostics = Vec::new();
            let mut endpoint_fallback_count = 0usize;
            let endpoint_resolver = PublishedEndpointResolver::new(&geometry);

            for identity in identities.iter().copied() {
                let edge = identity.edge;
                let suppresses_current_lifetime = suppressed_edge.is_some_and(|suppressed| {
                    suppressed.edge() == edge && graph.edge_instance_key(edge) == Some(suppressed)
                });
                if !admitted.insert(edge) || suppresses_current_lifetime {
                    continue;
                }
                let Some(graph_edge) = graph.graph().edge(&edge) else {
                    continue;
                };
                let source_fallback = self.explicit_endpoint_fallback(
                    &endpoint_resolver,
                    graph.graph(),
                    graph_edge.from,
                );
                let target_fallback = self.explicit_endpoint_fallback(
                    &endpoint_resolver,
                    graph.graph(),
                    graph_edge.to,
                );
                let Some(mut source_endpoint) = endpoint_resolver.resolve_route(
                    graph_edge.from,
                    PublishedRouteEndpointIdentity::new(edge, EdgeEndpoint::From),
                    source_fallback,
                ) else {
                    continue;
                };
                let Some(mut target_endpoint) = endpoint_resolver.resolve_route(
                    graph_edge.to,
                    PublishedRouteEndpointIdentity::new(edge, EdgeEndpoint::To),
                    target_fallback,
                ) else {
                    continue;
                };
                collect_scene_endpoint_diagnostic(
                    source_endpoint.diagnostic(),
                    &mut endpoint_diagnostics,
                    &mut endpoint_fallback_count,
                );
                collect_scene_endpoint_diagnostic(
                    target_endpoint.diagnostic(),
                    &mut endpoint_diagnostics,
                    &mut endpoint_fallback_count,
                );
                let published_route = geometry.route(edge);
                let fallback_route = if published_route.is_none() {
                    resolve_edge_route_path(
                        graph_edge,
                        EdgePosition {
                            source: source_endpoint.position(),
                            target: target_endpoint.position(),
                        },
                    )
                } else {
                    None
                };
                let Some(route) = published_route
                    .map(|route| route.facts())
                    .or(fallback_route.as_ref())
                else {
                    continue;
                };
                let drag_path = dragged_edge_path(graph.graph(), edge, route, |node| {
                    presentation_offsets.get(&node).copied()
                });
                let path_override =
                    drag_path.or_else(|| fallback_route.as_ref().map(|route| route.path.clone()));
                let visible_path = path_override.as_ref().unwrap_or(&route.path);
                if path_override.is_some() {
                    if let Ok(sample) = visible_path.sample(EdgePathAnchor::Source) {
                        source_endpoint = source_endpoint.with_route_sample(sample);
                    }
                    if let Ok(sample) = visible_path.sample(EdgePathAnchor::Target) {
                        target_endpoint = target_endpoint.with_route_sample(sample);
                    }
                }
                if !edge_path_intersects_viewport(visible_path, self.viewport, cull_margin) {
                    continue;
                }
                let visual_facts = OpenGpuiEdgeVisualFacts::default()
                    .with_selected(route.interaction.selected)
                    .with_active(
                        published_route.is_some_and(|route| Some(route.instance()) == active),
                    );
                let style_slot = graph
                    .graph()
                    .port(&graph_edge.from)
                    .and_then(|port| port.ty.as_ref())
                    .and_then(|ty| self.edge_painters.style_slot_for_type(ty));
                let paint_recipe = self.edge_painters.resolve_with_style_slot(
                    route.edge_kind,
                    visual_facts,
                    &route.presentation,
                    style_slot,
                    &self.graph_theme,
                );
                if paint_recipe.fallbacks().has_any() {
                    painter_fallback_count = painter_fallback_count.saturating_add(1);
                    if painter_fallbacks.len() < 16 {
                        painter_fallbacks.push((
                            route.presentation.renderer_key.clone(),
                            edge_painter_fallback_message(edge, paint_recipe.fallbacks()),
                        ));
                    }
                }
                let source_marker = prepare_resolved_edge_marker(
                    source_endpoint,
                    paint_recipe.source_marker().shape(),
                    true,
                );
                let target_marker = prepare_resolved_edge_marker(
                    target_endpoint,
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
                    style_slot,
                    visual_facts,
                    base_paint_key: paint_recipe.path_batch_key(),
                    halo_paint_key: paint_recipe.halo_path_batch_key(),
                    source_endpoint,
                    target_endpoint,
                    source_marker,
                    target_marker,
                    label,
                    drag_path: path_override,
                };
                if visual_facts.active() {
                    active_wires.push(wire);
                } else if visual_facts.selected() {
                    selected.push(wire);
                } else {
                    ordinary.push(wire);
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
            if endpoint_fallback_count > 0 {
                self.record_endpoint_geometry_diagnostics(
                    endpoint_diagnostics,
                    endpoint_fallback_count,
                );
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
            .filter(|instance| {
                let edge = instance.edge();
                geometry
                    .route(edge)
                    .is_some_and(|route| route.instance() == *instance)
                    && Some(*instance) != active
                    && !suppressed_edge.is_some_and(|suppressed| {
                        suppressed.edge() == edge
                            && graph.edge_instance_key(edge) == Some(suppressed)
                    })
            })
            .and_then(|instance| {
                let edge = instance.edge();
                let index = *prepared.indices.get(&edge)?;
                let wire = published.get(index)?;
                Some((edge, wire))
            })
            .and_then(|(edge, wire)| {
                let route = geometry.route(edge)?;
                let recipe = self.edge_painters.resolve_with_style_slot(
                    route.facts().edge_kind,
                    wire.visual_facts.with_hovered(true),
                    &route.facts().presentation,
                    wire.style_slot,
                    &self.graph_theme,
                );
                let mut overlay = wire.clone();
                overlay.visual_facts = wire.visual_facts.with_hovered(true);
                overlay.halo_paint_key = recipe.halo_path_batch_key();
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
        let emphasized_wire_count = wires.published.len().saturating_sub(wires.ordinary_count);
        let hover_wire_count = usize::from(wires.hover_overlay.is_some());
        let committed_marker_count = scene_wire_marker_count(&wires.published);
        let hover_marker_count = wires.hover_overlay.as_ref().map_or(0, |wire| {
            scene_wire_marker_count(std::slice::from_ref(wire))
        });
        let future_marker_count = committed_marker_count.saturating_add(hover_marker_count);
        let base = self.render_edge_path_canvas(
            wires.geometry.clone(),
            wires.published.clone(),
            0..wires.published.len(),
            transform,
            wires.published_plan_id,
            OpenGpuiEdgePaintPlanSegment::Ordinary,
            emphasized_wire_count
                .saturating_add(hover_wire_count)
                .saturating_add(future_marker_count),
            true,
            true,
            OpenGpuiSceneWirePaintLayer::Base,
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
                emphasized_wire_count.saturating_add(future_marker_count),
                false,
                false,
                OpenGpuiSceneWirePaintLayer::Halo,
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
                future_marker_count,
                false,
                false,
                OpenGpuiSceneWirePaintLayer::Halo,
            )
        });
        div()
            .absolute()
            .size_full()
            .children(emphasized)
            .children(overlay)
            .child(base)
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
        reserved_items: usize,
        _accumulate_paint_evidence: bool,
        _final_paint_evidence_segment: bool,
        paint_layer: OpenGpuiSceneWirePaintLayer,
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
                path_cache.borrow_mut().resolve_paint_plan_reserving(
                    plan_key,
                    reserved_items,
                    |cache, builder| {
                        let visible_wires = &published[range.clone()];
                        for (index, wire) in visible_wires.iter().enumerate() {
                            let paint_key = match paint_layer {
                                OpenGpuiSceneWirePaintLayer::Base => wire.base_paint_key,
                                OpenGpuiSceneWirePaintLayer::Halo => {
                                    let Some(paint_key) = wire.halo_paint_key else {
                                        continue;
                                    };
                                    paint_key
                                }
                            };
                            let stroke_width = paint_key.path().stroke_width();
                            let path = match wire.drag_path.as_ref() {
                                Some(path) => path,
                                None => {
                                    let Some(route) = geometry.route(wire.edge) else {
                                        builder.reject_unbuilt_items(1);
                                        continue;
                                    };
                                    &route.facts().path
                                }
                            };
                            let vertex_budget = builder
                                .next_path_vertex_budget(visible_wires.len().saturating_sub(index));
                            let built = if let Some(key) =
                                edge_path_cache_key(wire, paint_key, transform, bounds.origin)
                            {
                                cache.resolve_path(key, vertex_budget, || {
                                    build_scene_wire_path(
                                        path,
                                        stroke_width,
                                        paint_key,
                                        transform,
                                        bounds.origin,
                                        vertex_budget,
                                    )
                                })
                            } else {
                                build_scene_wire_path(
                                    path,
                                    stroke_width,
                                    paint_key,
                                    transform,
                                    bounds.origin,
                                    vertex_budget,
                                )
                                .map(Rc::new)
                            };
                            let Some(path) = built else {
                                builder.reject_unbuilt_items(1);
                                continue;
                            };
                            if !builder.merge_shared(paint_key, path) {
                                continue;
                            }
                        }
                    },
                )
            },
            move |_, plan, window, _| {
                let batches = plan.batches();
                let edge_count = plan.edge_count();
                #[cfg(test)]
                if _accumulate_paint_evidence {
                    paint_evidence.record_plan_paint(
                        plan_id,
                        edge_count,
                        _final_paint_evidence_segment,
                    );
                }
                paint_path_cache
                    .borrow_mut()
                    .record_native_paint(batches.len(), edge_count);
                // Open GPUI consumes owned paths; the plan builder bounds this aggregate clone.
                for batch in batches.iter() {
                    window.paint_path(batch.path.as_ref().clone(), batch.key.path().color());
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
        let emphasized_marker_count =
            scene_wire_marker_count(&wires.published[wires.ordinary_count..]);
        let hover_marker_count = wires.hover_overlay.as_ref().map_or(0, |wire| {
            scene_wire_marker_count(std::slice::from_ref(wire))
        });
        let ordinary_markers = self.render_edge_marker_canvas(
            wires.published.clone(),
            0..wires.ordinary_count,
            transform,
            wires.published_plan_id,
            OpenGpuiEdgePaintPlanSegment::MarkerOrdinary,
            emphasized_marker_count.saturating_add(hover_marker_count),
        );
        let hover_markers = wires.hover_overlay.clone().map(|wire| {
            let edge = wire.edge;
            self.render_edge_marker_canvas(
                Rc::<[OpenGpuiSceneWire]>::from(vec![wire]),
                0..1,
                transform,
                wires.published_plan_id,
                OpenGpuiEdgePaintPlanSegment::MarkerHover(edge),
                emphasized_marker_count,
            )
        });
        let emphasized_markers = (wires.ordinary_count < wires.published.len()).then(|| {
            self.render_edge_marker_canvas(
                wires.published.clone(),
                wires.ordinary_count..wires.published.len(),
                transform,
                wires.published_plan_id,
                OpenGpuiEdgePaintPlanSegment::MarkerEmphasized,
                0,
            )
        });
        let mut labels = Vec::new();
        visit_edge_chrome_wires_in_paint_order(wires, |wire| {
            if let Some(label) = self.render_edge_label(wire, transform) {
                labels.push(label);
            }
        });

        div()
            .absolute()
            .size_full()
            .child(ordinary_markers)
            .children(hover_markers)
            .children(emphasized_markers)
            .children(labels)
            .into_any_element()
    }

    fn render_edge_marker_canvas(
        &self,
        wires: Rc<[OpenGpuiSceneWire]>,
        range: std::ops::Range<usize>,
        transform: ViewportTransform,
        plan_id: u64,
        segment: OpenGpuiEdgePaintPlanSegment,
        reserved_items: usize,
    ) -> AnyElement {
        let marker_diameter = self.graph_theme.metrics.edge_marker_diameter;
        let marker_cache = self.edge_paths.clone();
        let marker_paint_cache = marker_cache.clone();
        canvas(
            move |bounds, _, _| {
                let key = OpenGpuiEdgePaintPlanKey::new(
                    plan_id,
                    segment,
                    transform.pan.x,
                    transform.pan.y,
                    transform.zoom,
                    bounds.origin.x.as_f32(),
                    bounds.origin.y.as_f32(),
                );
                marker_cache.borrow_mut().resolve_marker_plan_reserving(
                    key,
                    reserved_items,
                    |builder| {
                        let markers = wires[range.clone()].iter().flat_map(|wire| {
                            [wire.source_marker, wire.target_marker]
                                .into_iter()
                                .flatten()
                                .map(move |marker| (wire.base_paint_key, marker))
                        });
                        for (paint_key, marker) in markers {
                            let Some(center) =
                                project_scene_point(marker.sample.point, transform, bounds.origin)
                            else {
                                builder.reject_unbuilt_items(1);
                                continue;
                            };
                            let Some(path) = build_edge_marker_path(
                                center,
                                marker.sample.tangent,
                                marker.shape,
                                marker_diameter,
                                marker.reverse,
                            ) else {
                                builder.reject_unbuilt_items(1);
                                continue;
                            };
                            if !builder.merge_owned(paint_key, path) {
                                continue;
                            }
                        }
                    },
                )
            },
            move |_, plan, window, _| {
                let batches = plan.batches();
                let marker_count = batches.iter().map(|batch| batch.edge_count).sum::<usize>();
                marker_paint_cache
                    .borrow_mut()
                    .record_native_marker_paint(batches.len(), marker_count);
                // Open GPUI consumes owned paths; the plan builder bounds this aggregate clone.
                for batch in batches.iter() {
                    window.paint_path(batch.path.as_ref().clone(), batch.key.path().color());
                }
            },
        )
        .absolute()
        .size_full()
        .into_any_element()
    }

    fn render_edge_label(
        &self,
        wire: &OpenGpuiSceneWire,
        transform: ViewportTransform,
    ) -> Option<AnyElement> {
        let label_metrics = self.graph_theme.metrics;
        let label = wire.label.as_ref()?;
        let position = transform.screen_point_for_canvas(label.point)?;
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
                .border_color(wire.base_paint_key.path().color());
        }
        let label_height =
            label_metrics.edge_label_font_size + 2.0 * label_metrics.edge_label_vertical_padding;
        Some(
            div()
                .absolute()
                .left(px(position.x - label_metrics.edge_label_max_width / 2.0))
                .top(px(position.y - label_height / 2.0))
                .w(px(label_metrics.edge_label_max_width))
                .flex()
                .justify_center()
                .child(label_chrome)
                .into_any_element(),
        )
    }

    fn render_interaction_chrome_layer(
        &mut self,
        wires: &OpenGpuiSceneWires,
        transform: ViewportTransform,
        focus_entries: &mut Vec<OpenGpuiGraphFocusEntry>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> (AnyElement, AnyElement) {
        let host_id = cx.entity_id();
        let view_id = self.view.id();
        let host = cx.weak_entity();
        let native_interactions = self.interaction_owner.native_capability();
        let scene_listener = Self::host_scene_listener(cx);
        let mut edge_controls = div().absolute().size_full();
        let mut reconnect_chrome = div().absolute().size_full();
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
            let edge_instance = route.instance();
            let edge_name = route.facts().presentation.label.as_deref().map_or_else(
                || format!("Graph edge {edge:?}"),
                |label| format!("Edge {label}"),
            );
            let focus = self.graph_theme.palette.focus;
            let selected = interaction.selected;
            let edge_focus = self
                .interaction_owner
                .graph_focus_action_handle(OpenGpuiGraphFocusTarget::Edge(edge_instance), cx);
            focus_entries.push(OpenGpuiGraphFocusEntry::new(
                OpenGpuiGraphFocusTarget::Edge(edge_instance),
                edge_focus.clone(),
            ));
            let focused = edge_focus.is_focused(window);
            let activation: Option<OpenGpuiEdgeActivation> = interaction.selectable.then(|| {
                let host = host.clone();
                Rc::new(
                    move |window: &mut Window, cx: &mut open_gpui::App| -> bool {
                        let Some(host) = host.upgrade() else {
                            return false;
                        };
                        host.update(cx, |host, cx| {
                            host.activate_edge_action(
                                edge_instance,
                                jellyflow_runtime::interaction::SelectionMode::Replace,
                                window,
                                cx,
                            )
                        })
                    },
                ) as OpenGpuiEdgeActivation
            });
            let semantic_name = if selected {
                format!("{edge_name}, selected")
            } else {
                edge_name.clone()
            };
            let semantic_selector = open_gpui_edge_semantic_selector(edge);
            let semantic_size = OPEN_GPUI_EDGE_SEMANTIC_SIZE_IN_VIEW;
            let semantic_chrome = div()
                .debug_selector(move || semantic_selector.clone())
                .absolute()
                .left(px(position.x - semantic_size * 0.5))
                .top(px(position.y - semantic_size * 0.5))
                .w(px(semantic_size))
                .h(px(semantic_size))
                .when(focused, |chrome| {
                    chrome.rounded_full().border_2().border_color(focus)
                });
            if focused {
                let recipe = self.edge_painters.resolve_with_style_slot(
                    route.facts().edge_kind,
                    wire.visual_facts.with_focused(true),
                    &route.facts().presentation,
                    wire.style_slot,
                    &self.graph_theme,
                );
                let mut focused_wire = wire.clone();
                focused_wire.visual_facts = wire.visual_facts.with_focused(true);
                focused_wire.halo_paint_key = recipe.halo_path_batch_key();
                let focused_wire = Rc::<[OpenGpuiSceneWire]>::from(vec![focused_wire]);
                edge_controls = edge_controls
                    .child(self.render_edge_path_canvas(
                        wires.geometry.clone(),
                        focused_wire.clone(),
                        0..1,
                        transform,
                        wires.published_plan_id,
                        OpenGpuiEdgePaintPlanSegment::Focus(edge),
                        1,
                        false,
                        false,
                        OpenGpuiSceneWirePaintLayer::Halo,
                    ))
                    .child(self.render_edge_path_canvas(
                        wires.geometry.clone(),
                        focused_wire,
                        0..1,
                        transform,
                        wires.published_plan_id,
                        OpenGpuiEdgePaintPlanSegment::FocusBase(edge),
                        0,
                        false,
                        false,
                        OpenGpuiSceneWirePaintLayer::Base,
                    ));
            }
            let control = OpenGpuiEdgeSemanticElement::new(
                open_gpui_edge_element_id(host_id, view_id, edge),
                semantic_name,
                selected,
                edge_focus,
                activation,
                semantic_chrome,
            );
            edge_controls = edge_controls.child(control);

            if !selected {
                continue;
            }
            for (endpoint, resolved, allowed, endpoint_name) in [
                (
                    EdgeEndpoint::From,
                    wire.source_endpoint,
                    interaction.reconnect_source,
                    "source",
                ),
                (
                    EdgeEndpoint::To,
                    wire.target_endpoint,
                    interaction.reconnect_target,
                    "target",
                ),
            ] {
                if !allowed {
                    continue;
                }
                let Some(anchor_position) = transform.screen_point_for_canvas(resolved.point())
                else {
                    continue;
                };
                let position = reconnect_grip_center(
                    anchor_position,
                    resolved.outward(),
                    OPEN_GPUI_RECONNECT_GRIP_OFFSET_IN_VIEW,
                );
                let marker = self.graph_theme.metrics.reconnect_cap_length;
                let hit = self.graph_theme.metrics.reconnect_hit_diameter.max(marker);
                let marker_size = reconnect_grip_visual_size(resolved.outward(), marker);
                let interaction_handle = native_interactions.clone();
                let listener = scene_listener.clone();
                let keyboard_host = host.clone();
                let accessibility_host = host.clone();
                let focus = self.graph_theme.palette.focus;
                let target = OpenGpuiGraphFocusTarget::reconnect(edge_instance, endpoint);
                let reconnect_focus = self.interaction_owner.graph_focus_action_handle(target, cx);
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
                    .cursor(CursorStyle::DragLink)
                    .border_1()
                    .border_color(focus.opacity(0.0))
                    .hover(move |style| style.border_1().border_color(focus.opacity(0.72)))
                    .focus_visible(move |style| style.border_2().border_color(focus))
                    .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                        if interaction_handle.begin(
                            OpenGpuiSceneGestureKind::EdgeReconnect(edge_instance, endpoint),
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
                            host.activate_edge_reconnect_action(edge_instance, endpoint, window, cx)
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
                            host.activate_edge_reconnect_action(edge_instance, endpoint, window, cx)
                        }) {
                            window.prevent_default();
                            cx.stop_propagation();
                        }
                    });
                let reconnect = reconnect
                    .debug_selector(move || open_gpui_reconnect_hit_selector(edge, endpoint));
                edge_controls = edge_controls.child(reconnect);
                reconnect_chrome = reconnect_chrome.child(
                    div()
                        .debug_selector(move || open_gpui_reconnect_chrome_selector(edge, endpoint))
                        .absolute()
                        .left(px(position.x - marker_size.width * 0.5))
                        .top(px(position.y - marker_size.height * 0.5))
                        .w(px(marker_size.width))
                        .h(px(marker_size.height))
                        .rounded_full()
                        .bg(self.graph_theme.palette.selection),
                );
            }
        }
        (
            edge_controls.into_any_element(),
            reconnect_chrome.into_any_element(),
        )
    }

    fn render_node_layer(
        &self,
        feedback: Rc<OpenGpuiFrameTransaction>,
        transform: ViewportTransform,
        window: &Window,
        focus_entries: &mut Vec<OpenGpuiGraphFocusEntry>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let host_id = cx.entity_id();
        let selection_bindings = self.navigation.input_bindings().selection();
        let mut nodes = div().absolute().size_full();
        for (key, mounted) in self.mounts.active_in_order() {
            let presentation = mounted.graph_presentation();
            let interaction_island = mounted.has_interaction_island();

            let mut bounds = mounted.bounds;
            if let Some(delta) = feedback.measurements().presentation_offset(key.node()) {
                bounds.origin.x += delta.x;
                bounds.origin.y += delta.y;
            }
            let Some(origin) = transform.screen_point_for_canvas(bounds.origin) else {
                continue;
            };
            let screen_size = mounted.semantic_zoom.screen_size(
                transform.zoom,
                presentation,
                mounted.registration_policy.scale_policy(),
            );
            // The graph presentation remains the sole measured node and chrome surface. A
            // protected native control may overflow this wrapper at its canonical 1:1 size,
            // but it must not change overview geometry or create another ordered node entry.
            let wrapper_size = screen_size;
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
                        lifetime: key.lifetime(),
                        presentation,
                    },
                    node_focus.clone(),
                ));
            }
            if let Some(binding) = overview_binding {
                for (port, state) in binding.presentation().ports() {
                    let (Some(port_instance), Some(port_focus)) =
                        (binding.port_instance(port), binding.port_focus_handle(port))
                    else {
                        continue;
                    };
                    let port_focus = port_focus.clone().tab_stop(!state.disabled());
                    if !state.disabled() {
                        focus_entries.push(OpenGpuiGraphFocusEntry::new(
                            OpenGpuiGraphFocusTarget::Port(port_instance),
                            port_focus,
                        ));
                    }
                }
            }
            let content = match presentation {
                OpenGpuiNodePresentation::Detail => mounted.view.clone().into_any_element(),
                OpenGpuiNodePresentation::Overview => {
                    let overview = mounted.semantic_zoom.overview_element(
                        overview_binding,
                        &mounted.accessible_name,
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
            let node_for_keyboard = key.lifetime();
            let node_for_accessibility = key.lifetime();
            let node_for_pointer = key.lifetime();
            let keyboard_focus = node_focus.clone();
            let pointer_focus = node_focus.clone();
            let focused = node_focus.contains_focused(window, cx);
            let dragging = feedback
                .measurements()
                .presentation_offsets()
                .contains_key(&key.node());
            let chrome_color = if dragging || focused {
                self.graph_theme.palette.focus
            } else if mounted.selected {
                self.graph_theme.palette.selection
            } else {
                self.graph_theme.palette.node_border.opacity(0.0)
            };
            let chrome_corner_radius =
                node_chrome_corner_radius(presentation, wrapper_size, self.graph_theme);
            let child = node_semantic_element(
                div().id(open_gpui_node_element_id(host_id, key)),
                mounted.accessible_name.clone(),
                mounted.selected,
            )
            .aria_disabled(!node_enabled)
            .track_focus(&node_focus)
            .tab_stop(node_enabled)
            .relative()
            .size_full()
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
                        jellyflow_runtime::interaction::SelectionMode::Replace,
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
                        jellyflow_runtime::interaction::SelectionMode::Replace,
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
                    let selection_mode = selection_bindings.selection_mode(event.modifiers);
                    if let Some(host) = pointer_host.upgrade() {
                        host.update(cx, |host, cx| {
                            host.activate_node_action(node_for_pointer, selection_mode, window, cx);
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
                let chrome_node = key.node();
                node.child(
                    div()
                        .debug_selector(move || open_gpui_node_chrome_selector(chrome_node))
                        .absolute()
                        .inset_0()
                        .rounded(px(chrome_corner_radius))
                        .border_1()
                        .border_color(chrome_color),
                )
            });
            let instance = key.clone();
            let source = mounted.graph_measurement_source();
            let measurements = feedback.measurement_writer();
            let measured = measured_element(
                open_gpui_node_measurement_element_id(host_id, key),
                child,
                move |_, bounds, _, _, _| {
                    let _ = measurements.record_root(&instance, source, bounds);
                },
            );
            let mut wrapper = div()
                .absolute()
                .left(px(origin.x))
                .top(px(origin.y))
                .w(px(wrapper_size.width))
                .h(px(wrapper_size.height))
                .child(measured);
            if interaction_island {
                let interaction_size = mounted.semantic_zoom.canonical_size();
                wrapper = wrapper.child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .w(px(interaction_size.width))
                        .h(px(interaction_size.height))
                        .child(
                            self.interaction_owner
                                .overlay_boundary()
                                .wrap(mounted.view.clone()),
                        ),
                );
            }
            nodes = nodes.child(wrapper);
        }
        nodes.into_any_element()
    }

    fn render_preview_layer(
        &self,
        connection_preview: Option<OpenGpuiConnectionPreviewPlan>,
        transform: ViewportTransform,
    ) -> AnyElement {
        #[cfg(test)]
        {
            self.scene_evidence.last_connection_preview_paint.set(None);
            self.scene_evidence
                .last_painted_connection_preview_count
                .set(0);
        }
        let mut preview_layer = div().absolute().size_full();
        if let Some(connection_preview) = connection_preview {
            let OpenGpuiConnectionPreviewPlan {
                path: preview_path,
                edge_kind,
                style_slot,
                presentation,
                probe,
                suppressed_edge: _,
            } = connection_preview;
            let resolved = self.edge_painters.resolve_with_style_slot(
                edge_kind,
                OpenGpuiEdgeVisualFacts::default().with_preview(true),
                &presentation,
                style_slot,
                &self.graph_theme,
            );
            let resolved = if probe.is_invalid() {
                resolved.with_invalid_connection_cue()
            } else {
                resolved
            };
            let preview_style = resolved.style();
            let path_batch_key = resolved.path_batch_key();
            let halo_path_batch_key = resolved.halo_path_batch_key();
            let source_marker = resolved.source_marker().shape();
            let target_marker = resolved.target_marker().shape();
            #[cfg(test)]
            self.scene_evidence.last_connection_preview_paint.set(Some(
                OpenGpuiConnectionPreviewPaintEvidence {
                    edge_kind,
                    probe,
                    style_slot: preview_style.style_slot(),
                    paint_key: path_batch_key,
                    halo_paint_key: halo_path_batch_key,
                    stroke_width: preview_style.stroke_width(),
                    halo_stroke_width: preview_style.halo().map(|halo| halo.stroke_width()),
                    source_marker,
                    target_marker,
                },
            ));
            let label = resolved.label().and_then(|label| {
                let anchor = match label.anchor() {
                    EdgeLabelAnchor::Source => EdgePathAnchor::Progress(0.2),
                    EdgeLabelAnchor::Center => EdgePathAnchor::Center,
                    EdgeLabelAnchor::Target => EdgePathAnchor::Progress(0.8),
                };
                preview_path
                    .sample(anchor)
                    .ok()
                    .map(|sample| OpenGpuiPreparedEdgeLabel {
                        text: label.text().to_owned(),
                        point: sample.point,
                        appearance: label.appearance(),
                    })
            });
            let marker_diameter = self.graph_theme.metrics.edge_marker_diameter;
            #[cfg(test)]
            let paint_evidence = self.scene_evidence.clone();
            let preview = canvas(
                move |bounds, _, _| {
                    let base_path = build_scene_wire_path(
                        &preview_path,
                        preview_style.stroke_width(),
                        path_batch_key,
                        transform,
                        bounds.origin,
                        OPEN_GPUI_EDGE_PREVIEW_PATH_VERTEX_BUDGET,
                    );
                    let halo_path = halo_path_batch_key.and_then(|halo_key| {
                        build_scene_wire_path(
                            &preview_path,
                            halo_key.path().stroke_width(),
                            halo_key,
                            transform,
                            bounds.origin,
                            OPEN_GPUI_EDGE_PREVIEW_PATH_VERTEX_BUDGET,
                        )
                    });
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
                    OpenGpuiPreviewPaintPlan {
                        base_path,
                        halo_path,
                        markers,
                    }
                },
                move |_, plan, window, _| {
                    #[cfg(test)]
                    paint_evidence
                        .last_painted_connection_preview_count
                        .set(usize::from(plan.base_path.is_some()));
                    if let (Some(path), Some(halo_key)) = (plan.halo_path, halo_path_batch_key) {
                        window.paint_path(path, halo_key.path().color());
                    }
                    if let Some(path) = plan.base_path {
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
        if let Some(drag) = self.interaction_owner.presentation().node_drag() {
            let guide_width = self.graph_theme.metrics.guide_width;
            for guide in drag.guides() {
                let line = match guide.axis() {
                    jellyflow_runtime::interaction::AlignmentGuideAxis::Vertical => {
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
                    jellyflow_runtime::interaction::AlignmentGuideAxis::Horizontal => {
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

    fn submit_prepaint_frame(
        &mut self,
        feedback: &Rc<OpenGpuiFrameTransaction>,
        measured_origin: open_gpui::Point<Pixels>,
        measured_size: ViewSize,
        transform: ViewportTransform,
        cx: &mut Context<Self>,
    ) -> OpenGpuiPrepaintFrameOutcome {
        let Some(feedback) = self.take_frame_transaction(feedback) else {
            return OpenGpuiPrepaintFrameOutcome::idle();
        };
        #[cfg(test)]
        if std::mem::take(&mut self.prepaint_close_fault) {
            let _ = self.view.close();
        }
        let mut resolutions = Vec::new();
        self.scene_origin = Some(measured_origin);
        self.scene_transform = Some(transform);
        let viewport_changed = self.update_surface_viewport(measured_size, transform);
        if let Some(reason) = self.close_reason() {
            resolutions.extend(self.prepare_terminal_cleanup(reason));
            feedback.discard_after_journal_terminal();
            return OpenGpuiPrepaintFrameOutcome {
                request_next_frame: true,
                resolutions,
                submission_panic: None,
                terminal_cleanup_pending: true,
            };
        }
        let measurement_errors = self.take_prepaint_measurement_errors(feedback.measurements());
        for error in measurement_errors {
            self.record_measurement_failure(error);
        }
        let mut request_next_frame = viewport_changed || feedback.requires_follow_up_frame();
        let mut submission_panic = None;
        let mut terminal_cleanup_pending = false;
        let mut result = feedback.submit(&self.view);
        #[cfg(test)]
        if std::mem::take(&mut self.frame_submission_fault) {
            panic!("injected panic after native frame submission");
        }
        for error in std::mem::take(&mut result.measurement_errors) {
            self.record_measurement_failure(error);
        }
        if let Some(error) = result.command_pairing_error.take() {
            self.record_feedback_failure(None, error);
        }
        resolutions.extend(self.settle_retained_command_batch(result.take_command_settlement()));
        match result.submission {
            OpenGpuiFrameTransactionSubmission::Submitted(disposition) => {
                self.stats.feedback_submissions = self.stats.feedback_submissions.saturating_add(1);
                let disposition = self.apply_frame_receipt_disposition(disposition);
                request_next_frame |= disposition.request_next_frame;
                request_next_frame |= self.observe_content_measurements(
                    feedback.measurements(),
                    &disposition.accepted_content_nodes,
                );
            }
            OpenGpuiFrameTransactionSubmission::Failed(error) => match error {
                super::frame_feedback::OpenGpuiFrameTransactionError::Submission(error) => {
                    match error.as_ref() {
                        FeedbackSubmissionError::StaleFrame { .. } => {
                            request_next_frame = true;
                        }
                        FeedbackSubmissionError::ReplayedFrame { .. } => {}
                        FeedbackSubmissionError::Session(ViewSessionError::Closed { reason }) => {
                            resolutions.extend(self.prepare_terminal_cleanup(*reason));
                            feedback.discard_after_journal_terminal();
                            request_next_frame = true;
                            terminal_cleanup_pending = true;
                        }
                        error => self.record_feedback_failure(None, error.to_string()),
                    }
                }
                error => self.record_feedback_failure(None, error.to_string()),
            },
            OpenGpuiFrameTransactionSubmission::Panicked(payload) => {
                self.record_feedback_failure(
                    None,
                    OpenGpuiCommandTicketError::FeedbackOutcomeUnknown.to_string(),
                );
                request_next_frame = true;
                submission_panic = Some(payload);
            }
        }
        let measurement_errors = self.take_prepaint_measurement_errors(feedback.measurements());
        for error in measurement_errors {
            self.record_measurement_failure(error);
        }
        self.schedule_pending_node_size_prefix(cx);
        request_next_frame |= self.node_size_submission_needs_frame();
        OpenGpuiPrepaintFrameOutcome {
            request_next_frame,
            resolutions,
            submission_panic,
            terminal_cleanup_pending,
        }
    }

    pub(super) fn render_scene(
        &mut self,
        feedback: Rc<OpenGpuiFrameTransaction>,
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
        let preview_layer = self.render_preview_layer(connection_preview, transform);
        let edge_chrome_layer = self.render_edge_chrome_layer(&wires, transform);
        let mut focus_entries = Vec::new();
        let node_layer =
            self.render_node_layer(feedback.clone(), transform, window, &mut focus_entries, cx);
        let (edge_interaction_layer, interaction_chrome_layer) =
            self.render_interaction_chrome_layer(&wires, transform, &mut focus_entries, window, cx);
        let allow_focus_return =
            self.interaction_owner.presentation().phase() == CanvasInteractionPhase::Idle;
        self.interaction_owner.reconcile_graph_focus(
            focus_entries,
            &focus_handle,
            allow_focus_return,
            window,
            cx,
        );
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
            (OpenGpuiSceneLayer::EdgeChrome, edge_chrome_layer),
            (OpenGpuiSceneLayer::Preview, preview_layer),
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
        let native_interactions = self.interaction_owner.native_capability();
        let selection_listener = Self::host_scene_listener(cx);
        let pan_listener = selection_listener.clone();
        let right_pan_listener = selection_listener.clone();
        let input_bindings = self.navigation.input_bindings();
        let middle_button_pans = input_bindings.middle_button_pans();
        let right_button_pans = input_bindings.right_button_pans();
        let requests_blank_context =
            !right_button_pans && self.blank_canvas_context_handler.is_some();
        let space_left_pan = self.space_pan_held && input_bindings.space_left_pans();
        let presentation = self.interaction_owner.presentation();
        let hovered_edge_is_selectable = presentation.hovered_edge().is_some_and(|instance| {
            wires.geometry.route(instance.edge()).is_some_and(|route| {
                route.instance() == instance && route.facts().interaction.selectable
            })
        });
        let scene_cursor = native_scene_cursor(presentation.phase(), hovered_edge_is_selectable);
        let host = cx.weak_entity();
        let hover_host = host.clone();
        let hover_state_host = host.clone();
        let scroll_host = host.clone();
        let context_host = host.clone();
        let pinch_host = host;
        let graph_key_down_listener: OpenGpuiGraphKeyDownListener = {
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
        let graph_key_up_listener: OpenGpuiGraphKeyUpListener = {
            let host = cx.weak_entity();
            Rc::new(move |event, window, cx| {
                let Some(host) = host.upgrade() else {
                    return false;
                };
                host.update(cx, |host, cx| {
                    host.handle_graph_root_key_up(event, window, cx)
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
            let interaction_handle = native_interactions.clone();
            move |event, window, cx| {
                if window.default_prevented() {
                    cx.stop_propagation();
                    return;
                }
                let gesture = if space_left_pan {
                    OpenGpuiSceneGestureKind::Pan
                } else {
                    OpenGpuiSceneGestureKind::Selection
                };
                if interaction_handle.begin(
                    gesture,
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
        .when(middle_button_pans, |scene| {
            let interaction_handle = native_interactions.clone();
            scene.on_mouse_down(MouseButton::Middle, move |event, window, cx| {
                if interaction_handle.begin(
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
            })
        })
        .when(right_button_pans, |scene| {
            let interaction_handle = native_interactions.clone();
            scene.on_mouse_down(MouseButton::Right, move |event, window, cx| {
                if interaction_handle.begin(
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
        .when(requests_blank_context, |scene| {
            scene.on_mouse_down(MouseButton::Right, move |event, window, cx| {
                if window.default_prevented() {
                    return;
                }
                let handled = context_host.upgrade().is_some_and(|host| {
                    host.update(cx, |host, cx| {
                        host.request_blank_canvas_context(event, window, cx)
                    })
                });
                if !handled {
                    return;
                }
                window.prevent_default();
                cx.stop_propagation();
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
                feedback.measurements().record_scene(bounds);
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
                    let deferred = host.update(cx, |host, cx| {
                        let transaction =
                            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                host.submit_prepaint_frame(
                                    &feedback,
                                    measured_origin,
                                    measured_size,
                                    transform,
                                    cx,
                                )
                            }));
                        match transaction {
                            Ok(deferred) => deferred,
                            Err(payload) => {
                                let _ = host.take_frame_transaction(&feedback);
                                let message =
                                    "native frame host update panicked after prepaint".to_owned();
                                let completed = feedback.abort_frame_preparation(message.clone());
                                let resolutions = host.settle_command_batch(completed);
                                host.record_feedback_failure(None, message);
                                host.schedule_pending_node_size_prefix(cx);
                                OpenGpuiPrepaintFrameOutcome {
                                    request_next_frame: true,
                                    resolutions,
                                    submission_panic: Some(payload),
                                    terminal_cleanup_pending: false,
                                }
                            }
                        }
                    });
                    if deferred.terminal_cleanup_pending {
                        host.update(cx, |host, cx| {
                            host.capture_terminal_window_cleanup(window, cx);
                        });
                    }
                    resolve_command_tickets(deferred.resolutions);
                    suppress_host_panic(deferred.submission_panic);
                    if deferred.request_next_frame {
                        let host_id = host.entity_id();
                        window.on_next_frame(move |_, cx| cx.notify(host_id));
                        window.refresh();
                    }
                    if deferred.terminal_cleanup_pending {
                        host.update(cx, |host, _| {
                            host.resume_pending_terminal_panics();
                        });
                    }
                });
            },
        );
        interaction_root(
            open_gpui_interaction_element_id(host_id, self.view.id()),
            self.interaction_owner.native_capability(),
            focus_handle,
            Some(graph_key_down_listener),
            Some(graph_key_up_listener),
            scene,
        )
        .into_any_element()
    }

    pub(super) fn terminal_scene(&self, host_id: EntityId) -> AnyElement {
        scene_semantic_element(
            div().id(open_gpui_scene_element_id(host_id, self.view.id())),
            "Closed node graph",
        )
        .size_full()
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
    view: &jellyflow_runtime::view::ViewSession,
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

fn collect_scene_endpoint_diagnostic(
    diagnostic: Option<PublishedEndpointDiagnostic>,
    messages: &mut Vec<String>,
    total: &mut usize,
) {
    let Some(diagnostic) = diagnostic else {
        return;
    };
    *total = (*total).saturating_add(1);
    if messages.len() < 16 {
        messages.push(diagnostic.to_string());
    }
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

fn prepare_resolved_edge_marker(
    endpoint: ResolvedEndpoint,
    shape: OpenGpuiEdgeMarkerShape,
    reverse: bool,
) -> Option<OpenGpuiPreparedEdgeMarker> {
    (shape != OpenGpuiEdgeMarkerShape::None).then_some(OpenGpuiPreparedEdgeMarker {
        sample: EdgePathSample {
            point: endpoint.point(),
            tangent: endpoint.tangent(),
        },
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
    let center = if shape == OpenGpuiEdgeMarkerShape::Arrow {
        point(
            center.x - px(direction.x * radius),
            center.y - px(direction.y * radius),
        )
    } else {
        center
    };
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

#[cfg(test)]
mod marker_tests {
    use super::*;

    #[test]
    fn arrow_tip_terminates_at_the_endpoint_for_both_directions() {
        let endpoint = point(px(100.0), px(40.0));
        let tangent = CanvasPoint { x: 1.0, y: 0.0 };

        let target = build_edge_marker_path(
            endpoint,
            tangent,
            OpenGpuiEdgeMarkerShape::Arrow,
            8.0,
            false,
        )
        .expect("target arrow");
        assert!((target.bounds.origin.x.as_f32() - 92.0).abs() <= f32::EPSILON);
        assert!(
            (target.bounds.origin.x.as_f32() + target.bounds.size.width.as_f32() - 100.0).abs()
                <= f32::EPSILON
        );

        let source =
            build_edge_marker_path(endpoint, tangent, OpenGpuiEdgeMarkerShape::Arrow, 8.0, true)
                .expect("source arrow");
        assert!((source.bounds.origin.x.as_f32() - 100.0).abs() <= f32::EPSILON);
        assert!(
            (source.bounds.origin.x.as_f32() + source.bounds.size.width.as_f32() - 108.0).abs()
                <= f32::EPSILON
        );
    }
}

#[cfg(test)]
mod edge_path_budget_tests {
    use super::*;
    use jellyflow_runtime::geometry::{EdgePathLabel, EdgePresentationFacts};

    use crate::chrome::{
        OpenGpuiEdgeLabelAppearance, OpenGpuiEdgeMarkerShape, OpenGpuiEdgePainterRecipe,
        OpenGpuiEdgePainterRegistry, OpenGpuiEdgeTone,
    };
    use crate::theme::OpenGpuiGraphTheme;

    fn dotted_batch_key() -> OpenGpuiEdgePainterBatchKey {
        let mut painters = OpenGpuiEdgePainterRegistry::builder();
        painters
            .register(
                "test.dotted",
                OpenGpuiEdgePainterRecipe::new(
                    OpenGpuiEdgeStrokePattern::Dotted,
                    OpenGpuiEdgeTone::Data,
                    OpenGpuiEdgeMarkerShape::None,
                    OpenGpuiEdgeMarkerShape::None,
                    OpenGpuiEdgeLabelAppearance::Plain,
                ),
            )
            .expect("bounded test painter");
        let presentation = EdgePresentationFacts {
            renderer_key: Some("test.dotted".to_owned()),
            label: None,
            label_anchor: EdgeLabelAnchor::Center,
            source_marker_key: None,
            target_marker_key: None,
            style_token: None,
        };
        painters
            .build()
            .resolve(
                EdgeKind::Data,
                OpenGpuiEdgeVisualFacts::default(),
                &presentation,
                &OpenGpuiGraphTheme::default(),
            )
            .path_batch_key()
    }

    fn straight_path(from: f32, to: f32) -> EdgePath {
        EdgePath {
            commands: vec![
                PathCommand::MoveTo(CanvasPoint { x: from, y: 20.0 }),
                PathCommand::LineTo(CanvasPoint { x: to, y: 20.0 }),
            ],
            label: EdgePathLabel {
                point: CanvasPoint::default(),
                offset_x: 0.0,
                offset_y: 0.0,
            },
        }
    }

    #[test]
    fn extremely_long_finite_dotted_edge_uses_a_bounded_solid_fallback() {
        let transform =
            ViewportTransform::new(CanvasPoint::default(), 1.0).expect("valid transform");
        let path = straight_path(-1.0e30, 1.0e30);

        let built = build_scene_wire_path(
            &path,
            2.0,
            dotted_batch_key(),
            transform,
            point(px(0.0), px(0.0)),
            4_096,
        );

        let built = built.expect("extreme dotted route keeps a bounded visible fallback");
        assert!(!built.vertices.is_empty());
        assert!(built.vertices.len() <= 4_096);
    }

    #[test]
    fn ordinary_dotted_edge_never_exceeds_its_exact_output_vertex_budget() {
        let transform =
            ViewportTransform::new(CanvasPoint::default(), 1.0).expect("valid transform");
        let path = straight_path(0.0, 240.0);

        let built = build_scene_wire_path(
            &path,
            2.0,
            dotted_batch_key(),
            transform,
            point(px(0.0), px(0.0)),
            4_096,
        )
        .expect("ordinary dotted edge fits the bounded tessellator");

        assert!(!built.vertices.is_empty());
        assert!(built.vertices.len() <= 4_096);
    }

    #[test]
    fn dotted_edge_with_minimum_budget_degrades_to_one_visible_segment() {
        let transform =
            ViewportTransform::new(CanvasPoint::default(), 1.0).expect("valid transform");
        let path = straight_path(0.0, 240.0);

        let built = build_scene_wire_path(
            &path,
            2.0,
            dotted_batch_key(),
            transform,
            point(px(0.0), px(0.0)),
            OPEN_GPUI_MIN_STROKE_VERTICES_PER_SEGMENT,
        )
        .expect("minimum plan share keeps the edge visible");

        assert_eq!(
            built.vertices.len(),
            OPEN_GPUI_MIN_STROKE_VERTICES_PER_SEGMENT
        );
    }

    #[test]
    fn bounded_stroke_output_aborts_instead_of_growing_past_the_vertex_limit() {
        let segments = [ProjectedEdgeSegment {
            from: lyon_point(0.0, 0.0),
            to: lyon_point(4_096.0, 0.0),
        }];
        let source = build_dashed_stroke_source(&segments, 1.0, 3.0, 16_384)
            .expect("input contour budget remains bounded");
        let mut output = BoundedOpenGpuiStrokeOutput::new(96);
        let mut options = StrokeOptions::default().with_line_width(2.0);
        options.start_cap = LineCap::Round;
        options.end_cap = LineCap::Round;

        assert!(
            StrokeTessellator::new()
                .tessellate_path(&source, &options, &mut output)
                .is_err()
                || output.overflowed
        );
        assert!(output.path.vertices.len() <= 96);
    }
}

fn edge_path_cache_key(
    wire: &OpenGpuiSceneWire,
    paint_key: OpenGpuiEdgePainterBatchKey,
    transform: ViewportTransform,
    origin: open_gpui::Point<Pixels>,
) -> Option<OpenGpuiEdgePathCacheKey> {
    if wire.drag_path.is_some() {
        return None;
    }
    let revision = wire.geometry_revision?;
    Some(OpenGpuiEdgePathCacheKey::new(
        wire.edge,
        revision,
        transform.pan.x,
        transform.pan.y,
        transform.zoom,
        origin.x.as_f32(),
        origin.y.as_f32(),
        paint_key.path().stroke_width(),
        paint_key.stroke_pattern(),
        paint_key.uses_round_line_cap(),
        paint_key.uses_round_line_join(),
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
    paint_key: OpenGpuiEdgePainterBatchKey,
    transform: ViewportTransform,
    scene_origin: open_gpui::Point<Pixels>,
    vertex_budget: usize,
) -> Option<Path<Pixels>> {
    build_detailed_scene_wire_path(
        path,
        stroke_width,
        paint_key,
        transform,
        scene_origin,
        vertex_budget,
    )
    .or_else(|| {
        build_simplified_scene_wire_path(path, stroke_width, transform, scene_origin, vertex_budget)
    })
}

fn build_detailed_scene_wire_path(
    path: &EdgePath,
    stroke_width: f32,
    paint_key: OpenGpuiEdgePainterBatchKey,
    transform: ViewportTransform,
    scene_origin: open_gpui::Point<Pixels>,
    vertex_budget: usize,
) -> Option<Path<Pixels>> {
    if vertex_budget < OPEN_GPUI_MIN_STROKE_VERTICES_PER_SEGMENT
        || !stroke_width.is_finite()
        || stroke_width <= 0.0
    {
        return None;
    }

    let max_segments = (vertex_budget / OPEN_GPUI_MIN_STROKE_VERTICES_PER_SEGMENT)
        .clamp(1, OPEN_GPUI_EDGE_PATH_FLATTEN_MAX_SEGMENTS);
    let flatten_error_in_canvas = (0.25 / transform.zoom).max(f32::EPSILON);
    let flattened = flatten_edge_path(
        path,
        EdgeFlattenOptions::new(flatten_error_in_canvas).with_limits(12, max_segments),
    )
    .ok()?;
    let projected = flattened
        .segments()
        .iter()
        .filter_map(|segment| {
            let from = project_scene_point(segment.from(), transform, scene_origin)?;
            let to = project_scene_point(segment.to(), transform, scene_origin)?;
            let from = lyon_point(from.x.as_f32(), from.y.as_f32());
            let to = lyon_point(to.x.as_f32(), to.y.as_f32());
            (from != to).then_some(ProjectedEdgeSegment { from, to })
        })
        .collect::<Vec<_>>();
    if projected.is_empty() {
        return None;
    }

    let source =
        build_bounded_stroke_source(&projected, paint_key.stroke_pattern(), vertex_budget)?;
    let mut options = StrokeOptions::default().with_line_width(stroke_width);
    if paint_key.uses_round_line_cap() {
        options.start_cap = LineCap::Round;
        options.end_cap = LineCap::Round;
    }
    if paint_key.uses_round_line_join() {
        options.line_join = LineJoin::Round;
    }

    let mut output = BoundedOpenGpuiStrokeOutput::new(vertex_budget);
    StrokeTessellator::new()
        .tessellate_path(&source, &options, &mut output)
        .ok()?;
    output.finish()
}

fn build_simplified_scene_wire_path(
    path: &EdgePath,
    stroke_width: f32,
    transform: ViewportTransform,
    scene_origin: open_gpui::Point<Pixels>,
    vertex_budget: usize,
) -> Option<Path<Pixels>> {
    if vertex_budget < OPEN_GPUI_MIN_STROKE_VERTICES_PER_SEGMENT
        || !stroke_width.is_finite()
        || stroke_width <= 0.0
    {
        return None;
    }
    let (source, target) = edge_path_endpoints(path)?;
    let source = project_scene_point(source, transform, scene_origin)?;
    let target = project_scene_point(target, transform, scene_origin)?;
    let source = lyon_point(source.x.as_f32(), source.y.as_f32());
    let target = lyon_point(target.x.as_f32(), target.y.as_f32());
    if source == target {
        return None;
    }

    let projected = [ProjectedEdgeSegment {
        from: source,
        to: target,
    }];
    let source = build_solid_stroke_source(&projected);
    let options = StrokeOptions::default().with_line_width(stroke_width);
    let mut output = BoundedOpenGpuiStrokeOutput::new(vertex_budget);
    StrokeTessellator::new()
        .tessellate_path(&source, &options, &mut output)
        .ok()?;
    output.finish()
}

#[derive(Debug, Clone, Copy)]
struct ProjectedEdgeSegment {
    from: LyonPoint,
    to: LyonPoint,
}

fn build_bounded_stroke_source(
    segments: &[ProjectedEdgeSegment],
    pattern: OpenGpuiEdgeStrokePattern,
    vertex_budget: usize,
) -> Option<LyonPath> {
    match pattern {
        OpenGpuiEdgeStrokePattern::Solid => Some(build_solid_stroke_source(segments)),
        OpenGpuiEdgeStrokePattern::Dashed => {
            build_dashed_stroke_source(segments, 6.0, 4.0, vertex_budget)
        }
        OpenGpuiEdgeStrokePattern::Dotted => {
            build_dashed_stroke_source(segments, 1.0, 3.0, vertex_budget)
        }
    }
}

fn build_solid_stroke_source(segments: &[ProjectedEdgeSegment]) -> LyonPath {
    let mut builder = LyonPath::builder();
    let mut contour_end = None;
    for segment in segments {
        if contour_end != Some(segment.from) {
            if contour_end.is_some() {
                builder.end(false);
            }
            builder.begin(segment.from);
        }
        builder.line_to(segment.to);
        contour_end = Some(segment.to);
    }
    if contour_end.is_some() {
        builder.end(false);
    }
    builder.build()
}

fn build_dashed_stroke_source(
    segments: &[ProjectedEdgeSegment],
    draw_length: f64,
    gap_length: f64,
    vertex_budget: usize,
) -> Option<LyonPath> {
    let maximum_draw_contours = vertex_budget / OPEN_GPUI_MIN_STROKE_VERTICES_PER_SEGMENT;
    if maximum_draw_contours == 0
        || dashed_contour_upper_bound(segments, draw_length + gap_length)
            > maximum_draw_contours as f64
    {
        return None;
    }

    let mut builder = LyonPath::builder();
    let mut previous_end = None;
    let mut drawing = true;
    let mut remaining = draw_length;
    let mut contour_open = false;
    let mut contours = 0usize;

    for segment in segments {
        if previous_end != Some(segment.from) {
            if contour_open {
                builder.end(false);
                contour_open = false;
            }
            drawing = true;
            remaining = draw_length;
        }

        let dx = f64::from(segment.to.x - segment.from.x);
        let dy = f64::from(segment.to.y - segment.from.y);
        let segment_length = dx.hypot(dy);
        if !segment_length.is_finite() {
            return None;
        }
        if segment_length <= f64::EPSILON {
            previous_end = Some(segment.to);
            continue;
        }

        let mut traversed = 0.0;
        while traversed < segment_length {
            let step = remaining.min(segment_length - traversed);
            if !step.is_finite() || step <= f64::EPSILON {
                return None;
            }
            let from = interpolate_lyon_segment(*segment, traversed / segment_length)?;
            let to = interpolate_lyon_segment(*segment, (traversed + step) / segment_length)?;
            if drawing {
                if !contour_open {
                    contours = contours.saturating_add(1);
                    if contours > maximum_draw_contours {
                        return None;
                    }
                    builder.begin(from);
                    contour_open = true;
                }
                builder.line_to(to);
            }

            traversed += step;
            remaining -= step;
            if remaining <= f64::EPSILON {
                if drawing && contour_open {
                    builder.end(false);
                    contour_open = false;
                }
                drawing = !drawing;
                remaining = if drawing { draw_length } else { gap_length };
            }
        }
        previous_end = Some(segment.to);
    }
    if contour_open {
        builder.end(false);
    }
    Some(builder.build())
}

fn dashed_contour_upper_bound(segments: &[ProjectedEdgeSegment], cycle_length: f64) -> f64 {
    let mut upper_bound = 0.0;
    let mut run_length = 0.0;
    let mut previous_end = None;
    for segment in segments {
        if previous_end != Some(segment.from) {
            upper_bound +=
                (run_length / cycle_length).ceil() + if run_length > 0.0 { 1.0 } else { 0.0 };
            run_length = 0.0;
        }
        let dx = f64::from(segment.to.x - segment.from.x);
        let dy = f64::from(segment.to.y - segment.from.y);
        run_length += dx.hypot(dy);
        previous_end = Some(segment.to);
    }
    upper_bound + (run_length / cycle_length).ceil() + if run_length > 0.0 { 1.0 } else { 0.0 }
}

fn interpolate_lyon_segment(segment: ProjectedEdgeSegment, amount: f64) -> Option<LyonPoint> {
    let x =
        f64::from(segment.from.x) + (f64::from(segment.to.x) - f64::from(segment.from.x)) * amount;
    let y =
        f64::from(segment.from.y) + (f64::from(segment.to.y) - f64::from(segment.from.y)) * amount;
    let point = lyon_point(x as f32, y as f32);
    (point.x.is_finite() && point.y.is_finite()).then_some(point)
}

struct BoundedOpenGpuiStrokeOutput {
    positions: Vec<LyonPoint>,
    path: Path<Pixels>,
    vertex_budget: usize,
    overflowed: bool,
}

impl BoundedOpenGpuiStrokeOutput {
    fn new(vertex_budget: usize) -> Self {
        Self {
            positions: Vec::new(),
            path: Path::new(point(px(0.0), px(0.0))),
            vertex_budget,
            overflowed: false,
        }
    }

    fn finish(self) -> Option<Path<Pixels>> {
        (!self.overflowed && !self.path.vertices.is_empty()).then_some(self.path)
    }
}

impl GeometryBuilder for BoundedOpenGpuiStrokeOutput {
    fn begin_geometry(&mut self) {
        self.positions.clear();
        self.path.vertices.clear();
        self.overflowed = false;
    }

    fn end_geometry(&mut self) {}

    fn add_triangle(&mut self, a: VertexId, b: VertexId, c: VertexId) {
        if self.overflowed
            || self.path.vertices.len().saturating_add(3) > self.vertex_budget
            || self.path.vertices.try_reserve_exact(3).is_err()
        {
            self.overflowed = true;
            return;
        }
        let Some((&a, &b, &c)) = self
            .positions
            .get(a.to_usize())
            .zip(self.positions.get(b.to_usize()))
            .zip(self.positions.get(c.to_usize()))
            .map(|((a, b), c)| (a, b, c))
        else {
            self.overflowed = true;
            return;
        };
        self.path.push_triangle(
            (
                point(px(a.x), px(a.y)),
                point(px(b.x), px(b.y)),
                point(px(c.x), px(c.y)),
            ),
            (point(0.0, 1.0), point(0.0, 1.0), point(0.0, 1.0)),
        );
    }

    fn abort_geometry(&mut self) {
        self.overflowed = true;
        self.positions.clear();
        self.path.vertices.clear();
    }
}

impl StrokeGeometryBuilder for BoundedOpenGpuiStrokeOutput {
    fn add_stroke_vertex(
        &mut self,
        vertex: StrokeVertex,
    ) -> Result<VertexId, GeometryBuilderError> {
        if self.overflowed
            || self.positions.len() >= self.vertex_budget
            || self.positions.try_reserve(1).is_err()
        {
            self.overflowed = true;
            return Err(GeometryBuilderError::TooManyVertices);
        }
        let id = VertexId::from_usize(self.positions.len());
        self.positions.push(vertex.position());
        Ok(id)
    }
}

pub(super) fn visit_edge_chrome_wires_in_paint_order(
    wires: &OpenGpuiSceneWires,
    mut visit: impl FnMut(&OpenGpuiSceneWire),
) {
    let hovered = wires.hover_overlay.as_ref().map(|wire| wire.edge);
    for wire in wires.published[..wires.ordinary_count]
        .iter()
        .filter(|wire| Some(wire.edge) != hovered)
    {
        visit(wire);
    }
    if let Some(wire) = wires.hover_overlay.as_ref() {
        visit(wire);
    }
    for wire in wires.published[wires.ordinary_count..].iter() {
        visit(wire);
    }
}

fn scene_wire_marker_count(wires: &[OpenGpuiSceneWire]) -> usize {
    wires.iter().fold(0, |count, wire| {
        count
            .saturating_add(usize::from(wire.source_marker.is_some()))
            .saturating_add(usize::from(wire.target_marker.is_some()))
    })
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

fn reconnect_grip_center(
    anchor: ViewPoint,
    outward: HandlePosition,
    offset_in_view: f32,
) -> ViewPoint {
    let direction = outward_tangent(outward);
    ViewPoint {
        x: anchor.x + direction.x * offset_in_view,
        y: anchor.y + direction.y * offset_in_view,
    }
}

fn reconnect_grip_visual_size(outward: HandlePosition, extent: f32) -> CanvasSize {
    let thickness = (extent / 3.0).max(2.0).min(extent);
    match outward {
        HandlePosition::Top | HandlePosition::Bottom => CanvasSize {
            width: extent,
            height: thickness,
        },
        HandlePosition::Left | HandlePosition::Right => CanvasSize {
            width: thickness,
            height: extent,
        },
    }
}

fn native_scene_cursor(
    phase: CanvasInteractionPhase,
    hovered_edge_is_selectable: bool,
) -> CursorStyle {
    match phase {
        CanvasInteractionPhase::Pan => CursorStyle::ClosedHand,
        CanvasInteractionPhase::ConnectionDrag => CursorStyle::DragLink,
        _ if hovered_edge_is_selectable => CursorStyle::PointingHand,
        _ => CursorStyle::Arrow,
    }
}

fn node_chrome_corner_radius(
    presentation: OpenGpuiNodePresentation,
    screen_size: CanvasSize,
    theme: crate::theme::OpenGpuiGraphTheme,
) -> f32 {
    match presentation {
        OpenGpuiNodePresentation::Overview => overview_node_corner_radius(screen_size, theme),
        OpenGpuiNodePresentation::Detail => theme
            .metrics
            .node_corner_radius
            .min(screen_size.width.min(screen_size.height).max(0.0) * 0.5),
    }
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

    #[test]
    fn reconnect_grip_moves_outward_for_every_handle_side() {
        let anchor = ViewPoint { x: 40.0, y: 60.0 };
        let offset = OPEN_GPUI_RECONNECT_GRIP_OFFSET_IN_VIEW;

        assert_eq!(
            reconnect_grip_center(anchor, HandlePosition::Top, offset),
            ViewPoint { x: 40.0, y: 48.0 }
        );
        assert_eq!(
            reconnect_grip_center(anchor, HandlePosition::Right, offset),
            ViewPoint { x: 52.0, y: 60.0 }
        );
        assert_eq!(
            reconnect_grip_center(anchor, HandlePosition::Bottom, offset),
            ViewPoint { x: 40.0, y: 72.0 }
        );
        assert_eq!(
            reconnect_grip_center(anchor, HandlePosition::Left, offset),
            ViewPoint { x: 28.0, y: 60.0 }
        );
    }

    #[test]
    fn reconnect_grip_offset_remains_fixed_in_screen_space_at_every_zoom() {
        let canvas_anchor = CanvasPoint { x: 40.0, y: 60.0 };
        for zoom in [0.5, 1.0, 2.0] {
            let transform = ViewportTransform::new(CanvasPoint::default(), zoom)
                .expect("valid viewport transform");
            let anchor = transform
                .screen_point_for_canvas(canvas_anchor)
                .expect("visible reconnect anchor");
            let grip = reconnect_grip_center(
                anchor,
                HandlePosition::Right,
                OPEN_GPUI_RECONNECT_GRIP_OFFSET_IN_VIEW,
            );

            assert_eq!(grip.x - anchor.x, 12.0);
            assert_eq!(grip.y, anchor.y);
        }
    }

    #[test]
    fn reconnect_grip_is_an_oriented_endpoint_cap() {
        assert_eq!(
            reconnect_grip_visual_size(HandlePosition::Top, 9.0),
            CanvasSize {
                width: 9.0,
                height: 3.0,
            }
        );
        assert_eq!(
            reconnect_grip_visual_size(HandlePosition::Right, 9.0),
            CanvasSize {
                width: 3.0,
                height: 9.0,
            }
        );
    }

    #[test]
    fn scene_cursor_maps_runtime_presentation_without_local_picking() {
        assert_eq!(
            native_scene_cursor(CanvasInteractionPhase::Idle, true),
            CursorStyle::PointingHand,
        );
        assert_eq!(
            native_scene_cursor(CanvasInteractionPhase::ConnectionDrag, false),
            CursorStyle::DragLink,
        );
        assert_eq!(
            native_scene_cursor(CanvasInteractionPhase::Pan, true),
            CursorStyle::ClosedHand,
        );
        assert_eq!(
            native_scene_cursor(CanvasInteractionPhase::Idle, false),
            CursorStyle::Arrow,
        );
    }

    #[test]
    fn node_chrome_corner_radius_matches_detail_and_overview_surfaces() {
        let theme = crate::theme::OpenGpuiGraphTheme::default();
        let normal = CanvasSize {
            width: 220.0,
            height: 120.0,
        };
        assert_eq!(
            node_chrome_corner_radius(OpenGpuiNodePresentation::Detail, normal, theme),
            6.0
        );
        assert_eq!(
            node_chrome_corner_radius(OpenGpuiNodePresentation::Overview, normal, theme),
            overview_node_corner_radius(normal, theme)
        );
        assert_eq!(
            node_chrome_corner_radius(
                OpenGpuiNodePresentation::Detail,
                CanvasSize {
                    width: 10.0,
                    height: 8.0,
                },
                theme,
            ),
            4.0
        );
        assert_eq!(
            node_chrome_corner_radius(OpenGpuiNodePresentation::Overview, normal, theme),
            node_chrome_corner_radius(OpenGpuiNodePresentation::Detail, normal, theme),
        );
    }
}
