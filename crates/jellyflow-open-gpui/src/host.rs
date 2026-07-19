mod connection;
mod diagnostics;
mod edge_actions;
mod edge_layer;
mod focus_navigation;
mod frame_feedback;
mod interaction_session;
mod lifecycle;
mod node_lifecycle;
mod presentation;
mod retained_mount_store;
mod scene_gesture;
mod scene_render;

pub use self::connection::{OpenGpuiConnectionSearchError, OpenGpuiConnectionSearchSubmission};
pub use self::diagnostics::{
    OpenGpuiEdgeLayerOverflow, OpenGpuiSceneHostDiagnostic, OpenGpuiSceneHostDiagnosticKind,
    OpenGpuiSceneHostStats,
};
pub use self::lifecycle::OpenGpuiSceneHostCloseError;

use self::connection::MountedConnectionOwner;
use self::edge_layer::{OpenGpuiEdgePathCache, OpenGpuiVisibleRouteCache};
use self::focus_navigation::OpenGpuiGraphFocusCoordinator;
use self::frame_feedback::DeferredFrameFeedback;
#[cfg(test)]
use self::frame_feedback::{LayoutReceiptAction, layout_receipt_action};
#[cfg(test)]
use self::lifecycle::HostLifecycleSignal;
use self::node_lifecycle::MountedNode;
#[cfg(test)]
use self::scene_render::{
    OPEN_GPUI_SCENE_LAYER_ORDER, OPEN_GPUI_SELECTION_RECTANGLE_SELECTOR,
    OpenGpuiConnectionPreviewEvidence, OpenGpuiSceneEvidence, OpenGpuiSceneLayer,
};

use std::cell::RefCell;
use std::collections::{BTreeSet, VecDeque};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use jellyflow::core::{CanvasPoint, CanvasRect, CanvasSize, NodeGraphConnectionMode, PortId};
use jellyflow::runtime::runtime::viewport::ViewportTransform;
use jellyflow::runtime::schema::NodeCatalog;
use jellyflow::runtime::{
    AdapterEnvironment, CanvasInteractionError, CanvasInteractionPhase, CanvasInteractionQuery,
    CanvasInteractionSession, FrameMaterializationBudget, FrameMaterializationError,
    FrameMaterializationOverflow, FrameRequest, RendererStamp, ViewCloseReason, ViewPoint,
    ViewRect, ViewSession, ViewSessionError, ViewSize, WeakViewSession, fit_viewport,
};
use open_gpui::{Context, FocusHandle, IntoElement, Pixels, Render, Window};

use self::lifecycle::HostLifecycleBridge;
use self::retained_mount_store::RetainedMountStore;
use self::scene_render::OpenGpuiPreparedEdgeCache;

use crate::activity::OpenGpuiActivityCoordinator;
use crate::chrome::OpenGpuiEdgePainterRegistry;
use crate::command_journal::{
    OpenGpuiCommandJournal, OpenGpuiCommandPrefixBarrierResult, OpenGpuiCommandTicketError,
    resolve_command_tickets,
};
use crate::interaction::{OpenGpuiInteractionCoordinator, OpenGpuiSceneInteractionCoordinator};
use crate::navigation::OpenGpuiNavigationConfig;
use crate::port_handle::OpenGpuiPendingMeasurements;
use crate::registry::OpenGpuiNodeRegistry;
use crate::theme::OpenGpuiGraphTheme;

#[cfg(test)]
use crate::binding::OpenGpuiNodeBinding;
#[cfg(test)]
use crate::command_journal::OpenGpuiCommandOrigin;
#[cfg(test)]
use crate::interaction::{OpenGpuiSceneGestureCancelReason, OpenGpuiSceneGesturePhase};

