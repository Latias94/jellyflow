use std::collections::BTreeSet;
use std::rc::Rc;

#[cfg(test)]
use std::cell::{Cell, RefCell};

use jellyflow::core::{CanvasPoint, CanvasRect, EdgeId, Graph, NodeId};
use jellyflow::runtime::runtime::geometry::{
    EdgePath, EdgeRouteFacts, PathCommand, resolve_edge_route_path,
};
use jellyflow::runtime::runtime::viewport::ViewportTransform;
use jellyflow::runtime::{FeedbackSubmissionError, GeometrySnapshot, ViewSessionError, ViewSize};
use open_gpui::{
    AnyElement, Context, EntityId, FocusHandle, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Path, PathBuilder, Pixels, Styled, canvas, div, measured_element, point, px,
    rgb, rgba,
};

use crate::accessibility::{
    node_semantic_element, open_gpui_interaction_element_id, open_gpui_node_element_id,
    open_gpui_node_measurement_element_id, open_gpui_scene_element_id,
    open_gpui_scene_measurement_element_id, scene_semantic_element,
};
use crate::command_journal::{OpenGpuiCommandTicketError, resolve_command_tickets};
use crate::interaction::{OpenGpuiSceneGestureKind, interaction_root};
use crate::semantic_zoom::OpenGpuiNodePresentation;

#[cfg(test)]
use crate::interaction::{
    OpenGpuiSceneGestureCancelReason, OpenGpuiSceneGestureEvent, OpenGpuiSceneGesturePhase,
};

use super::OpenGpuiSceneHost;
use super::connection::OpenGpuiConnectionPreviewPlan;
use super::diagnostics::OpenGpuiEdgeLayerOverflow;
use super::frame_feedback::{
    DeferredFeedbackError, DeferredFeedbackSubmission, DeferredFrameFeedback,
};
use super::node_lifecycle::suppress_host_panic;

pub(super) const OPEN_GPUI_SELECTION_RECTANGLE_SELECTOR: &str = "jellyflow-selection-rectangle";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OpenGpuiSceneLayer {
    Edge,
    Node,
    Chrome,
    Overlay,
}

#[cfg(test)]
pub(super) const OPEN_GPUI_SCENE_LAYER_ORDER: [OpenGpuiSceneLayer; 4] = [
    OpenGpuiSceneLayer::Edge,
    OpenGpuiSceneLayer::Node,
    OpenGpuiSceneLayer::Chrome,
    OpenGpuiSceneLayer::Overlay,
];

#[derive(Clone)]
pub(super) struct OpenGpuiSceneWire {
    pub(super) edge: EdgeId,
    pub(super) selected: bool,
    pub(super) drag_path: Option<EdgePath>,
}

pub(super) struct OpenGpuiSceneWires {
    pub(super) geometry: GeometrySnapshot,
    // Adapter-owned ephemeral previews compose separately and never consume published budgets.
    pub(super) published: Vec<OpenGpuiSceneWire>,
    pub(super) connection_preview: Option<OpenGpuiConnectionPreviewPlan>,
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
    pub(super) last_layers: Cell<Option<[OpenGpuiSceneLayer; 4]>>,
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

