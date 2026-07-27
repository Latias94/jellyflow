use std::collections::BTreeMap;
use std::rc::Rc;

use jellyflow_core::{CanvasRect, NodeId};
use jellyflow_runtime::{
    graph::GraphRevision, interaction::CanvasInteractionPhase, interaction::InteractionOwner,
    view::FrameNode, view::GeometrySnapshot, view::NodeInstanceKey, view::ViewCloseReason,
    view::ViewSessionError,
};
use open_gpui::{AnyView, AppContext, Context, Entity, FocusHandle, Window};

use crate::activity::OpenGpuiNodeActivityRegistry;
use crate::binding::{
    OpenGpuiBindingAdvance, OpenGpuiBindingMount, OpenGpuiLayoutInvalidator,
    OpenGpuiMountCapability, OpenGpuiNodeBinding,
};
use crate::command_journal::OpenGpuiCommandSink;
use crate::interaction::{OpenGpuiPortDragCancelReason, OpenGpuiSceneGestureCancelReason};
use crate::node_drag_handle::OpenGpuiNodeDragHandle;
use crate::panic_sink::OpenGpuiPanicAccumulator;
use crate::port_handle::{
    OpenGpuiGraphPresentationSource, OpenGpuiMeasurementSink, OpenGpuiPendingMeasurements,
};
use crate::registry::{
    OpenGpuiNodeMountOutcome, OpenGpuiNodeOverviewMountOutcome, OpenGpuiNodeRegistrationPolicy,
};
use crate::scale_policy::OpenGpuiNodeScaleState;
use crate::semantic_zoom::{OpenGpuiNodePresentation, OpenGpuiSemanticZoomState};
use crate::size_policy::{OpenGpuiNodeSizeEpoch, OpenGpuiNodeSizeState};

use super::OpenGpuiSceneHost;
use super::diagnostics::{OpenGpuiSceneHostDiagnosticKind, binding_diagnostic, mount_diagnostic};
use super::lifecycle::NodeChanges;
use super::retained_mount_store::{EvictedMount, RetiredMount};

pub(super) struct MountedNode {
    pub(super) view: AnyView,
    content_kind: MountedNodeContentKind,
    pub(super) binding: Option<Entity<OpenGpuiNodeBinding>>,
    pub(super) activity: OpenGpuiNodeActivityRegistry,
    pub(super) measurement: OpenGpuiMeasurementSink,
    pub(super) focus: FocusHandle,
    pub(super) semantic_zoom: OpenGpuiSemanticZoomState,
    pub(super) registration_policy: OpenGpuiNodeRegistrationPolicy,
    pub(super) scale_state: OpenGpuiNodeScaleState,
    pub(super) size_state: OpenGpuiNodeSizeState,
    pub(super) content_epoch: u64,
    binding_revision: jellyflow_runtime::graph::GraphRevision,
    pub(super) bounds: CanvasRect,
    pub(super) accessible_name: String,
    pub(super) selected: bool,
    pub(super) mount: OpenGpuiMountCapability,
}

#[derive(Default)]
struct PendingMountedNodeRetirements {
    mounted_nodes: Vec<MountedNode>,
}

impl PendingMountedNodeRetirements {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            mounted_nodes: Vec::with_capacity(capacity),
        }
    }

    fn push(&mut self, mounted: MountedNode) {
        self.mounted_nodes.push(mounted);
    }

    fn release_pointer_capture_if_owned(&self, window: &mut Window) -> bool {
        self.mounted_nodes
            .iter()
            .any(|mounted| mounted.activity.release_pointer_capture_if_owned(window))
    }

    fn retire(mut self) -> OpenGpuiPanicAccumulator {
        retire_mounted_nodes(std::mem::take(&mut self.mounted_nodes))
    }
}