/// Maximum number of inactive, unprotected retained nodes kept by a scene host.
pub const OPEN_GPUI_DEFAULT_ELIGIBLE_PARKED_CAPACITY: usize = 256;
/// Maximum number of distinct retained nodes that may own retention-protecting activity per view.
pub const OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY: usize = 64;
/// Maximum number of published edge paths materialized by one native paint pass.
pub const OPEN_GPUI_DEFAULT_EDGE_LAYER_BUDGET: usize = 4_096;
/// Defensive maximum number of published edge paths admitted to one native frame.
pub const OPEN_GPUI_MAX_EDGE_LAYER_BUDGET: usize = 16_384;
/// Maximum indexed route entries examined by one visible-route query page.
pub const OPEN_GPUI_DEFAULT_VISIBLE_ROUTE_QUERY_BUDGET: usize = 4_096;

/// Adapter-local policy for native connection and reconnect gestures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenGpuiConnectionOptions {
    /// Renderer-neutral validation mode used by every connection gesture.
    pub mode: NodeGraphConnectionMode,
    /// Whether releasing on empty canvas may enter application-provided create-from-wire search.
    pub allow_create_from_wire: bool,
}

impl Default for OpenGpuiConnectionOptions {
    fn default() -> Self {
        Self {
            mode: NodeGraphConnectionMode::default(),
            allow_create_from_wire: true,
        }
    }
}

const DIAGNOSTIC_RENDERER_KEY: &str = "jellyflow.open-gpui.diagnostic";
const RENDERER_INCARNATION: u64 = 1;

/// Retained Open GPUI scene host backed by one headless [`ViewSession`].
pub struct OpenGpuiSceneHost {
    view: ViewSession,
    commands: Rc<OpenGpuiCommandJournal>,
    catalog: Arc<NodeCatalog>,
    registry: OpenGpuiNodeRegistry,
    frame_request_template: FrameRequest,
    viewport: CanvasRect,
    surface_size: Option<ViewSize>,
    tracks_surface_viewport: bool,
    frame_materialization_budget: FrameMaterializationBudget,
    edge_layer_budget: usize,
    visible_route_query_budget: usize,
    visible_routes: OpenGpuiVisibleRouteCache,
    prepared_edges: OpenGpuiPreparedEdgeCache,
    edge_paths: Rc<RefCell<OpenGpuiEdgePathCache>>,
    connection_options: OpenGpuiConnectionOptions,
    navigation: OpenGpuiNavigationConfig,
    graph_theme: OpenGpuiGraphTheme,
    edge_painters: Arc<OpenGpuiEdgePainterRegistry>,
    mounts: RetainedMountStore<MountedNode>,
    activity_coordinator: OpenGpuiActivityCoordinator,
    interactions: OpenGpuiInteractionCoordinator,
    scene_interactions: OpenGpuiSceneInteractionCoordinator,
    canvas_interaction: CanvasInteractionSession,
    pending_interaction_queries: VecDeque<CanvasInteractionQuery>,
    pending_interaction_submission: Option<interaction_session::PendingInteractionSubmission>,
    ready_interaction_prefixes: VecDeque<(
        jellyflow::runtime::InteractionTicketId,
        OpenGpuiCommandPrefixBarrierResult,
    )>,
    interaction_tick_scheduled: bool,
    last_interaction_tick_at: Option<Instant>,
    connection_owner: Option<MountedConnectionOwner>,
    scene_origin: Option<open_gpui::Point<Pixels>>,
    scene_transform: Option<ViewportTransform>,
    interaction_focus: Option<FocusHandle>,
    graph_focus: OpenGpuiGraphFocusCoordinator,
    hovered_node: Option<jellyflow::core::NodeId>,
    hovered_port: Option<PortId>,
    document_epoch: Option<jellyflow::runtime::DocumentEpoch>,
    adapter_environment_epoch: u64,
    terminal_reason: Option<ViewCloseReason>,
    terminal_window_cleanup_pending: bool,
    lifecycle_bridge: Option<HostLifecycleBridge>,
    #[cfg(test)]
    scene_evidence: Rc<OpenGpuiSceneEvidence>,
    #[cfg(test)]
    scene_wires_fault: Option<ViewSessionError>,
    stats: OpenGpuiSceneHostStats,
    last_diagnostic: Option<OpenGpuiSceneHostDiagnostic>,
    last_frame_materialization_overflow: FrameMaterializationOverflow,
    last_edge_layer_overflow: OpenGpuiEdgeLayerOverflow,
}

