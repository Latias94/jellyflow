use jellyflow_core::{CanvasSize, NodeId, PortId};
use jellyflow_runtime::{
    graph::{DocumentEpoch, GraphRevision, PortInstanceKey},
    view::{FrameNode, GeometrySnapshot, NodeInstanceKey, ViewSession},
};
use open_gpui::{Entity, Window};
use std::collections::{BTreeMap, BTreeSet};

use crate::activity::{
    OpenGpuiActivityCapacityWait, OpenGpuiActivityCoordinator, OpenGpuiActivityCoordinatorStats,
    OpenGpuiActivityKind, OpenGpuiNodeActivityRegistry,
};
use crate::binding::{
    OpenGpuiBindingAdvance, OpenGpuiMountCapability, OpenGpuiNodeBinding, OpenGpuiNodeBindingError,
};
use crate::panic_sink::OpenGpuiPanicAccumulator;
use crate::port_handle::{
    OpenGpuiGraphPresentationSource, OpenGpuiMeasurementError, OpenGpuiMeasurementSink,
};
use crate::registry::OpenGpuiNodeRegistrationPolicy;
use crate::semantic_zoom::OpenGpuiNodePresentation;
use crate::size_policy::OpenGpuiNodeSizePolicy;

use super::OpenGpuiSceneHost;
use super::lifecycle::NodeChanges;
use super::node_lifecycle::MountedNode;
use super::presentation::{OpenGpuiPresentationPortCache, OpenGpuiPresentationSyncError};
use super::retained_mount_store::{
    EvictedMount, RetainedMountSnapshot, RetainedMountStore, RetiredMount,
};

/// Owns every retained-node index and the adapter state whose lifetime follows those mounts.
///
/// `RetainedMountStore` remains the only authority for active, parked, incarnation, LRU, and
/// retirement state. This owner adds lifecycle-scoped activity and presentation caches without
/// mirroring any of those indexes.
pub(super) struct RetainedNodeOwner {
    store: RetainedMountStore<MountedNode>,
    activity: OpenGpuiActivityCoordinator,
    presentation_ports: OpenGpuiPresentationPortCache,
    document_epoch: Option<DocumentEpoch>,
    hovered_node: Option<NodeInstanceKey>,
    hovered_port: Option<PortInstanceKey>,
    #[cfg(test)]
    reconcile_fault: Option<RetainedFrameReconcileFault>,
}

pub(super) struct RetainedNodeOwnerSnapshot {
    pub(super) mounts: RetainedMountSnapshot,
    pub(super) activity: OpenGpuiActivityCoordinatorStats,
}

#[derive(Clone)]
pub(super) struct RetainedContentSizeSource {
    pub(super) instance: NodeInstanceKey,
    pub(super) mount: OpenGpuiMountCapability,
    pub(super) measurement: OpenGpuiMeasurementSink,
    pub(super) presentation: OpenGpuiGraphPresentationSource,
    pub(super) registration_policy: OpenGpuiNodeRegistrationPolicy,
    pub(super) canonical_size: CanvasSize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RetainedNodeReleaseKind {
    HardRetirement,
    Eviction,
}

pub(super) struct RetainedNodeRelease {
    pub(super) instance: NodeInstanceKey,
    mounted: Option<MountedNode>,
    pub(super) kind: RetainedNodeReleaseKind,
}

impl RetainedNodeRelease {
    pub(super) fn mount(&self) -> &OpenGpuiMountCapability {
        &self
            .mounted
            .as_ref()
            .expect("an unsettled retained-node release still owns its mount")
            .mount
    }

    pub(super) fn take_mounted(&mut self) -> MountedNode {
        self.mounted
            .take()
            .expect("a retained-node release is settled at most once")
    }
}

impl Drop for RetainedNodeRelease {
    fn drop(&mut self) {
        if let Some(mounted) = self.mounted.take() {
            retire_mounted_nodes([mounted]).suppress();
        }
    }
}

/// Revokes mount-scoped authority if native construction unwinds before producing a mounted node.
pub(super) struct RetainedNodeConstructionGuard {
    mount: OpenGpuiMountCapability,
    activity: OpenGpuiNodeActivityRegistry,
    armed: bool,
}

impl RetainedNodeConstructionGuard {
    pub(super) fn new(
        mount: OpenGpuiMountCapability,
        activity: OpenGpuiNodeActivityRegistry,
    ) -> Self {
        Self {
            mount,
            activity,
            armed: true,
        }
    }

