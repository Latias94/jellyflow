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

/// Current resource ownership retained by one scene host.
///
/// This snapshot deliberately excludes cumulative work counters. Use operation receipts and
/// diagnostics to observe completed work; use this value only to inspect resources that are still
/// owned or scheduled when the snapshot is taken.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OpenGpuiSceneHostResourceSnapshot {
    pub(super) active_nodes: usize,
    pub(super) parked_nodes: usize,
    pub(super) eligible_parked_nodes: usize,
    pub(super) eligible_lru_entries: usize,
    pub(super) strong_instances: usize,
    pub(super) protected_instances: usize,
    pub(super) protected_leases: usize,
    pub(super) interaction_pending_queries: usize,
    pub(super) interaction_submission_pending: bool,
    pub(super) interaction_ready_prefixes: usize,
    pub(super) interaction_tick_scheduled: bool,
    pub(super) interaction_tick_frame_requested: bool,
    pub(super) interaction_query_frame_scheduled: bool,
    pub(super) interaction_query_resolution_active: bool,
    pub(super) interaction_native_tick_wake_active: bool,
    pub(super) interaction_native_query_wake_active: bool,
    pub(super) edge_path_cache_resident: usize,
    pub(super) edge_path_cache_vertices: usize,
    pub(super) edge_path_cache_estimated_bytes: usize,
    pub(super) edge_paint_plan_cache_resident: usize,
    pub(super) edge_marker_plan_cache_resident: usize,
    pub(super) edge_plan_cache_generations: usize,
    pub(super) edge_plan_cache_vertices: usize,
    pub(super) edge_plan_cache_estimated_bytes: usize,
    pub(super) edge_plan_live_vertices: usize,
    pub(super) edge_plan_live_estimated_bytes: usize,
}

impl OpenGpuiSceneHostResourceSnapshot {
    #[must_use]
    pub const fn active_nodes(self) -> usize {
        self.active_nodes
    }

    #[must_use]
    pub const fn parked_nodes(self) -> usize {
        self.parked_nodes
    }

    #[must_use]
    pub const fn eligible_parked_nodes(self) -> usize {
        self.eligible_parked_nodes
    }

    #[must_use]
    pub const fn eligible_lru_entries(self) -> usize {
        self.eligible_lru_entries
    }

    #[must_use]
    pub const fn strong_instances(self) -> usize {
        self.strong_instances
    }

    #[must_use]
    pub const fn protected_instances(self) -> usize {
        self.protected_instances
    }

    #[must_use]
    pub const fn protected_leases(self) -> usize {
        self.protected_leases
    }

    #[must_use]
    pub const fn interaction_pending_queries(self) -> usize {
        self.interaction_pending_queries
    }

    #[must_use]
    pub const fn interaction_submission_pending(self) -> bool {
        self.interaction_submission_pending
    }

    #[must_use]
    pub const fn interaction_ready_prefixes(self) -> usize {
        self.interaction_ready_prefixes
    }

    #[must_use]
    pub const fn interaction_tick_scheduled(self) -> bool {
        self.interaction_tick_scheduled
    }

    #[must_use]
    pub const fn interaction_tick_frame_requested(self) -> bool {
        self.interaction_tick_frame_requested
    }

    #[must_use]
    pub const fn interaction_query_frame_scheduled(self) -> bool {
        self.interaction_query_frame_scheduled
    }

    #[must_use]
    pub const fn interaction_query_resolution_active(self) -> bool {
        self.interaction_query_resolution_active
    }

    #[must_use]
    pub const fn interaction_native_tick_wake_active(self) -> bool {
        self.interaction_native_tick_wake_active
    }

    #[must_use]
    pub const fn interaction_native_query_wake_active(self) -> bool {
        self.interaction_native_query_wake_active
    }

    #[must_use]
    pub const fn edge_path_cache_resident(self) -> usize {
        self.edge_path_cache_resident
    }

    #[must_use]
    pub const fn edge_path_cache_vertices(self) -> usize {
        self.edge_path_cache_vertices
    }

    #[must_use]
    pub const fn edge_path_cache_estimated_bytes(self) -> usize {
        self.edge_path_cache_estimated_bytes
    }

    #[must_use]
    pub const fn edge_paint_plan_cache_resident(self) -> usize {
        self.edge_paint_plan_cache_resident
    }

    #[must_use]
    pub const fn edge_marker_plan_cache_resident(self) -> usize {
        self.edge_marker_plan_cache_resident
    }