fn catalog_frame_request(catalog: &NodeCatalog, viewport: CanvasRect) -> FrameRequest {
    let mut request = FrameRequest::new(
        viewport,
        RendererStamp::new(DIAGNOSTIC_RENDERER_KEY, RENDERER_INCARNATION),
    );
    for spec in catalog.kinds() {
        request = request.with_kind_renderer(
            spec.kind().clone(),
            RendererStamp::new(spec.renderer_key(), RENDERER_INCARNATION),
        );
        if let Some(size) = spec.default_size() {
            request = request.with_kind_default(spec.kind().clone(), size);
        }
    }
    request.with_incident_routing_dependencies()
}

fn effective_frame_materialization_budget(
    configured: FrameMaterializationBudget,
    protected_required_nodes: usize,
) -> FrameMaterializationBudget {
    FrameMaterializationBudget::new(
        configured.visible_nodes(),
        protected_required_nodes,
        configured.routing_dependency_nodes(),
    )
    .with_visible_node_scan_limit(configured.visible_node_scan_limit())
}

impl OpenGpuiSceneHost {
    #[must_use]
    pub fn new(view: ViewSession, registry: OpenGpuiNodeRegistry, viewport: CanvasRect) -> Self {
        let catalog = view.catalog_owned();
        let frame_request_template = catalog_frame_request(&catalog, viewport);
        let navigation = OpenGpuiNavigationConfig::default();
        let canvas_interaction = CanvasInteractionSession::new(
            view.id(),
            view.generation()
                .expect("a newly constructed native host owns an open view"),
            navigation.runtime_config(),
        )
        .expect("the built-in canvas interaction configuration is valid");
        Self {
            view,
            commands: OpenGpuiCommandJournal::new(),
            catalog,
            registry,
            frame_request_template,
            viewport,
            surface_size: None,
            tracks_surface_viewport: false,
            frame_materialization_budget: FrameMaterializationBudget::default(),
            edge_layer_budget: OPEN_GPUI_DEFAULT_EDGE_LAYER_BUDGET,
            visible_route_query_budget: OPEN_GPUI_DEFAULT_VISIBLE_ROUTE_QUERY_BUDGET,
            visible_routes: OpenGpuiVisibleRouteCache::default(),
            prepared_edges: OpenGpuiPreparedEdgeCache::default(),
            edge_paths: Rc::new(RefCell::new(OpenGpuiEdgePathCache::default())),
            connection_options: OpenGpuiConnectionOptions::default(),
            navigation,
            graph_theme: OpenGpuiGraphTheme::default(),
            edge_painters: Arc::new(OpenGpuiEdgePainterRegistry::default()),
            mounts: RetainedMountStore::new(OPEN_GPUI_DEFAULT_ELIGIBLE_PARKED_CAPACITY),
            activity_coordinator: OpenGpuiActivityCoordinator::new(
                OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY,
            ),
            interactions: OpenGpuiInteractionCoordinator::new(),
            scene_interactions: OpenGpuiSceneInteractionCoordinator::new(),
            canvas_interaction,
            pending_interaction_queries: VecDeque::new(),
            pending_interaction_submission: None,
            ready_interaction_prefixes: VecDeque::new(),
            interaction_tick_scheduled: false,
            last_interaction_tick_at: None,
            connection_owner: None,
            scene_origin: None,
            scene_transform: None,
            interaction_focus: None,
            graph_focus: OpenGpuiGraphFocusCoordinator::default(),
            hovered_node: None,
            hovered_port: None,
            document_epoch: None,
            adapter_environment_epoch: 0,
            terminal_reason: None,
            terminal_window_cleanup_pending: false,
            lifecycle_bridge: None,
            #[cfg(test)]
            scene_evidence: Rc::new(OpenGpuiSceneEvidence::new()),
            #[cfg(test)]
            scene_wires_fault: None,
            stats: OpenGpuiSceneHostStats::default(),
            last_diagnostic: None,
            last_frame_materialization_overflow: FrameMaterializationOverflow::default(),
            last_edge_layer_overflow: OpenGpuiEdgeLayerOverflow::default(),
        }
    }

