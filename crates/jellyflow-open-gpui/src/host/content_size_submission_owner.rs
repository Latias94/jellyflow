use std::collections::{BTreeMap, BTreeSet, VecDeque};
#[cfg(test)]
use std::future::Future;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
#[cfg(test)]
use std::pin::Pin;
use std::rc::Rc;
#[cfg(test)]
use std::task::{Context as TaskContext, Poll, Waker};

use jellyflow_core::{CanvasSize, NodeId};
use jellyflow_runtime::commands::{GraphCommand, NodeGeometryEdit};
use jellyflow_runtime::view::{NodeInstanceKey, ViewSession};
use open_gpui::{Context, Task};

use crate::authoring::{OpenGpuiNodeAuthoring, OpenGpuiNodeAuthoringError};
use crate::binding::{OpenGpuiMountCapability, OpenGpuiNodeBinding};
#[cfg(test)]
use crate::command_journal::OpenGpuiCommandPrefixBarrierError;
use crate::command_journal::{
    OpenGpuiCommandJournal, OpenGpuiCommandPrefixBarrierCancellation,
    OpenGpuiCommandPrefixBarrierResult, OpenGpuiCommandPrefixPermit,
};
use crate::managed_mutation::{
    MutationDispatchError, MutationLayoutEffect, MutationRequest, MutationSettlement,
    OpenGpuiManagedMutation,
};
use crate::port_handle::{OpenGpuiMeasurementError, OpenGpuiPendingMeasurements};
use crate::size_policy::{
    OpenGpuiNodeSizeCommand, OpenGpuiNodeSizeEpoch, OpenGpuiNodeSizePolicy, OpenGpuiNodeSizeState,
    OpenGpuiNodeSizeTicket, OpenGpuiNodeSizeUpdate,
};

use super::retained_node_owner::{RetainedContentSizeSource, RetainedNodeOwner};
use super::{OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY, OpenGpuiSceneHost};

const CONTENT_SIZE_SUBMISSION_BATCH_LIMIT: usize = OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY;

#[derive(Default)]
pub(super) struct ContentSizeEffects {
    diagnostics: Vec<String>,
    request_next_frame: bool,
}

impl ContentSizeEffects {
    pub(super) fn diagnostics(self) -> impl Iterator<Item = String> {
        self.diagnostics.into_iter()
    }

    pub(super) const fn requests_next_frame(&self) -> bool {
        self.request_next_frame
    }

    fn merge(&mut self, mut other: Self) {
        self.diagnostics.append(&mut other.diagnostics);
        self.request_next_frame |= other.request_next_frame;
    }
}

struct ContentSizeNodeState {
    mount: OpenGpuiMountCapability,
    policy: OpenGpuiNodeSizePolicy,
    canonical_size: CanvasSize,
    content_epoch: u64,
    content_epoch_live: bool,
    reducer: OpenGpuiNodeSizeState,
}

impl ContentSizeNodeState {
    fn new(
        source: &RetainedContentSizeSource,
        environment_epoch: u64,
    ) -> Option<ContentSizeNodeState> {
        let policy = source.registration_policy.size_policy();
        let reducer = OpenGpuiNodeSizeState::new(
            OpenGpuiNodeSizeEpoch::new(0, environment_epoch),
            source.canonical_size,
        )
        .ok()?;
        Some(Self {
            mount: source.mount.clone(),
            policy,
            canonical_size: source.canonical_size,
            content_epoch: 0,
            content_epoch_live: true,
            reducer,
        })
    }

    fn epoch(&self, environment_epoch: u64) -> OpenGpuiNodeSizeEpoch {
        OpenGpuiNodeSizeEpoch::new(self.content_epoch, environment_epoch)
    }

    fn matches_mount(&self, mount: &OpenGpuiMountCapability) -> bool {
        self.mount.is_same_mount(mount)
    }
}

#[derive(Clone, Debug)]
struct ContentSizeTarget {
    instance: NodeInstanceKey,
    mount: OpenGpuiMountCapability,
    policy: OpenGpuiNodeSizePolicy,
    command: OpenGpuiNodeSizeCommand,
}

impl ContentSizeTarget {
    fn ticket(&self) -> OpenGpuiNodeSizeTicket {
        self.command.ticket()
    }

    fn desired_size(&self) -> CanvasSize {
        self.command.size()
    }
}

#[derive(Clone, Debug)]
struct ContentSizeBatchIntent {
    targets: Vec<ContentSizeTarget>,
}

impl ContentSizeBatchIntent {
    fn matches(&self, targets: &[ContentSizeTarget]) -> bool {
        self.targets.len() == targets.len()
            && self.targets.iter().zip(targets).all(|(left, right)| {
                left.instance == right.instance
                    && left.mount.is_same_mount(&right.mount)
                    && left.command.ticket() == right.command.ticket()
                    && left.command.epoch() == right.command.epoch()
                    && left.command.size() == right.command.size()
            })
    }
}

struct PendingContentSizePrefix {
    owner_lineage: Rc<()>,
    prefix_lineage: Rc<()>,
    targets: Vec<ContentSizeTarget>,
    task: Option<Task<()>>,
    cancellation: Option<OpenGpuiCommandPrefixBarrierCancellation>,
}

struct PendingContentSizeCapacityWait {
    owner_lineage: Rc<()>,
    wait_lineage: Rc<()>,
    task: Option<Task<()>>,
}

struct InFlightContentSizeMutation {
    targets: Vec<ContentSizeTarget>,
    mutation: OpenGpuiManagedMutation<ContentSizeBatchIntent>,
    last_projection: Option<MutationSettlement>,
}

#[derive(Clone, Debug)]
struct AwaitingContentSizePublication {
    target: ContentSizeTarget,
    recovery: ContentSizePublicationRecovery,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ContentSizePublicationRecovery {
    Fresh,
    Requested,
    Dirty,
}

impl AwaitingContentSizePublication {
    fn new(target: ContentSizeTarget) -> Self {
        Self {
            target,
            recovery: ContentSizePublicationRecovery::Fresh,
        }
    }

    fn observe_missing_geometry(
        &mut self,
        allow_recovery_frame: bool,
        miss: ContentSizePublicationMiss,
    ) -> MissingContentSizePublication {
        self.recovery
            .observe_missing_geometry(allow_recovery_frame, miss)
    }

    fn needs_frame(&self) -> bool {
        self.recovery.needs_frame()
    }

    fn blocks_new_batch(&self) -> bool {
        self.recovery.blocks_new_batch()
    }
}

impl ContentSizePublicationRecovery {
    fn observe_missing_geometry(
        &mut self,
        allow_recovery_frame: bool,
        miss: ContentSizePublicationMiss,
    ) -> MissingContentSizePublication {
        if !allow_recovery_frame {
            return MissingContentSizePublication::Wait;
        }
        match self {
            Self::Fresh => {
                *self = Self::Requested;
                MissingContentSizePublication::RequestRecoveryFrame
            }
            Self::Requested => {
                *self = Self::Dirty;
                MissingContentSizePublication::RecoveryDeferred { miss }
            }
            Self::Dirty => {
                *self = Self::Requested;
                MissingContentSizePublication::RequestRecoveryFrame
            }
        }
    }

    const fn needs_frame(self) -> bool {
        matches!(self, Self::Fresh | Self::Requested)
    }

