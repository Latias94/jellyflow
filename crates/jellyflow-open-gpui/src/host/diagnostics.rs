use jellyflow_core::{NodeId, PortDirection};
use jellyflow_runtime::{
    view::FrameMaterializationOverflow, view::FrameNode, view::LayoutNodeStatus,
    view::NodeInstanceKey,
};
use open_gpui::{
    AnyElement, AnyView, AppContext, Context, Entity, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Render, Role, StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::accessibility::diagnostic_semantic_element;
use crate::binding::{OpenGpuiNodeBinding, OpenGpuiNodeBindingError};
use crate::port_handle::OpenGpuiMeasurementError;

use super::OpenGpuiSceneHost;

/// Category of the most recent recoverable scene-host failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenGpuiSceneHostDiagnosticKind {
    FrameMaterialization,
    Binding,
    MissingRenderer,
    InvalidRendererRegistration,
    FactoryFailure,
    Feedback,
    EdgePainter,
    EndpointGeometry,
}

/// Recoverable failure retained by the adapter instead of panicking.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenGpuiSceneHostDiagnostic {
    pub(super) kind: OpenGpuiSceneHostDiagnosticKind,
    pub(super) instance: Option<NodeInstanceKey>,
    pub(super) node: Option<NodeId>,
    pub(super) renderer_key: Option<String>,
    pub(super) message: String,
}

impl OpenGpuiSceneHostDiagnostic {
    #[must_use]
    pub fn kind(&self) -> OpenGpuiSceneHostDiagnosticKind {
        self.kind
    }

    #[must_use]
    pub fn instance(&self) -> Option<&NodeInstanceKey> {
        self.instance.as_ref()
    }

    #[must_use]
    pub fn node(&self) -> Option<NodeId> {
        self.node
    }

    #[must_use]
    pub fn renderer_key(&self) -> Option<&str> {
        self.renderer_key.as_deref()
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// Evidence from the most recent indexed visible-route and paint admission pass.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OpenGpuiEdgeLayerOverflow {
    pub(super) query_entries_examined: u64,
    pub(super) query_candidates_matched: u64,
    pub(super) geometry_overflows: usize,
    pub(super) query_pending: bool,
    pub(super) query_budget_exhausted: bool,
    pub(super) reused_previous_plan: bool,
    pub(super) budget_omitted: usize,
}

impl OpenGpuiEdgeLayerOverflow {
    /// Number of spatial-index entries examined by the latest query page.
    #[must_use]
    pub const fn query_entries_examined(self) -> u64 {
        self.query_entries_examined
    }

    /// Number of distinct viewport candidates matched by the latest query page.
    #[must_use]
    pub const fn query_candidates_matched(self) -> u64 {
        self.query_candidates_matched
    }

    #[must_use]
    pub const fn geometry_overflows(self) -> usize {
        self.geometry_overflows
    }

    #[must_use]
    pub const fn query_pending(self) -> bool {
        self.query_pending
    }

    #[must_use]
    pub const fn query_budget_exhausted(self) -> bool {
        self.query_budget_exhausted
    }

    #[must_use]
    pub const fn reused_previous_plan(self) -> bool {
        self.reused_previous_plan
    }