    /// Creates a host whose first frame only bootstraps native surface measurement.
    #[must_use]
    pub fn new_measured_surface(view: ViewSession, registry: OpenGpuiNodeRegistry) -> Self {
        let mut host = Self::new(
            view,
            registry,
            CanvasRect {
                origin: CanvasPoint::default(),
                size: CanvasSize {
                    width: 1.0,
                    height: 1.0,
                },
            },
        );
        host.tracks_surface_viewport = true;
        host
    }

    /// Overrides native frame materialization limits before the host is mounted.
    ///
    /// The host replaces the required-node limit per frame with the exact number of protected
    /// owners. Those owners are independently bounded by [`OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY`].
    #[must_use]
    pub fn with_frame_materialization_budget(mut self, budget: FrameMaterializationBudget) -> Self {
        self.frame_materialization_budget = budget;
        self
    }

    /// Replaces native frame materialization limits and schedules a fresh frame when they change.
    ///
    /// The host replaces the required-node limit per frame with the exact number of protected
    /// owners. Those owners are independently bounded by [`OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY`].
    pub fn set_frame_materialization_budget(
        &mut self,
        budget: FrameMaterializationBudget,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.frame_materialization_budget == budget {
            return false;
        }
        self.frame_materialization_budget = budget;
        cx.notify();
        true
    }

    #[must_use]
    pub const fn frame_materialization_budget(&self) -> FrameMaterializationBudget {
        self.frame_materialization_budget
    }

    /// Overrides the independent published edge-path limit before the host is mounted.
    #[must_use]
    pub fn with_edge_layer_budget(mut self, budget: usize) -> Self {
        self.edge_layer_budget = budget.min(OPEN_GPUI_MAX_EDGE_LAYER_BUDGET);
        self
    }

    /// Replaces the published edge-path limit and schedules a fresh native paint pass.
    pub fn set_edge_layer_budget(&mut self, budget: usize, cx: &mut Context<Self>) -> bool {
        let budget = budget.min(OPEN_GPUI_MAX_EDGE_LAYER_BUDGET);
        if self.edge_layer_budget == budget {
            return false;
        }
        self.edge_layer_budget = budget;
        self.prepared_edges.clear();
        cx.notify();
        true
    }

    #[must_use]
    pub const fn edge_layer_budget(&self) -> usize {
        self.edge_layer_budget
    }

    /// Overrides the indexed visible-route query budget before the host is mounted.
    #[must_use]
    pub fn with_visible_route_query_budget(mut self, budget: usize) -> Self {
        self.visible_route_query_budget = budget.max(1);
        self
    }

    /// Replaces the visible-route query budget and schedules a fresh native paint pass.
    pub fn set_visible_route_query_budget(
        &mut self,
        budget: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        let budget = budget.max(1);
        if self.visible_route_query_budget == budget {
            return false;
        }
        self.visible_route_query_budget = budget;
        self.visible_routes.clear();
        self.prepared_edges.clear();
        cx.notify();
        true
    }

    #[must_use]
    pub const fn visible_route_query_budget(&self) -> usize {
        self.visible_route_query_budget
    }

    /// Overrides native connection policy before the host is mounted.
    #[must_use]
    pub fn with_connection_options(mut self, options: OpenGpuiConnectionOptions) -> Self {
        self.connection_options = options;
        self
    }

    /// Replaces the policy used by subsequently armed connection gestures.
    pub fn set_connection_options(&mut self, options: OpenGpuiConnectionOptions) -> bool {
        if self.connection_options == options {
            return false;
        }
        self.connection_options = options;
        true
    }

    #[must_use]
    pub const fn connection_options(&self) -> OpenGpuiConnectionOptions {
        self.connection_options
    }