    const fn blocks_new_batch(self) -> bool {
        matches!(self, Self::Fresh | Self::Requested)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MissingContentSizePublication {
    Wait,
    RequestRecoveryFrame,
    RecoveryDeferred { miss: ContentSizePublicationMiss },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ContentSizePublicationMiss {
    IncompleteGeometry { materialized: bool },
    StaleFeedback,
    GeometryUnavailable,
}

impl ContentSizePublicationMiss {
    const fn reason(self) -> &'static str {
        match self {
            Self::IncompleteGeometry { materialized: true } => {
                "the node was materialized but complete geometry was not published"
            }
            Self::IncompleteGeometry {
                materialized: false,
            } => "the required node could not be materialized",
            Self::StaleFeedback => "the replacement layout feedback was stale",
            Self::GeometryUnavailable => "the authoritative geometry snapshot was unavailable",
        }
    }
}

pub(super) struct ContentSizeSubmissionOwner {
    nodes: BTreeMap<NodeInstanceKey, ContentSizeNodeState>,
    environment_epoch: u64,
    environment_epoch_live: bool,
    owner_lineage: Rc<()>,
    queued: BTreeMap<NodeInstanceKey, ContentSizeTarget>,
    prefix: Option<PendingContentSizePrefix>,
    ready: VecDeque<(Rc<()>, Rc<()>, OpenGpuiCommandPrefixBarrierResult)>,
    capacity_wait: Option<PendingContentSizeCapacityWait>,
    capacity_wait_ready: bool,
    in_flight: Option<InFlightContentSizeMutation>,
    // Authoritative targets remain frame-materialized until their matching geometry is published
    // or a newer authority replaces them. Dirty targets pause self-scheduling between bounded
    // recovery attempts so unrelated content-size work can continue without a frame loop.
    awaiting_publication: BTreeMap<NodeInstanceKey, AwaitingContentSizePublication>,
}

impl Default for ContentSizeSubmissionOwner {
    fn default() -> Self {
        Self {
            nodes: BTreeMap::new(),
            environment_epoch: 0,
            environment_epoch_live: true,
            owner_lineage: Rc::new(()),
            queued: BTreeMap::new(),
            prefix: None,
            ready: VecDeque::new(),
            capacity_wait: None,
            capacity_wait_ready: false,
            in_flight: None,
            awaiting_publication: BTreeMap::new(),
        }
    }
}

impl ContentSizeSubmissionOwner {
    pub(super) fn environment_epoch(&self) -> u64 {
        self.environment_epoch
    }

    pub(super) fn synchronize_retained(
        &mut self,
        sources: Vec<RetainedContentSizeSource>,
    ) -> ContentSizeEffects {
        let sources = sources
            .into_iter()
            .map(|source| (source.instance.clone(), source))
            .collect::<BTreeMap<_, _>>();
        let invalid = self
            .nodes
            .iter()
            .filter(|(instance, state)| {
                let Some(source) = sources.get(instance) else {
                    return true;
                };
                if !state.matches_mount(&source.mount) {
                    return true;
                }
                let policy = source.registration_policy.size_policy();
                state.policy != policy
                    || (state.canonical_size != source.canonical_size
                        && !state
                            .reducer
                            .recognizes_authoritative_size(policy, source.canonical_size))
            })
            .map(|(instance, _)| instance.clone())
            .collect::<BTreeSet<_>>();

        let mut effects = self.retire_interruptible_work_touching(&invalid);
        self.nodes.retain(|instance, state| {
            sources
                .get(instance)
                .is_some_and(|source| state.matches_mount(&source.mount))
        });

        for (instance, source) in sources {
            let policy = source.registration_policy.size_policy();
            if let Some(state) = self.nodes.get_mut(&instance) {
                let reset = invalid.contains(&instance);
                let content_epoch_was_live = state.content_epoch_live;
                state.policy = policy;
                state.canonical_size = source.canonical_size;
                if self.environment_epoch_live
                    && reset
                    && advance_epoch(&mut state.content_epoch, &mut state.content_epoch_live)
                {
                    if let Err(error) = state
                        .reducer
                        .reset_epoch(state.epoch(self.environment_epoch), source.canonical_size)
                    {
                        effects.diagnostics.push(format!(
                            "failed to reset content size convergence for node {:?}: {error}",
                            instance.node()
                        ));
                    }
                } else if reset && content_epoch_was_live && !state.content_epoch_live {
                    effects.diagnostics.push(format!(
                        "content size epoch identity exhausted for node {:?}; measurements are disabled for this mount",
                        instance.node()
                    ));
                }
                continue;
            }
            let Some(state) = ContentSizeNodeState::new(&source, self.environment_epoch) else {
                effects.diagnostics.push(format!(
                    "ignored invalid authoritative content size for node {:?}",
                    instance.node()
                ));
                continue;
            };
            self.nodes.insert(instance, state);
        }
        effects.request_next_frame |= self.needs_frame();
        effects
    }

    pub(super) fn advance_environment(&mut self) -> ContentSizeEffects {
        let mut effects = self.retire_all_interruptible_work();
        let environment_epoch_was_live = self.environment_epoch_live;
        if !advance_epoch(
            &mut self.environment_epoch,
            &mut self.environment_epoch_live,
        ) {
            if environment_epoch_was_live {
                effects.diagnostics.push(
                    "content size environment epoch identity exhausted; measurements are disabled"
                        .to_owned(),
                );
            }
            return effects;
        }
        for (instance, state) in &mut self.nodes {
            if !state.content_epoch_live {
                continue;
            }
            if let Err(error) = state
                .reducer
                .reset_epoch(state.epoch(self.environment_epoch), state.canonical_size)
            {
                effects.diagnostics.push(format!(
                    "failed to advance content size environment for node {:?}: {error}",
                    instance.node()
                ));
            }
        }
        effects.request_next_frame = true;
        effects
    }

    pub(super) fn invalidate_content(
        &mut self,
        instance: &NodeInstanceKey,
        mount: &OpenGpuiMountCapability,
    ) -> (bool, ContentSizeEffects) {
        let Some(state) = self.nodes.get(instance) else {
            return (false, ContentSizeEffects::default());
        };
        if !state.matches_mount(mount) {
            return (false, ContentSizeEffects::default());
        }
        let mut effects =
            self.retire_interruptible_work_touching(&BTreeSet::from([instance.clone()]));
        let state = self
            .nodes
            .get_mut(instance)
            .expect("the exact content-size mount remains installed");
        let content_epoch_was_live = state.content_epoch_live;
        if !self.environment_epoch_live
            || !advance_epoch(&mut state.content_epoch, &mut state.content_epoch_live)
        {
            if self.environment_epoch_live && content_epoch_was_live {
                effects.diagnostics.push(format!(
                    "content size epoch identity exhausted for node {:?}; measurements are disabled for this mount",
                    instance.node()
                ));
            }
            return (true, effects);
        }
        if let Err(error) = state
            .reducer
            .reset_epoch(state.epoch(self.environment_epoch), state.canonical_size)
        {
            effects.diagnostics.push(format!(
                "failed to invalidate content size for node {:?}: {error}",
                instance.node()
            ));
        }
        effects.request_next_frame = true;
        (true, effects)
    }

    pub(super) fn retire_mounts<'a>(
        &mut self,
        mounts: impl IntoIterator<Item = (&'a NodeInstanceKey, &'a OpenGpuiMountCapability)>,
    ) -> ContentSizeEffects {
        let instances = mounts
            .into_iter()
            .filter(|(instance, mount)| {
                self.nodes
                    .get(*instance)
                    .is_some_and(|state| state.matches_mount(mount))
            })
            .map(|(instance, _)| instance.clone())
            .collect::<BTreeSet<_>>();
        let effects = self.retire_interruptible_work_touching(&instances);
        for instance in instances {
            self.nodes.remove(&instance);
        }
        effects
    }

    pub(super) fn activate_measurements(
        &self,
        sources: Vec<RetainedContentSizeSource>,
        measurements: &Rc<OpenGpuiPendingMeasurements>,
    ) -> Vec<OpenGpuiMeasurementError> {
        sources
            .into_iter()
            .filter_map(|source| {
                let state = self.nodes.get(&source.instance)?;
                (self.environment_epoch_live
                    && state.content_epoch_live
                    && state.matches_mount(&source.mount))
                .then(|| {
                    source.measurement.activate(
                        measurements,
                        source.presentation,
                        state.content_epoch,
                    )
                })
            })
            .filter_map(Result::err)
            .collect()
    }

    pub(super) fn observe_measurements(
        &mut self,
        measurements: &OpenGpuiPendingMeasurements,
        accepted_nodes: &BTreeSet<NodeId>,
    ) -> ContentSizeEffects {
        if !self.environment_epoch_live
            || measurements.environment_epoch() != self.environment_epoch
        {
            return ContentSizeEffects::default();
        }
        let mut updates = Vec::new();
        for (instance, content_epoch, measured_size) in measurements.content_sizes() {
            if !accepted_nodes.contains(&instance.node()) {
                continue;
            }
            let Some(state) = self.nodes.get_mut(&instance) else {
                continue;
            };
            if !state.content_epoch_live || content_epoch != state.content_epoch {
                continue;
            }
            let update = state.reducer.observe(
                state.policy,
                state.epoch(self.environment_epoch),
                state.canonical_size,
                measured_size,
            );
            if !update.is_idle() {
                updates.push((instance, update));
            }
        }
        let mut effects = ContentSizeEffects::default();
        for (instance, update) in updates {
            self.queue_update(instance, update, &mut effects);
        }
        effects.request_next_frame |= self.needs_frame();
        effects
    }

    pub(super) fn observe_in_flight(
        &mut self,
        retained: &RetainedNodeOwner,
        view: &ViewSession,
        cx: &mut Context<OpenGpuiSceneHost>,
    ) -> ContentSizeEffects {
        let Some(in_flight) = self.in_flight.as_ref() else {
            return ContentSizeEffects::default();
        };
        let mut authoring = None;
        let mut closed_error = None;
        for target in &in_flight.targets {
            let Some(binding) = retained.binding_for_mount(&target.instance, &target.mount) else {
                continue;
            };
            match binding.read(cx).authoring() {
                Ok(candidate) => {
                    authoring = Some(candidate);
                    break;
                }
                Err(error) => {
                    if matches!(&error, OpenGpuiNodeAuthoringError::Closed { .. }) {
                        closed_error = Some(error);
                        break;
                    }
                }
            }
        }
        let satisfied = authoring.as_ref().is_some_and(|authoring| {
            self.batch_is_authoritative(&in_flight.targets, retained, authoring)
        });
        let settlement = {
            let in_flight = self
                .in_flight
                .as_mut()
                .expect("the observed content-size mutation remains installed");
            let InFlightContentSizeMutation {
                targets, mutation, ..
            } = in_flight;
            match authoring.as_ref() {
                Some(authoring) => {
                    mutation.observe(authoring, |intent| intent.matches(targets) && satisfied, cx)
                }
                None => match closed_error.as_ref() {
                    Some(error) => mutation.observe_authoring_error(error),
                    None => mutation.observe_without_authority(cx),
                },
            }
        };
        let mut effects = self.consume_shared_settlement(
            settlement,
            authoring.as_ref().map(|authoring| (retained, authoring)),
        );
        effects.merge(self.observe_publications(
            retained,
            view,
            false,
            &BTreeSet::new(),
            &BTreeSet::new(),
        ));
        effects
    }

    pub(super) fn schedule_prefix(
        &mut self,
        commands: Rc<OpenGpuiCommandJournal>,
        retained: &RetainedNodeOwner,
        cx: &mut Context<OpenGpuiSceneHost>,
    ) -> ContentSizeEffects {
        self.consume_capacity_ready();
        if self.queued.is_empty() {
            self.cancel_capacity_wait();
            return ContentSizeEffects::default();
        }
        if self.prefix.is_some()
            || self.in_flight.is_some()
            || self
                .awaiting_publication
                .values()
                .any(AwaitingContentSizePublication::blocks_new_batch)
        {
            return ContentSizeEffects::default();
        }
        let targets = self.take_admissible_targets(retained);
        if targets.is_empty() {
            self.begin_capacity_wait(retained, cx);
            return ContentSizeEffects::default();
        }
        self.cancel_capacity_wait();
        let barrier = match catch_unwind(AssertUnwindSafe(|| commands.capture_prefix_barrier())) {
            Ok(Ok(barrier)) => barrier,
            Ok(Err(error)) if error.is_pre_admission_backpressure() => {
                let mut effects = ContentSizeEffects {
                    diagnostics: Vec::new(),
                    request_next_frame: true,
                };
                self.requeue_targets(targets, &mut effects);
                return effects;
            }
            Ok(Err(error)) => {
                let mut effects = ContentSizeEffects::default();
                effects.diagnostics.push(format!(
                    "failed to capture content size command prefix: {error}"
                ));
                self.complete_targets_failed(targets, &mut effects);
                return effects;
            }
            Err(payload) => {
                let mut effects = ContentSizeEffects::default();
                self.requeue_targets(targets, &mut effects);
                resume_unwind(payload);
            }
        };
        let owner_lineage = Rc::clone(&self.owner_lineage);
        let prefix_lineage = Rc::new(());
        let cancellation = barrier.cancellation();
        let host = cx.weak_entity();
        let callback_owner_lineage = Rc::clone(&owner_lineage);
        let callback_prefix_lineage = Rc::clone(&prefix_lineage);
        let task = catch_unwind(AssertUnwindSafe(|| {
            cx.spawn(async move |_, cx| {
                let result = barrier.await;
                let _ = host.update(cx, |host, cx| {
                    if host.content_sizes.push_ready(
                        callback_owner_lineage,
                        callback_prefix_lineage,
                        result,
                    ) {
                        cx.notify();
                    }
                });
            })
        }));
        let task = match task {
            Ok(task) => task,
            Err(payload) => {
                let mut effects = ContentSizeEffects::default();
                self.requeue_targets(targets, &mut effects);
                resume_unwind(payload);
            }
        };
        self.prefix = Some(PendingContentSizePrefix {
            owner_lineage,
            prefix_lineage,
            targets,
            task: Some(task),
            cancellation: Some(cancellation),
        });
        ContentSizeEffects::default()
    }

    fn take_admissible_targets(&mut self, retained: &RetainedNodeOwner) -> Vec<ContentSizeTarget> {
        let candidate_instances = self.queued.keys().cloned().collect::<Vec<_>>();
        retained
            .content_size_admission_batch(&candidate_instances, CONTENT_SIZE_SUBMISSION_BATCH_LIMIT)
            .into_iter()
            .filter_map(|instance| self.queued.remove(&instance))
            .collect()
    }

    #[cfg(test)]
    pub(super) fn dispatch_queued_for_test(
        &mut self,
        commands: Rc<OpenGpuiCommandJournal>,
        retained: &RetainedNodeOwner,
        cx: &mut Context<OpenGpuiSceneHost>,
    ) -> ContentSizeEffects {
        assert!(self.prefix.is_none());
        assert!(self.in_flight.is_none());
        let targets = self.take_admissible_targets(retained);
        assert!(
            !targets.is_empty(),
            "test dispatch requires one admissible target"
        );
        let mut barrier = commands
            .capture_prefix_barrier()
            .expect("capture empty content-size test prefix");
        let mut context = TaskContext::from_waker(Waker::noop());
        let permit = match Future::poll(Pin::new(&mut barrier), &mut context) {
            Poll::Ready(Ok(permit)) => permit,
            Poll::Ready(Err(error)) => panic!("content-size test prefix failed: {error}"),
            Poll::Pending => panic!("empty content-size test prefix remained pending"),
        };
        self.dispatch_prefix(targets, permit, retained, cx)
    }

    fn requeue_targets(
        &mut self,
        targets: Vec<ContentSizeTarget>,
        effects: &mut ContentSizeEffects,
    ) {
        let mut obsolete = Vec::new();
        for target in targets {
            if self.target_submission_is_current(&target) {
                self.queued.entry(target.instance.clone()).or_insert(target);
            } else {
                obsolete.push(target);
            }
        }
        self.complete_targets_obsolete(obsolete, effects);
    }

    pub(super) fn process_one_ready_prefix(
        &mut self,
        retained: &RetainedNodeOwner,
        cx: &mut Context<OpenGpuiSceneHost>,
    ) -> ContentSizeEffects {
        if self.in_flight.is_some() {
            return ContentSizeEffects::default();
        }
        let Some((owner_lineage, prefix_lineage, result)) = self.ready.pop_front() else {
            return ContentSizeEffects::default();
        };
        let matches = self.prefix.as_ref().is_some_and(|prefix| {
            Rc::ptr_eq(&prefix.owner_lineage, &owner_lineage)
                && Rc::ptr_eq(&prefix.prefix_lineage, &prefix_lineage)
        });
        if !matches {
            drop(result);
            return ContentSizeEffects::default();
        }
        let mut prefix = self
            .prefix
            .take()
            .expect("a matching content-size prefix remains installed");
        prefix.task.take();
        prefix.cancellation.take();
        match result {
            Ok(permit) => self.dispatch_prefix(prefix.targets, permit, retained, cx),
            Err(error) => {
                let mut effects = ContentSizeEffects::default();
                effects.diagnostics.push(format!(
                    "content size command prefix did not settle: {error}"
                ));
                self.complete_targets_failed(prefix.targets, &mut effects);
                effects.request_next_frame |= self.needs_frame();
                effects
            }
        }
    }

    pub(super) fn needs_frame(&self) -> bool {
        !self.ready.is_empty()
            || self.capacity_wait_ready
            || self
                .awaiting_publication
                .values()
                .any(AwaitingContentSizePublication::needs_frame)
            || (!self.queued.is_empty()
                && self.prefix.is_none()
                && self.capacity_wait.is_none()
                && self.in_flight.is_none())
    }

    pub(super) fn awaiting_publication_nodes(&self) -> BTreeSet<NodeId> {
        self.awaiting_publication
            .values()
            .map(|publication| publication.target.instance.node())
            .collect()
    }

    pub(super) fn clear(&mut self) {
        self.owner_lineage = Rc::new(());
        self.prefix = None;
        self.ready.clear();
        self.cancel_capacity_wait();
        self.in_flight = None;
        self.awaiting_publication.clear();
        self.queued.clear();
        self.nodes.clear();
    }

    fn push_ready(
        &mut self,
        owner_lineage: Rc<()>,
        prefix_lineage: Rc<()>,
        result: OpenGpuiCommandPrefixBarrierResult,
    ) -> bool {
        let current_prefix = self.prefix.as_ref().is_some_and(|prefix| {
            Rc::ptr_eq(&prefix.owner_lineage, &owner_lineage)
                && Rc::ptr_eq(&prefix.prefix_lineage, &prefix_lineage)
        });
        if !Rc::ptr_eq(&owner_lineage, &self.owner_lineage) || !current_prefix {
            drop(result);
            return false;
        }
        self.ready
            .push_back((owner_lineage, prefix_lineage, result));
        true
    }

    fn begin_capacity_wait(
        &mut self,
        retained: &RetainedNodeOwner,
        cx: &mut Context<OpenGpuiSceneHost>,
    ) {
        if self.capacity_wait.is_some() {
            return;
        }
        let wait = retained.wait_for_content_size_capacity();
        let owner_lineage = Rc::clone(&self.owner_lineage);
        let wait_lineage = Rc::new(());
        let host = cx.weak_entity();
        let callback_owner_lineage = Rc::clone(&owner_lineage);
        let callback_wait_lineage = Rc::clone(&wait_lineage);
        let task = cx.spawn(async move |_, cx| {
            wait.await;
            let _ = host.update(cx, |host, cx| {
                if host
                    .content_sizes
                    .push_capacity_ready(callback_owner_lineage, callback_wait_lineage)
                {
                    cx.notify();
                }
            });
        });
        self.capacity_wait = Some(PendingContentSizeCapacityWait {
            owner_lineage,
            wait_lineage,
            task: Some(task),
        });
    }

    fn push_capacity_ready(&mut self, owner_lineage: Rc<()>, wait_lineage: Rc<()>) -> bool {
        let current = self.capacity_wait.as_ref().is_some_and(|pending| {
            Rc::ptr_eq(&pending.owner_lineage, &owner_lineage)
                && Rc::ptr_eq(&pending.wait_lineage, &wait_lineage)
        });
        if Rc::ptr_eq(&owner_lineage, &self.owner_lineage) && current {
            self.capacity_wait_ready = true;
            true
        } else {
            false
        }
    }

    fn consume_capacity_ready(&mut self) {
        if !self.capacity_wait_ready {
            return;
        }
        if let Some(mut pending) = self.capacity_wait.take() {
            pending.task.take();
        }
        self.capacity_wait_ready = false;
    }

    fn cancel_capacity_wait(&mut self) {
        self.capacity_wait = None;
        self.capacity_wait_ready = false;
    }

    fn dispatch_prefix(
        &mut self,
        targets: Vec<ContentSizeTarget>,
        permit: OpenGpuiCommandPrefixPermit,
        retained: &RetainedNodeOwner,
        cx: &mut Context<OpenGpuiSceneHost>,
    ) -> ContentSizeEffects {
        if targets.is_empty() || !self.batch_mounts_are_current(&targets, retained) {
            permit.abandon();
            let mut effects = ContentSizeEffects::default();
            self.retire_targets(targets, &mut effects);
            effects.request_next_frame |= self.needs_frame();
            return effects;
        }

        let bindings = targets
            .iter()
            .map(|target| retained.binding_for_mount(&target.instance, &target.mount))
            .collect::<Option<Vec<open_gpui::Entity<OpenGpuiNodeBinding>>>>();
        let Some(bindings) = bindings else {
            permit.abandon();
            let mut effects = ContentSizeEffects::default();
            self.retire_targets(targets, &mut effects);
            effects.request_next_frame |= self.needs_frame();
            return effects;
        };
        let authoring = match bindings[0].read(cx).authoring() {
            Ok(authoring) => authoring,
            Err(error) => {
                permit.abandon();
                let mut effects = ContentSizeEffects::default();
                effects.diagnostics.push(format!(
                    "failed to open content size authoring facts: {error}"
                ));
                self.retire_targets(targets, &mut effects);
                return effects;
            }
        };
        if !Self::batch_lifetimes_match_authoring(&targets, &authoring) {
            permit.abandon();
            let mut effects = ContentSizeEffects::default();
            effects
                .diagnostics
                .push("content size targets no longer match one authoring snapshot".to_owned());
            self.retire_targets(targets, &mut effects);
            return effects;
        }
        let command = GraphCommand::set_node_geometry(
            authoring.version(),
            targets.iter().map(|target| {
                NodeGeometryEdit::new(target.instance.node()).with_size(Some(target.desired_size()))
            }),
        )
        .without_history();
        let owners = bindings
            .iter()
            .map(|binding| binding.read(cx).managed_mutation_owner())
            .collect::<Vec<_>>();
        let Some(mutation) = OpenGpuiManagedMutation::mount_set_draining_admitted(owners) else {
            permit.abandon();
            let mut effects = ContentSizeEffects::default();
            effects
                .diagnostics
                .push("content size targets did not share one exact view provenance".to_owned());
            self.retire_targets(targets, &mut effects);
            return effects;
        };
        let intent = ContentSizeBatchIntent {
            targets: targets.clone(),
        };
        self.in_flight = Some(InFlightContentSizeMutation {
            targets,
            mutation,
            last_projection: None,
        });
        let dispatched = catch_unwind(AssertUnwindSafe(|| {
            self.in_flight
                .as_mut()
                .expect("content-size mutation is installed before admission")
                .mutation
                .dispatch_with_admission(
                    authoring,
                    command,
                    MutationRequest::new(intent),
                    MutationLayoutEffect::Preserve,
                    move |authoring, command| authoring.admit_reserved(command, permit),
                    cx,
                )
        }));
        match dispatched {
            Ok(Ok(settlement)) => self.consume_shared_settlement(settlement, None),
            Ok(Err(error)) => {
                let mut effects = ContentSizeEffects::default();
                let is_capacity_backpressure = matches!(
                    &error,
                    MutationDispatchError::NotDispatched { diagnostic }
                        if diagnostic.is_native_activity_backpressure()
                );
                if !is_capacity_backpressure {
                    effects
                        .diagnostics
                        .push(format!("content size mutation was not dispatched: {error}"));
                }
                let settlement = self
                    .in_flight
                    .as_ref()
                    .expect("failed dispatch preserves its managed mutation")
                    .mutation
                    .settlement();
                effects.merge(self.consume_shared_settlement(settlement, None));
                effects
            }
            Err(payload) => resume_unwind(payload),
        }
    }

    fn consume_shared_settlement(
        &mut self,
        settlement: MutationSettlement,
        observation: Option<(&RetainedNodeOwner, &OpenGpuiNodeAuthoring)>,
    ) -> ContentSizeEffects {
        let mut effects = ContentSizeEffects::default();
        match &settlement {
            MutationSettlement::Pending { .. } => {}
            MutationSettlement::Uncertain { diagnostic, .. } => {
                let changed = self.in_flight.as_ref().is_some_and(|in_flight| {
                    in_flight.last_projection.as_ref() != Some(&settlement)
                });
                if changed {
                    effects.diagnostics.push(format!(
                        "content size mutation outcome is uncertain: {diagnostic}"
                    ));
                }
            }
            MutationSettlement::Satisfied { .. } => {
                let targets = self
                    .in_flight
                    .take()
                    .map(|in_flight| in_flight.targets)
                    .unwrap_or_default();
                let (current, obsolete) = self.partition_settlement_targets(targets, observation);
                self.complete_targets_obsolete(obsolete, &mut effects);
                for target in current {
                    let instance = target.instance.clone();
                    if let Some(stale) = self
                        .awaiting_publication
                        .insert(instance, AwaitingContentSizePublication::new(target))
                    {
                        self.retire_targets(vec![stale.target], &mut effects);
                    }
                }
            }
            MutationSettlement::Retryable { diagnostic, .. } => {
                let targets = self
                    .in_flight
                    .take()
                    .map(|in_flight| in_flight.targets)
                    .unwrap_or_default();
                let (targets, obsolete) = self.partition_settlement_targets(targets, observation);
                self.complete_targets_obsolete(obsolete, &mut effects);
                if diagnostic.is_native_activity_backpressure() {
                    self.requeue_targets(targets, &mut effects);
                } else {
                    effects.diagnostics.push(format!(
                        "content size mutation failed definitively: {diagnostic}"
                    ));
                    self.complete_targets_failed(targets, &mut effects);
                }
            }
            MutationSettlement::Retired { diagnostic }
            | MutationSettlement::Closed { diagnostic, .. } => {
                effects
                    .diagnostics
                    .push(format!("content size mutation owner retired: {diagnostic}"));
                let targets = self
                    .in_flight
                    .take()
                    .map(|in_flight| in_flight.targets)
                    .unwrap_or_default();
                self.retire_targets(targets, &mut effects);
            }
            MutationSettlement::Ready => {
                let targets = self
                    .in_flight
                    .take()
                    .map(|in_flight| in_flight.targets)
                    .unwrap_or_default();
                if !targets.is_empty() {
                    effects.diagnostics.push(
                        "content size mutation returned to ready before settlement".to_owned(),
                    );
                    self.complete_targets_failed(targets, &mut effects);
                }
            }
        }
        if let Some(in_flight) = self.in_flight.as_mut() {
            in_flight.last_projection = Some(settlement);
        }
        effects.request_next_frame |= self.needs_frame();
        effects
    }

    fn observe_publications(
        &mut self,
        retained: &RetainedNodeOwner,
        view: &ViewSession,
        allow_recovery_frame: bool,
        materialized_instances: &BTreeSet<NodeInstanceKey>,
        stale_nodes: &BTreeSet<NodeId>,
    ) -> ContentSizeEffects {
        let mut effects = ContentSizeEffects::default();
        if self.awaiting_publication.is_empty() {
            return effects;
        }
        let instances = self
            .awaiting_publication
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        let geometry = match view.geometry_snapshot() {
            Ok(geometry) => geometry,
            Err(_) => {
                let mut retired = Vec::new();
                for instance in instances {
                    let target = self
                        .awaiting_publication
                        .get(&instance)
                        .expect("a collected content-size publication remains installed")
                        .target
                        .clone();
                    if !self.target_mount_is_current(&target, retained) {
                        retired.push(
                            self.awaiting_publication
                                .remove(&instance)
                                .expect("a retired content-size publication remains installed")
                                .target,
                        );
                        continue;
                    }
                    self.observe_publication_miss(
                        &instance,
                        allow_recovery_frame,
                        ContentSizePublicationMiss::GeometryUnavailable,
                        &mut effects,
                    );
                }
                self.retire_targets(retired, &mut effects);
                effects.request_next_frame |= self.needs_frame();
                return effects;
            }
        };
        let mut published = Vec::new();
        let mut retired = Vec::new();
        for instance in instances {
            let target = self
                .awaiting_publication
                .get(&instance)
                .expect("a collected content-size publication remains installed")
                .target
                .clone();
            if !self.target_mount_is_current(&target, retained) {
                retired.push(
                    self.awaiting_publication
                        .remove(&instance)
                        .expect("a retired content-size publication remains installed")
                        .target,
                );
                continue;
            }
            let is_published = retained.has_complete_content_size_publication(
                &target.instance,
                &target.mount,
                target.desired_size(),
                target.policy,
                &geometry,
            );
            if !is_published {
                let miss = if stale_nodes.contains(&instance.node()) {
                    ContentSizePublicationMiss::StaleFeedback
                } else {
                    ContentSizePublicationMiss::IncompleteGeometry {
                        materialized: materialized_instances.contains(&instance),
                    }
                };
                self.observe_publication_miss(&instance, allow_recovery_frame, miss, &mut effects);
                continue;
            }
            published.push(
                self.awaiting_publication
                    .remove(&instance)
                    .expect("a published content-size target remains installed")
                    .target,
            );
        }
        self.retire_targets(retired, &mut effects);
        self.complete_targets_satisfied(published, &mut effects);
        effects.request_next_frame |= self.needs_frame();
        effects
    }

    fn observe_publication_miss(
        &mut self,
        instance: &NodeInstanceKey,
        allow_recovery_frame: bool,
        miss: ContentSizePublicationMiss,
        effects: &mut ContentSizeEffects,
    ) {
        let missing = self
            .awaiting_publication
            .get_mut(instance)
            .expect("an incomplete content-size publication remains installed")
            .observe_missing_geometry(allow_recovery_frame, miss);
        match missing {
            MissingContentSizePublication::Wait => {}
            MissingContentSizePublication::RequestRecoveryFrame => {
                effects.request_next_frame = true;
            }
            MissingContentSizePublication::RecoveryDeferred { miss } => {
                effects.diagnostics.push(format!(
                    "content size for node {:?} became authoritative but {} after one recovery frame; keeping the target dirty until matching geometry is published or a newer authority replaces it",
                    instance.node(),
                    miss.reason(),
                ));
            }
        }
    }

    pub(super) fn observe_publication_after_feedback(
        &mut self,
        retained: &RetainedNodeOwner,
        view: &ViewSession,
        materialized_instances: &BTreeSet<NodeInstanceKey>,
        stale_nodes: &BTreeSet<NodeId>,
    ) -> ContentSizeEffects {
        self.observe_publications(retained, view, true, materialized_instances, stale_nodes)
    }

    fn target_mount_is_current(
        &self,
        target: &ContentSizeTarget,
        retained: &RetainedNodeOwner,
    ) -> bool {
        let Some(state) = self.nodes.get(&target.instance) else {
            return false;
        };
        state.matches_mount(&target.mount)
            && self.environment_epoch_live
            && state.content_epoch_live
            && state.policy == target.policy
            && state.epoch(self.environment_epoch) == target.command.epoch()
            && retained.contains_mount(&target.instance, &target.mount)
    }

    fn batch_mounts_are_current(
        &self,
        targets: &[ContentSizeTarget],
        retained: &RetainedNodeOwner,
    ) -> bool {
        targets
            .iter()
            .all(|target| self.target_mount_is_current(target, retained))
    }

    fn batch_is_authoritative(
        &self,
        targets: &[ContentSizeTarget],
        retained: &RetainedNodeOwner,
        authoring: &OpenGpuiNodeAuthoring,
    ) -> bool {
        let mut observed_current_target = false;
        for target in targets {
            if !self.target_mount_is_current(target, retained)
                || authoring.node_lifetime(target.instance.node())
                    != Some(target.instance.lifetime())
            {
                continue;
            }
            observed_current_target = true;
            if !authoring
                .node_size(target.instance.node())
                .flatten()
                .is_some_and(|size| {
                    target
                        .policy
                        .sizes_are_equivalent(size, target.desired_size())
                })
            {
                return false;
            }
        }
        observed_current_target
    }

    fn target_submission_is_current(&self, target: &ContentSizeTarget) -> bool {
        self.nodes.get(&target.instance).is_some_and(|state| {
            state.matches_mount(&target.mount)
                && self.environment_epoch_live
                && state.content_epoch_live
                && state.policy == target.policy
                && state.epoch(self.environment_epoch) == target.command.epoch()
        })
    }

    fn partition_submission_targets(
        &self,
        targets: Vec<ContentSizeTarget>,
    ) -> (Vec<ContentSizeTarget>, Vec<ContentSizeTarget>) {
        targets
            .into_iter()
            .partition(|target| self.target_submission_is_current(target))
    }

    fn partition_settlement_targets(
        &self,
        targets: Vec<ContentSizeTarget>,
        observation: Option<(&RetainedNodeOwner, &OpenGpuiNodeAuthoring)>,
    ) -> (Vec<ContentSizeTarget>, Vec<ContentSizeTarget>) {
        let Some((retained, authoring)) = observation else {
            return self.partition_submission_targets(targets);
        };
        targets.into_iter().partition(|target| {
            self.target_mount_is_current(target, retained)
                && authoring.node_lifetime(target.instance.node())
                    == Some(target.instance.lifetime())
        })
    }

    fn batch_lifetimes_match_authoring(
        targets: &[ContentSizeTarget],
        authoring: &OpenGpuiNodeAuthoring,
    ) -> bool {
        targets.iter().all(|target| {
            authoring.node_lifetime(target.instance.node()) == Some(target.instance.lifetime())
        })
    }

    fn queue_update(
        &mut self,
        instance: NodeInstanceKey,
        update: OpenGpuiNodeSizeUpdate,
        effects: &mut ContentSizeEffects,
    ) {
        if let Some(diagnostic) = update.diagnostic() {
            effects.diagnostics.push(format!(
                "content size convergence froze for node {:?}: reason={:?}, attempts={}, last_applied={:?}, suppressed={:?}",
                instance.node(),
                diagnostic.reason(),
                diagnostic.attempts(),
                diagnostic.last_applied_size(),
                diagnostic.suppressed_size(),
            ));
        }
        let Some(command) = update.command() else {
            return;
        };
        let Some(state) = self.nodes.get(&instance) else {
            return;
        };
        self.queued.insert(
            instance.clone(),
            ContentSizeTarget {
                instance,
                mount: state.mount.clone(),
                policy: state.policy,
                command,
            },
        );
    }

    fn complete_targets_satisfied(
        &mut self,
        targets: Vec<ContentSizeTarget>,
        effects: &mut ContentSizeEffects,
    ) {
        let (targets, obsolete) = self.partition_submission_targets(targets);
        self.complete_targets_obsolete(obsolete, effects);
        let mut updates = Vec::new();
        for target in targets {
            let Some(state) = self.nodes.get_mut(&target.instance) else {
                continue;
            };
            let update = state
                .reducer
                .complete_satisfied(state.policy, target.ticket());
            if !update.is_idle() {
                updates.push((target.instance, update));
            }
        }
        for (instance, update) in updates {
            self.queue_update(instance, update, effects);
        }
    }

    fn complete_targets_failed(
        &mut self,
        targets: Vec<ContentSizeTarget>,
        effects: &mut ContentSizeEffects,
    ) {
        let (targets, obsolete) = self.partition_submission_targets(targets);
        self.complete_targets_obsolete(obsolete, effects);
        let mut updates = Vec::new();
        for target in targets {
            let Some(state) = self.nodes.get_mut(&target.instance) else {
                continue;
            };
            let update = state
                .reducer
                .complete_definitive_failure(state.policy, target.ticket());
            if !update.is_idle() {
                updates.push((target.instance, update));
            }
        }
        for (instance, update) in updates {
            self.queue_update(instance, update, effects);
        }
    }

    fn complete_targets_obsolete(
        &mut self,
        targets: Vec<ContentSizeTarget>,
        effects: &mut ContentSizeEffects,
    ) {
        let mut updates = Vec::new();
        for target in targets {
            let Some(state) = self.nodes.get_mut(&target.instance) else {
                continue;
            };
            if !state.matches_mount(&target.mount) {
                continue;
            }
            let update = state
                .reducer
                .complete_obsolete(state.policy, target.ticket());
            if !update.is_idle() {
                updates.push((target.instance, update));
            }
        }
        for (instance, update) in updates {
            self.queue_update(instance, update, effects);
        }
    }

    fn retire_targets(
        &mut self,
        targets: Vec<ContentSizeTarget>,
        effects: &mut ContentSizeEffects,
    ) {
        for target in targets {
            let Some(state) = self.nodes.get_mut(&target.instance) else {
                continue;
            };
            if !state.matches_mount(&target.mount) {
                continue;
            }
            let update = state.reducer.retire(target.ticket());
            if !update.is_idle() {
                self.queue_update(target.instance, update, effects);
            }
        }
    }

    fn retire_interruptible_work_touching(
        &mut self,
        instances: &BTreeSet<NodeInstanceKey>,
    ) -> ContentSizeEffects {
        if instances.is_empty() {
            return ContentSizeEffects::default();
        }
        let mut effects = ContentSizeEffects::default();
        if self.prefix.as_ref().is_some_and(|prefix| {
            prefix
                .targets
                .iter()
                .any(|target| instances.contains(&target.instance))
        }) {
            let targets = self
                .prefix
                .take()
                .expect("a matching content-size prefix remains installed")
                .targets;
            self.retire_targets(targets, &mut effects);
        }
        let awaiting = instances
            .iter()
            .filter_map(|instance| {
                self.awaiting_publication
                    .remove(instance)
                    .map(|publication| publication.target)
            })
            .collect();
        self.retire_targets(awaiting, &mut effects);
        let queued = instances
            .iter()
            .filter_map(|instance| self.queued.remove(instance))
            .collect();
        self.retire_targets(queued, &mut effects);
        if self.queued.is_empty() {
            self.cancel_capacity_wait();
        }
        effects.request_next_frame |= self.needs_frame();
        effects
    }

    fn retire_all_interruptible_work(&mut self) -> ContentSizeEffects {
        let mut effects = ContentSizeEffects::default();
        if let Some(prefix) = self.prefix.take() {
            self.retire_targets(prefix.targets, &mut effects);
        }
        let awaiting = std::mem::take(&mut self.awaiting_publication)
            .into_values()
            .map(|publication| publication.target)
            .collect();
        self.retire_targets(awaiting, &mut effects);
        let queued = std::mem::take(&mut self.queued).into_values().collect();
        self.retire_targets(queued, &mut effects);
        self.ready.clear();
        self.cancel_capacity_wait();
        effects
    }

    #[cfg(test)]
    pub(super) fn pending_command_counts(&self) -> Option<(usize, usize)> {
        let prefix = self
            .prefix
            .as_ref()
            .map_or(0, |prefix| prefix.targets.len());
        let remaining = self.queued.len()
            + self.awaiting_publication.len()
            + self
                .in_flight
                .as_ref()
                .map_or(0, |flight| flight.targets.len());
        (prefix != 0 || remaining != 0).then_some((prefix, remaining))
    }

    #[cfg(test)]
    pub(super) fn has_prefix_task(&self) -> bool {
        self.prefix
            .as_ref()
            .is_some_and(|prefix| prefix.task.is_some())
    }

    #[cfg(test)]
    pub(super) fn has_capacity_wait_task(&self) -> bool {
        self.capacity_wait
            .as_ref()
            .is_some_and(|pending| pending.task.is_some())
    }

    #[cfg(test)]
    pub(super) fn ready_is_empty(&self) -> bool {
        self.ready.is_empty()
    }

    #[cfg(test)]
    pub(super) fn issue_stable_height_for_test(
        &mut self,
        instance: &NodeInstanceKey,
        height: f32,
    ) -> ContentSizeEffects {
        let Some(state) = self.nodes.get_mut(instance) else {
            return ContentSizeEffects::default();
        };
        let measured_size = CanvasSize {
            width: state.canonical_size.width,
            height,
        };
        let epoch = state.epoch(self.environment_epoch);
        let first = state
            .reducer
            .observe(state.policy, epoch, state.canonical_size, measured_size);
        assert!(
            first.is_idle(),
            "the first sample only establishes stability"
        );
        let update =
            state
                .reducer
                .observe(state.policy, epoch, state.canonical_size, measured_size);
        assert!(
            update.command().is_some(),
            "the second stable sample emits one size command"
        );
        let mut effects = ContentSizeEffects::default();
        self.queue_update(instance.clone(), update, &mut effects);
        effects.request_next_frame = true;
        effects
    }

    #[cfg(test)]
    pub(super) fn coalesce_stable_height_for_test(
        &mut self,
        instance: &NodeInstanceKey,
        height: f32,
    ) {
        let Some(state) = self.nodes.get_mut(instance) else {
            return;
        };
        let measured_size = CanvasSize {
            width: state.canonical_size.width,
            height,
        };
        let epoch = state.epoch(self.environment_epoch);
        assert!(
            state
                .reducer
                .observe(state.policy, epoch, state.canonical_size, measured_size)
                .is_idle(),
            "the first sample only establishes stability"
        );
        assert!(
            state
                .reducer
                .observe(state.policy, epoch, state.canonical_size, measured_size)
                .is_idle(),
            "a stable target coalesces behind the active content-size ticket"
        );
    }

    #[cfg(test)]
    pub(super) fn force_queued_publication_for_test(&mut self, instance: &NodeInstanceKey) {
        let target = self
            .queued
            .remove(instance)
            .expect("a queued content-size target must exist before publication is forced");
        self.awaiting_publication.insert(
            instance.clone(),
            AwaitingContentSizePublication::new(target),
        );
    }
}

fn advance_epoch(epoch: &mut u64, live: &mut bool) -> bool {
    if !*live {
        return false;
    }
    let Some(next) = epoch.checked_add(1) else {
        *live = false;
        return false;
    };
    *epoch = next;
    true
}

impl OpenGpuiSceneHost {
    pub(super) fn apply_content_size_effects(&mut self, effects: ContentSizeEffects) -> bool {
        let request_next_frame = effects.requests_next_frame();
        for diagnostic in effects.diagnostics() {
            self.record_command_diagnostic(diagnostic);
        }
        request_next_frame
    }