    fn record_plan(&self, layers: [OpenGpuiSceneLayer; 4], wire_count: usize) {
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

    pub(super) fn scene_wires(&mut self) -> Result<OpenGpuiSceneWires, ViewSessionError> {
        #[cfg(test)]
        if let Some(error) = self.scene_wires_fault.take() {
            return Err(error);
        }
        let geometry = self.view.geometry_snapshot()?;
        let graph = self
            .scene_gesture
            .has_node_drag_visual()
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
        let selected_edges = self.view.selected_edge_ids(self.edge_route_scan_limit)?;
        let transform = self.view.viewport_transform()?;
        let cull_margin = 4.0 / transform.zoom;
        let routes = geometry.routes();
        let mut remaining_scan = self.edge_route_scan_limit;
        let mut selected_ids_examined = 0_usize;
        let mut route_entries_examined = 0_usize;
        let mut examined_route_ids = BTreeSet::new();
        let mut viewport_culled = 0_usize;
        let mut selected = Vec::new();
        let mut ordinary = Vec::new();

        {
            let mut inspect_route = |edge: EdgeId,
                                     route: &jellyflow::runtime::PublishedEdgeRoute,
                                     selected_route: bool| {
                if !examined_route_ids.insert(edge) {
                    return;
                }
                if Some(edge) == suppressed_edge {
                    return;
                }
                let drag_path = graph.as_ref().and_then(|graph| {
                    dragged_edge_path(graph.graph(), edge, route.facts(), |node| {
                        self.scene_gesture.node_drag_delta(node)
                    })
                });
                let visible_path = drag_path.as_ref().unwrap_or(&route.facts().path);
                if !edge_path_intersects_viewport(visible_path, self.viewport, cull_margin) {
                    viewport_culled = viewport_culled.saturating_add(1);
                    return;
                }
                let wire = OpenGpuiSceneWire {
                    edge,
                    selected: selected_route,
                    drag_path,
                };
                if selected_route {
                    selected.push(wire);
                } else {
                    ordinary.push(wire);
                }
            };

            for edge in selected_edges.edges() {
                remaining_scan -= 1;
                selected_ids_examined = selected_ids_examined.saturating_add(1);
                if let Some(route) = routes.get(edge) {
                    inspect_route(*edge, route, true);
                }
            }

            for (edge, route) in routes.iter().take(remaining_scan) {
                route_entries_examined = route_entries_examined.saturating_add(1);
                inspect_route(*edge, route, false);
            }
        }

        let selected_ids_unexamined = selected_edges
            .total_count()
            .saturating_sub(selected_ids_examined);
        let routes_examined = examined_route_ids.len();
        let routes_unexamined = routes.len().saturating_sub(routes_examined);
        let scan_truncated = selected_ids_unexamined > 0 || routes_unexamined > 0;
        let visible = selected.len().saturating_add(ordinary.len());
        selected.truncate(self.edge_layer_budget);
        let remaining = self.edge_layer_budget.saturating_sub(selected.len());
        ordinary.truncate(remaining);
        selected.extend(ordinary);
        let budget_omitted = visible.saturating_sub(selected.len());
        let overflow = OpenGpuiEdgeLayerOverflow {
            selected_ids_examined,
            selected_ids_unexamined,
            published_route_entries_examined: route_entries_examined,
            published_routes_examined: routes_examined,
            published_routes_unexamined: routes_unexamined,
            scan_truncated,
            viewport_culled,
            budget_omitted,
        };
        self.last_edge_layer_overflow = overflow;
        self.stats.edge_layer_plans = self.stats.edge_layer_plans.saturating_add(1);
        self.stats.edge_route_scan_inputs_examined = self
            .stats
            .edge_route_scan_inputs_examined
            .saturating_add(overflow.scan_inputs_examined() as u64);
        self.stats.edge_selected_ids_examined = self
            .stats
            .edge_selected_ids_examined
            .saturating_add(selected_ids_examined as u64);
        self.stats.edge_route_entries_examined = self
            .stats
            .edge_route_entries_examined
            .saturating_add(route_entries_examined as u64);
        self.stats.edge_routes_examined = self
            .stats
            .edge_routes_examined
            .saturating_add(routes_examined as u64);
        self.stats.edge_routes_unexamined = self
            .stats
            .edge_routes_unexamined
            .saturating_add(routes_unexamined as u64);
        if scan_truncated {
            self.stats.edge_route_scan_truncations =
                self.stats.edge_route_scan_truncations.saturating_add(1);
        }
        self.stats.edge_routes_viewport_culled = self
            .stats
            .edge_routes_viewport_culled
            .saturating_add(viewport_culled as u64);
        self.stats.edge_routes_planned = self
            .stats
            .edge_routes_planned
            .saturating_add(selected.len() as u64);
        self.stats.edge_routes_budget_omitted = self
            .stats
            .edge_routes_budget_omitted
            .saturating_add(budget_omitted as u64);
        if !overflow.is_empty() {
            self.stats.edge_layer_overflows = self.stats.edge_layer_overflows.saturating_add(1);
        }
        Ok(OpenGpuiSceneWires {
            geometry,
            published: selected,
            connection_preview,
        })
    }

    fn render_edge_layer(
        &self,
        wires: OpenGpuiSceneWires,
        transform: ViewportTransform,
    ) -> AnyElement {
        #[cfg(test)]
        let paint_evidence = self.scene_evidence.clone();
        let OpenGpuiSceneWires {
            geometry,
            published,
            connection_preview: _,
        } = wires;
        canvas(
            move |bounds, _, _| {
                published
                    .iter()
                    .filter_map(|wire| {
                        let route = geometry.route(wire.edge)?;
                        let path = wire.drag_path.as_ref().unwrap_or(&route.facts().path);
                        build_scene_wire_path(path, wire.selected, transform, bounds.origin)
                    })
                    .collect::<Vec<_>>()
            },
            move |_, paths, window, _| {
                #[cfg(test)]
                paint_evidence.record_paint(paths.len());
                for (path, selected) in paths {
                    let color = if selected {
                        rgb(0x78a9ff)
                    } else {
                        rgb(0x7d8798)
                    };
                    window.paint_path(path, color);
                }
            },
        )
        .absolute()
        .size_full()
        .into_any_element()
    }

    fn render_node_layer(
        &self,
        feedback: Rc<DeferredFrameFeedback>,
        transform: ViewportTransform,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let host_id = cx.entity_id();
        let scene_interactions = self.scene_interactions.clone();
        let scene_listener = Self::host_scene_listener(cx);
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
                if let Some(delta) = self.scene_gesture.node_drag_delta(key.node()) {
                    bounds.origin.x += delta.x;
                    bounds.origin.y += delta.y;
                }
                let origin = transform
                    .screen_point_for_canvas(bounds.origin)
                    .unwrap_or(bounds.origin);
                let screen_size = mounted
                    .semantic_zoom
                    .screen_size(transform.zoom, presentation);
                let content = match presentation {
                    OpenGpuiNodePresentation::Native => mounted.view.clone().into_any_element(),
                    OpenGpuiNodePresentation::Overview => mounted.semantic_zoom.overview_element(
                        key,
                        &mounted.accessible_name,
                        mounted.selected,
                        screen_size,
                        &mounted.measurement,
                    ),
                };
                let instance_for_gesture = key.clone();
                let activity = mounted.activity.clone();
                let coordinator = scene_interactions.clone();
                let listener = scene_listener.clone();
                let child = node_semantic_element(
                    div().id(open_gpui_node_element_id(host_id, key)),
                    mounted.accessible_name.clone(),
                    mounted.selected,
                )
                .absolute()
                .left(px(origin.x))
                .top(px(origin.y))
                .w(px(screen_size.width))
                .h(px(screen_size.height))
                .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                    if coordinator.begin(
                        OpenGpuiSceneGestureKind::Node(instance_for_gesture.clone()),
                        MouseButton::Left,
                        listener.clone(),
                        Some(activity.clone()),
                        event,
                        window,
                        cx,
                    ) {
                        cx.stop_propagation();
                    }
                })
                .child(content);
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

