mod connection;
mod content_size_submission_owner;
mod diagnostics;
mod edge_actions;
mod edge_layer;
mod edge_semantics;
mod endpoint_resolver;
mod focus_navigation;
mod frame_feedback;
mod interaction_session;
mod lifecycle;
mod native_frame_wake;
pub(crate) mod native_input;
mod node_lifecycle;
mod presentation;
mod retained_mount_store;
mod retained_node_owner;
mod scene_gesture;
mod scene_render;
#[cfg(feature = "test-support")]
pub(crate) mod test_support;

pub use self::connection::{OpenGpuiConnectionSearchError, OpenGpuiConnectionSearchSubmission};
pub use self::diagnostics::{
    OpenGpuiEdgeLayerOverflow, OpenGpuiSceneHostDiagnostic, OpenGpuiSceneHostDiagnosticKind,
    OpenGpuiSceneHostResourceSnapshot,
};
pub use self::lifecycle::OpenGpuiSceneHostCloseError;
#[cfg(feature = "test-support")]
pub use self::test_support::{
    OpenGpuiSmokeNodeBounds, OpenGpuiSmokePortGeometry, OpenGpuiSmokePresentedScene,
    OpenGpuiSmokePreviewRoute, OpenGpuiSmokePreviewTarget, OpenGpuiSmokeRoute,
    resolve_smoke_reconnect_hit_center,
};

use self::content_size_submission_owner::ContentSizeSubmissionOwner;
use self::edge_layer::{OpenGpuiEdgePathCache, OpenGpuiVisibleRouteCache};
use self::focus_navigation::OpenGpuiGraphFocusTarget;
use self::frame_feedback::OpenGpuiFrameTransaction;
#[cfg(test)]
use self::frame_feedback::{LayoutReceiptAction, layout_receipt_action};
#[cfg(test)]
use self::lifecycle::HostLifecycleSignal;
use self::native_input::NativeInputOwner;
use self::retained_node_owner::RetainedNodeOwner;
#[cfg(test)]
use self::scene_render::OPEN_GPUI_SELECTION_RECTANGLE_SELECTOR;

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;
use std::sync::Arc;

use jellyflow_core::{CanvasPoint, CanvasRect, CanvasSize, NodeGraphConnectionMode};
use jellyflow_runtime::viewport::{ViewportPointerSample, ViewportTransform};
use jellyflow_runtime::{
    geometry::ViewPoint, geometry::ViewRect, geometry::ViewSize,
    interaction::CanvasInteractionError, interaction::CanvasInteractionPhase,
    interaction::CanvasInteractionSession, interaction::fit_viewport, view::AdapterEnvironment,
    view::FrameMaterializationBudget, view::FrameMaterializationError,
    view::FrameMaterializationOverflow, view::FrameRequest, view::RendererStamp,
    view::ViewCloseReason, view::ViewSession, view::ViewSessionError,
};
use open_gpui::{
    App, Context, FocusHandle, IntoElement, Modifiers, MouseButton, Pixels, Render, Subscription,
    Window, WindowId,
};

use self::lifecycle::HostLifecycleBridge;
use self::scene_render::OpenGpuiPreparedEdgeCache;
#[cfg(feature = "test-support")]
use self::test_support::OpenGpuiSmokePresentedSceneOwner;

use crate::chrome::OpenGpuiEdgePainterRegistry;
use crate::command_journal::{OpenGpuiCommandJournal, resolve_command_tickets};
use crate::interaction::OpenGpuiOverlayInteractionBoundary;
use crate::navigation::OpenGpuiNavigationConfig;
use crate::panic_sink::OpenGpuiPanicAccumulator;
use crate::port_handle::OpenGpuiPendingMeasurements;
use crate::registry::OpenGpuiNodeRegistry;
use crate::theme::OpenGpuiGraphTheme;

/// Requests a platform-backed redraw for test-support real-window drivers.
#[cfg(feature = "test-support")]
#[doc(hidden)]
pub fn request_native_test_frame(window: &mut Window) {
    #[cfg(target_os = "macos")]
    {
        if let Some(wake) = native_frame_wake::prepare(window) {
            wake.request();
        }
    }
    #[cfg(not(target_os = "macos"))]
    window.refresh();
}