    pub(super) fn synchronize_content_size_mounts(&mut self) -> bool {
        let sources = self.retained_nodes.content_size_sources();
        let effects = self.content_sizes.synchronize_retained(sources);
        self.apply_content_size_effects(effects)
    }

    pub(super) fn activate_content_size_measurements(
        &mut self,
        measurements: &Rc<OpenGpuiPendingMeasurements>,
    ) {
        let sources = self.retained_nodes.active_content_size_sources();
        for error in self
            .content_sizes
            .activate_measurements(sources, measurements)
        {
            self.record_measurement_failure(error);
        }
    }

    pub(super) fn observe_content_measurements(
        &mut self,
        measurements: &OpenGpuiPendingMeasurements,
        accepted_nodes: &BTreeSet<NodeId>,
    ) -> bool {
        let effects = self
            .content_sizes
            .observe_measurements(measurements, accepted_nodes);
        self.apply_content_size_effects(effects)
    }

    pub(super) fn observe_content_size_mutation(
        &mut self,
        cx: &mut Context<OpenGpuiSceneHost>,
    ) -> bool {
        let effects = self
            .content_sizes
            .observe_in_flight(&self.retained_nodes, &self.view, cx);
        self.apply_content_size_effects(effects)
    }

    pub(super) fn observe_content_size_publication_after_feedback(
        &mut self,
        stale_nodes: &BTreeSet<NodeId>,
    ) -> bool {
        let effects = self.content_sizes.observe_publication_after_feedback(
            &self.retained_nodes,
            &self.view,
            &self.materialized_frame_instances,
            stale_nodes,
        );
        self.apply_content_size_effects(effects)
    }