    /// Number of complete visible routes omitted by the independent paint budget.
    #[must_use]
    pub const fn budget_omitted(self) -> usize {
        self.budget_omitted
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        !self.query_pending
            && !self.query_budget_exhausted
            && self.geometry_overflows == 0
            && self.budget_omitted == 0
    }
}

/// Deterministic retained-lifecycle counters for one scene host.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OpenGpuiSceneHostStats {
    pub(super) active_nodes: usize,
    pub(super) parked_nodes: usize,
    pub(super) mounts: u64,
    pub(super) restores: u64,
    pub(super) evictions: u64,
    pub(super) hard_retirements: u64,
    pub(super) feedback_submissions: u64,
    pub(super) feedback_failures: u64,
    pub(super) layout_rejections: u64,
    pub(super) measurement_failures: u64,
    pub(super) binding_mount_initializations: u64,
    pub(super) binding_reconciliation_visits: u64,
    pub(super) binding_updates: u64,
    pub(super) binding_notifications: u64,
    pub(super) binding_presentation_queries: u64,
    pub(super) binding_presentation_materializations: u64,
    pub(super) node_drag_offset_map_builds: u64,
    pub(super) interaction_tick_callback_registrations: u64,
    pub(super) interaction_tick_callback_entries: u64,
    pub(super) interaction_tick_frame_requests: u64,
    pub(super) interaction_callback_epoch: u64,
    pub(super) interaction_pending_queries: usize,
    pub(super) interaction_submission_pending: bool,
    pub(super) interaction_ready_prefixes: usize,
    pub(super) interaction_tick_scheduled: bool,
    pub(super) interaction_tick_frame_requested: bool,
    pub(super) interaction_query_frame_scheduled: bool,
    pub(super) interaction_query_resolution_active: bool,
    pub(super) interaction_native_tick_wake_active: bool,
    pub(super) interaction_native_query_wake_active: bool,
    pub(super) protected_instances: usize,
    pub(super) protected_leases: usize,
    pub(super) protected_high_water: usize,
    pub(super) protected_overflows: u64,
    pub(super) eligible_parked_nodes: usize,
    pub(super) eligible_lru_entries: usize,
    pub(super) strong_instances: usize,
    pub(super) strong_instance_high_water: usize,
    pub(super) edge_layer_plans: u64,
    pub(super) edge_route_list_reuses: u64,
    pub(super) edge_route_query_entries_examined: u64,
    pub(super) edge_route_query_candidates_matched: u64,
    pub(super) edge_route_query_pending: u64,
    pub(super) edge_route_query_budget_exhausted: u64,
    pub(super) edge_route_geometry_overflows: u64,
    pub(super) edge_route_previous_plan_reuses: u64,
    pub(super) edge_routes_planned: u64,
    pub(super) edge_routes_budget_omitted: u64,
    pub(super) edge_layer_overflows: u64,
    pub(super) edge_path_cache_hits: u64,
    pub(super) edge_path_cache_misses: u64,
    pub(super) edge_path_cache_resident: usize,
    pub(super) edge_path_cache_vertices: usize,
    pub(super) edge_path_cache_estimated_bytes: usize,
    pub(super) edge_path_cache_budget_evictions: u64,
    pub(super) edge_path_cache_budget_rejections: u64,
    pub(super) edge_paint_plan_cache_resident: usize,
    pub(super) edge_marker_plan_cache_resident: usize,
    pub(super) edge_plan_cache_generations: usize,
    pub(super) edge_plan_cache_vertices: usize,
    pub(super) edge_plan_cache_estimated_bytes: usize,
    pub(super) edge_plan_cache_budget_rejections: u64,
    pub(super) edge_plan_incomplete_builds: u64,
    pub(super) edge_plan_rejected_items: u64,
    pub(super) edge_plan_build_peak_vertices: usize,
    pub(super) edge_plan_build_peak_estimated_bytes: usize,
    pub(super) edge_plan_live_vertices: usize,
    pub(super) edge_plan_live_estimated_bytes: usize,
    pub(super) edge_plan_live_peak_vertices: usize,
    pub(super) edge_plan_live_peak_estimated_bytes: usize,
    pub(super) edge_painter_fallbacks: u64,
    pub(super) endpoint_geometry_fallbacks: u64,
    pub(super) edge_paint_preparations: u64,
    pub(super) edge_paint_batches: u64,
    pub(super) edge_paths_painted: u64,
    pub(super) edge_paint_plan_reuses: u64,
    pub(super) edge_native_paint_calls: u64,
    pub(super) edge_native_edges_painted: u64,
    pub(super) edge_marker_preparations: u64,
    pub(super) edge_marker_batches: u64,
    pub(super) edge_marker_plan_reuses: u64,
    pub(super) edge_native_marker_paint_calls: u64,
    pub(super) edge_native_markers_painted: u64,
}

impl OpenGpuiSceneHostStats {
    #[must_use]
    pub fn active_nodes(self) -> usize {
        self.active_nodes
    }

    #[must_use]
    pub fn parked_nodes(self) -> usize {
        self.parked_nodes
    }

    #[must_use]
    pub fn mounts(self) -> u64 {
        self.mounts
    }

    #[must_use]
    pub fn restores(self) -> u64 {
        self.restores
    }

    #[must_use]
    pub fn evictions(self) -> u64 {
        self.evictions
    }

    #[must_use]
    pub fn hard_retirements(self) -> u64 {
        self.hard_retirements
    }

    #[must_use]
    pub fn feedback_submissions(self) -> u64 {
        self.feedback_submissions
    }

    #[must_use]
    pub fn feedback_failures(self) -> u64 {
        self.feedback_failures
    }

    /// Number of node layouts explicitly rejected by the headless feedback protocol.
    #[must_use]
    pub fn layout_rejections(self) -> u64 {
        self.layout_rejections
    }

    /// Number of native prepaint or measurement-assembly failures observed by this host.
    #[must_use]
    pub fn measurement_failures(self) -> u64 {
        self.measurement_failures
    }

    #[must_use]
    pub fn binding_mount_initializations(self) -> u64 {
        self.binding_mount_initializations
    }

    /// Number of retained instances reached through affected-node patch lookups.
    #[must_use]
    pub fn binding_reconciliation_visits(self) -> u64 {
        self.binding_reconciliation_visits
    }