    pub(super) fn complete(mut self, mounted: MountedNode) -> MountedNode {
        self.armed = false;
        mounted
    }
}

impl Drop for RetainedNodeConstructionGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let mut panics = OpenGpuiPanicAccumulator::default();
        panics.capture(|| self.mount.retire());
        panics.capture(|| {
            self.activity.revoke_all();
        });
        panics.suppress();
    }
}

/// Owns mounts after they leave the retained indexes and until retirement completes.
///
/// Dropping an unfinished batch still retires every mount. This keeps mount callbacks and
/// activity revocation bounded even when host-side interaction settlement unwinds.
#[derive(Default)]
pub(super) struct RetainedNodeRetirementBatch {
    mounted_nodes: Vec<MountedNode>,
}

impl RetainedNodeRetirementBatch {
    pub(super) fn with_capacity(capacity: usize) -> Self {
        Self {
            mounted_nodes: Vec::with_capacity(capacity),
        }
    }

    pub(super) fn push(&mut self, mounted: MountedNode) {
        self.mounted_nodes.push(mounted);
    }

    pub(super) fn release_pointer_capture_if_owned(&self, window: &mut Window) -> bool {
        self.mounted_nodes
            .iter()
            .any(|mounted| mounted.activity.release_pointer_capture_if_owned(window))
    }

    pub(super) fn retire(mut self) -> OpenGpuiPanicAccumulator {
        retire_mounted_nodes(std::mem::take(&mut self.mounted_nodes))
    }
}

impl Drop for RetainedNodeRetirementBatch {
    fn drop(&mut self) {
        retire_mounted_nodes(std::mem::take(&mut self.mounted_nodes)).suppress();
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

pub(super) struct RetainedFramePreparation {
    document_replaced: bool,
    document_epoch: DocumentEpoch,
    frame_instances: BTreeMap<NodeId, NodeInstanceKey>,
    missing_frame_indices: Vec<usize>,
    releases: Vec<RetainedNodeRelease>,
    new_mounts: PendingRetainedMounts,
}

impl RetainedFramePreparation {
    pub(super) const fn document_replaced(&self) -> bool {
        self.document_replaced
    }

    pub(super) fn take_releases(&mut self) -> Vec<RetainedNodeRelease> {
        std::mem::take(&mut self.releases)
    }

    pub(super) fn take_missing_frame_indices(&mut self) -> Vec<usize> {
        std::mem::take(&mut self.missing_frame_indices)
    }

    pub(super) fn insert_mount(&mut self, instance: NodeInstanceKey, mounted: MountedNode) {
        self.new_mounts.insert(instance.clone(), mounted);
        assert_eq!(
            self.frame_instances.get(&instance.node()),
            Some(&instance),
            "a pending retained mount must belong to the prepared runtime frame",
        );
    }
}

struct PendingRetainedMounts {
    mounts: BTreeMap<NodeInstanceKey, MountedNode>,
}

impl PendingRetainedMounts {
    fn new() -> Self {
        Self {
            mounts: BTreeMap::new(),
        }
    }

    fn insert(&mut self, instance: NodeInstanceKey, mounted: MountedNode) {
        if let Some(replaced) = self.mounts.insert(instance, mounted) {
            retire_mounted_nodes([replaced]).resume();
            panic!("a prepared runtime frame cannot contain duplicate retained instances");
        }
    }

    fn remove(&mut self, instance: &NodeInstanceKey) -> Option<MountedNode> {
        self.mounts.remove(instance)
    }

    fn is_empty(&self) -> bool {
        self.mounts.is_empty()
    }
}

impl Drop for PendingRetainedMounts {
    fn drop(&mut self) {
        retire_mounted_nodes(std::mem::take(&mut self.mounts).into_values()).suppress();
    }
}

pub(super) struct RetainedFrameReconcileInput<'a> {
    frame: &'a [FrameNode],
    view: &'a ViewSession,
    zoom: f32,
    previous_geometry: Option<&'a GeometrySnapshot>,
}

impl<'a> RetainedFrameReconcileInput<'a> {
    pub(super) fn new(
        frame: &'a [FrameNode],
        view: &'a ViewSession,
        zoom: f32,
        previous_geometry: Option<&'a GeometrySnapshot>,
    ) -> Self {
        Self {
            frame,
            view,
            zoom,
            previous_geometry,
        }
    }
}

pub(super) struct RetainedBindingFailure {
    pub(super) frame_index: usize,
    pub(super) error: OpenGpuiNodeBindingError,
}

pub(super) struct RetainedFrameReconcileReceipt {
    pub(super) releases: Vec<RetainedNodeRelease>,
    pub(super) binding_failures: Vec<RetainedBindingFailure>,
    pub(super) panic: Option<Box<dyn std::any::Any + Send + 'static>>,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RetainedFrameReconcileFault {
    AfterNewMountInsert,
}

pub(super) struct RetainedNodeChangeFailure {
    pub(super) node: NodeId,
    pub(super) error: OpenGpuiNodeBindingError,
}

pub(super) struct RetainedNodeChangesReceipt {
    pub(super) releases: Vec<RetainedNodeRelease>,
    pub(super) binding_failures: Vec<RetainedNodeChangeFailure>,
}

impl RetainedNodeOwner {
    pub(super) fn new(eligible_parked_capacity: usize, protected_capacity: usize) -> Self {
        Self {
            store: RetainedMountStore::new(eligible_parked_capacity),
            activity: OpenGpuiActivityCoordinator::new(protected_capacity),
            presentation_ports: OpenGpuiPresentationPortCache::default(),
            document_epoch: None,
            hovered_node: None,
            hovered_port: None,
            #[cfg(test)]
            reconcile_fault: None,
        }
    }