    pub(super) fn process_one_ready_content_size_prefix(
        &mut self,
        cx: &mut Context<OpenGpuiSceneHost>,
    ) -> bool {
        let effects = self
            .content_sizes
            .process_one_ready_prefix(&self.retained_nodes, cx);
        self.apply_content_size_effects(effects)
    }

    pub(super) fn schedule_content_size_prefix(
        &mut self,
        cx: &mut Context<OpenGpuiSceneHost>,
    ) -> bool {
        let effects =
            self.content_sizes
                .schedule_prefix(Rc::clone(&self.commands), &self.retained_nodes, cx);
        self.apply_content_size_effects(effects)
    }
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::pin::Pin;
    use std::task::{Context as TaskContext, Poll, Waker};

    use super::*;

    fn empty_prefix_permit(journal: &Rc<OpenGpuiCommandJournal>) -> OpenGpuiCommandPrefixPermit {
        let mut barrier = journal
            .capture_prefix_barrier()
            .expect("capture empty content-size prefix");
        let mut context = TaskContext::from_waker(Waker::noop());
        match Future::poll(Pin::new(&mut barrier), &mut context) {
            Poll::Ready(Ok(permit)) => permit,
            Poll::Ready(Err(error)) => panic!("content-size prefix failed: {error}"),
            Poll::Pending => panic!("empty content-size prefix remained pending"),
        }
    }