impl Drop for PendingMountedNodeRetirements {
    fn drop(&mut self) {
        retire_mounted_nodes(std::mem::take(&mut self.mounted_nodes)).suppress();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MountedNodeContentKind {
    Renderer,
    Diagnostic(OpenGpuiSceneHostDiagnosticKind),
}

impl MountedNodeContentKind {
    fn is_retryable_factory_failure(self) -> bool {
        self == Self::Diagnostic(OpenGpuiSceneHostDiagnosticKind::FactoryFailure)
    }
}

impl MountedNode {
    pub(super) fn graph_presentation(&self) -> OpenGpuiNodePresentation {
        OpenGpuiNodePresentation::from_scale_state(self.scale_state)
    }

    pub(super) fn graph_measurement_source(&self) -> OpenGpuiGraphPresentationSource {
        match self.graph_presentation() {
            OpenGpuiNodePresentation::Detail => OpenGpuiGraphPresentationSource::Detail,
            OpenGpuiNodePresentation::Overview => OpenGpuiGraphPresentationSource::Overview,
        }
    }

    pub(super) fn has_interaction_island(&self) -> bool {
        self.graph_presentation() == OpenGpuiNodePresentation::Overview
            && self.activity.requires_interaction_island()
    }

    pub(super) fn retire(self) {
        self.mount.retire();
        self.activity.revoke_all();
    }
}

pub(super) fn retire_mounted_nodes(
    mounted_nodes: impl IntoIterator<Item = MountedNode>,
) -> OpenGpuiPanicAccumulator {
    let mut panics = OpenGpuiPanicAccumulator::default();
    for mounted in mounted_nodes {
        panics.capture(|| mounted.retire());
    }
    panics
}

pub(super) fn suppress_host_panic(panic: Option<Box<dyn std::any::Any + Send + 'static>>) {
    let mut panics = OpenGpuiPanicAccumulator::default();
    panics.push(panic);
    panics.suppress();
}

impl OpenGpuiSceneHost {
    /// Invalidates geometry for the currently retained instance of `node` in this view.
    ///
    /// Returns `false` when the host has not mounted the node or the retained instance has already
    /// been superseded. Other views backed by the same graph runtime are not affected.
    pub fn invalidate_node_layout(
        &mut self,
        node: NodeId,
        cx: &mut Context<Self>,
    ) -> Result<bool, ViewSessionError> {
        let Some(instance) = self.mounts.retained_instance(node).cloned() else {
            return Ok(false);
        };
        let Some(mount) = self.mount_capability(&instance).cloned() else {
            return Ok(false);
        };
        self.invalidate_instance_layout(&instance, &mount, cx)
    }

    /// Retires a renderer-factory fallback so the node is mounted again on the next frame.
    ///
    /// Returns `false` unless the currently retained node content came from a recoverable factory
    /// failure. Missing or invalid registrations, binding failures, healthy renderers, and nodes
    /// that are not retained are deterministic no-ops. A panicking retirement callback is isolated
    /// as a host feedback diagnostic so it cannot roll back the redraw that performs the retry.
    pub fn retry_mount(&mut self, node: NodeId, cx: &mut Context<Self>) -> bool {
        let Some(instance) = self.mounts.retained_instance(node).cloned() else {
            return false;
        };
        self.retry_mount_instance(&instance, cx)
    }

    pub(super) fn retry_mount_instance(
        &mut self,
        instance: &NodeInstanceKey,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.mounts.retained_instance(instance.node()) != Some(instance) {
            return false;
        }
        let retryable = self
            .mounts
            .get(instance)
            .is_some_and(|mounted| mounted.content_kind.is_retryable_factory_failure());
        if !retryable {
            return false;
        }

        let Some(mounted) = self.detach_instance(instance) else {
            return false;
        };
        let panics = retire_mounted_nodes([mounted]);
        let panicked = !panics.is_empty() || panics.leaked_recursive_sentinel();
        if panicked {
            let leaked_recursive_sentinel = panics.suppress();
            let message = if leaked_recursive_sentinel {
                "factory fallback retirement callback panicked; retry continued and retained one bounded recursive panic payload sentinel"
            } else {
                "factory fallback retirement callback panicked; retry continued"
            };
            self.record_feedback_failure(Some(instance.node()), message.to_owned());
        }
        cx.notify();
        true
    }

    pub(super) fn reconcile_frame(
        &mut self,
        frame: &[FrameNode],
        graph_revision: GraphRevision,
        document_epoch: jellyflow_runtime::graph::DocumentEpoch,
        zoom: f32,
        measurements: &Rc<OpenGpuiPendingMeasurements>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cancel_invalid_node_drag_residency();
        if self
            .document_epoch
            .is_some_and(|current| current != document_epoch)
        {
            self.interaction_owner
                .cancel_scene_deferred(OpenGpuiSceneGestureCancelReason::OwnerRetired);
            self.hard_retire_all(window);
        }
        self.document_epoch = Some(document_epoch);

        let frame_instances = frame
            .iter()
            .map(|node| (node.id(), node.instance()))
            .collect::<BTreeMap<_, _>>();
        let mut retired_nodes = self.retire_stale_parked(&frame_instances);

        let previous_geometry = self.view.geometry_snapshot().ok();
        self.mounts.begin_reconcile();

        for frame_node in frame {
            let key = frame_node.instance();
            let retained = self.mounts.visit_active(key) || self.mounts.restore_active(key);
            if retained {
                let binding_advance = {
                    let mounted = self
                        .mounts
                        .get_mut(key)
                        .expect("a visited retained mount remains active");
                    Self::remember_detail_geometry(
                        mounted,
                        frame_node.id(),
                        previous_geometry.as_ref(),
                    );
                    Self::reconcile_mounted(
                        mounted,
                        frame_node,
                        document_epoch,
                        zoom,
                        &self.catalog,
                    )
                };
                if let Some(binding) = binding_advance {
                    match self.advance_binding_from_frame(&binding, frame_node, cx) {
                        Ok(OpenGpuiBindingAdvance::Applied) => {
                            if let Some(mounted) = self.mounts.get_mut(key) {
                                mounted.binding_revision = frame_node.binding_revision();
                            }
                        }
                        Ok(OpenGpuiBindingAdvance::Retry) => {}
                        Err(error) => {
                            self.last_diagnostic = Some(binding_diagnostic(frame_node, error));
                            if let Some(mounted) = self.detach_instance(key) {
                                retired_nodes.push(mounted);
                            }
                            cx.notify();
                            continue;
                        }
                    }
                }
                let activation = {
                    let mounted = self
                        .mounts
                        .get(key)
                        .expect("a reconciled retained mount remains active");
                    mounted.measurement.activate(
                        measurements,
                        mounted.graph_measurement_source(),
                        mounted.content_epoch,
                    )
                };
                if let Err(error) = activation {
                    self.record_measurement_failure(error);
                }
                continue;
            }

            let mounted = self.mount_node(frame_node, document_epoch, zoom, window, cx);
            let activation = mounted.measurement.activate(
                measurements,
                mounted.graph_measurement_source(),
                mounted.content_epoch,
            );
            if let Err(error) = activation {
                self.record_measurement_failure(error);
            }
            if let Some(evicted) = self.mounts.insert_active(key.clone(), mounted) {
                retired_nodes.push(self.settle_eviction(evicted));
            }
        }

        for (key, mounted) in self.mounts.take_unseen_active() {
            if self.instance_is_retired(&key, &frame_instances) {
                retired_nodes.push(self.hard_retire(&key, mounted));
            } else {
                let protected = mounted.activity.protected_count() > 0;
                if let Some(evicted) = self.mounts.park(key, mounted, protected) {
                    retired_nodes.push(self.settle_eviction(evicted));
                }
            }
        }
        self.mounts
            .refresh_eligibility(|mounted| mounted.activity.protected_count() > 0);
        for evicted in self.mounts.evict_eligible_overflow() {
            retired_nodes.push(self.settle_eviction(evicted));
        }
        self.mounts.finish_reconcile();
        self.interaction_owner.observe_materialized_frame(
            graph_revision,
            frame.iter().map(|node| node.instance().lifetime()),
        );
        retired_nodes.release_pointer_capture_if_owned(window);
        retired_nodes.retire().resume();
    }

    fn remember_detail_geometry(
        mounted: &mut MountedNode,
        node: NodeId,
        geometry: Option<&GeometrySnapshot>,
    ) {
        if mounted.graph_presentation() != OpenGpuiNodePresentation::Detail {
            return;
        }
        if let Some(geometry) = geometry {
            mounted
                .semantic_zoom
                .remember_detail_geometry(node, geometry);
        }
    }

    fn reconcile_mounted(
        mounted: &mut MountedNode,
        frame: &FrameNode,
        document_epoch: jellyflow_runtime::graph::DocumentEpoch,
        zoom: f32,
        catalog: &jellyflow_runtime::schema::NodeCatalog,
    ) -> Option<Entity<OpenGpuiNodeBinding>> {
        let previous_size = mounted.semantic_zoom.canonical_size();
        mounted
            .semantic_zoom
            .reconcile(frame, catalog, document_epoch);
        let canonical_size = mounted.semantic_zoom.canonical_size();
        if canonical_size != previous_size
            && !mounted.size_state.recognizes_authoritative_size(
                mounted.registration_policy.size_policy(),
                canonical_size,
            )
        {
            mounted.content_epoch = mounted.content_epoch.saturating_add(1);
        }
        mounted
            .scale_state
            .reconcile(mounted.registration_policy.scale_policy(), zoom);
        mounted.bounds = CanvasRect {
            origin: mounted.semantic_zoom.canonical_origin(),
            size: mounted.semantic_zoom.canonical_size(),
        };
        mounted.selected = frame.selected();
        if mounted.binding_revision == frame.binding_revision() {
            return None;
        }
        mounted.binding.clone()
    }

    pub(super) fn apply_node_changes(
        &mut self,
        changes: &NodeChanges,
        cx: &mut Context<Self>,
    ) -> Vec<MountedNode> {
        self.cancel_node_drag_with_retired_member(changes);
        self.cancel_connection_with_removed_source(changes);
        if let Some(owner) = self.interaction_owner.active_port_owner()
            && let Some(change) = changes.get(&owner.instance().node())
            && !change.is_stale_for(owner.instance())
            && !self
                .view
                .graph_snapshot()
                .ok()
                .map(|snapshot| snapshot.graph().port(&owner.port()).is_some())
                .unwrap_or(false)
            && let Some(mount) = self.mount_capability(owner.instance()).cloned()
        {
            self.interaction_owner.cancel_port_owner_deferred(
                &owner,
                &mount,
                OpenGpuiPortDragCancelReason::PortDisabled,
            );
        }
        let mut bindings = Vec::new();
        let mut retired = Vec::new();
        for (node, change) in changes {
            let Some(instance) = self.mounts.retained_instance(*node).cloned() else {
                continue;
            };
            self.stats.binding_reconciliation_visits =
                self.stats.binding_reconciliation_visits.saturating_add(1);

            if change.is_stale_for(&instance) {
                if let Some(mounted) = self.detach_instance(&instance) {
                    retired.push(mounted);
                }
                continue;
            }

            let mounted = self.mounts.get_mut(&instance);
            let Some(mounted) = mounted else {
                continue;
            };
            if mounted.binding_revision >= change.revision() {
                continue;
            }
            if let Some(binding) = &mounted.binding {
                bindings.push((instance, binding.clone(), change.revision()));
            }
        }
        for (instance, binding, revision) in bindings {
            match self.advance_binding(&binding, revision, cx) {
                Ok(OpenGpuiBindingAdvance::Applied) => {
                    if let Some(mounted) = self.mounts.get_mut(&instance) {
                        mounted.binding_revision = revision;
                    }
                }
                Ok(OpenGpuiBindingAdvance::Retry) => {}
                Err(error) => {
                    self.record_feedback_failure(
                        Some(instance.node()),
                        format!("native binding refresh failed: {error}"),
                    );
                    if let Some(mounted) = self.detach_instance(&instance) {
                        retired.push(mounted);
                    }
                }
            }
        }
        retired
    }

    fn cancel_node_drag_with_retired_member(&mut self, changes: &NodeChanges) {
        let retired = changes.into_iter().find_map(|(node, _)| {
            let member = self.interaction_owner.node_drag_member(*node)?;
            match self.view.node_lifetime_key(*node) {
                Ok(Some(current)) if current == member => None,
                Ok(_) | Err(ViewSessionError::Closed { .. }) => Some(member),
                Err(_) => None,
            }
        });
        let Some(instance) = retired else {
            return;
        };
        self.interaction_owner
            .cancel_scene_deferred(OpenGpuiSceneGestureCancelReason::OwnerRetired);
        self.retire_canvas_interaction_owner(InteractionOwner::Node(instance));
    }

    fn advance_binding(
        &mut self,
        binding: &Entity<OpenGpuiNodeBinding>,
        revision: jellyflow_runtime::graph::GraphRevision,
        cx: &mut Context<Self>,
    ) -> Result<OpenGpuiBindingAdvance, crate::binding::OpenGpuiNodeBindingError> {
        let outcome = binding.update(cx, |binding, cx| {
            let outcome = binding.advance(revision, cx);
            cx.notify();
            outcome
        });
        self.finish_binding_advance(outcome, cx)
    }

    fn advance_binding_from_frame(
        &mut self,
        binding: &Entity<OpenGpuiNodeBinding>,
        frame: &FrameNode,
        cx: &mut Context<Self>,
    ) -> Result<OpenGpuiBindingAdvance, crate::binding::OpenGpuiNodeBindingError> {
        let outcome = binding.update(cx, |binding, cx| {
            let outcome = binding.advance_from_frame(frame, cx);
            cx.notify();
            outcome
        });
        self.finish_binding_advance(outcome, cx)
    }

    fn finish_binding_advance(
        &mut self,
        outcome: Result<OpenGpuiBindingAdvance, crate::binding::OpenGpuiNodeBindingError>,
        cx: &mut Context<Self>,
    ) -> Result<OpenGpuiBindingAdvance, crate::binding::OpenGpuiNodeBindingError> {
        match outcome {
            Ok(OpenGpuiBindingAdvance::Applied) => {
                self.stats.binding_updates = self.stats.binding_updates.saturating_add(1);
                self.stats.binding_notifications =
                    self.stats.binding_notifications.saturating_add(1);
                Ok(OpenGpuiBindingAdvance::Applied)
            }
            Ok(OpenGpuiBindingAdvance::Retry) => {
                // Keep the old revision so the next materialized frame retries atomically.
                cx.notify();
                Ok(OpenGpuiBindingAdvance::Retry)
            }
            Err(error) => Err(error),
        }
    }

    fn layout_invalidator(cx: &Context<Self>) -> OpenGpuiLayoutInvalidator {
        let host = cx.weak_entity();
        Rc::new(move |instance, mount, cx| {
            let Some(host) = host.upgrade() else {
                return Err(ViewSessionError::Closed {
                    reason: ViewCloseReason::HostClosed,
                });
            };
            host.update(cx, |host, cx| {
                host.invalidate_instance_layout(instance, mount, cx)
            })
        })
    }

    fn invalidate_instance_layout(
        &mut self,
        instance: &NodeInstanceKey,
        mount: &OpenGpuiMountCapability,
        cx: &mut Context<Self>,
    ) -> Result<bool, ViewSessionError> {
        if !mount.is_live()
            || self.mounts.retained_instance(instance.node()) != Some(instance)
            || !self
                .mount_capability(instance)
                .is_some_and(|current| current.is_same_mount(mount))
        {
            return Ok(false);
        }
        if let Some(mounted) = self.mounts.get_mut(instance) {
            mounted.content_epoch = mounted.content_epoch.saturating_add(1);
        }
        let invalidated = self.view.invalidate_node_layout(instance)?;
        if invalidated {
            cx.notify();
        }
        Ok(invalidated)
    }

    fn mount_node(
        &mut self,
        frame: &FrameNode,
        document_epoch: jellyflow_runtime::graph::DocumentEpoch,
        zoom: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> MountedNode {
        self.stats.mounts = self.stats.mounts.saturating_add(1);
        let mount = OpenGpuiMountCapability::new();
        let activity = OpenGpuiNodeActivityRegistry::for_host(
            frame.instance().clone(),
            self.activity_coordinator.clone(),
        );
        let measurement = OpenGpuiMeasurementSink::new(
            cx.entity_id(),
            frame.instance().clone(),
            activity.clone(),
            self.interaction_owner.native_capability(),
            Self::host_port_listener(cx, mount.clone()),
            Self::host_port_hover_listener(cx, mount.clone()),
            mount.clone(),
        );
        let command_sink = OpenGpuiCommandSink::new(Rc::downgrade(&self.commands), cx.entity_id());
        let drag_handle = OpenGpuiNodeDragHandle::new(
            frame.instance().clone(),
            activity.clone(),
            self.interaction_owner.native_capability(),
            Self::host_scene_listener(cx),
            mount.clone(),
        );
        let binding_mount = OpenGpuiBindingMount::from_frame(
            frame,
            self.view.downgrade(),
            mount.clone(),
            command_sink,
        );
        let accessible_name = self.accessible_node_name(frame);
        let mut semantic_zoom =
            OpenGpuiSemanticZoomState::from_frame(frame, &self.catalog, document_epoch);

        let (view, overview, content_kind, binding, registration_policy) = match binding_mount {
            Ok(binding_mount) => {
                self.stats.binding_mount_initializations =
                    self.stats.binding_mount_initializations.saturating_add(1);
                let layout_invalidator = Self::layout_invalidator(cx);
                let binding = cx.new(|cx| {
                    OpenGpuiNodeBinding::new(
                        binding_mount,
                        activity.clone(),
                        drag_handle,
                        measurement.clone(),
                        layout_invalidator,
                        cx,
                    )
                });
                let (view, overview, content_kind, registration_policy) =
                    self.mount_registered_or_fallback(frame, binding.clone(), window, cx);
                (
                    view,
                    overview,
                    content_kind,
                    Some(binding),
                    registration_policy,
                )
            }
            Err(error) => {
                let diagnostic = binding_diagnostic(frame, error);
                let view = self.diagnostic_fallback(&diagnostic, None, cx);
                self.last_diagnostic = Some(diagnostic);
                (
                    view,
                    None,
                    MountedNodeContentKind::Diagnostic(OpenGpuiSceneHostDiagnosticKind::Binding),
                    None,
                    OpenGpuiNodeRegistrationPolicy::default(),
                )
            }
        };
        if let Some(overview) = overview {
            semantic_zoom.install_application_overview(overview);
        }
        let scale_state = OpenGpuiNodeScaleState::new(registration_policy.scale_policy(), zoom);
        let size_state = OpenGpuiNodeSizeState::new(
            OpenGpuiNodeSizeEpoch::new(0, self.adapter_environment_epoch),
            semantic_zoom.canonical_size(),
        )
        .expect("frame geometry always has a positive finite canonical node size");

        MountedNode {
            view,
            content_kind,
            binding,
            activity,
            measurement,
            focus: cx.focus_handle(),
            bounds: CanvasRect {
                origin: semantic_zoom.canonical_origin(),
                size: semantic_zoom.canonical_size(),
            },
            semantic_zoom,
            registration_policy,
            scale_state,
            size_state,
            content_epoch: 0,
            binding_revision: frame.binding_revision(),
            accessible_name,
            selected: frame.selected(),
            mount,
        }
    }

    fn accessible_node_name(&self, frame: &FrameNode) -> String {
        let Some(node) = frame.node() else {
            return format!("Node {}", frame.id().0);
        };
        self.catalog
            .get(&node.kind)
            .map(|spec| spec.title().to_owned())
            .unwrap_or_else(|| node.kind.0.clone())
    }

    fn mount_registered_or_fallback(
        &mut self,
        frame: &FrameNode,
        binding: Entity<OpenGpuiNodeBinding>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (
        AnyView,
        Option<AnyView>,
        MountedNodeContentKind,
        OpenGpuiNodeRegistrationPolicy,
    ) {
        let renderer_key = frame.renderer().key();
        match self.registry.mount(frame, binding.clone(), window, cx) {
            OpenGpuiNodeMountOutcome::Mounted {
                view,
                overview,
                policy,
            } => {
                self.clear_matching_factory_diagnostic(frame);
                let overview = match overview {
                    OpenGpuiNodeOverviewMountOutcome::Generic => None,
                    OpenGpuiNodeOverviewMountOutcome::Mounted(view) => Some(view),
                    OpenGpuiNodeOverviewMountOutcome::FactoryFailed(error) => {
                        let diagnostic = mount_diagnostic(
                            frame,
                            OpenGpuiSceneHostDiagnosticKind::FactoryFailure,
                            format!("application-native overview fallback: {error}"),
                        );
                        self.last_diagnostic = Some(diagnostic);
                        None
                    }
                };
                (view, overview, MountedNodeContentKind::Renderer, policy)
            }
            OpenGpuiNodeMountOutcome::Invalid(issue) => {
                let diagnostic = mount_diagnostic(
                    frame,
                    OpenGpuiSceneHostDiagnosticKind::InvalidRendererRegistration,
                    issue.to_string(),
                );
                let view = self.diagnostic_fallback(&diagnostic, Some(binding), cx);
                self.last_diagnostic = Some(diagnostic);
                (
                    view,
                    None,
                    MountedNodeContentKind::Diagnostic(
                        OpenGpuiSceneHostDiagnosticKind::InvalidRendererRegistration,
                    ),
                    OpenGpuiNodeRegistrationPolicy::default(),
                )
            }
            OpenGpuiNodeMountOutcome::Missing => {
                let diagnostic = mount_diagnostic(
                    frame,
                    OpenGpuiSceneHostDiagnosticKind::MissingRenderer,
                    format!("no renderer is registered for `{renderer_key}`"),
                );
                let view = self.diagnostic_fallback(&diagnostic, Some(binding), cx);
                self.last_diagnostic = Some(diagnostic);
                (
                    view,
                    None,
                    MountedNodeContentKind::Diagnostic(
                        OpenGpuiSceneHostDiagnosticKind::MissingRenderer,
                    ),
                    OpenGpuiNodeRegistrationPolicy::default(),
                )
            }
            OpenGpuiNodeMountOutcome::FactoryFailed(error) => {
                let diagnostic = mount_diagnostic(
                    frame,
                    OpenGpuiSceneHostDiagnosticKind::FactoryFailure,
                    error.to_string(),
                );
                let view = self.diagnostic_fallback(&diagnostic, Some(binding), cx);
                self.last_diagnostic = Some(diagnostic);
                (
                    view,
                    None,
                    MountedNodeContentKind::Diagnostic(
                        OpenGpuiSceneHostDiagnosticKind::FactoryFailure,
                    ),
                    OpenGpuiNodeRegistrationPolicy::default(),
                )
            }
        }
    }

    fn retire_stale_parked(
        &mut self,
        frame: &BTreeMap<NodeId, &NodeInstanceKey>,
    ) -> PendingMountedNodeRetirements {
        let stale = self
            .mounts
            .parked_instances()
            .filter(|instance| self.instance_is_retired(instance, frame))
            .cloned()
            .collect::<Vec<_>>();
        let mut retired_nodes = PendingMountedNodeRetirements::with_capacity(stale.len());
        for instance in stale {
            if let Some(retired) = self.mounts.retire(&instance) {
                retired_nodes.push(self.settle_retirement(retired));
            }
        }
        retired_nodes
    }

    fn instance_is_retired(
        &self,
        instance: &NodeInstanceKey,
        frame: &BTreeMap<NodeId, &NodeInstanceKey>,
    ) -> bool {
        if let Some(current) = frame.get(&instance.node()) {
            return *current != instance;
        }
        match self.view.node_lifetime_key(instance.node()) {
            Ok(Some(current)) => current != instance.lifetime(),
            Ok(None) | Err(ViewSessionError::Closed { .. }) => true,
            Err(_) => false,
        }
    }

    pub(super) fn mount_capability(
        &self,
        instance: &NodeInstanceKey,
    ) -> Option<&OpenGpuiMountCapability> {
        self.mounts.get(instance).map(|mounted| &mounted.mount)
    }

    pub(super) fn detach_instance(&mut self, instance: &NodeInstanceKey) -> Option<MountedNode> {
        let retired = self.mounts.retire(instance)?;
        Some(self.settle_retirement(retired))
    }

    fn hard_retire(&mut self, instance: &NodeInstanceKey, mounted: MountedNode) -> MountedNode {
        let retired = self.mounts.retire_detached(instance.clone(), mounted);
        self.settle_retirement(retired)
    }

    fn settle_retirement(&mut self, retired: RetiredMount<MountedNode>) -> MountedNode {
        self.interaction_owner.cancel_scene_owner_deferred(
            &retired.instance,
            OpenGpuiSceneGestureCancelReason::OwnerRetired,
        );
        self.retire_canvas_interaction_owner(InteractionOwner::Node(retired.instance.lifetime()));
        self.retire_connection_context(&retired.instance, &retired.value.mount);
        retired.value
    }

    fn settle_eviction(&mut self, evicted: EvictedMount<MountedNode>) -> MountedNode {
        self.retire_connection_context(&evicted.instance, &evicted.value.mount);
        evicted.value
    }

    pub(super) fn detach_all_instances(&mut self) -> Vec<MountedNode> {
        let retired_nodes = self
            .mounts
            .iter()
            .map(|(instance, _)| instance.lifetime())
            .collect::<Vec<_>>();
        for instance in retired_nodes {
            self.retire_canvas_interaction_owner(InteractionOwner::Node(instance));
        }
        if let Some(edge) = self.interaction_owner.presentation().active_edge() {
            self.retire_canvas_interaction_owner(InteractionOwner::Edge(edge));
        }
        if self.interaction_owner.has_node_drag_residency()
            || !matches!(
                self.interaction_owner.presentation().phase(),
                CanvasInteractionPhase::Idle | CanvasInteractionPhase::Closed
            )
        {
            let update = self.interaction_owner.cancel();
            self.discard_pending_interaction_submission();
            match update {
                Ok(update) => self.queue_interaction_queries(update.into_dispatch().queries),
                Err(error) => self.record_command_diagnostic(error.to_string()),
            }
        }
        if let Some((owner, mount)) = self.interaction_owner.connection_identity() {
            self.interaction_owner.cancel_port_owner_deferred(
                &owner,
                &mount,
                OpenGpuiPortDragCancelReason::OwnerRetired,
            );
        }
        self.mounts.retire_all()
    }

    fn hard_retire_all(&mut self, window: &mut Window) {
        let mounted = self.detach_all_instances();
        mounted
            .iter()
            .any(|node| node.activity.release_pointer_capture_if_owned(window));
        retire_mounted_nodes(mounted).resume();
    }
}