    #[must_use]
    pub fn binding_updates(self) -> u64 {
        self.binding_updates
    }

    #[must_use]
    pub fn binding_notifications(self) -> u64 {
        self.binding_notifications
    }

    /// Number of coherent runtime port-summary batches requested for native presentation.
    #[must_use]
    pub fn binding_presentation_queries(self) -> u64 {
        self.binding_presentation_queries
    }

    /// Number of node presentation snapshots materialized after input-signature comparison.
    #[must_use]
    pub fn binding_presentation_materializations(self) -> u64 {
        self.binding_presentation_materializations
    }

    /// Number of per-frame multi-node drag offset maps constructed by the host.
    #[must_use]
    pub fn node_drag_offset_map_builds(self) -> u64 {
        self.node_drag_offset_map_builds
    }

    /// Number of continuous-interaction callbacks registered with the native frame scheduler.
    #[must_use]
    pub fn interaction_tick_callback_registrations(self) -> u64 {
        self.interaction_tick_callback_registrations
    }

    /// Number of registered continuous-interaction callbacks that entered the native frame loop.
    #[must_use]
    pub fn interaction_tick_callback_entries(self) -> u64 {
        self.interaction_tick_callback_entries
    }

    /// Number of deduplicated Open GPUI animation-frame requests made for continuous interactions.
    #[must_use]
    pub fn interaction_tick_frame_requests(self) -> u64 {
        self.interaction_tick_frame_requests
    }

    /// Current semantic callback epoch used to invalidate stale native-frame work.
    #[must_use]
    pub fn interaction_callback_epoch(self) -> u64 {
        self.interaction_callback_epoch
    }

    #[must_use]
    pub fn interaction_pending_queries(self) -> usize {
        self.interaction_pending_queries
    }

    #[must_use]
    pub fn interaction_submission_pending(self) -> bool {
        self.interaction_submission_pending
    }

    #[must_use]
    pub fn interaction_ready_prefixes(self) -> usize {
        self.interaction_ready_prefixes
    }

    /// Whether a continuous-interaction callback token remains scheduled.
    #[must_use]
    pub fn interaction_tick_scheduled(self) -> bool {
        self.interaction_tick_scheduled
    }

    /// Whether the scheduled tick still owns an animation-frame request token.
    #[must_use]
    pub fn interaction_tick_frame_requested(self) -> bool {
        self.interaction_tick_frame_requested
    }

    /// Whether a query callback token remains scheduled for a native frame.
    #[must_use]
    pub fn interaction_query_frame_scheduled(self) -> bool {
        self.interaction_query_frame_scheduled
    }

    #[must_use]
    pub fn interaction_query_resolution_active(self) -> bool {
        self.interaction_query_resolution_active
    }

    #[must_use]
    pub fn interaction_native_tick_wake_active(self) -> bool {
        self.interaction_native_tick_wake_active
    }

    #[must_use]
    pub fn interaction_native_query_wake_active(self) -> bool {
        self.interaction_native_query_wake_active
    }

    #[must_use]
    pub fn protected_instances(self) -> usize {
        self.protected_instances
    }

    #[must_use]
    pub fn protected_leases(self) -> usize {
        self.protected_leases
    }

    #[must_use]
    pub fn protected_high_water(self) -> usize {
        self.protected_high_water
    }

    #[must_use]
    pub fn protected_overflows(self) -> u64 {
        self.protected_overflows
    }

    #[must_use]
    pub fn eligible_parked_nodes(self) -> usize {
        self.eligible_parked_nodes
    }

    #[must_use]
    pub fn eligible_lru_entries(self) -> usize {
        self.eligible_lru_entries
    }

    #[must_use]
    pub fn strong_instances(self) -> usize {
        self.strong_instances
    }

    #[must_use]
    pub fn strong_instance_high_water(self) -> usize {
        self.strong_instance_high_water
    }

    /// Number of successful published edge-layer admission passes.
    #[must_use]
    pub fn edge_layer_plans(self) -> u64 {
        self.edge_layer_plans
    }

    /// Number of route spatial-index entries examined by native visible-route queries.
    #[must_use]
    pub fn edge_route_query_entries_examined(self) -> u64 {
        self.edge_route_query_entries_examined
    }

    /// Number of frames that reused the immutable prepared visible-edge list.
    #[must_use]
    pub fn edge_route_list_reuses(self) -> u64 {
        self.edge_route_list_reuses
    }

    #[must_use]
    pub fn edge_route_query_candidates_matched(self) -> u64 {
        self.edge_route_query_candidates_matched
    }

    #[must_use]
    pub fn edge_route_query_pending(self) -> u64 {
        self.edge_route_query_pending
    }

    #[must_use]
    pub fn edge_route_query_budget_exhausted(self) -> u64 {
        self.edge_route_query_budget_exhausted
    }