    /// Distinct transform/plan generations currently retained by paint and marker plans.
    #[must_use]
    pub const fn edge_plan_cache_generations(self) -> usize {
        self.edge_plan_cache_generations
    }

    #[must_use]
    pub const fn edge_plan_cache_vertices(self) -> usize {
        self.edge_plan_cache_vertices
    }

    #[must_use]
    pub const fn edge_plan_cache_estimated_bytes(self) -> usize {
        self.edge_plan_cache_estimated_bytes
    }

    /// Conservative simultaneous vertex ownership for retained native paint plans.
    #[must_use]
    pub const fn edge_plan_live_vertices(self) -> usize {
        self.edge_plan_live_vertices
    }

    /// Conservative simultaneous byte ownership for retained native paint plans.
    #[must_use]
    pub const fn edge_plan_live_estimated_bytes(self) -> usize {
        self.edge_plan_live_estimated_bytes
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
    /// Returns current retained resources and native interaction scheduling state.
    #[must_use]
    pub fn resource_snapshot(&self) -> OpenGpuiSceneHostResourceSnapshot {
        let retained = self.retained_nodes.snapshot();
        let mounts = retained.mounts;
        let activity = retained.activity;
        let interaction = self.interaction_owner.tick_diagnostics();
        let edge_cache = self
            .edge_paths
            .try_borrow()
            .map(|cache| cache.usage())
            .unwrap_or_default();

        OpenGpuiSceneHostResourceSnapshot {
            active_nodes: mounts.active_nodes,
            parked_nodes: mounts.parked_nodes,
            eligible_parked_nodes: mounts.eligible_parked_nodes,
            eligible_lru_entries: mounts.eligible_lru_entries,
            strong_instances: mounts.strong_instances,
            protected_instances: activity.protected_instances,
            protected_leases: activity.protected_leases,
            interaction_pending_queries: interaction.pending_queries,
            interaction_submission_pending: interaction.pending_submission,
            interaction_ready_prefixes: interaction.ready_prefixes,
            interaction_tick_scheduled: interaction.tick_scheduled,
            interaction_tick_frame_requested: interaction.tick_frame_requested,
            interaction_query_frame_scheduled: interaction.query_frame_scheduled,
            interaction_query_resolution_active: interaction.query_resolution_active,
            interaction_native_tick_wake_active: interaction.native_tick_wake_active,
            interaction_native_query_wake_active: interaction.native_query_wake_active,
            edge_path_cache_resident: edge_cache.resident_paths,
            edge_path_cache_vertices: edge_cache.path_footprint.vertices,
            edge_path_cache_estimated_bytes: edge_cache.path_footprint.estimated_bytes,
            edge_paint_plan_cache_resident: edge_cache.resident_paint_plans,
            edge_marker_plan_cache_resident: edge_cache.resident_marker_plans,
            edge_plan_cache_generations: edge_cache.plan_generations,
            edge_plan_cache_vertices: edge_cache.plan_footprint.vertices,
            edge_plan_cache_estimated_bytes: edge_cache.plan_footprint.estimated_bytes,
            edge_plan_live_vertices: edge_cache.plan_live_footprint.vertices,
            edge_plan_live_estimated_bytes: edge_cache.plan_live_footprint.estimated_bytes,
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

    /// Takes the first non-empty frame overflow published since the previous take.
    ///
    /// This one-shot evidence survives later frames that fit within the budget, so
    /// asynchronous diagnostics cannot miss a short-lived overflow.
    pub fn take_unobserved_frame_materialization_overflow(
        &mut self,
    ) -> Option<FrameMaterializationOverflow> {
        self.unobserved_frame_materialization_overflow.take()
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
        if self.unobserved_frame_materialization_overflow.is_none() {
            self.unobserved_frame_materialization_overflow = Some(overflow);
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
        self.record_feedback_diagnostic(
            Some(node),
            format!("native layout for node {node:?} was rejected: {status:?}"),
        );
    }

    pub(super) fn record_measurement_failure(&mut self, error: OpenGpuiMeasurementError) {
        self.record_feedback_diagnostic(error.node(), error.to_string());
    }

    pub(super) fn record_feedback_failure(&mut self, node: Option<NodeId>, message: String) {
        self.record_feedback_diagnostic(node, message);
    }

    pub(super) fn record_feedback_diagnostic(&mut self, node: Option<NodeId>, message: String) {
        let instance = node
            .and_then(|node| self.retained_nodes.retained_instance(node))
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