    /// Overrides adapter-local navigation before mounting the host.
    #[must_use]
    pub fn with_navigation_config(mut self, config: OpenGpuiNavigationConfig) -> Self {
        let config = config.sanitized();
        self.canvas_interaction
            .set_config(config.runtime_config())
            .expect("a newly constructed native host has an idle interaction session");
        self.navigation = config;
        self
    }

    /// Replaces navigation policy while no graph interaction is active.
    pub fn set_navigation_config(
        &mut self,
        config: OpenGpuiNavigationConfig,
        cx: &mut Context<Self>,
    ) -> Result<bool, CanvasInteractionError> {
        let config = config.sanitized();
        let runtime_changed = self
            .canvas_interaction
            .set_config(config.runtime_config())?;
        let navigation_changed = self.navigation != config;
        if navigation_changed {
            self.navigation = config;
        }
        if runtime_changed || navigation_changed {
            cx.notify();
        }
        Ok(runtime_changed || navigation_changed)
    }

    #[must_use]
    pub const fn navigation_config(&self) -> OpenGpuiNavigationConfig {
        self.navigation
    }

    /// Fits every visible node with canonical size into the measured native surface.
    ///
    /// Hidden nodes and nodes without either a document size or catalog default are ignored. An
    /// empty fit set, an unmeasured surface, or an active graph gesture is a deterministic no-op.
    pub fn fit_view(&mut self, cx: &mut Context<Self>) -> Result<bool, ViewSessionError> {
        if self.canvas_interaction.presentation().phase() != CanvasInteractionPhase::Idle {
            return Ok(false);
        }
        let Some(surface) = self.surface_size else {
            return Ok(false);
        };
        let snapshot = self.view.graph_snapshot()?;
        let bounds = snapshot
            .graph()
            .nodes()
            .into_iter()
            .filter_map(|(_, node)| {
                if node.hidden {
                    return None;
                }
                let size = node.size.or_else(|| {
                    self.catalog
                        .get(&node.kind)
                        .and_then(|spec| spec.default_size())
                })?;
                node.bounds_for_size(size)
            })
            .reduce(union_canvas_rects);
        let Some(bounds) = bounds else {
            return Ok(false);
        };
        let Some(transform) = fit_viewport(
            bounds,
            surface,
            self.navigation.fit_view_padding_in_view(),
            self.navigation.min_zoom(),
            self.navigation.max_zoom(),
        ) else {
            return Ok(false);
        };
        if self.view.viewport_transform()? == transform {
            return Ok(false);
        }
        self.view.set_viewport(transform.pan, transform.zoom)?;
        self.scene_transform = Some(transform);
        self.update_surface_viewport(surface, transform);
        cx.notify();
        Ok(true)
    }

    /// Overrides adapter-local graph chrome colors and metrics before mounting the host.
    #[must_use]
    pub fn with_graph_theme(mut self, theme: OpenGpuiGraphTheme) -> Self {
        self.graph_theme = theme.sanitized();
        self
    }

    /// Replaces adapter-local graph chrome colors and metrics.
    pub fn set_graph_theme(&mut self, theme: OpenGpuiGraphTheme, cx: &mut Context<Self>) -> bool {
        let theme = theme.sanitized();
        if self.graph_theme == theme {
            return false;
        }
        self.graph_theme = theme;
        self.prepared_edges.clear();
        self.edge_paths.borrow_mut().clear();
        cx.notify();
        true
    }

    /// Installs a sealed, bounded native edge-painter registry before mounting the host.
    #[must_use]
    pub fn with_edge_painter_registry(mut self, registry: OpenGpuiEdgePainterRegistry) -> Self {
        self.edge_painters = Arc::new(registry);
        self
    }

    /// Replaces native edge painter recipes and schedules a fresh chrome pass.
    pub fn set_edge_painter_registry(
        &mut self,
        registry: OpenGpuiEdgePainterRegistry,
        cx: &mut Context<Self>,
    ) {
        self.edge_painters = Arc::new(registry);
        self.prepared_edges.clear();
        self.edge_paths.borrow_mut().clear();
        cx.notify();
    }