    pub(super) fn clear_document(&mut self) {
        self.document_epoch = None;
        self.presentation_ports.clear();
        self.clear_hover();
    }

    pub(super) fn hovered_node(&self) -> Option<&NodeInstanceKey> {
        self.hovered_node.as_ref()
    }

    pub(super) const fn hovered_port(&self) -> Option<PortInstanceKey> {
        self.hovered_port
    }

    pub(super) fn set_hover(
        &mut self,
        node: Option<NodeInstanceKey>,
        port: Option<PortInstanceKey>,
    ) -> bool {
        debug_assert!(port.is_none_or(|port| {
            node.as_ref()
                .is_some_and(|node| node.lifetime() == port.owner())
        }));
        if self.hovered_node == node && self.hovered_port == port {
            return false;
        }
        self.hovered_node = node;
        self.hovered_port = port;
        true
    }

    pub(super) fn clear_hover(&mut self) -> bool {
        self.set_hover(None, None)
    }

    pub(super) fn clear_hovered_port(&mut self, port: PortInstanceKey) -> bool {
        if self.hovered_port != Some(port) {
            return false;
        }
        self.hovered_port = None;
        true
    }

    pub(super) fn prepare_frame(
        &mut self,
        frame: &[FrameNode],
        document_epoch: DocumentEpoch,
        view: &ViewSession,
    ) -> RetainedFramePreparation {
        let frame_instances = frame
            .iter()
            .map(|node| (node.id(), node.instance().clone()))
            .collect::<BTreeMap<_, _>>();
        let document_replaced = self
            .document_epoch
            .is_some_and(|current| current != document_epoch);
        let mut releases = Vec::new();
        if self.document_epoch != Some(document_epoch) {
            self.document_epoch = Some(document_epoch);
            self.presentation_ports.clear();
            self.clear_hover();
        }
        if document_replaced {
            let retired = self.store.retire_all();
            releases.extend(
                retired
                    .into_iter()
                    .map(|retired| self.hard_release(retired)),
            );
        } else {
            let stale = self
                .store
                .iter()
                .filter(|(instance, _)| Self::instance_is_retired(instance, &frame_instances, view))
                .map(|(instance, _)| instance.clone())
                .collect::<Vec<_>>();
            let retired = self.store.retire_many(stale);
            releases.extend(
                retired
                    .into_iter()
                    .map(|retired| self.hard_release(retired)),
            );
        }
        let missing_frame_indices = frame
            .iter()
            .enumerate()
            .filter_map(|(index, node)| self.store.get(node.instance()).is_none().then_some(index))
            .collect();
        RetainedFramePreparation {
            document_replaced,
            document_epoch,
            frame_instances,
            missing_frame_indices,
            releases,
            new_mounts: PendingRetainedMounts::new(),
        }
    }