    #[must_use]
    pub fn edge_route_geometry_overflows(self) -> u64 {
        self.edge_route_geometry_overflows
    }

    #[must_use]
    pub fn edge_route_previous_plan_reuses(self) -> u64 {
        self.edge_route_previous_plan_reuses
    }

    /// Number of published routes admitted for native path construction.
    #[must_use]
    pub fn edge_routes_planned(self) -> u64 {
        self.edge_routes_planned
    }

    /// Number of visible published routes omitted by the independent edge-layer budget.
    #[must_use]
    pub fn edge_routes_budget_omitted(self) -> u64 {
        self.edge_routes_budget_omitted
    }

    /// Number of native edge paths reused without rebuilding view-space geometry.
    #[must_use]
    pub fn edge_path_cache_hits(self) -> u64 {
        self.edge_path_cache_hits
    }

    /// Number of native edge paths built and inserted into the bounded cache.
    #[must_use]
    pub fn edge_path_cache_misses(self) -> u64 {
        self.edge_path_cache_misses
    }

    /// Number of edge-local native paths currently retained by the host.
    #[must_use]
    pub fn edge_path_cache_resident(self) -> usize {
        self.edge_path_cache_resident
    }

    /// Logical vertices held by the shared edge-local path cache.
    #[must_use]
    pub fn edge_path_cache_vertices(self) -> usize {
        self.edge_path_cache_vertices
    }

    /// Deterministic retained-byte estimate for shared edge-local paths and their indices.
    #[must_use]
    pub fn edge_path_cache_estimated_bytes(self) -> usize {
        self.edge_path_cache_estimated_bytes
    }

    #[must_use]
    pub fn edge_path_cache_budget_evictions(self) -> u64 {
        self.edge_path_cache_budget_evictions
    }

    #[must_use]
    pub fn edge_path_cache_budget_rejections(self) -> u64 {
        self.edge_path_cache_budget_rejections
    }

    #[must_use]
    pub fn edge_paint_plan_cache_resident(self) -> usize {
        self.edge_paint_plan_cache_resident
    }

    #[must_use]
    pub fn edge_marker_plan_cache_resident(self) -> usize {
        self.edge_marker_plan_cache_resident
    }

    /// Distinct transform/plan generations retained across path and marker plans.
    #[must_use]
    pub fn edge_plan_cache_generations(self) -> usize {
        self.edge_plan_cache_generations
    }

    #[must_use]
    pub fn edge_plan_cache_vertices(self) -> usize {
        self.edge_plan_cache_vertices
    }

    #[must_use]
    pub fn edge_plan_cache_estimated_bytes(self) -> usize {
        self.edge_plan_cache_estimated_bytes
    }

    #[must_use]
    pub fn edge_plan_cache_budget_rejections(self) -> u64 {
        self.edge_plan_cache_budget_rejections
    }

    /// Number of paint or marker plans explicitly truncated while they were being built.
    #[must_use]
    pub fn edge_plan_incomplete_builds(self) -> u64 {
        self.edge_plan_incomplete_builds
    }

    /// Number of edge or marker items rejected from incomplete native paint plans.
    #[must_use]
    pub fn edge_plan_rejected_items(self) -> u64 {
        self.edge_plan_rejected_items
    }

    /// Maximum logical vertex count reached by any single current-plan build.
    #[must_use]
    pub fn edge_plan_build_peak_vertices(self) -> usize {
        self.edge_plan_build_peak_vertices
    }

    /// Maximum deterministic byte estimate reached by any single current-plan build.
    #[must_use]
    pub fn edge_plan_build_peak_estimated_bytes(self) -> usize {
        self.edge_plan_build_peak_estimated_bytes
    }

    /// Conservative simultaneous vertex ownership for retained native paint plans.
    ///
    /// This includes the cached plan plus the copies Open GPUI may keep or create while painting.
    #[must_use]
    pub fn edge_plan_live_vertices(self) -> usize {
        self.edge_plan_live_vertices
    }

    /// Conservative simultaneous byte ownership for retained native paint plans.
    #[must_use]
    pub fn edge_plan_live_estimated_bytes(self) -> usize {
        self.edge_plan_live_estimated_bytes
    }

    /// Highest conservative simultaneous native-path vertex ownership observed while building.
    #[must_use]
    pub fn edge_plan_live_peak_vertices(self) -> usize {
        self.edge_plan_live_peak_vertices
    }

    /// Highest conservative simultaneous native-path byte ownership observed while building.
    #[must_use]
    pub fn edge_plan_live_peak_estimated_bytes(self) -> usize {
        self.edge_plan_live_peak_estimated_bytes
    }

    #[must_use]
    pub fn edge_painter_fallbacks(self) -> u64 {
        self.edge_painter_fallbacks
    }