    /// Returns the immutable painter recipes used by this host.
    #[must_use]
    pub fn edge_painter_registry(&self) -> &OpenGpuiEdgePainterRegistry {
        &self.edge_painters
    }

    #[must_use]
    pub const fn graph_theme(&self) -> OpenGpuiGraphTheme {
        self.graph_theme
    }

    /// Returns a non-owning capability for controls that operate on this exact view session.
    #[must_use]
    pub fn view_handle(&self) -> WeakViewSession {
        self.view.downgrade()
    }

    /// Advances geometry-affecting native environment generations for this view only.
    ///
    /// A changed environment makes previously issued feedback stale and schedules a fresh native
    /// measurement pass. Reapplying the same environment is a benign no-op.
    pub fn set_adapter_environment(
        &mut self,
        environment: AdapterEnvironment,
        cx: &mut Context<Self>,
    ) -> Result<bool, ViewSessionError> {
        let changed = self.view.set_adapter_environment(environment)?;
        if changed {
            self.adapter_environment_epoch = self.adapter_environment_epoch.saturating_add(1);
            cx.notify();
        }
        Ok(changed)
    }

    pub fn set_frame_bounds(&mut self, frame_bounds: CanvasRect) {
        self.tracks_surface_viewport = false;
        self.viewport = frame_bounds;
    }

    /// Resumes deriving frame bounds from the measured native surface and current pan/zoom.
    ///
    /// Calling [`Self::set_frame_bounds`] switches to manual frame bounds; this method restores the
    /// default measured-surface behavior.
    pub fn track_surface_viewport(&mut self) {
        self.tracks_surface_viewport = true;
    }

    #[must_use]
    pub fn frame_bounds(&self) -> CanvasRect {
        self.viewport
    }

    /// Returns the latest measured surface size in local logical pixels.
    #[must_use]
    pub fn surface_size(&self) -> Option<ViewSize> {
        self.surface_size
    }

    fn frame_request(&self) -> FrameRequest {
        let viewport = if self.tracks_surface_viewport {
            self.surface_size
                .and_then(|size| {
                    self.view
                        .viewport_transform()
                        .ok()
                        .and_then(|transform| canvas_viewport_for_surface(size, transform))
                })
                .unwrap_or(self.viewport)
        } else {
            self.viewport
        };
        let protected_required_nodes = self
            .mounts
            .iter()
            .filter(|(_, mounted)| mounted.activity.protected_count() > 0)
            .map(|(instance, _)| instance.node())
            .collect::<BTreeSet<_>>();
        let materialization_budget = effective_frame_materialization_budget(
            self.frame_materialization_budget,
            protected_required_nodes.len(),
        );
        self.frame_request_template
            .clone()
            .with_viewport(viewport)
            .with_materialization_budget(materialization_budget)
            .with_required_nodes(protected_required_nodes)
    }

    fn update_surface_viewport(
        &mut self,
        surface_size: ViewSize,
        transform: ViewportTransform,
    ) -> bool {
        self.surface_size = Some(surface_size);
        if !self.tracks_surface_viewport {
            return false;
        }
        if let Some(viewport) = canvas_viewport_for_surface(surface_size, transform) {
            let changed = self.viewport != viewport;
            self.viewport = viewport;
            return changed;
        }
        false
    }
}

fn union_canvas_rects(left: CanvasRect, right: CanvasRect) -> CanvasRect {
    let min_x = left.origin.x.min(right.origin.x);
    let min_y = left.origin.y.min(right.origin.y);
    let max_x = (left.origin.x + left.size.width).max(right.origin.x + right.size.width);
    let max_y = (left.origin.y + left.size.height).max(right.origin.y + right.size.height);
    CanvasRect {
        origin: CanvasPoint { x: min_x, y: min_y },
        size: CanvasSize {
            width: max_x - min_x,
            height: max_y - min_y,
        },
    }
}