#[cfg(test)]
use crate::binding::OpenGpuiNodeBinding;
#[cfg(test)]
use crate::command_journal::OpenGpuiCommandOrigin;
#[cfg(test)]
use crate::interaction::OpenGpuiSceneGestureCancelReason;

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

/// Failure to bind a native host to its exact view and renderer configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum OpenGpuiSceneHostInitError {
    /// The renderer-neutral interaction session could not attach to the supplied view.
    #[error(transparent)]
    Interaction(#[from] CanvasInteractionError),
}

/// Adapter-local policy for native connection and reconnect gestures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenGpuiConnectionOptions {
    /// Renderer-neutral validation mode used by every connection gesture.
    pub mode: NodeGraphConnectionMode,
    /// Whether releasing on empty canvas may enter application-provided create-from-wire search.
    pub allow_create_from_wire: bool,
}

/// One application-owned context-menu request emitted from verified blank canvas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpenGpuiBlankCanvasContextRequest {
    position: CanvasPoint,
    button: MouseButton,
    modifiers: Modifiers,
}

impl OpenGpuiBlankCanvasContextRequest {
    #[must_use]
    pub const fn position(self) -> CanvasPoint {
        self.position
    }

    #[must_use]
    pub const fn button(self) -> MouseButton {
        self.button
    }

    #[must_use]
    pub const fn modifiers(self) -> Modifiers {
        self.modifiers
    }
}

/// Application callback for blank-canvas catalog, quick-add, or context-menu presentation.
pub type OpenGpuiBlankCanvasContextHandler =
    Rc<dyn Fn(OpenGpuiBlankCanvasContextRequest, &mut Window, &mut App) + 'static>;

struct PendingBlankCanvasContext {
    handler: OpenGpuiBlankCanvasContextHandler,
    request: OpenGpuiBlankCanvasContextRequest,
    sample: ViewportPointerSample,
    waits_for_scene_release: bool,
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

/// Retained Open GPUI scene host backed by one headless [`ViewSession`].
///
/// The first render permanently binds the entity to that window. Releasing the final entity handle
/// then closes the view and releases native focus and pointer capture before the host is dropped.
///
/// The host deliberately does not expose its underlying view capability. Native node commands
/// must enter through the mount-scoped authoring API so journal ordering, ticket settlement, and
/// host invalidation cannot be bypassed.
///
/// ```compile_fail
/// use jellyflow_open_gpui::host::OpenGpuiSceneHost;
///
/// fn bypass_host_journal(host: &OpenGpuiSceneHost) {
///     let _ = host.view_handle();
/// }
/// ```
pub struct OpenGpuiSceneHost {
    view: ViewSession,
    commands: Rc<OpenGpuiCommandJournal>,
    frame_transaction: Option<Rc<OpenGpuiFrameTransaction>>,
    materialized_frame_instances: BTreeSet<jellyflow_runtime::view::NodeInstanceKey>,
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
    blank_canvas_context_handler: Option<OpenGpuiBlankCanvasContextHandler>,
    pending_blank_canvas_context: Option<PendingBlankCanvasContext>,
    navigation: OpenGpuiNavigationConfig,
    initial_viewport_policy: InitialViewportPolicy,
    graph_theme: OpenGpuiGraphTheme,
    edge_painters: Arc<OpenGpuiEdgePainterRegistry>,
    retained_nodes: RetainedNodeOwner,
    interaction_owner: interaction_session::OpenGpuiNativeInteractionOwner,
    native_input: NativeInputOwner,
    content_sizes: ContentSizeSubmissionOwner,
    scene_origin: Option<open_gpui::Point<Pixels>>,
    scene_transform: Option<ViewportTransform>,
    terminal_reason: Option<ViewCloseReason>,
    pending_terminal_panics: OpenGpuiPanicAccumulator,
    window_owner: Option<OpenGpuiSceneHostWindowOwner>,
    lifecycle_bridge: Option<HostLifecycleBridge>,
    #[cfg(test)]
    scene_wires_fault: Option<ViewSessionError>,
    #[cfg(test)]
    frame_submission_fault: bool,
    #[cfg(test)]
    prepaint_close_fault: bool,
    last_diagnostic: Option<OpenGpuiSceneHostDiagnostic>,
    last_frame_materialization_overflow: Option<FrameMaterializationOverflow>,
    unobserved_frame_materialization_overflow: Option<FrameMaterializationOverflow>,
    last_edge_layer_overflow: OpenGpuiEdgeLayerOverflow,
    #[cfg(feature = "test-support")]
    smoke_presented_scene: OpenGpuiSmokePresentedSceneOwner,
}

/// One-shot policy for initializing a measured native surface viewport.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum InitialViewportPolicy {
    /// Keep the attached view's current viewport exactly as supplied.
    #[default]
    Preserve,
    /// Fit once after the first successful frame publishes fresh measured node geometry.
    FitOnFirstMeasuredGeometry,
}

