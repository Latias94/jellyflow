use std::rc::Rc;

use jellyflow_core::{CanvasRect, NodeId};
use jellyflow_runtime::{
    graph::GraphRevision, interaction::CanvasInteractionPhase, interaction::InteractionOwner,
    view::FrameNode, view::NodeInstanceKey, view::ViewCloseReason, view::ViewSessionError,
};
use open_gpui::{AnyView, AppContext, Context, Entity, FocusHandle, Window};

use crate::activity::OpenGpuiNodeActivityRegistry;
use crate::binding::{
    OpenGpuiBindingMount, OpenGpuiLayoutInvalidator, OpenGpuiMountCapability, OpenGpuiNodeBinding,
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
use super::retained_node_owner::{
    RetainedFrameReconcileInput, RetainedFrameReconcileReceipt, RetainedNodeConstructionGuard,
    RetainedNodeRelease, RetainedNodeReleaseKind, RetainedNodeRetirementBatch,
    retire_mounted_nodes,
};

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
    pub(super) binding_revision: jellyflow_runtime::graph::GraphRevision,
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
    pub(super) fn canonical_bounds(&self) -> CanvasRect {
        CanvasRect {
            origin: self.semantic_zoom.canonical_origin(),
            size: self.semantic_zoom.canonical_size(),
        }
    }

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
        let Some(instance) = self.retained_nodes.retained_instance(node).cloned() else {
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
        let Some(instance) = self.retained_nodes.retained_instance(node).cloned() else {
            return false;
        };
        self.retry_mount_instance(&instance, cx)
    }

    pub(super) fn retry_mount_instance(
        &mut self,
        instance: &NodeInstanceKey,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.retained_nodes.retained_instance(instance.node()) != Some(instance) {
            return false;
        }
        let retryable = self
            .retained_nodes
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
        let mut preparation = self
            .retained_nodes
            .prepare_frame(frame, document_epoch, &self.view);
        if preparation.document_replaced() {
            self.interaction_owner
                .cancel_scene_deferred(OpenGpuiSceneGestureCancelReason::OwnerRetired);
        }
        let releases = preparation.take_releases();
        let mut retired_nodes = RetainedNodeRetirementBatch::with_capacity(releases.len());
        for release in releases {
            retired_nodes.push(self.settle_release(release));
        }
        retired_nodes.release_pointer_capture_if_owned(window);
        let mut retirement_panics = retired_nodes.retire();
        let mut retired_nodes = RetainedNodeRetirementBatch::default();

        for frame_index in preparation.take_missing_frame_indices() {
            let frame_node = &frame[frame_index];
            let mounted = self.mount_node(frame_node, document_epoch, zoom, window, cx);
            preparation.insert_mount(frame_node.instance().clone(), mounted);
        }
        let previous_geometry = self.view.geometry_snapshot().ok();
        let input = RetainedFrameReconcileInput::new(
            frame,
            &self.view,
            zoom,
            previous_geometry.as_ref(),
            measurements,
        );
        let RetainedFrameReconcileReceipt {
            releases,
            binding_failures,
            measurement_errors,
            panic,
        } = self
            .retained_nodes
            .reconcile_frame(preparation, input, &mut self.work_ledger, cx);
        for failure in binding_failures {
            self.last_diagnostic = Some(binding_diagnostic(
                &frame[failure.frame_index],
                failure.error,
            ));
            cx.notify();
        }
        for error in measurement_errors {
            self.record_measurement_failure(error);
        }
        for release in releases {
            retired_nodes.push(self.settle_release(release));
        }
        if panic.is_none() {
            self.interaction_owner.observe_materialized_frame(
                graph_revision,
                frame.iter().map(|node| node.instance().lifetime()),
            );
        }
        retired_nodes.release_pointer_capture_if_owned(window);
        retirement_panics.merge(retired_nodes.retire());
        retirement_panics.push(panic);
        retirement_panics.resume();
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
        let receipt = self
            .retained_nodes
            .apply_node_changes(changes, &mut self.work_ledger, cx);
        for failure in receipt.binding_failures {
            self.record_feedback_failure(
                Some(failure.node),
                format!("native binding refresh failed: {}", failure.error),
            );
        }
        receipt
            .releases
            .into_iter()
            .map(|release| self.settle_release(release))
            .collect()
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
            || self.retained_nodes.retained_instance(instance.node()) != Some(instance)
            || !self
                .mount_capability(instance)
                .is_some_and(|current| current.is_same_mount(mount))
        {
            return Ok(false);
        }
        let invalidated_content = self
            .retained_nodes
            .invalidate_mount_content(instance, mount);
        debug_assert!(invalidated_content);
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
        self.work_ledger.mounts = self.work_ledger.mounts.saturating_add(1);
        let mount = OpenGpuiMountCapability::new();
        let activity = self
            .retained_nodes
            .activity_registry(frame.instance().clone());
        let construction = RetainedNodeConstructionGuard::new(mount.clone(), activity.clone());
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
        let mut semantic_zoom = OpenGpuiSemanticZoomState::from_frame(frame, document_epoch);

        let (view, overview, content_kind, binding, registration_policy) = match binding_mount {
            Ok(binding_mount) => {
                self.work_ledger.binding_mount_initializations = self
                    .work_ledger
                    .binding_mount_initializations
                    .saturating_add(1);
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

        construction.complete(MountedNode {
            view,
            content_kind,
            binding,
            activity,
            measurement,
            focus: cx.focus_handle(),
            semantic_zoom,
            registration_policy,
            scale_state,
            size_state,
            content_epoch: 0,
            binding_revision: frame.binding_revision(),
            accessible_name,
            selected: frame.selected(),
            mount,
        })
    }

    fn accessible_node_name(&self, frame: &FrameNode) -> String {
        let Some(node) = frame.node() else {
            return format!("Node {}", frame.id().0);
        };
        self.registry
            .presentation(&node.kind, node.kind_version)
            .map(|presentation| presentation.accessible_name().to_owned())
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
            OpenGpuiNodeMountOutcome::Missing => {
                let identity = frame.node().map_or_else(
                    || format!("node {}", frame.id().0),
                    |node| format!("{}@{}", node.kind.0, node.kind_version),
                );
                let diagnostic = mount_diagnostic(
                    frame,
                    OpenGpuiSceneHostDiagnosticKind::MissingRenderer,
                    format!(
                        "no native renderer mapping matches semantic node `{identity}` and stamp `{}@{}`",
                        frame.renderer().key(),
                        frame.renderer().incarnation(),
                    ),
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

    pub(super) fn mount_capability(
        &self,
        instance: &NodeInstanceKey,
    ) -> Option<&OpenGpuiMountCapability> {
        self.retained_nodes.mount_capability(instance)
    }

    pub(super) fn detach_instance(&mut self, instance: &NodeInstanceKey) -> Option<MountedNode> {
        let release = self.retained_nodes.retire_instance(instance)?;
        Some(self.settle_release(release))
    }

    fn settle_release(&mut self, mut release: RetainedNodeRelease) -> MountedNode {
        if release.kind == RetainedNodeReleaseKind::HardRetirement {
            self.interaction_owner.cancel_scene_owner_deferred(
                &release.instance,
                OpenGpuiSceneGestureCancelReason::OwnerRetired,
            );
            self.retire_canvas_interaction_owner(InteractionOwner::Node(
                release.instance.lifetime(),
            ));
        }
        self.retire_connection_context(&release.instance, release.mount());
        release.take_mounted()
    }

    pub(super) fn close_all_instances(&mut self, reason: ViewCloseReason) -> Vec<MountedNode> {
        let releases = self.retained_nodes.close_and_retire_all(reason);
        self.settle_all_releases(releases)
    }

    #[cfg(test)]
    pub(super) fn detach_all_instances(&mut self) -> Vec<MountedNode> {
        let releases = self.retained_nodes.retire_all();
        self.settle_all_releases(releases)
    }

    fn settle_all_releases(&mut self, releases: Vec<RetainedNodeRelease>) -> Vec<MountedNode> {
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
                Ok(_update) => {}
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
        releases
            .into_iter()
            .map(|release| self.settle_release(release))
            .collect()
    }
}