    /// Number of published endpoint disagreements or bounded endpoint fallbacks observed.
    #[must_use]
    pub fn endpoint_geometry_fallbacks(self) -> u64 {
        self.endpoint_geometry_fallbacks
    }

    #[must_use]
    pub fn edge_paint_preparations(self) -> u64 {
        self.edge_paint_preparations
    }

    #[must_use]
    pub fn edge_paint_batches(self) -> u64 {
        self.edge_paint_batches
    }

    #[must_use]
    pub fn edge_paths_painted(self) -> u64 {
        self.edge_paths_painted
    }

    /// Number of native edge batch plans reused without regrouping their route-local items.
    #[must_use]
    pub fn edge_paint_plan_reuses(self) -> u64 {
        self.edge_paint_plan_reuses
    }

    /// Number of native Open GPUI path primitives submitted by edge batch paint closures.
    #[must_use]
    pub fn edge_native_paint_calls(self) -> u64 {
        self.edge_native_paint_calls
    }

    /// Number of logical edge paths represented by native batch paint submissions.
    #[must_use]
    pub fn edge_native_edges_painted(self) -> u64 {
        self.edge_native_edges_painted
    }

    #[must_use]
    pub fn edge_marker_preparations(self) -> u64 {
        self.edge_marker_preparations
    }

    #[must_use]
    pub fn edge_marker_batches(self) -> u64 {
        self.edge_marker_batches
    }

    #[must_use]
    pub fn edge_marker_plan_reuses(self) -> u64 {
        self.edge_marker_plan_reuses
    }

    #[must_use]
    pub fn edge_native_marker_paint_calls(self) -> u64 {
        self.edge_native_marker_paint_calls
    }

    #[must_use]
    pub fn edge_native_markers_painted(self) -> u64 {
        self.edge_native_markers_painted
    }

    /// Native edges are batch-painted and never own retained Open GPUI entities.
    #[must_use]
    pub const fn retained_edge_entities(self) -> usize {
        0
    }

    /// Number of edge-layer passes with an incomplete query, geometry overflow, or paint omission.
    #[must_use]
    pub fn edge_layer_overflows(self) -> u64 {
        self.edge_layer_overflows
    }
}

struct DiagnosticFallback {
    title: String,
    detail: String,
    binding: Option<Entity<OpenGpuiNodeBinding>>,
    retry: Option<DiagnosticRetry>,
}

#[derive(Clone)]
struct DiagnosticRetry {
    host: open_gpui::WeakEntity<OpenGpuiSceneHost>,
    pub(super) instance: NodeInstanceKey,
}

impl Render for DiagnosticFallback {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut inputs = Vec::new();
        let mut outputs = Vec::new();
        let mut drag_handle = None;
        if let Some(binding) = &self.binding {
            let binding = binding.read(cx);
            drag_handle = Some(binding.drag_handle());
            if let Ok(authoring) = binding.authoring() {
                for port in authoring.ports() {
                    let marker = port
                        .handle()
                        .interactive(
                            div()
                                .w(px(10.0))
                                .h(px(10.0))
                                .rounded_full()
                                .bg(open_gpui::rgb(0x64748b)),
                        )
                        .into_any_element();
                    match port.facts().dir {
                        PortDirection::In => inputs.push(marker),
                        PortDirection::Out => outputs.push(marker),
                    }
                }
            }
        }
        let retry = self.retry.clone().map(|retry| {
            let node = retry.instance.node();
            let selector = factory_retry_selector(node);
            let debug_selector = selector.clone();
            div()
                .id(selector)
                .debug_selector(move || debug_selector)
                .role(Role::Button)
                .aria_label("Retry renderer")
                .tab_index(0)
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(move |_, _, cx| {
                    let _ = retry.host.update(cx, |host, cx| {
                        host.retry_mount_instance(&retry.instance, cx);
                    });
                })
                .px(px(8.0))
                .py(px(4.0))
                .child("Retry")
        });
        let title = div()
            .h(px(24.0))
            .flex()
            .items_center()
            .child(self.title.clone());
        let title = match drag_handle {
            Some(handle) => handle.wrap(title).into_any_element(),
            None => title.into_any_element(),
        };

        diagnostic_semantic_element(
            div().id(("jellyflow-diagnostic-fallback", cx.entity_id())),
            format!("{}: {}", self.title, self.detail),
        )
        .relative()
        .size_full()
        .p(px(8.0))
        .flex()
        .flex_col()
        .child(title)
        .child(self.detail.clone())
        .children(retry)
        .child(
            div()
                .absolute()
                .left(px(-5.0))
                .top(px(8.0))
                .bottom(px(8.0))
                .flex()
                .flex_col()
                .justify_around()
                .children(inputs),
        )
        .child(
            div()
                .absolute()
                .right(px(-5.0))
                .top(px(8.0))
                .bottom(px(8.0))
                .flex()
                .flex_col()
                .justify_around()
                .children(outputs),
        )
    }
}