struct OpenGpuiSceneHostWindowOwner {
    window: WindowId,
    _release_subscription: Subscription,
    _key_interceptor: Subscription,
}

fn registry_frame_request(registry: &OpenGpuiNodeRegistry, viewport: CanvasRect) -> FrameRequest {
    let mut request = FrameRequest::new(viewport, RendererStamp::new(DIAGNOSTIC_RENDERER_KEY, 1));
    for (kind, kind_version, renderer) in registry.mappings() {
        request = request.with_kind_renderer(kind.clone(), kind_version, renderer.clone());
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
    .with_required_node_scan_limit(
        configured
            .required_node_scan_limit()
            .max(protected_required_nodes),
    )
    .with_routing_dependency_scan_limit(configured.routing_dependency_scan_limit())
    .with_port_scan_limit(configured.port_scan_limit())
}

impl OpenGpuiSceneHost {
    /// Returns a capability that gives an application-owned overlay precedence over graph input.
    #[must_use]
    pub fn overlay_boundary(&self) -> OpenGpuiOverlayInteractionBoundary {
        self.interaction_owner.overlay_boundary()
    }

    /// Uses an application-owned focus handle as the graph root authority.
    ///
    /// Wrappers that host overlays or connection search can retain this handle for focus
    /// restoration. The scene tracks it; callers must not register the same handle twice.
    #[must_use]
    pub fn with_graph_focus_handle(mut self, focus: FocusHandle) -> Self {
        let _ = self.interaction_owner.set_focus_handle(focus);
        self
    }

    pub fn new(
        view: ViewSession,
        registry: OpenGpuiNodeRegistry,
        viewport: CanvasRect,
    ) -> Result<Self, OpenGpuiSceneHostInitError> {
        let frame_request_template = registry_frame_request(&registry, viewport);
        let navigation = OpenGpuiNavigationConfig::default();
        let canvas_interaction =
            CanvasInteractionSession::attach(&view, navigation.runtime_config())?;
        Ok(Self {
            view,
            commands: OpenGpuiCommandJournal::new(),
            frame_transaction: None,
            materialized_frame_instances: BTreeSet::new(),
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
            blank_canvas_context_handler: None,
            pending_blank_canvas_context: None,
            navigation,
            initial_viewport_policy: InitialViewportPolicy::default(),
            graph_theme: OpenGpuiGraphTheme::default(),
            edge_painters: Arc::new(OpenGpuiEdgePainterRegistry::default()),
            retained_nodes: RetainedNodeOwner::new(
                OPEN_GPUI_DEFAULT_ELIGIBLE_PARKED_CAPACITY,
                OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY,
            ),
            interaction_owner: interaction_session::OpenGpuiNativeInteractionOwner::new(
                canvas_interaction,
            ),
            native_input: NativeInputOwner::new(),
            content_sizes: ContentSizeSubmissionOwner::default(),
            scene_origin: None,
            scene_transform: None,
            terminal_reason: None,
            pending_terminal_panics: OpenGpuiPanicAccumulator::default(),
            window_owner: None,
            lifecycle_bridge: None,
            #[cfg(test)]
            scene_wires_fault: None,
            #[cfg(test)]
            frame_submission_fault: false,
            #[cfg(test)]
            prepaint_close_fault: false,
            last_diagnostic: None,
            last_frame_materialization_overflow: None,
            unobserved_frame_materialization_overflow: None,
            last_edge_layer_overflow: OpenGpuiEdgeLayerOverflow::default(),
            #[cfg(feature = "test-support")]
            smoke_presented_scene: OpenGpuiSmokePresentedSceneOwner::default(),
        })
    }

    /// Creates a host whose first frame only bootstraps native surface measurement.
    pub fn new_measured_surface(
        view: ViewSession,
        registry: OpenGpuiNodeRegistry,
    ) -> Result<Self, OpenGpuiSceneHostInitError> {
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
        )?;
        host.tracks_surface_viewport = true;
        Ok(host)
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

    /// Installs an application-owned blank-canvas context-menu request handler before mounting.
    #[must_use]
    pub fn with_blank_canvas_context_handler(
        mut self,
        handler: impl Fn(OpenGpuiBlankCanvasContextRequest, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.blank_canvas_context_handler = Some(Rc::new(handler));
        self
    }

    /// Replaces the application-owned blank-canvas context-menu request handler.
    pub fn set_blank_canvas_context_handler(
        &mut self,
        handler: Option<OpenGpuiBlankCanvasContextHandler>,
        cx: &mut Context<Self>,
    ) {
        self.pending_blank_canvas_context = None;
        self.blank_canvas_context_handler = handler;
        cx.notify();
    }

    /// Overrides adapter-local navigation before mounting the host.
    #[must_use]
    pub fn with_navigation_config(mut self, config: OpenGpuiNavigationConfig) -> Self {
        let config = config.sanitized();
        self.interaction_owner
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
        let runtime_changed = self.interaction_owner.set_config(config.runtime_config())?;
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

    /// Selects a one-shot initial viewport policy before the host is mounted.
    #[must_use]
    pub fn with_initial_viewport_policy(mut self, policy: InitialViewportPolicy) -> Self {
        self.initial_viewport_policy = policy;
        self
    }

    /// Replaces the pending initial viewport policy.
    ///
    /// Runtime ownership is authoritative: enabling fit after a persisted or user viewport write
    /// cannot restore an already consumed initial claim.
    pub fn set_initial_viewport_policy(&mut self, policy: InitialViewportPolicy) -> bool {
        if self.initial_viewport_policy == policy {
            return false;
        }
        self.initial_viewport_policy = policy;
        true
    }

    #[must_use]
    pub const fn initial_viewport_policy(&self) -> InitialViewportPolicy {
        self.initial_viewport_policy
    }

    /// Fits every visible node with canonical size into the measured native surface.
    ///
    /// Hidden nodes and nodes without a document size are ignored. An empty fit set, an unmeasured
    /// surface, or an active graph gesture is a deterministic no-op.
    pub fn fit_view(&mut self, cx: &mut Context<Self>) -> Result<bool, ViewSessionError> {
        if self.interaction_owner.presentation().phase() != CanvasInteractionPhase::Idle {
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
                let size = node.size?;
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
        self.view.set_viewport(transform)?;
        self.scene_transform = Some(transform);
        self.update_surface_viewport(surface, transform);
        cx.notify();
        Ok(true)
    }

    pub(super) fn apply_initial_viewport_policy(&mut self) -> Result<bool, ViewSessionError> {
        if self.initial_viewport_policy != InitialViewportPolicy::FitOnFirstMeasuredGeometry
            || self.interaction_owner.presentation().phase() != CanvasInteractionPhase::Idle
        {
            return Ok(false);
        }
        let Some(surface) = self.surface_size else {
            return Ok(false);
        };
        let expected = self.view.viewport_snapshot()?;
        let graph = self.view.graph_snapshot()?;
        let geometry = self.view.geometry_snapshot()?;
        let mut saw_fresh_measurement = false;
        let bounds = graph
            .graph()
            .nodes()
            .into_iter()
            .filter_map(|(node_id, node)| {
                if node.hidden {
                    return None;
                }
                let measured = geometry.node(*node_id).filter(|published| {
                    published.is_fresh()
                        && published.bounds().is_positive_finite()
                        && graph.node_lifetime_key(*node_id) == Some(published.lifetime())
                });
                if let Some(measured) = measured {
                    saw_fresh_measurement = true;
                    return Some(measured.bounds());
                }
                let size = node.size?;
                node.bounds_for_size(size)
            })
            .reduce(union_canvas_rects);
        if !saw_fresh_measurement {
            return Ok(false);
        }
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
        let claimed = self.view.claim_initial_viewport(expected, transform)?;
        self.initial_viewport_policy = InitialViewportPolicy::Preserve;
        let Some(claimed) = claimed else {
            return Ok(false);
        };
        self.scene_transform = Some(claimed.transform());
        self.update_surface_viewport(surface, claimed.transform());
        Ok(claimed.transform() != expected.transform())
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
            let effects = self.content_sizes.advance_environment();
            self.apply_content_size_effects(effects);
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

    fn current_scene_viewport(&self) -> Result<OpenGpuiCurrentSceneViewport, ViewSessionError> {
        let transform = self.view.viewport_transform()?;
        resolve_current_scene_viewport(
            self.surface_size,
            self.viewport,
            self.tracks_surface_viewport,
            transform,
        )
        .ok_or(ViewSessionError::InvalidViewport)
    }

    fn frame_request(&self) -> FrameRequest {
        let viewport = self
            .current_scene_viewport()
            .map(|current| current.canvas_bounds)
            .unwrap_or(self.viewport);
        let mut protected_required_nodes = self.retained_nodes.protected_required_nodes();
        protected_required_nodes.extend(self.content_sizes.awaiting_publication_nodes());
        if let Ok(snapshot) = self.view.graph_snapshot()
            && let Ok(node_drag_required) =
                self.interaction_owner.required_node_drag_nodes(&snapshot)
        {
            protected_required_nodes.extend(node_drag_required);
            protected_required_nodes
                .extend(self.interaction_owner.required_connection_nodes(&snapshot));
        }
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

#[derive(Debug, Clone, Copy, PartialEq)]
struct OpenGpuiCurrentSceneViewport {
    transform: ViewportTransform,
    canvas_bounds: CanvasRect,
    query_bounds: ViewRect,
}

fn resolve_current_scene_viewport(
    surface_size: Option<ViewSize>,
    configured_bounds: CanvasRect,
    tracks_surface_viewport: bool,
    transform: ViewportTransform,
) -> Option<OpenGpuiCurrentSceneViewport> {
    if tracks_surface_viewport
        && let Some(size) = surface_size.filter(|size| size.is_positive_finite())
    {
        let query_bounds = ViewRect::new(ViewPoint::default(), size);
        return Some(OpenGpuiCurrentSceneViewport {
            transform,
            canvas_bounds: query_bounds.to_canvas_rect(transform)?,
            query_bounds,
        });
    }

    Some(OpenGpuiCurrentSceneViewport {
        transform,
        canvas_bounds: configured_bounds,
        query_bounds: view_bounds_for_canvas(configured_bounds, transform)?,
    })
}

fn view_bounds_for_canvas(
    canvas_bounds: CanvasRect,
    transform: ViewportTransform,
) -> Option<ViewRect> {
    if !canvas_bounds.is_positive_finite() || !transform.is_valid() {
        return None;
    }
    let origin = transform.screen_point_for_canvas(canvas_bounds.origin)?;
    let bounds = ViewRect::new(
        ViewPoint::new(origin.x, origin.y),
        ViewSize::new(
            canvas_bounds.size.width * transform.zoom,
            canvas_bounds.size.height * transform.zoom,
        ),
    );
    bounds.is_positive_finite().then_some(bounds)
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
        self.ensure_window_owner(window, cx);
        self.ensure_lifecycle_bridge(window, cx);
        if let Some(reason) = self.close_reason() {
            self.interaction_owner.clear_space_pan();
            self.finish_terminal_render_cleanup(reason, window, cx);
            return self.terminal_scene(host_id);
        }
        self.advance_ready_interaction_tick(window, cx);
        if self.interaction_owner.prepare_animation_frame_request() {
            window.request_animation_frame();
        }
        let focus_handle = self
            .interaction_owner
            .focus_handle()
            .unwrap_or_else(|| cx.focus_handle().tab_stop(true));
        if !focus_handle.is_focused(window) {
            self.interaction_owner.clear_space_pan();
        }
        self.interaction_owner
            .set_focus_handle(focus_handle.clone());
        self.interaction_owner
            .schedule_pending_native_drain(window, cx);
        if self.observe_content_size_mutation(cx) {
            window.request_animation_frame();
        }
        let superseded = self.abort_frame_transaction(
            "native frame was superseded before prepaint submission".to_owned(),
        );
        let resolutions = self.settle_command_batch(superseded);
        cx.defer(move |_| resolve_command_tickets(resolutions));
        self.retry_pending_interaction_settlement();
        // Retained completion evidence must make progress even when frame materialization below
        // fails. Request the successor before entering the fallible frame path; GPUI coalesces
        // requests for the same window frame.
        if self.interaction_owner.has_pending_settlement() {
            window.request_animation_frame();
        }
        self.process_one_ready_interaction_prefix(window, cx);
        self.process_one_interaction_query(window, cx);
        self.sync_native_connection_state(window, cx);
        self.materialized_frame_instances.clear();
        let frame = match self.view.frame(self.frame_request()) {
            Ok(frame) => frame,
            Err(FrameMaterializationError::Session(ViewSessionError::Closed { reason })) => {
                self.finish_terminal_render_cleanup(reason, window, cx);
                return self.terminal_scene(host_id);
            }
            Err(error) => return self.frame_failure(error.to_string(), cx),
        };
        self.materialized_frame_instances
            .extend(frame.nodes().iter().map(|node| node.instance().clone()));
        self.record_frame_materialization_overflow(frame.materialization_receipt().overflow());
        let collector = frame.collector();
        let presentation_offsets = self.interaction_node_drag_offsets();
        let measurements = Rc::new(OpenGpuiPendingMeasurements::new(
            frame.nodes(),
            frame.transform(),
            presentation_offsets,
            self.content_sizes.environment_epoch(),
        ));
        self.reconcile_frame(
            frame.nodes(),
            frame.graph_revision(),
            frame.document_epoch(),
            frame.transform().zoom,
            &measurements,
            window,
            cx,
        );
        self.process_one_ready_content_size_prefix(cx);
        self.schedule_content_size_prefix(cx);
        let command_batch = match self.commands.collect_next_batch(&collector) {
            Ok(batch) => batch,
            Err(error) => {
                return self.frame_failure(
                    format!("failed to collect native command batch: {error}"),
                    cx,
                );
            }
        };
        let transaction = Rc::new(OpenGpuiFrameTransaction::new(
            collector,
            Rc::clone(&measurements),
            Rc::clone(&self.commands),
            command_batch,
        ));
        self.frame_transaction = Some(Rc::clone(&transaction));
        if let Err(error) =
            self.sync_binding_presentations(measurements.presentation_offsets(), window, cx)
        {
            self.record_feedback_failure(None, error.to_string());
        }
        self.interaction_owner
            .schedule_pending_native_drain(window, cx);
        let focused_edge = match self.interaction_owner.exact_focused_graph_target(window) {
            Some(OpenGpuiGraphFocusTarget::Edge(edge)) => Some(edge),
            _ => None,
        };
        let wires = match self
            .scene_wires_with_offsets(measurements.presentation_offsets(), focused_edge)
        {
            Ok(wires) => wires,
            Err(ViewSessionError::Closed { reason }) => {
                self.finish_terminal_render_cleanup(reason, window, cx);
                return self.terminal_scene(host_id);
            }
            Err(error) => {
                let message = error.to_string();
                let completed = self.abort_frame_transaction(message.clone());
                let batch_failure = completed.as_ref().err().map(ToString::to_string);
                let resolutions = self.settle_command_batch(completed);
                cx.defer(move |_| resolve_command_tickets(resolutions));
                let message = batch_failure.map_or(message.clone(), |error| {
                    format!("{message}; failed to settle native command batch: {error}")
                });
                return self.frame_failure(message, cx);
            }
        };
        if let Some(token) = wires.node_drag_route_handoff {
            transaction.mark_node_drag_route_handoff(token);
        }
        if self.last_edge_layer_overflow.query_pending() {
            let host_id = cx.entity_id();
            window.on_next_frame(move |_, cx| cx.notify(host_id));
            window.refresh();
        }
        self.render_scene(
            transaction,
            frame.graph_revision(),
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
