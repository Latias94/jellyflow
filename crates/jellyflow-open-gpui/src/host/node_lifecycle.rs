use std::collections::BTreeMap;
use std::rc::Rc;

use jellyflow::core::{CanvasRect, NodeId};
use jellyflow::runtime::{
    CanvasInteractionPhase, FrameNode, InteractionOwner, NodeInstanceKey, ViewCloseReason,
    ViewSessionError,
};
use open_gpui::{AnyView, AppContext, Context, Entity, FocusHandle, Window};

use crate::activity::OpenGpuiNodeActivityRegistry;
use crate::binding::{
    OpenGpuiLayoutInvalidator, OpenGpuiMountCapability, OpenGpuiNodeBinding,
    OpenGpuiNodeBindingSnapshot,
};
use crate::command_journal::OpenGpuiCommandSink;
use crate::interaction::{OpenGpuiPortDragCancelReason, OpenGpuiSceneGestureCancelReason};
use crate::node_drag_handle::OpenGpuiNodeDragHandle;
use crate::port_handle::{
    OpenGpuiGraphPresentationSource, OpenGpuiMeasurementSink, OpenGpuiPendingMeasurements,
};
use crate::registry::{OpenGpuiNodeMountOutcome, OpenGpuiNodeRegistrationPolicy};
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
    binding_revision: jellyflow::runtime::GraphRevision,
    pub(super) bounds: CanvasRect,
    pub(super) accessible_name: String,
    pub(super) selected: bool,
    pub(super) mount: OpenGpuiMountCapability,
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

    pub(super) fn revoke(self) {
        self.mount.revoke();
        self.activity.revoke_all();
    }
}

pub(super) fn revoke_mounted_nodes(
    mounted_nodes: impl IntoIterator<Item = MountedNode>,
) -> Option<Box<dyn std::any::Any + Send + 'static>> {
    let mut first_panic = None;
    for mounted in mounted_nodes {
        let panic =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| mounted.revoke())).err();
        if first_panic.is_none() {
            first_panic = panic;
        } else if let Some(payload) = panic {
            std::mem::forget(payload);
        }
    }
    first_panic
}

pub(super) fn resume_mounted_node_panic(panic: Option<Box<dyn std::any::Any + Send + 'static>>) {
    if let Some(payload) = panic {
        std::panic::resume_unwind(payload);
    }
}

pub(super) fn suppress_host_panic(panic: Option<Box<dyn std::any::Any + Send + 'static>>) {
    if let Some(payload) = panic {
        std::mem::forget(payload);
    }
}

