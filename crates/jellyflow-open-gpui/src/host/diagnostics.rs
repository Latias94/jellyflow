use jellyflow::core::{NodeId, PortDirection};
use jellyflow::runtime::{
    FrameMaterializationOverflow, FrameNode, LayoutNodeStatus, NodeInstanceKey,
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

/// Evidence from the most recent published edge-layer admission pass.
///
/// Viewport culling is expected work reduction. Scan truncation reports bounded uncertainty about
/// the unexamined snapshot suffix; `budget_omitted` reports inspected visible routes that could not
/// enter the native paint budget.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OpenGpuiEdgeLayerOverflow {
    pub(super) selected_ids_examined: usize,
    pub(super) selected_ids_unexamined: usize,
    pub(super) published_route_entries_examined: usize,
    pub(super) published_routes_examined: usize,
    pub(super) published_routes_unexamined: usize,
    pub(super) scan_truncated: bool,
    pub(super) viewport_culled: usize,
    pub(super) budget_omitted: usize,
}

impl OpenGpuiEdgeLayerOverflow {
    /// Number of selected edge IDs inspected through direct route lookup.
    #[must_use]
    pub const fn selected_ids_examined(self) -> usize {
        self.selected_ids_examined
    }

    /// Exact number of selected edge IDs not inspected after exhausting the shared scan limit.
    #[must_use]
    pub const fn selected_ids_unexamined(self) -> usize {
        self.selected_ids_unexamined
    }

    /// Number of entries consumed from the deterministic published-route map prefix.
    #[must_use]
    pub const fn published_route_entries_examined(self) -> usize {
        self.published_route_entries_examined
    }

    /// Total selected IDs plus published route entries charged to the scan limit.
    #[must_use]
    pub const fn scan_inputs_examined(self) -> usize {
        self.selected_ids_examined
            .saturating_add(self.published_route_entries_examined)
    }

    /// Number of unique published routes inspected through either bounded input stream.
    #[must_use]
    pub const fn published_routes_examined(self) -> usize {
        self.published_routes_examined
    }

    /// Exact number of published routes whose facts were not inspected by this pass.
    #[must_use]
    pub const fn published_routes_unexamined(self) -> usize {
        self.published_routes_unexamined
    }

    /// Whether selected IDs or published route facts remained after the scan limit was exhausted.
    #[must_use]
    pub const fn scan_truncated(self) -> bool {
        self.scan_truncated
    }

    /// Number of inspected published routes conservatively rejected outside the canvas viewport.
    ///
    /// This is a lower bound for the complete snapshot when [`Self::scan_truncated`] is true.
    #[must_use]
    pub const fn viewport_culled(self) -> usize {
        self.viewport_culled
    }

    /// Number of inspected viewport-intersecting routes omitted by the fixed paint budget.
    ///
    /// This is a lower bound for the complete snapshot when [`Self::scan_truncated`] is true.
    #[must_use]
    pub const fn budget_omitted(self) -> usize {
        self.budget_omitted
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        !self.scan_truncated && self.budget_omitted == 0
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
    pub(super) binding_snapshot_builds: u64,
    pub(super) binding_reconciliation_visits: u64,
    pub(super) binding_updates: u64,
    pub(super) binding_notifications: u64,
    pub(super) protected_instances: usize,
    pub(super) protected_leases: usize,
    pub(super) protected_high_water: usize,
    pub(super) protected_overflows: u64,
    pub(super) eligible_parked_nodes: usize,
    pub(super) eligible_lru_entries: usize,
    pub(super) strong_instances: usize,
    pub(super) strong_instance_high_water: usize,
    pub(super) frame_materialization_overflows: u64,
    pub(super) edge_layer_plans: u64,
    pub(super) edge_route_scan_inputs_examined: u64,
    pub(super) edge_selected_ids_examined: u64,
    pub(super) edge_route_entries_examined: u64,
    pub(super) edge_routes_examined: u64,
    pub(super) edge_routes_unexamined: u64,
    pub(super) edge_route_scan_truncations: u64,
    pub(super) edge_routes_viewport_culled: u64,
    pub(super) edge_routes_planned: u64,
    pub(super) edge_routes_budget_omitted: u64,
    pub(super) edge_layer_overflows: u64,
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
    pub fn binding_snapshot_builds(self) -> u64 {
        self.binding_snapshot_builds
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

    /// Number of rendered frames that exceeded at least one materialization budget.
    #[must_use]
    pub fn frame_materialization_overflows(self) -> u64 {
        self.frame_materialization_overflows
    }

    /// Number of successful published edge-layer admission passes.
    #[must_use]
    pub fn edge_layer_plans(self) -> u64 {
        self.edge_layer_plans
    }

    /// Number of selected IDs plus published route entries charged to edge scan limits.
    #[must_use]
    pub fn edge_route_scan_inputs_examined(self) -> u64 {
        self.edge_route_scan_inputs_examined
    }

    #[must_use]
    pub fn edge_selected_ids_examined(self) -> u64 {
        self.edge_selected_ids_examined
    }

    #[must_use]
    pub fn edge_route_entries_examined(self) -> u64 {
        self.edge_route_entries_examined
    }

    /// Number of published routes examined by edge-layer viewport admission.
    #[must_use]
    pub fn edge_routes_examined(self) -> u64 {
        self.edge_routes_examined
    }

    /// Sum of exact per-pass counts of published routes whose facts were not inspected.
    #[must_use]
    pub fn edge_routes_unexamined(self) -> u64 {
        self.edge_routes_unexamined
    }

    #[must_use]
    pub fn edge_route_scan_truncations(self) -> u64 {
        self.edge_route_scan_truncations
    }

    /// Number of published routes rejected outside the canvas viewport.
    #[must_use]
    pub fn edge_routes_viewport_culled(self) -> u64 {
        self.edge_routes_viewport_culled
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

    /// Number of edge-layer passes that truncated scanning or omitted an inspected visible route.
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
        if let Some(binding) = &self.binding {
            let binding = binding.read(cx);
            if let Ok(snapshot) = binding.snapshot().read() {
                let handles = binding.port_handles(&snapshot).unwrap_or_default();
                for (port, handle) in snapshot.ports().zip(handles) {
                    let marker = handle
                        .interactive(
                            div()
                                .w(px(10.0))
                                .h(px(10.0))
                                .rounded_full()
                                .bg(open_gpui::rgb(0x64748b)),
                            |_, _, _| {},
                        )
                        .into_any_element();
                    match port.port().dir {
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

        diagnostic_semantic_element(
            div().id(("jellyflow-diagnostic-fallback", cx.entity_id())),
            format!("{}: {}", self.title, self.detail),
        )
        .relative()
        .size_full()
        .p(px(8.0))
        .flex()
        .flex_col()
        .child(self.title.clone())
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
            ..self.stats
        }
    }

    #[must_use]
    pub fn last_diagnostic(&self) -> Option<&OpenGpuiSceneHostDiagnostic> {
        self.last_diagnostic.as_ref()
    }

    /// Returns overflow evidence from the most recently materialized frame.
    #[must_use]
    pub const fn last_frame_materialization_overflow(&self) -> FrameMaterializationOverflow {
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

    pub(super) fn record_frame_materialization_overflow(
        &mut self,
        overflow: FrameMaterializationOverflow,
    ) {
        self.last_frame_materialization_overflow = overflow;
        if overflow.is_empty() {
            return;
        }
        self.stats.frame_materialization_overflows =
            self.stats.frame_materialization_overflows.saturating_add(1);
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