    pub(super) fn reconcile_frame(
        &mut self,
        mut preparation: RetainedFramePreparation,
        input: RetainedFrameReconcileInput<'_>,
        cx: &mut open_gpui::Context<OpenGpuiSceneHost>,
    ) -> RetainedFrameReconcileReceipt {
        debug_assert!(preparation.releases.is_empty());
        debug_assert!(preparation.missing_frame_indices.is_empty());
        let mut releases = Vec::new();
        let mut binding_failures = Vec::new();
        let mut binding_retirements = Vec::new();

        let reconcile = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.store.begin_reconcile();
            for (frame_index, frame_node) in input.frame.iter().enumerate() {
                let instance = frame_node.instance();
                let retained =
                    self.store.visit_active(instance) || self.store.restore_active(instance);
                if !retained {
                    let mounted = preparation
                        .new_mounts
                        .remove(instance)
                        .expect("frame preparation supplies every missing retained mount");
                    if let Some(evicted) = self.store.insert_active(instance.clone(), mounted) {
                        releases.push(self.evicted_release(evicted));
                    }
                    #[cfg(test)]
                    self.panic_at_reconcile_fault(RetainedFrameReconcileFault::AfterNewMountInsert);
                }

                let binding = self.reconcile_mounted(
                    instance,
                    frame_node,
                    preparation.document_epoch,
                    input.zoom,
                    input.previous_geometry,
                );
                if let Some(binding) = binding {
                    match Self::advance_binding_from_frame(&binding, frame_node, cx) {
                        Ok(OpenGpuiBindingAdvance::Applied) => {
                            if let Some(mounted) = self.store.get_mut(instance) {
                                mounted.binding_revision = frame_node.binding_revision();
                            }
                        }
                        Ok(OpenGpuiBindingAdvance::Retry) => {}
                        Err(error) => {
                            binding_failures.push(RetainedBindingFailure { frame_index, error });
                            binding_retirements.push(instance.clone());
                            continue;
                        }
                    }
                }
            }

            let retired = self.store.retire_many(binding_retirements);
            releases.extend(
                retired
                    .into_iter()
                    .map(|retired| self.hard_release(retired)),
            );

            debug_assert!(preparation.new_mounts.is_empty());
            for (instance, mounted) in self.store.take_unseen_active() {
                if Self::instance_is_retired(&instance, &preparation.frame_instances, input.view) {
                    let retired = self.store.retire_detached(instance, mounted);
                    releases.push(self.hard_release(retired));
                } else {
                    let protected = mounted.activity.protected_count() > 0;
                    if let Some(evicted) = self.store.park(instance, mounted, protected) {
                        releases.push(self.evicted_release(evicted));
                    }
                }
            }
            self.store
                .refresh_eligibility(|mounted| mounted.activity.protected_count() > 0);
            let evicted = self.store.evict_eligible_overflow();
            releases.extend(
                evicted
                    .into_iter()
                    .map(|evicted| self.evicted_release(evicted)),
            );
            self.store.finish_reconcile();
        }));
        let panic = reconcile.err();
        if panic.is_some() {
            releases.extend(self.retire_all());
        }

        RetainedFrameReconcileReceipt {
            releases,
            binding_failures,
            panic,
        }
    }

    #[cfg(test)]
    pub(super) fn inject_reconcile_fault(&mut self, fault: RetainedFrameReconcileFault) {
        self.reconcile_fault = Some(fault);
    }

    #[cfg(test)]
    fn panic_at_reconcile_fault(&mut self, fault: RetainedFrameReconcileFault) {
        if self.reconcile_fault == Some(fault) {
            self.reconcile_fault = None;
            panic!("injected retained-node reconcile fault at {fault:?}");
        }
    }

    pub(super) fn apply_node_changes(
        &mut self,
        changes: &NodeChanges,
        cx: &mut open_gpui::Context<OpenGpuiSceneHost>,
    ) -> RetainedNodeChangesReceipt {
        let mut bindings = Vec::new();
        let mut releases = Vec::new();
        let mut retirements = Vec::new();
        for (node, change) in changes {
            let Some(instance) = self.store.retained_instance(*node).cloned() else {
                continue;
            };
            if change.is_stale_for(&instance) {
                retirements.push(instance);
                continue;
            }
            let Some(mounted) = self.store.get(&instance) else {
                continue;
            };
            if mounted.binding_revision >= change.revision() {
                continue;
            }
            if let Some(binding) = mounted.binding.clone() {
                bindings.push((instance, binding, change.revision()));
            }
        }

        let mut binding_failures = Vec::new();
        for (instance, binding, revision) in bindings {
            match Self::advance_binding(&binding, revision, cx) {
                Ok(OpenGpuiBindingAdvance::Applied) => {
                    if let Some(mounted) = self.store.get_mut(&instance) {
                        mounted.binding_revision = revision;
                    }
                }
                Ok(OpenGpuiBindingAdvance::Retry) => {}
                Err(error) => {
                    binding_failures.push(RetainedNodeChangeFailure {
                        node: instance.node(),
                        error,
                    });
                    retirements.push(instance);
                }
            }
        }
        let retired = self.store.retire_many(retirements);
        releases.extend(
            retired
                .into_iter()
                .map(|retired| self.hard_release(retired)),
        );
        RetainedNodeChangesReceipt {
            releases,
            binding_failures,
        }
    }

    pub(super) fn retained_instance(&self, node: NodeId) -> Option<&NodeInstanceKey> {
        self.store.retained_instance(node)
    }

    pub(super) fn get(&self, instance: &NodeInstanceKey) -> Option<&MountedNode> {
        self.store.get(instance)
    }

    pub(super) fn active_in_order(&self) -> impl Iterator<Item = (&NodeInstanceKey, &MountedNode)> {
        self.store.active_in_order()
    }

    pub(super) fn protected_required_nodes(&self) -> BTreeSet<NodeId> {
        self.store
            .iter()
            .filter(|(_, mounted)| mounted.activity.protected_count() > 0)
            .map(|(instance, _)| instance.node())
            .collect()
    }

    pub(super) fn retire_all(&mut self) -> Vec<RetainedNodeRelease> {
        self.presentation_ports.clear();
        self.clear_hover();
        let retired = self.store.retire_all();
        retired
            .into_iter()
            .map(|retired| self.hard_release(retired))
            .collect()
    }

    pub(super) fn close_and_retire_all(
        &mut self,
        reason: jellyflow_runtime::view::ViewCloseReason,
    ) -> Vec<RetainedNodeRelease> {
        for (_, mounted) in self.store.iter() {
            mounted.mount.close(reason);
        }
        self.retire_all()
    }

    pub(super) fn mount_capability(
        &self,
        instance: &NodeInstanceKey,
    ) -> Option<&OpenGpuiMountCapability> {
        self.store.get(instance).map(|mounted| &mounted.mount)
    }

    pub(super) fn activity_for_mount(
        &self,
        instance: &NodeInstanceKey,
        mount: &OpenGpuiMountCapability,
    ) -> Option<OpenGpuiNodeActivityRegistry> {
        let mounted = self.store.get(instance)?;
        mounted
            .mount
            .is_same_mount(mount)
            .then(|| mounted.activity.clone())
    }

    pub(super) fn content_size_admission_batch(
        &self,
        instances: &[NodeInstanceKey],
        limit: usize,
    ) -> Vec<NodeInstanceKey> {
        self.activity.admissible_protected_batch(instances, limit)
    }

    pub(super) fn wait_for_content_size_capacity(&self) -> OpenGpuiActivityCapacityWait {
        self.activity.wait_for_protected_capacity()
    }

    pub(super) fn content_size_sources(&self) -> Vec<RetainedContentSizeSource> {
        self.store.iter().map(Self::content_size_source).collect()
    }

    pub(super) fn active_content_size_sources(&self) -> Vec<RetainedContentSizeSource> {
        self.store
            .active_in_order()
            .map(Self::content_size_source)
            .collect()
    }

    pub(super) fn binding_for_mount(
        &self,
        instance: &NodeInstanceKey,
        mount: &OpenGpuiMountCapability,
    ) -> Option<Entity<OpenGpuiNodeBinding>> {
        let mounted = self.store.get(instance)?;
        mounted
            .mount
            .is_same_mount(mount)
            .then(|| mounted.binding.clone())
            .flatten()
    }

    pub(super) fn contains_mount(
        &self,
        instance: &NodeInstanceKey,
        mount: &OpenGpuiMountCapability,
    ) -> bool {
        self.store
            .get(instance)
            .is_some_and(|mounted| mounted.mount.is_same_mount(mount))
    }

    pub(super) fn has_complete_content_size_publication(
        &self,
        instance: &NodeInstanceKey,
        mount: &OpenGpuiMountCapability,
        desired_size: CanvasSize,
        policy: OpenGpuiNodeSizePolicy,
        geometry: &GeometrySnapshot,
    ) -> bool {
        let Some(mounted) = self.store.get(instance) else {
            return false;
        };
        if !mounted.mount.is_same_mount(mount)
            || !policy.sizes_are_equivalent(mounted.semantic_zoom.canonical_size(), desired_size)
        {
            return false;
        }
        // The target generation is the publication fence. Its canvas bounds may intentionally
        // differ from the document size when semantic LOD keeps native detail at 1:1 pixels.
        geometry.node(instance.node()).is_some_and(|published| {
            published.is_fresh()
                && published.lifetime() == instance.lifetime()
                && published.renderer() == instance.renderer()
                && published.layout_generation()
                    == mounted.semantic_zoom.expected_layout_generation()
        })
    }

    pub(super) fn take_active_measurement_errors(&self) -> Vec<OpenGpuiMeasurementError> {
        self.store
            .active_values()
            .flat_map(|mounted| mounted.measurement.take_errors())
            .collect()
    }

    pub(super) fn activity_registry(
        &self,
        instance: NodeInstanceKey,
    ) -> OpenGpuiNodeActivityRegistry {
        OpenGpuiNodeActivityRegistry::for_host(instance, self.activity.clone())
    }

    pub(super) fn requires_window_cleanup(&self) -> bool {
        self.activity.requires_window_cleanup()
    }

    pub(super) fn has_activity(&self, kind: OpenGpuiActivityKind) -> bool {
        self.store
            .iter()
            .any(|(_, mounted)| mounted.activity.has_activity(kind))
    }

    pub(super) fn release_pointer_capture_if_owned(&self, window: &mut Window) -> bool {
        self.store
            .iter()
            .any(|(_, mounted)| mounted.activity.release_pointer_capture_if_owned(window))
    }

    pub(super) fn snapshot(&self) -> RetainedNodeOwnerSnapshot {
        RetainedNodeOwnerSnapshot {
            mounts: self.store.snapshot(),
            activity: self.activity.stats(),
        }
    }

    pub(super) fn refresh_port_presentations(
        &mut self,
        view: &ViewSession,
        revision: GraphRevision,
        requested: &[PortId],
    ) -> Result<bool, OpenGpuiPresentationSyncError> {
        self.presentation_ports.refresh(view, revision, requested)
    }

    pub(super) fn port_presentations(
        &self,
    ) -> &[(
        PortId,
        Option<jellyflow_runtime::graph::PortConnectionSummary>,
    )] {
        self.presentation_ports.entries()
    }

    fn reconcile_mounted(
        &mut self,
        instance: &NodeInstanceKey,
        frame: &FrameNode,
        document_epoch: DocumentEpoch,
        zoom: f32,
        previous_geometry: Option<&GeometrySnapshot>,
    ) -> Option<Entity<OpenGpuiNodeBinding>> {
        let mounted = self
            .store
            .get_mut(instance)
            .expect("a visited retained mount remains active");
        if mounted.graph_presentation() == OpenGpuiNodePresentation::Detail
            && let Some(geometry) = previous_geometry
        {
            mounted
                .semantic_zoom
                .remember_detail_geometry(frame.id(), geometry);
        }

        mounted.semantic_zoom.reconcile(frame, document_epoch);
        mounted
            .scale_state
            .reconcile(mounted.registration_policy.scale_policy(), zoom);
        mounted.selected = frame.selected();
        if mounted.binding_revision == frame.binding_revision() {
            return None;
        }
        mounted.binding.clone()
    }

    fn content_size_source(
        (instance, mounted): (&NodeInstanceKey, &MountedNode),
    ) -> RetainedContentSizeSource {
        RetainedContentSizeSource {
            instance: instance.clone(),
            mount: mounted.mount.clone(),
            measurement: mounted.measurement.clone(),
            presentation: mounted.graph_measurement_source(),
            registration_policy: mounted.registration_policy,
            canonical_size: mounted.semantic_zoom.canonical_size(),
        }
    }

    pub(super) fn retire_instance(
        &mut self,
        instance: &NodeInstanceKey,
    ) -> Option<RetainedNodeRelease> {
        let retired = self.store.retire(instance)?;
        Some(self.hard_release(retired))
    }

    fn instance_is_retired(
        instance: &NodeInstanceKey,
        frame: &BTreeMap<NodeId, NodeInstanceKey>,
        view: &ViewSession,
    ) -> bool {
        if let Some(current) = frame.get(&instance.node()) {
            return current != instance;
        }
        match view.node_lifetime_key(instance.node()) {
            Ok(Some(current)) => current != instance.lifetime(),
            Ok(None) | Err(jellyflow_runtime::view::ViewSessionError::Closed { .. }) => true,
            Err(_) => false,
        }
    }

    fn hard_release(&mut self, retired: RetiredMount<MountedNode>) -> RetainedNodeRelease {
        self.clear_hover_for_instance(&retired.instance);
        RetainedNodeRelease {
            instance: retired.instance,
            mounted: Some(retired.value),
            kind: RetainedNodeReleaseKind::HardRetirement,
        }
    }

    fn evicted_release(&mut self, evicted: EvictedMount<MountedNode>) -> RetainedNodeRelease {
        self.clear_hover_for_instance(&evicted.instance);
        RetainedNodeRelease {
            instance: evicted.instance,
            mounted: Some(evicted.value),
            kind: RetainedNodeReleaseKind::Eviction,
        }
    }

    fn clear_hover_for_instance(&mut self, instance: &NodeInstanceKey) -> bool {
        let node_matches = self.hovered_node.as_ref() == Some(instance);
        let port_matches = self
            .hovered_port
            .is_some_and(|port| port.owner() == instance.lifetime());
        (node_matches || port_matches) && self.clear_hover()
    }

    fn advance_binding(
        binding: &Entity<OpenGpuiNodeBinding>,
        revision: GraphRevision,
        cx: &mut open_gpui::Context<OpenGpuiSceneHost>,
    ) -> Result<OpenGpuiBindingAdvance, OpenGpuiNodeBindingError> {
        let outcome = binding.update(cx, |binding, cx| {
            let outcome = binding.advance(revision, cx);
            cx.notify();
            outcome
        });
        Self::finish_binding_advance(outcome, cx)
    }

    fn advance_binding_from_frame(
        binding: &Entity<OpenGpuiNodeBinding>,
        frame: &FrameNode,
        cx: &mut open_gpui::Context<OpenGpuiSceneHost>,
    ) -> Result<OpenGpuiBindingAdvance, OpenGpuiNodeBindingError> {
        let outcome = binding.update(cx, |binding, cx| {
            let outcome = binding.advance_from_frame(frame, cx);
            cx.notify();
            outcome
        });
        Self::finish_binding_advance(outcome, cx)
    }

    fn finish_binding_advance(
        outcome: Result<OpenGpuiBindingAdvance, OpenGpuiNodeBindingError>,
        cx: &mut open_gpui::Context<OpenGpuiSceneHost>,
    ) -> Result<OpenGpuiBindingAdvance, OpenGpuiNodeBindingError> {
        match outcome {
            Ok(OpenGpuiBindingAdvance::Applied) => Ok(OpenGpuiBindingAdvance::Applied),
            Ok(OpenGpuiBindingAdvance::Retry) => {
                cx.notify();
                Ok(OpenGpuiBindingAdvance::Retry)
            }
            Err(error) => Err(error),
        }
    }
}