    fn render_overlay_layer(
        &self,
        connection_preview: Option<OpenGpuiConnectionPreviewPlan>,
        transform: ViewportTransform,
    ) -> AnyElement {
        #[cfg(test)]
        self.scene_evidence
            .last_painted_connection_preview_count
            .set(0);
        let mut overlay = div().absolute().size_full();
        if let Some(connection_preview) = connection_preview {
            #[cfg(test)]
            let paint_evidence = self.scene_evidence.clone();
            let preview = canvas(
                move |bounds, _, _| {
                    build_scene_wire_path(&connection_preview.path, true, transform, bounds.origin)
                        .map(|(path, _)| path)
                },
                move |_, path, window, _| {
                    #[cfg(test)]
                    paint_evidence
                        .last_painted_connection_preview_count
                        .set(usize::from(path.is_some()));
                    if let Some(path) = path {
                        window.paint_path(path, rgb(0x78a9ff));
                    }
                },
            )
            .absolute()
            .size_full();
            overlay = overlay.child(preview);
        }
        if let Some(bounds) = self.scene_gesture.selection_rectangle() {
            overlay = overlay.child(
                div()
                    .debug_selector(|| OPEN_GPUI_SELECTION_RECTANGLE_SELECTOR.to_owned())
                    .absolute()
                    .left(px(bounds.origin.x))
                    .top(px(bounds.origin.y))
                    .w(px(bounds.size.width))
                    .h(px(bounds.size.height))
                    .border_1()
                    .border_color(rgb(0x78a9ff))
                    .bg(rgba(0x78a9ff24)),
            );
        }
        overlay.into_any_element()
    }