    #[test]
    fn stale_owner_callback_releases_its_ready_prefix_permit() {
        let journal = OpenGpuiCommandJournal::new();
        let permit = empty_prefix_permit(&journal);
        let mut owner = ContentSizeSubmissionOwner::default();
        let stale_owner = Rc::clone(&owner.owner_lineage);
        let stale_prefix = Rc::new(());

        owner.clear();
        owner.push_ready(stale_owner, stale_prefix, Ok(permit));

        assert!(owner.ready.is_empty());
        assert!(!journal.has_pending());
    }

    #[test]
    fn stale_prefix_callback_cannot_match_its_successor() {
        let journal = OpenGpuiCommandJournal::new();
        let permit = empty_prefix_permit(&journal);
        let mut owner = ContentSizeSubmissionOwner::default();
        let owner_lineage = Rc::clone(&owner.owner_lineage);
        let stale_prefix = Rc::new(());
        owner.prefix = Some(PendingContentSizePrefix {
            owner_lineage: Rc::clone(&owner_lineage),
            prefix_lineage: Rc::new(()),
            targets: Vec::new(),
            task: None,
            cancellation: None,
        });

        owner.push_ready(owner_lineage, stale_prefix, Ok(permit));

        assert!(owner.ready.is_empty());
        assert!(!journal.has_pending());
    }