fn canvas_viewport_for_surface(
    surface_size: ViewSize,
    transform: ViewportTransform,
) -> Option<CanvasRect> {
    ViewRect::new(ViewPoint::default(), surface_size).to_canvas_rect(transform)
}

impl Render for OpenGpuiSceneHost {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let host_id = cx.entity_id();
        let focus_handle = self
            .interaction_focus
            .get_or_insert_with(|| cx.focus_handle().tab_stop(true))
            .clone();
        self.interactions.set_focus_handle(focus_handle.clone());
        self.scene_interactions
            .set_focus_handle(focus_handle.clone());
        self.interactions.schedule_pending_drain(window, cx);
        self.scene_interactions.schedule_pending_drain(window, cx);
        self.ensure_lifecycle_bridge(cx);
        if let Some(reason) = self.close_reason() {
            self.finish_terminal_render_cleanup(reason, window, cx);
            return self.terminal_scene(host_id, focus_handle);
        }
        self.process_one_ready_interaction_prefix(window, cx);
        self.process_one_interaction_query(window, cx);
        self.sync_native_connection_state(window, cx);
        let frame = match self.view.frame(self.frame_request()) {
            Ok(frame) => frame,
            Err(FrameMaterializationError::Session(ViewSessionError::Closed { reason })) => {
                self.finish_terminal_render_cleanup(reason, window, cx);
                return self.terminal_scene(host_id, focus_handle);
            }
            Err(error) => return self.frame_failure(error.to_string(), cx),
        };
        self.record_frame_materialization_overflow(frame.materialization_overflow());
        let collector = frame.collector();
        let command_batch = match self.commands.collect_next_batch(&collector) {
            Ok(batch) => batch,
            Err(error) => {
                return self.frame_failure(
                    format!("failed to collect native command batch: {error}"),
                    cx,
                );
            }
        };
        let presentation_offsets = self.interaction_node_drag_offsets();
        let measurements = Rc::new(OpenGpuiPendingMeasurements::new(
            frame.nodes(),
            frame.transform(),
            presentation_offsets,
            self.adapter_environment_epoch,
        ));
        self.reconcile_frame(
            frame.nodes(),
            frame.document_epoch(),
            frame.transform().zoom,
            &measurements,
            window,
            cx,
        );
        if let Err(error) = self.sync_binding_presentations(window, cx) {
            self.record_feedback_failure(None, error.to_string());
        }
        self.interactions.schedule_pending_drain(window, cx);
        let wires = match self.scene_wires_with_offsets(measurements.presentation_offsets()) {
            Ok(wires) => wires,
            Err(ViewSessionError::Closed { reason }) => {
                self.finish_terminal_render_cleanup(reason, window, cx);
                return self.terminal_scene(host_id, focus_handle);
            }
            Err(error) => {
                let message = error.to_string();
                let mut resolutions = Vec::new();
                let mut batch_failure = None;
                if let Some(batch) = command_batch {
                    match self.commands.fail_batch(batch, || {
                        OpenGpuiCommandTicketError::FramePreparation {
                            message: message.clone(),
                        }
                    }) {
                        Ok(completed) => {
                            resolutions.extend(self.handle_completed_commands(completed));
                        }
                        Err(error) => batch_failure = Some(error),
                    }
                }
                cx.defer(move |_| resolve_command_tickets(resolutions));
                let message = batch_failure.map_or(message.clone(), |error| {
                    format!("{message}; failed to settle native command batch: {error}")
                });
                return self.frame_failure(message, cx);
            }
        };
        if self.last_edge_layer_overflow.query_pending() {
            let host_id = cx.entity_id();
            window.on_next_frame(move |_, cx| cx.notify(host_id));
            window.refresh();
        }
        self.render_scene(
            Rc::new(DeferredFrameFeedback::new(
                collector,
                measurements,
                Rc::clone(&self.commands),
                command_batch,
            )),
            frame.transform(),
            focus_handle,
            wires,
            window,
            cx,
        )
    }
}

#[cfg(test)]
#[path = "host_tests.rs"]
mod tests;