impl OpenGpuiSceneHost {
    #[must_use]
    pub fn stats(&self) -> OpenGpuiSceneHostStats {
        let activity = self.activity_coordinator.stats();
        let mounts = self.mounts.snapshot();
        let interaction_ticks = self.interaction_owner.tick_diagnostics();
        let path_cache = self
            .edge_paths
            .try_borrow()
            .map(|cache| cache.usage())
            .unwrap_or_default();
        let paint_usage = self
            .edge_paths
            .try_borrow()
            .map(|cache| cache.paint_usage())
            .unwrap_or_default();
        OpenGpuiSceneHostStats {
            active_nodes: mounts.active_nodes,
            parked_nodes: mounts.parked_nodes,
            restores: mounts.restores,
            evictions: mounts.evictions,
            hard_retirements: mounts.hard_retirements,
            protected_instances: activity.protected_instances,
            protected_leases: activity.protected_leases,
            protected_high_water: activity.protected_high_water,
            protected_overflows: activity.overflows,
            eligible_parked_nodes: mounts.eligible_parked_nodes,
            eligible_lru_entries: mounts.eligible_lru_entries,
            strong_instances: mounts.strong_instances,
            strong_instance_high_water: mounts.strong_instance_high_water,
            interaction_tick_callback_registrations: interaction_ticks.callback_registrations,
            interaction_tick_callback_entries: interaction_ticks.callback_entries,
            interaction_tick_frame_requests: interaction_ticks.frame_requests,
            interaction_callback_epoch: interaction_ticks.callback_epoch,
            interaction_pending_queries: interaction_ticks.pending_queries,
            interaction_submission_pending: interaction_ticks.pending_submission,
            interaction_ready_prefixes: interaction_ticks.ready_prefixes,
            interaction_tick_scheduled: interaction_ticks.tick_scheduled,
            interaction_tick_frame_requested: interaction_ticks.tick_frame_requested,
            interaction_query_frame_scheduled: interaction_ticks.query_frame_scheduled,
            interaction_query_resolution_active: interaction_ticks.query_resolution_active,
            interaction_native_tick_wake_active: interaction_ticks.native_tick_wake_active,
            interaction_native_query_wake_active: interaction_ticks.native_query_wake_active,
            edge_path_cache_hits: path_cache.path_hits,
            edge_path_cache_misses: path_cache.path_misses,
            edge_path_cache_resident: path_cache.resident_paths,
            edge_path_cache_vertices: path_cache.path_footprint.vertices,
            edge_path_cache_estimated_bytes: path_cache.path_footprint.estimated_bytes,
            edge_path_cache_budget_evictions: path_cache.path_budget_evictions,
            edge_path_cache_budget_rejections: path_cache.path_budget_rejections,
            edge_paint_plan_cache_resident: path_cache.resident_paint_plans,
            edge_marker_plan_cache_resident: path_cache.resident_marker_plans,
            edge_plan_cache_generations: path_cache.plan_generations,
            edge_plan_cache_vertices: path_cache.plan_footprint.vertices,
            edge_plan_cache_estimated_bytes: path_cache.plan_footprint.estimated_bytes,
            edge_plan_cache_budget_rejections: path_cache.plan_budget_rejections,
            edge_plan_incomplete_builds: path_cache.plan_incomplete_builds,
            edge_plan_rejected_items: path_cache.plan_rejected_items,
            edge_plan_build_peak_vertices: path_cache.plan_build_peak_footprint.vertices,
            edge_plan_build_peak_estimated_bytes: path_cache
                .plan_build_peak_footprint
                .estimated_bytes,
            edge_plan_live_vertices: path_cache.plan_live_footprint.vertices,
            edge_plan_live_estimated_bytes: path_cache.plan_live_footprint.estimated_bytes,
            edge_plan_live_peak_vertices: path_cache.plan_live_peak_footprint.vertices,
            edge_plan_live_peak_estimated_bytes: path_cache
                .plan_live_peak_footprint
                .estimated_bytes,
            edge_paint_preparations: paint_usage.paint_preparations,
            edge_paint_batches: paint_usage.paint_batches,
            edge_paths_painted: paint_usage.painted_paths,
            edge_paint_plan_reuses: paint_usage.paint_plan_reuses,
            edge_native_paint_calls: paint_usage.native_paint_calls,
            edge_native_edges_painted: paint_usage.native_edges_painted,
            edge_marker_preparations: paint_usage.marker_preparations,
            edge_marker_batches: paint_usage.marker_batches,
            edge_marker_plan_reuses: paint_usage.marker_plan_reuses,
            edge_native_marker_paint_calls: paint_usage.native_marker_paint_calls,
            edge_native_markers_painted: paint_usage.native_markers_painted,
            ..self.stats
        }
    }