    #[test]
    fn epoch_identity_exhaustion_is_terminal_instead_of_wrapping() {
        let mut epoch = u64::MAX - 1;
        let mut live = true;

        assert!(advance_epoch(&mut epoch, &mut live));
        assert_eq!(epoch, u64::MAX);
        assert!(live);
        assert!(!advance_epoch(&mut epoch, &mut live));
        assert_eq!(epoch, u64::MAX);
        assert!(!live);
        assert!(!advance_epoch(&mut epoch, &mut live));
        assert_eq!(epoch, u64::MAX);
    }

    #[test]
    fn prefix_capture_backpressure_classification_is_exact() {
        assert!(OpenGpuiCommandPrefixBarrierError::Busy.is_pre_admission_backpressure());
        assert!(
            OpenGpuiCommandPrefixBarrierError::QueueFull { capacity: 1 }
                .is_pre_admission_backpressure()
        );
        for terminal in [
            OpenGpuiCommandPrefixBarrierError::HostClosing,
            OpenGpuiCommandPrefixBarrierError::HostDropped,
            OpenGpuiCommandPrefixBarrierError::IdentityExhausted,
            OpenGpuiCommandPrefixBarrierError::ReservationInactive,
        ] {
            assert!(!terminal.is_pre_admission_backpressure());
        }
    }