    pub(super) fn render_scene(
        &self,
        feedback: Rc<DeferredFrameFeedback>,
        transform: ViewportTransform,
        focus_handle: FocusHandle,
        mut wires: OpenGpuiSceneWires,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let host_id = cx.entity_id();
        #[cfg(test)]
        let wire_count = wires.published.len();
        let connection_preview = wires.connection_preview.take();
        let layers = [
            (
                OpenGpuiSceneLayer::Edge,
                self.render_edge_layer(wires, transform),
            ),
            (
                OpenGpuiSceneLayer::Node,
                self.render_node_layer(feedback.clone(), transform, cx),
            ),
            (
                OpenGpuiSceneLayer::Chrome,
                div().absolute().size_full().into_any_element(),
            ),
            (
                OpenGpuiSceneLayer::Overlay,
                self.render_overlay_layer(connection_preview, transform),
            ),
        ];
        #[cfg(test)]
        let mut recorded_layers = [OpenGpuiSceneLayer::Overlay; 4];
        let scene_interactions = self.scene_interactions.clone();
        let selection_listener = Self::host_scene_listener(cx);
        let pan_listener = selection_listener.clone();
        let host = cx.weak_entity();
        let scroll_host = host.clone();
        let pinch_host = host;
        let mut scene = scene_semantic_element(
            div().id(open_gpui_scene_element_id(host_id, self.view.id())),
            "Node graph",
        )
        .relative()
        .size_full()
        .on_mouse_down(MouseButton::Left, {
            let coordinator = scene_interactions.clone();
            move |event, window, cx| {
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
        .on_scroll_wheel(move |event, window, cx| {
            let Some(host) = scroll_host.upgrade() else {
                return;
            };
            if host.update(cx, |host, cx| host.handle_scene_scroll(event, cx)) {
                window.refresh();
                cx.stop_propagation();
            }
        })
        .on_pinch(move |event, window, cx| {
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
            scene_semantic_element(
                div().id(open_gpui_scene_element_id(host_id, self.view.id())),
                "Closed node graph",
            )
            .size_full(),
        )
        .into_any_element()
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

fn build_scene_wire_path(
    path: &EdgePath,
    selected: bool,
    transform: ViewportTransform,
    scene_origin: open_gpui::Point<Pixels>,
) -> Option<(Path<Pixels>, bool)> {
    let width = if selected { 2.5 } else { 2.0 };
    let mut builder = PathBuilder::stroke(px(width));
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
    builder.build().ok().map(|path| (path, selected))
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