    #[must_use]
    pub fn last_diagnostic(&self) -> Option<&OpenGpuiSceneHostDiagnostic> {
        self.last_diagnostic.as_ref()
    }

    /// Returns overflow evidence from the most recently materialized frame.
    #[must_use]
    pub const fn last_frame_materialization_overflow(
        &self,
    ) -> Option<FrameMaterializationOverflow> {
        self.last_frame_materialization_overflow
    }

    /// Returns edge-layer culling and visible-route overflow evidence from the latest plan.
    #[must_use]
    pub const fn last_edge_layer_overflow(&self) -> OpenGpuiEdgeLayerOverflow {
        self.last_edge_layer_overflow
    }

    pub(super) fn record_command_diagnostic(&mut self, message: String) {
        self.record_feedback_diagnostic(None, message);
    }

    pub(super) fn record_edge_painter_diagnostics(
        &mut self,
        fallbacks: Vec<(Option<String>, String)>,
        total: usize,
    ) {
        if total == 0 {
            return;
        }
        let renderer_key = fallbacks
            .first()
            .and_then(|(renderer_key, _)| renderer_key.clone())
            .filter(|first| {
                fallbacks
                    .iter()
                    .all(|(renderer_key, _)| renderer_key.as_ref() == Some(first))
            });
        let mut messages = fallbacks
            .into_iter()
            .map(|(_, message)| message)
            .collect::<Vec<_>>();
        if messages.len() < total {
            messages.push(format!(
                "{} additional edge painter fallbacks omitted",
                total - messages.len()
            ));
        }
        let message = messages.join("; ");
        let duplicate = self.last_diagnostic.as_ref().is_some_and(|diagnostic| {
            diagnostic.kind == OpenGpuiSceneHostDiagnosticKind::EdgePainter
                && diagnostic.message == message
        });
        if duplicate {
            return;
        }
        self.stats.edge_painter_fallbacks = self
            .stats
            .edge_painter_fallbacks
            .saturating_add(total as u64);
        self.last_diagnostic = Some(OpenGpuiSceneHostDiagnostic {
            kind: OpenGpuiSceneHostDiagnosticKind::EdgePainter,
            instance: None,
            node: None,
            renderer_key,
            message,
        });
    }

    pub(super) fn record_endpoint_geometry_diagnostics(
        &mut self,
        mut messages: Vec<String>,
        total: usize,
    ) {
        if total == 0 {
            return;
        }
        if messages.len() < total {
            messages.push(format!(
                "{} additional endpoint geometry fallbacks omitted",
                total - messages.len()
            ));
        }
        let message = messages.join("; ");
        let duplicate = self.last_diagnostic.as_ref().is_some_and(|diagnostic| {
            diagnostic.kind == OpenGpuiSceneHostDiagnosticKind::EndpointGeometry
                && diagnostic.message == message
        });
        if duplicate {
            return;
        }
        self.stats.endpoint_geometry_fallbacks = self
            .stats
            .endpoint_geometry_fallbacks
            .saturating_add(total as u64);
        self.last_diagnostic = Some(OpenGpuiSceneHostDiagnostic {
            kind: OpenGpuiSceneHostDiagnosticKind::EndpointGeometry,
            instance: None,
            node: None,
            renderer_key: None,
            message,
        });
    }

    pub(super) fn record_frame_materialization_overflow(
        &mut self,
        overflow: FrameMaterializationOverflow,
    ) {
        self.last_frame_materialization_overflow = Some(overflow);
        if overflow.is_empty() {
            return;
        }
        self.last_diagnostic = Some(OpenGpuiSceneHostDiagnostic {
            kind: OpenGpuiSceneHostDiagnosticKind::FrameMaterialization,
            instance: None,
            node: None,
            renderer_key: None,
            message: format!(
                "frame materialization budget overflow: visible={}, required={}, routing={}, incident_scan_truncated={}",
                overflow.visible_nodes(),
                overflow.required_nodes(),
                overflow.routing_dependency_nodes(),
                overflow.incident_scan_truncated(),
            ),
        });
    }

    pub(super) fn record_layout_rejection(&mut self, node: NodeId, status: LayoutNodeStatus) {
        self.stats.layout_rejections = self.stats.layout_rejections.saturating_add(1);
        self.stats.feedback_failures = self.stats.feedback_failures.saturating_add(1);
        self.record_feedback_diagnostic(
            Some(node),
            format!("native layout for node {node:?} was rejected: {status:?}"),
        );
    }