    #[test]
    fn dirty_publication_reactivates_without_continuous_frame_polling() {
        let mut recovery = ContentSizePublicationRecovery::Fresh;
        let unavailable = ContentSizePublicationMiss::IncompleteGeometry {
            materialized: false,
        };

        assert_eq!(
            recovery.observe_missing_geometry(true, unavailable),
            MissingContentSizePublication::RequestRecoveryFrame
        );
        assert_eq!(
            recovery.observe_missing_geometry(true, unavailable),
            MissingContentSizePublication::RecoveryDeferred { miss: unavailable }
        );
        assert_eq!(recovery, ContentSizePublicationRecovery::Dirty);
        assert!(!recovery.needs_frame());
        assert!(!recovery.blocks_new_batch());
        assert_eq!(
            recovery.observe_missing_geometry(true, unavailable),
            MissingContentSizePublication::RequestRecoveryFrame
        );
        assert_eq!(recovery, ContentSizePublicationRecovery::Requested);
        assert!(recovery.needs_frame());
        assert!(recovery.blocks_new_batch());

        let mut recovery = ContentSizePublicationRecovery::Fresh;
        let incomplete = ContentSizePublicationMiss::IncompleteGeometry { materialized: true };
        assert_eq!(
            recovery.observe_missing_geometry(true, incomplete),
            MissingContentSizePublication::RequestRecoveryFrame
        );
        assert!(recovery.needs_frame());
        assert_eq!(
            recovery.observe_missing_geometry(true, incomplete),
            MissingContentSizePublication::RecoveryDeferred { miss: incomplete }
        );
        assert_eq!(recovery, ContentSizePublicationRecovery::Dirty);
        assert!(!recovery.needs_frame());
    }