pub(super) fn catch_host_cleanup(cleanup: impl FnOnce()) -> bool {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(cleanup)) {
        Ok(()) => false,
        Err(payload) => {
            suppress_host_panic(Some(payload));
            true
        }
    }
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
    /// that are not retained are deterministic no-ops.
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
        mounted.revoke();
        cx.notify();
        true
    }

    pub(super) fn reconcile_frame(
        &mut self,
        frame: &[FrameNode],
        document_epoch: jellyflow::runtime::DocumentEpoch,
        zoom: f32,
        measurements: &Rc<OpenGpuiPendingMeasurements>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .document_epoch
            .is_some_and(|current| current != document_epoch)
        {
            self.scene_interactions
                .cancel_deferred(OpenGpuiSceneGestureCancelReason::OwnerRetired);
            self.hard_retire_all();
        }
        self.document_epoch = Some(document_epoch);

        let frame_instances = frame
            .iter()
            .map(|node| (node.id(), node.instance().clone()))
            .collect::<BTreeMap<_, _>>();
        self.retire_stale_parked(&frame_instances);

        let mut previous_active = self.mounts.take_active();

        for frame_node in frame {
            let key = frame_node.instance().clone();
            let retained = previous_active
                .remove(&key)
                .or_else(|| self.mounts.restore(&key));
            let mounted = match retained {
                Some(mounted) => self.reconcile_binding(mounted, frame_node, zoom, cx),
                None => self.mount_node(frame_node, zoom, window, cx),
            };
            if let Err(error) = mounted.measurement.activate(
                measurements,
                mounted.graph_measurement_source(),
                mounted.content_epoch,
            ) {
                self.record_measurement_failure(error);
            }
            if let Some(evicted) = self.mounts.insert_active(key, mounted) {
                self.settle_eviction(evicted);
            }
        }

        for (key, mounted) in previous_active {
            if self.instance_is_retired(&key, &frame_instances) {
                self.hard_retire(&key, mounted);
            } else {
                let protected = mounted.activity.protected_count() > 0;
                if let Some(evicted) = self.mounts.park(key, mounted, protected) {
                    self.settle_eviction(evicted);
                }
            }
        }
        self.mounts
            .refresh_eligibility(|mounted| mounted.activity.protected_count() > 0);
        for evicted in self.mounts.evict_eligible_overflow() {
            self.settle_eviction(evicted);
        }
        self.mounts.finish_reconcile();
    }

    fn reconcile_binding(
        &mut self,
        mut mounted: MountedNode,
        frame: &FrameNode,
        zoom: f32,
        cx: &mut Context<Self>,
    ) -> MountedNode {
        let previous_size = mounted.semantic_zoom.canonical_size();
        mounted.semantic_zoom.reconcile(frame, &self.catalog);
        if mounted.semantic_zoom.canonical_size() != previous_size {
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
            return mounted;
        }
        if let Some(binding) = mounted.binding.clone() {
            self.advance_binding(&binding, frame.binding_revision(), cx);
            mounted.binding_revision = frame.binding_revision();
        }
        mounted
    }

    pub(super) fn apply_node_changes(
        &mut self,
        changes: &NodeChanges,
        cx: &mut Context<Self>,
    ) -> Vec<MountedNode> {
        self.cancel_connection_with_removed_source(changes);
        if let Some(owner) = self.interactions.active_owner()
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
            self.interactions.cancel_owner_deferred(
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
            mounted.binding_revision = change.revision();
            if let Some(binding) = &mounted.binding {
                bindings.push((binding.clone(), change.revision()));
            }
        }
        for (binding, revision) in bindings {
            self.advance_binding(&binding, revision, cx);
        }
        retired
    }

    fn advance_binding(
        &mut self,
        binding: &Entity<OpenGpuiNodeBinding>,
        revision: jellyflow::runtime::GraphRevision,
        cx: &mut Context<Self>,
    ) {
        binding.update(cx, |binding, cx| {
            binding.advance(revision, cx);
            cx.notify();
        });
        self.stats.binding_updates = self.stats.binding_updates.saturating_add(1);
        self.stats.binding_notifications = self.stats.binding_notifications.saturating_add(1);
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
            self.interactions.clone(),
            Self::host_port_listener(cx, mount.clone()),
            mount.clone(),
        );
        let command_sink = OpenGpuiCommandSink::new(Rc::downgrade(&self.commands), cx.entity_id());
        let drag_handle = OpenGpuiNodeDragHandle::new(
            frame.instance().clone(),
            activity.clone(),
            self.scene_interactions.clone(),
            Self::host_scene_listener(cx),
            mount.clone(),
        );
        let snapshot = OpenGpuiNodeBindingSnapshot::from_frame(
            frame,
            self.view.downgrade(),
            mount.clone(),
            command_sink,
        );
        let accessible_name = self.accessible_node_name(frame);
        let semantic_zoom = OpenGpuiSemanticZoomState::from_frame(frame, &self.catalog);

        let (view, content_kind, binding, registration_policy) = match snapshot {
            Ok(snapshot) => {
                self.stats.binding_snapshot_builds =
                    self.stats.binding_snapshot_builds.saturating_add(1);
                let layout_invalidator = Self::layout_invalidator(cx);
                let binding = cx.new(|cx| {
                    OpenGpuiNodeBinding::new(
                        snapshot,
                        activity.clone(),
                        drag_handle,
                        measurement.clone(),
                        layout_invalidator,
                        cx,
                    )
                });
                let (view, content_kind, registration_policy) =
                    self.mount_registered_or_fallback(frame, binding.clone(), window, cx);
                (view, content_kind, Some(binding), registration_policy)
            }
            Err(error) => {
                let diagnostic = binding_diagnostic(frame, error);
                let view = self.diagnostic_fallback(&diagnostic, None, cx);
                self.last_diagnostic = Some(diagnostic);
                (
                    view,
                    MountedNodeContentKind::Diagnostic(OpenGpuiSceneHostDiagnosticKind::Binding),
                    None,
                    OpenGpuiNodeRegistrationPolicy::default(),
                )
            }
        };
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
        MountedNodeContentKind,
        OpenGpuiNodeRegistrationPolicy,
    ) {
        let renderer_key = frame.renderer().key();
        match self
            .registry
            .mount(renderer_key, binding.clone(), window, cx)
        {
            OpenGpuiNodeMountOutcome::Mounted { view, policy } => {
                self.clear_matching_factory_diagnostic(frame);
                (view, MountedNodeContentKind::Renderer, policy)
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
                    MountedNodeContentKind::Diagnostic(
                        OpenGpuiSceneHostDiagnosticKind::FactoryFailure,
                    ),
                    OpenGpuiNodeRegistrationPolicy::default(),
                )
            }
        }
    }

    fn retire_stale_parked(&mut self, frame: &BTreeMap<NodeId, NodeInstanceKey>) {
        let retired = self
            .mounts
            .parked_instances()
            .filter(|instance| self.instance_is_retired(instance, frame))
            .cloned()
            .collect::<Vec<_>>();
        for instance in retired {
            if let Some(retired) = self.mounts.retire(&instance) {
                self.settle_retirement(retired).revoke();
            }
        }
    }

    fn instance_is_retired(
        &self,
        instance: &NodeInstanceKey,
        frame: &BTreeMap<NodeId, NodeInstanceKey>,
    ) -> bool {
        if let Some(current) = frame.get(&instance.node()) {
            return current != instance;
        }
        match self.view.node_incarnation(instance.node()) {
            Ok(Some(current)) => current != instance.incarnation(),
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

    fn hard_retire(&mut self, instance: &NodeInstanceKey, mounted: MountedNode) {
        let retired = self.mounts.retire_detached(instance.clone(), mounted);
        self.settle_retirement(retired).revoke();
    }

    fn settle_retirement(&mut self, retired: RetiredMount<MountedNode>) -> MountedNode {
        self.scene_interactions.cancel_owner_deferred(
            &retired.instance,
            OpenGpuiSceneGestureCancelReason::OwnerRetired,
        );
        self.retire_canvas_interaction_owner(InteractionOwner::Node(retired.instance.node()));
        self.retire_connection_owner(&retired.instance, &retired.value.mount);
        retired.value
    }

    fn settle_eviction(&mut self, evicted: EvictedMount<MountedNode>) {
        self.retire_connection_owner(&evicted.instance, &evicted.value.mount);
        evicted.value.revoke();
    }

    pub(super) fn detach_all_instances(&mut self) -> Vec<MountedNode> {
        let retired_nodes = self
            .mounts
            .iter()
            .map(|(instance, _)| instance.node())
            .collect::<Vec<_>>();
        for node in retired_nodes {
            self.retire_canvas_interaction_owner(InteractionOwner::Node(node));
        }
        if let Some(edge) = self.canvas_interaction.presentation().active_edge() {
            self.retire_canvas_interaction_owner(InteractionOwner::Edge(edge));
        }
        if !matches!(
            self.canvas_interaction.presentation().phase(),
            CanvasInteractionPhase::Idle | CanvasInteractionPhase::Closed
        ) {
            let update = self.canvas_interaction.cancel();
            self.discard_pending_interaction_submission();
            self.queue_interaction_queries(update.into_dispatch().queries);
        }
        if let Some((owner, mount)) = self
            .connection_owner
            .as_ref()
            .map(|current| (current.owner.clone(), current.mount.clone()))
        {
            self.interactions.cancel_owner_deferred(
                &owner,
                &mount,
                OpenGpuiPortDragCancelReason::OwnerRetired,
            );
        }
        self.mounts.retire_all()
    }

    fn hard_retire_all(&mut self) {
        let panic = revoke_mounted_nodes(self.detach_all_instances());
        resume_mounted_node_panic(panic);
    }
}