    pub(super) fn record_measurement_failure(&mut self, error: OpenGpuiMeasurementError) {
        self.stats.measurement_failures = self.stats.measurement_failures.saturating_add(1);
        self.stats.feedback_failures = self.stats.feedback_failures.saturating_add(1);
        self.record_feedback_diagnostic(error.node(), error.to_string());
    }

    pub(super) fn record_feedback_failure(&mut self, node: Option<NodeId>, message: String) {
        self.stats.feedback_failures = self.stats.feedback_failures.saturating_add(1);
        self.record_feedback_diagnostic(node, message);
    }

    pub(super) fn record_feedback_diagnostic(&mut self, node: Option<NodeId>, message: String) {
        let instance = node
            .and_then(|node| self.mounts.retained_instance(node))
            .cloned();
        let renderer_key = instance
            .as_ref()
            .map(|instance| instance.renderer().key().to_owned());
        self.last_diagnostic = Some(OpenGpuiSceneHostDiagnostic {
            kind: OpenGpuiSceneHostDiagnosticKind::Feedback,
            instance,
            node,
            renderer_key,
            message,
        });
    }

    pub(super) fn diagnostic_fallback(
        &self,
        diagnostic: &OpenGpuiSceneHostDiagnostic,
        binding: Option<Entity<OpenGpuiNodeBinding>>,
        cx: &mut Context<Self>,
    ) -> AnyView {
        let title = match diagnostic.kind {
            OpenGpuiSceneHostDiagnosticKind::FrameMaterialization => "Scene unavailable",
            OpenGpuiSceneHostDiagnosticKind::Binding => "Node data unavailable",
            OpenGpuiSceneHostDiagnosticKind::MissingRenderer => "Renderer unavailable",
            OpenGpuiSceneHostDiagnosticKind::InvalidRendererRegistration => {
                "Renderer registration invalid"
            }
            OpenGpuiSceneHostDiagnosticKind::FactoryFailure => "Renderer failed",
            OpenGpuiSceneHostDiagnosticKind::Feedback => "Layout feedback failed",
            OpenGpuiSceneHostDiagnosticKind::EdgePainter => "Edge painter fallback",
            OpenGpuiSceneHostDiagnosticKind::EndpointGeometry => "Endpoint geometry fallback",
        };
        let retry = diagnostic
            .instance
            .clone()
            .filter(|_| diagnostic.kind == OpenGpuiSceneHostDiagnosticKind::FactoryFailure)
            .map(|instance| DiagnosticRetry {
                host: cx.weak_entity(),
                instance,
            });
        AnyView::from(cx.new(|_| DiagnosticFallback {
            title: title.to_owned(),
            detail: diagnostic.message.clone(),
            binding,
            retry,
        }))
    }

    pub(super) fn clear_matching_factory_diagnostic(&mut self, frame: &FrameNode) {
        let clears = self.last_diagnostic.as_ref().is_some_and(|diagnostic| {
            diagnostic.kind == OpenGpuiSceneHostDiagnosticKind::FactoryFailure
                && diagnostic.instance.as_ref() == Some(frame.instance())
                && diagnostic.renderer_key() == Some(frame.renderer().key())
        });
        if clears {
            self.last_diagnostic = None;
        }
    }

    pub(super) fn frame_failure(&mut self, message: String, cx: &mut Context<Self>) -> AnyElement {
        let diagnostic = OpenGpuiSceneHostDiagnostic {
            kind: OpenGpuiSceneHostDiagnosticKind::FrameMaterialization,
            instance: None,
            node: None,
            renderer_key: None,
            message,
        };
        let fallback = self.diagnostic_fallback(&diagnostic, None, cx);
        self.last_diagnostic = Some(diagnostic);
        div().size_full().child(fallback).into_any_element()
    }
}

pub(super) fn binding_diagnostic(
    frame: &FrameNode,
    error: OpenGpuiNodeBindingError,
) -> OpenGpuiSceneHostDiagnostic {
    mount_diagnostic(
        frame,
        OpenGpuiSceneHostDiagnosticKind::Binding,
        error.to_string(),
    )
}

pub(super) fn mount_diagnostic(
    frame: &FrameNode,
    kind: OpenGpuiSceneHostDiagnosticKind,
    message: String,
) -> OpenGpuiSceneHostDiagnostic {
    OpenGpuiSceneHostDiagnostic {
        kind,
        instance: Some(frame.instance().clone()),
        node: Some(frame.id()),
        renderer_key: Some(frame.renderer().key().to_owned()),
        message,
    }
}

fn factory_retry_selector(node: NodeId) -> String {
    format!("jellyflow-factory-retry-{}", node.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_geometry_fallback_stats_start_at_zero() {
        assert_eq!(
            OpenGpuiSceneHostStats::default().endpoint_geometry_fallbacks(),
            0
        );
    }
}