    #[test]
    fn synchronous_probe_does_not_consume_but_stale_feedback_bounds_publication_recovery() {
        let mut recovery = ContentSizePublicationRecovery::Fresh;
        let stale = ContentSizePublicationMiss::StaleFeedback;

        assert_eq!(
            recovery.observe_missing_geometry(false, stale),
            MissingContentSizePublication::Wait
        );
        assert_eq!(recovery, ContentSizePublicationRecovery::Fresh);
        assert_eq!(
            recovery.observe_missing_geometry(true, stale),
            MissingContentSizePublication::RequestRecoveryFrame
        );
        assert_eq!(
            recovery.observe_missing_geometry(false, stale),
            MissingContentSizePublication::Wait
        );
        assert_eq!(recovery, ContentSizePublicationRecovery::Requested);
        assert_eq!(
            recovery.observe_missing_geometry(true, stale),
            MissingContentSizePublication::RecoveryDeferred { miss: stale }
        );
        assert_eq!(recovery, ContentSizePublicationRecovery::Dirty);
        assert!(!recovery.needs_frame());
        assert_eq!(
            recovery.observe_missing_geometry(false, stale),
            MissingContentSizePublication::Wait
        );
        assert_eq!(recovery, ContentSizePublicationRecovery::Dirty);
    }
}
