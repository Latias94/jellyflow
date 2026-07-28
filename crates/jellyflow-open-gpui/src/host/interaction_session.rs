use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::rc::{Rc, Weak};
#[cfg(target_os = "macos")]
use std::time::Duration;
use std::time::Instant;

use jellyflow_core::{CanvasPoint, EdgeId, NodeGraphConnectionMode, NodeId};
#[cfg(test)]
use jellyflow_runtime::view::ViewSession;
use jellyflow_runtime::viewport::ViewportTransform;
use jellyflow_runtime::{
    commands::CommandExecutionCompletion, commands::GraphCommand, geometry::ViewPoint,
    geometry::ViewRect, geometry::ViewSize, graph::GraphRevision, graph::GraphSnapshot,
    graph::NodeLifetimeKey, interaction::CanvasInteractionConfig,
    interaction::CanvasInteractionEffect, interaction::CanvasInteractionError,
    interaction::CanvasInteractionPhase, interaction::CanvasInteractionPresentation,
    interaction::CanvasInteractionQueryAdvance, interaction::CanvasInteractionSession,
    interaction::CanvasInteractionUpdate, interaction::CanvasPressTarget,
    interaction::ConnectionGestureBinding, interaction::ConnectionInteractionStart,
    interaction::ConnectionSearchRequest, interaction::ConnectionSearchSelection,
    interaction::InteractionGeneration, interaction::InteractionGraphSubmission,
    interaction::InteractionGraphSubmissionBindError, interaction::InteractionOwner,
    interaction::InteractionTerminalReason, interaction::InteractionTickDemand,
    interaction::InteractionTicketId, interaction::KeyboardConnectionFeedback,
    interaction::KeyboardConnectionPort, interaction::MAX_NODE_MOVE_COHORT_SIZE,
    interaction::NodeDragStart, interaction::NodeNudgeDirection, interaction::SelectionMode,
    view::NodeInstanceKey, viewport::ViewportPointerSample,
};
use open_gpui::{
    AsyncWindowContext, Context, FocusHandle, Pixels, Point, Task, WeakEntity, Window,
};

use crate::activity::OpenGpuiActivityLease;
use crate::binding::OpenGpuiMountCapability;
#[cfg(test)]
use crate::command_journal::OpenGpuiCommandJournal;
use crate::command_journal::{
    OpenGpuiCommandOrigin, OpenGpuiCommandPrefixBarrierCancellation,
    OpenGpuiCommandPrefixBarrierError, OpenGpuiCommandPrefixBarrierResult,
    OpenGpuiCommandPrefixPermit,
};
use crate::interaction::{
    NativeInteractionGeneration, OpenGpuiNativeInteractionAuthority,
    OpenGpuiNativeInteractionHandle, OpenGpuiOverlayInteractionBoundary,
    OpenGpuiPortDragCancelReason, OpenGpuiPortDragOwner, OpenGpuiPortFocusExit,
    OpenGpuiSceneGestureCancelReason,
};
use crate::port_handle::projected_view_rect;

use super::OpenGpuiSceneHost;
use super::focus_navigation::{
    OpenGpuiGraphFocusCoordinator, OpenGpuiGraphFocusEntry, OpenGpuiGraphFocusTarget,
};

pub(super) struct OpenGpuiNativeInteractionOwner {
    native: OpenGpuiNativeInteractionAuthority,
    session: CanvasInteractionSession,
    pending_submission: Option<PendingInteractionSubmission>,
    ready_prefixes: VecDeque<ReadyInteractionPrefix>,
    node_drag_residency: Option<NodeDragResidency>,
    connection: Option<NativeConnectionContext>,
    focus: Option<FocusHandle>,
    graph_focus: OpenGpuiGraphFocusCoordinator,
    tick_scheduled: Option<ScheduledTick>,
    tick_frame_requested: Option<ScheduledTick>,
    query_frame_scheduled: Option<CallbackToken>,
    query_resolution_active: Rc<Cell<bool>>,
    #[cfg(target_os = "macos")]
    native_tick_frame_wake: Option<Task<()>>,
    #[cfg(target_os = "macos")]
    native_query_frame_wake: Option<Task<()>>,
    next_tick_sequence: u64,
    last_tick_at: Option<Instant>,
    epoch: Rc<Cell<u64>>,
    lifecycle: NativeInteractionOwnerLifecycle,
    tick_callback_entries: Rc<Cell<u64>>,
    tick_callback_registrations: u64,
    tick_frame_requests: u64,
    #[cfg(test)]
    terminal_window_transitions: u64,
}

/// Leave the normal display link enough time to deliver its requested frame before using AppKit's
/// invalidation path. This is a redraw wake only; semantic progression remains in `on_next_frame`.
#[cfg(target_os = "macos")]
const NATIVE_FRAME_FALLBACK_DELAY: Duration = Duration::from_millis(24);

/// A failed native display request must not turn the fallback into a semantic timer. Each of the
/// two demand kinds owns one task with a bounded six-second wake window, so a query cannot replace
/// a continuous-interaction wake. A real frame, stale token, or terminal interaction cancels it.
#[cfg(target_os = "macos")]
const NATIVE_FRAME_FALLBACK_ATTEMPTS: u16 = 256;

enum NativeInteractionOwnerLifecycle {
    Active,
    Prepared(PendingTerminalWindowCleanup),
    Settled,
}

struct PendingTerminalWindowCleanup {
    root_focus: Option<FocusHandle>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct CallbackToken {
    epoch: u64,
    semantic_generation: Option<InteractionGeneration>,
}

/// Identifies one concrete request to advance a continuous interaction.
///
/// `CallbackToken` protects semantic lifetime. The sequence additionally prevents a stale native
/// callback from consuming a successor tick after an external frame clock has already advanced the
/// current one.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ScheduledTick {
    callback: CallbackToken,
    sequence: u64,
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NativeFrameFallbackToken {
    Tick(ScheduledTick),
    Query(CallbackToken),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct InteractionTickDiagnostics {
    pub(super) callback_registrations: u64,
    pub(super) callback_entries: u64,
    pub(super) frame_requests: u64,
    pub(super) callback_epoch: u64,
    pub(super) pending_queries: usize,
    pub(super) pending_submission: bool,
    pub(super) ready_prefixes: usize,
    pub(super) tick_scheduled: bool,
    pub(super) tick_frame_requested: bool,
    pub(super) query_frame_scheduled: bool,
    pub(super) query_resolution_active: bool,
    pub(super) native_tick_wake_active: bool,
    pub(super) native_query_wake_active: bool,
}

struct QueryResolutionGuard(Rc<Cell<bool>>);

impl Drop for QueryResolutionGuard {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

struct ReadyInteractionPrefix {
    token: CallbackToken,
    ticket: InteractionTicketId,
    result: OpenGpuiCommandPrefixBarrierResult,
}

#[derive(Debug)]
struct NodeDragResidency {
    generation: InteractionGeneration,
    members: Box<[NodeLifetimeKey]>,
    ticket: Option<InteractionTicketId>,
    stage: NodeDragResidencyStage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NodeDragResidencyStage {
    Semantic,
    AwaitingAppliedFrame { revision: GraphRevision },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NodeDragRequiredNodesError {
    CapacityExceeded {
        count: usize,
    },
    MemberRetired {
        member: NodeLifetimeKey,
        current: Option<NodeLifetimeKey>,
    },
}

impl NodeDragResidency {
    fn from_start(
        start: &NodeDragStart,
        generation: InteractionGeneration,
    ) -> Result<Self, NodeDragRequiredNodesError> {
        let members = start.members();
        if members.is_empty() || members.len() > MAX_NODE_MOVE_COHORT_SIZE {
            return Err(NodeDragRequiredNodesError::CapacityExceeded {
                count: members.len(),
            });
        }
        Ok(Self {
            generation,
            members: members.into(),
            ticket: None,
            stage: NodeDragResidencyStage::Semantic,
        })
    }

    fn required_nodes(
        &self,
        snapshot: &GraphSnapshot,
    ) -> Result<BTreeSet<NodeId>, NodeDragRequiredNodesError> {
        if self.members.is_empty() || self.members.len() > MAX_NODE_MOVE_COHORT_SIZE {
            return Err(NodeDragRequiredNodesError::CapacityExceeded {
                count: self.members.len(),
            });
        }
        let mut nodes = BTreeSet::new();
        for member in &self.members {
            let current = snapshot.node_lifetime_key(member.node());
            if current != Some(*member) {
                return Err(NodeDragRequiredNodesError::MemberRetired {
                    member: *member,
                    current,
                });
            }
            nodes.insert(member.node());
        }
        Ok(nodes)
    }

    fn member(&self, node: NodeId) -> Option<NodeLifetimeKey> {
        self.members
            .iter()
            .copied()
            .find(|member| member.node() == node)
    }
}

pub(super) struct NativeConnectionAdmission {
    owner: OpenGpuiPortDragOwner,
    mount: OpenGpuiMountCapability,
    native_generation: Option<NativeInteractionGeneration>,
}

impl NativeConnectionAdmission {
    fn new(
        owner: OpenGpuiPortDragOwner,
        mount: OpenGpuiMountCapability,
        native_generation: Option<NativeInteractionGeneration>,
    ) -> Option<Self> {
        mount.is_live().then_some(Self {
            owner,
            mount,
            native_generation,
        })
    }

    pub(super) fn pointer(
        owner: OpenGpuiPortDragOwner,
        mount: OpenGpuiMountCapability,
        native_generation: NativeInteractionGeneration,
    ) -> Option<Self> {
        Self::new(owner, mount, Some(native_generation))
    }

    pub(super) fn activation(
        owner: OpenGpuiPortDragOwner,
        mount: OpenGpuiMountCapability,
    ) -> Option<Self> {
        Self::new(owner, mount, None)
    }

    fn matches_start(&self, start: &ConnectionInteractionStart) -> bool {
        self.matches_binding(&start.binding())
    }

    fn matches_binding(&self, binding: &ConnectionGestureBinding) -> bool {
        let interaction_port = binding
            .detached_port_instance()
            .unwrap_or_else(|| binding.fixed_port_instance());
        self.owner.port_instance() == interaction_port
            && self.owner.instance().lifetime() == binding.owner()
    }

    fn matches_keyboard_port(&self, port: KeyboardConnectionPort) -> bool {
        self.owner.port() == port.port() && self.owner.instance().node() == port.node()
    }
}

struct NativeConnectionContext {
    owner: OpenGpuiPortDragOwner,
    mount: OpenGpuiMountCapability,
    native_generation: Option<NativeInteractionGeneration>,
    continuation_lease: Option<OpenGpuiActivityLease>,
}

impl NativeConnectionContext {
    fn from_admission(admission: NativeConnectionAdmission) -> Self {
        Self {
            owner: admission.owner,
            mount: admission.mount,
            native_generation: admission.native_generation,
            continuation_lease: None,
        }
    }

    fn matches(&self, owner: &OpenGpuiPortDragOwner, mount: &OpenGpuiMountCapability) -> bool {
        self.owner == *owner && self.mount.is_same_mount(mount)
    }

    fn matches_pointer(
        &self,
        owner: &OpenGpuiPortDragOwner,
        mount: &OpenGpuiMountCapability,
        native_generation: Option<NativeInteractionGeneration>,
    ) -> bool {
        self.matches(owner, mount)
            && native_generation.is_some()
            && self.native_generation == native_generation
    }

    fn promote_to_activation(&mut self) {
        self.native_generation = None;
    }

    fn matches_binding(&self, binding: &ConnectionGestureBinding) -> bool {
        let interaction_port = binding
            .detached_port_instance()
            .unwrap_or_else(|| binding.fixed_port_instance());
        self.owner.port_instance() == interaction_port
            && self.owner.instance().lifetime() == binding.owner()
    }

    fn retain_connection(&mut self, lease: OpenGpuiActivityLease) {
        self.continuation_lease = Some(lease);
    }

    fn retains_connection(&self) -> bool {
        self.continuation_lease.is_some()
    }
}

impl OpenGpuiNativeInteractionOwner {
    pub(super) fn new(session: CanvasInteractionSession) -> Self {
        Self {
            native: OpenGpuiNativeInteractionAuthority::new(),
            session,
            pending_submission: None,
            ready_prefixes: VecDeque::new(),
            node_drag_residency: None,
            connection: None,
            focus: None,
            graph_focus: OpenGpuiGraphFocusCoordinator::default(),
            tick_scheduled: None,
            tick_frame_requested: None,
            query_frame_scheduled: None,
            query_resolution_active: Rc::new(Cell::new(false)),
            #[cfg(target_os = "macos")]
            native_tick_frame_wake: None,
            #[cfg(target_os = "macos")]
            native_query_frame_wake: None,
            next_tick_sequence: 1,
            last_tick_at: None,
            epoch: Rc::new(Cell::new(1)),
            lifecycle: NativeInteractionOwnerLifecycle::Active,
            tick_callback_entries: Rc::new(Cell::new(0)),
            tick_callback_registrations: 0,
            tick_frame_requests: 0,
            #[cfg(test)]
            terminal_window_transitions: 0,
        }
    }

    pub(super) fn presentation(&self) -> CanvasInteractionPresentation {
        self.session.presentation()
    }

    pub(super) fn set_config(
        &mut self,
        config: CanvasInteractionConfig,
    ) -> Result<bool, CanvasInteractionError> {
        self.session.set_config(config)
    }

    pub(super) fn press(
        &mut self,
        target: CanvasPressTarget,
        sample: ViewportPointerSample,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        let node_drag_start = match &target {
            CanvasPressTarget::NodeDrag(start) => Some(start.clone()),
            _ => None,
        };
        let update = self.session.press(target, sample)?;
        if let Some(start) = node_drag_start {
            let Some(generation) = self.session.presentation().generation() else {
                return match self.session.cancel() {
                    Ok(_) => Err(CanvasInteractionError::InvalidConfiguration),
                    Err(error) => Err(error),
                };
            };
            let residency = match NodeDragResidency::from_start(&start, generation) {
                Ok(residency) => residency,
                Err(_) => {
                    return match self.session.cancel() {
                        Ok(_) => Err(CanvasInteractionError::InvalidConfiguration),
                        Err(error) => Err(error),
                    };
                }
            };
            self.node_drag_residency = Some(residency);
        }
        self.reconcile_node_drag_residency();
        Ok(update)
    }

    pub(super) fn activate_node(
        &mut self,
        node: NodeLifetimeKey,
        selection_mode: SelectionMode,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.session.activate_node(node, selection_mode)
    }

    pub(super) fn move_to(
        &mut self,
        sample: ViewportPointerSample,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        let update = self.session.move_to(sample)?;
        self.reconcile_node_drag_residency();
        Ok(update)
    }

    pub(super) fn release(
        &mut self,
        sample: ViewportPointerSample,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        let update = self.session.release(sample)?;
        self.reconcile_node_drag_residency();
        Ok(update)
    }

    pub(super) fn release_connection_or_activate_port(
        &mut self,
        source: KeyboardConnectionPort,
        prospective_edge: EdgeId,
        mode: NodeGraphConnectionMode,
        sample: ViewportPointerSample,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        let result = self.session.release_connection_or_activate_port(
            source,
            prospective_edge,
            mode,
            sample,
        );
        if self.session.keyboard_connection_source() == Some(source) {
            if let Some(connection) = self.connection.as_mut() {
                connection.promote_to_activation();
            }
        } else if matches!(
            self.session.presentation().phase(),
            CanvasInteractionPhase::Idle | CanvasInteractionPhase::Closed
        ) {
            self.connection = None;
        }
        result
    }

    pub(super) fn cancel(&mut self) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        let update = self.session.cancel();
        self.node_drag_residency = None;
        self.stop_query_scheduling_if_idle();
        update
    }

    pub(super) fn bind_submission(
        &mut self,
        submission: InteractionGraphSubmission,
    ) -> Result<Option<GraphCommand>, InteractionGraphSubmissionBindError> {
        self.session.bind_submission(submission)
    }

    pub(super) fn close_semantic(&mut self) -> CanvasInteractionUpdate {
        let update = self.session.close();
        self.node_drag_residency = None;
        self.stop_query_scheduling_if_idle();
        update
    }

    pub(super) fn owner_retired(&mut self, owner: InteractionOwner) -> CanvasInteractionUpdate {
        let retires_residency = match owner {
            InteractionOwner::Node(node) => self
                .node_drag_residency
                .as_ref()
                .is_some_and(|residency| residency.member(node.node()).is_some()),
            InteractionOwner::Port(_) | InteractionOwner::Edge(_) => false,
        };
        let update = self.session.owner_retired(owner);
        if retires_residency {
            self.node_drag_residency = None;
        } else {
            self.reconcile_node_drag_residency();
        }
        self.stop_query_scheduling_if_idle();
        update
    }

    pub(super) fn uses_owner(&self, owner: InteractionOwner) -> bool {
        self.session.uses_owner(owner)
    }

    pub(super) fn required_node_drag_nodes(
        &self,
        snapshot: &GraphSnapshot,
    ) -> Result<BTreeSet<NodeId>, NodeDragRequiredNodesError> {
        self.node_drag_residency.as_ref().map_or_else(
            || Ok(BTreeSet::new()),
            |residency| residency.required_nodes(snapshot),
        )
    }

    pub(super) fn node_drag_member(&self, node: NodeId) -> Option<NodeLifetimeKey> {
        self.node_drag_residency
            .as_ref()
            .and_then(|residency| residency.member(node))
    }

    pub(super) fn has_node_drag_residency(&self) -> bool {
        self.node_drag_residency.is_some()
    }

    pub(super) fn observe_materialized_frame(
        &mut self,
        revision: GraphRevision,
        materialized: impl IntoIterator<Item = NodeLifetimeKey>,
    ) -> bool {
        let Some(residency) = self.node_drag_residency.as_ref() else {
            return false;
        };
        let NodeDragResidencyStage::AwaitingAppliedFrame { revision: applied } = residency.stage
        else {
            return false;
        };
        if revision < applied {
            return false;
        }

        let materialized = materialized.into_iter().collect::<BTreeSet<_>>();
        if !residency
            .members
            .iter()
            .all(|member| materialized.contains(member))
        {
            return false;
        }

        self.node_drag_residency = None;
        true
    }

    fn reconcile_node_drag_residency(&mut self) {
        let Some(residency) = self.node_drag_residency.as_ref() else {
            return;
        };
        if residency.stage != NodeDragResidencyStage::Semantic {
            return;
        }
        let presentation = self.session.presentation();
        let same_generation = presentation.generation() == Some(residency.generation);
        let semantic_phase = matches!(
            presentation.phase(),
            CanvasInteractionPhase::Armed
                | CanvasInteractionPhase::NodeDrag
                | CanvasInteractionPhase::Committing
        );
        if !same_generation || !semantic_phase {
            self.node_drag_residency = None;
            return;
        }
        let Some(drag) = presentation.node_drag() else {
            return;
        };
        if drag.members() != residency.members.as_ref() {
            self.node_drag_residency = None;
            return;
        }
        if let Some(ticket) = drag.ticket()
            && let Some(residency) = self.node_drag_residency.as_mut()
        {
            residency.ticket = Some(ticket);
        }
    }

    fn settle_node_drag_residency(
        &mut self,
        ticket: InteractionTicketId,
        applied_revision: Option<GraphRevision>,
    ) {
        let Some(residency) = self
            .node_drag_residency
            .as_mut()
            .filter(|residency| residency.ticket == Some(ticket))
        else {
            return;
        };
        if let Some(revision) = applied_revision {
            residency.stage = NodeDragResidencyStage::AwaitingAppliedFrame { revision };
        } else {
            self.node_drag_residency = None;
        }
    }

    pub(super) fn delete_selection(
        &mut self,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.session.delete_selection()
    }

    pub(super) fn request_edge_pick(
        &mut self,
        sample: ViewportPointerSample,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.session.request_edge_pick(sample)
    }

    pub(super) fn clear_edge_hover(
        &mut self,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.session.clear_edge_hover()
    }

    /// Grants runtime a bounded query advancement opportunity from the native frame clock.
    ///
    /// Query identity, payload, cursor, stale policy, and cleanup remain in the semantic session.
    /// Reconcile drag residency after every grant because an alignment result may complete the
    /// drag commit.
    pub(super) fn advance_query_work(
        &mut self,
        page_budget: usize,
    ) -> CanvasInteractionQueryAdvance {
        let advance = self.session.advance_query_work(page_budget);
        self.reconcile_node_drag_residency();
        advance
    }

    pub(super) fn search_request(&self) -> Option<ConnectionSearchRequest> {
        self.session.search_request()
    }

    pub(super) fn search_candidates(
        &self,
        request: &ConnectionSearchRequest,
    ) -> Result<jellyflow_runtime::graph::CreateFromWireCandidates, CanvasInteractionError> {
        self.session.search_candidates(request)
    }

    pub(super) fn create_search_selection(
        &self,
        request: &ConnectionSearchRequest,
        target: jellyflow_core::PortId,
        creation: jellyflow_runtime::schema::NodeCreation,
    ) -> Result<Option<ConnectionSearchSelection>, CanvasInteractionError> {
        self.session
            .create_search_selection(request, target, creation)
    }

    pub(super) fn search_ready(
        &mut self,
        request: &ConnectionSearchRequest,
        result_count: usize,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.session.search_ready(request, result_count)
    }

    pub(super) fn search_failed(
        &mut self,
        request: &ConnectionSearchRequest,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.session.search_failed(request)
    }

    pub(super) fn retry_search(
        &mut self,
        request: &ConnectionSearchRequest,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.session.retry_search(request)
    }

    pub(super) fn select_search_result(
        &mut self,
        selection: ConnectionSearchSelection,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.session.select_search_result(selection)
    }

    pub(super) fn complete_submission(
        &mut self,
        ticket: InteractionTicketId,
        completion: CommandExecutionCompletion,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        let applied_revision = completion.applied_revision();
        let update = self.session.complete_submission(ticket, completion)?;
        self.settle_node_drag_residency(ticket, applied_revision);
        self.reconcile_node_drag_residency();
        Ok(update)
    }

    pub(super) fn complete_submission_without_command(
        &mut self,
        ticket: InteractionTicketId,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        let update = self.session.complete_submission_without_command(ticket)?;
        self.settle_node_drag_residency(ticket, None);
        self.reconcile_node_drag_residency();
        Ok(update)
    }

    pub(super) fn submission_not_dispatched(
        &mut self,
        ticket: InteractionTicketId,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        let update = self.session.submission_not_dispatched(ticket)?;
        self.settle_node_drag_residency(ticket, None);
        self.reconcile_node_drag_residency();
        Ok(update)
    }

    pub(super) fn submission_outcome_unknown(
        &mut self,
        ticket: InteractionTicketId,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        let update = self.session.submission_outcome_unknown(ticket)?;
        self.settle_node_drag_residency(ticket, None);
        self.reconcile_node_drag_residency();
        Ok(update)
    }

    pub(super) fn resume_submission_after_prefix(
        &mut self,
        submission: InteractionGraphSubmission,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.session.resume_submission_after_prefix(submission)
    }

    pub(super) fn tick(
        &mut self,
        elapsed_seconds: f32,
        viewport: ViewSize,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.session.tick(elapsed_seconds, viewport)
    }

    pub(super) fn native_capability(&self) -> OpenGpuiNativeInteractionHandle {
        self.native.capability()
    }

    pub(super) fn overlay_boundary(&self) -> OpenGpuiOverlayInteractionBoundary {
        OpenGpuiOverlayInteractionBoundary::new(self.native.capability())
    }

    pub(super) fn active_port_owner(&self) -> Option<OpenGpuiPortDragOwner> {
        self.native.active_owner()
    }

    pub(super) fn schedule_pending_native_drain(
        &self,
        window: &Window,
        cx: &mut open_gpui::App,
    ) -> bool {
        if self.terminal() {
            return false;
        }
        self.native.schedule_pending_drain(window, cx)
    }

    pub(super) fn cancel_scene_deferred(&self, reason: OpenGpuiSceneGestureCancelReason) -> bool {
        self.native.cancel_scene_deferred(reason)
    }

    pub(super) fn cancel_scene_owner_deferred(
        &self,
        owner: &NodeInstanceKey,
        reason: OpenGpuiSceneGestureCancelReason,
    ) -> bool {
        self.native.cancel_scene_owner_deferred(owner, reason)
    }

    pub(super) fn cancel_port_owner_deferred(
        &self,
        owner: &OpenGpuiPortDragOwner,
        mount: &OpenGpuiMountCapability,
        reason: OpenGpuiPortDragCancelReason,
    ) -> bool {
        self.native.cancel_port_owner_deferred(owner, mount, reason)
    }

    pub(super) fn cancel_native_for_query_failure(
        &self,
        window: &Window,
        cx: &mut open_gpui::App,
    ) -> bool {
        let cancelled_port = self.connection.as_ref().is_some_and(|connection| {
            self.native.cancel_port_owner_deferred(
                &connection.owner,
                &connection.mount,
                OpenGpuiPortDragCancelReason::InteractionUnavailable,
            )
        });
        let cancelled_scene = self
            .native
            .cancel_scene_deferred(OpenGpuiSceneGestureCancelReason::InteractionUnavailable);
        let cancelled = cancelled_port || cancelled_scene;
        if cancelled {
            self.native.schedule_pending_drain(window, cx);
        }
        cancelled
    }

    pub(super) fn reject_port_start(
        &self,
        owner: &OpenGpuiPortDragOwner,
        mount: &OpenGpuiMountCapability,
        reason: OpenGpuiPortDragCancelReason,
        window: &mut Window,
        cx: &mut open_gpui::App,
    ) -> bool {
        self.native
            .reject_port_start(owner, mount, reason, window, cx)
    }

    pub(super) fn settle_port_focus(
        &self,
        owner: &OpenGpuiPortDragOwner,
        mount: &OpenGpuiMountCapability,
        exit: OpenGpuiPortFocusExit,
        window: &mut Window,
        cx: &mut open_gpui::App,
    ) -> bool {
        self.native
            .settle_port_focus(owner, mount, exit, window, cx)
    }

    pub(super) fn set_focus_handle(&mut self, focus: FocusHandle) -> bool {
        if self.terminal() {
            return false;
        }
        self.native.set_focus_handle(focus.clone());
        self.focus = Some(focus);
        true
    }

    pub(super) fn focus_handle(&self) -> Option<FocusHandle> {
        self.focus.clone()
    }

    pub(super) fn graph_focus_action_handle(
        &mut self,
        target: OpenGpuiGraphFocusTarget,
        cx: &mut Context<OpenGpuiSceneHost>,
    ) -> FocusHandle {
        self.graph_focus.action_handle(target, cx)
    }

    pub(super) fn exact_focused_graph_target(
        &self,
        window: &Window,
    ) -> Option<OpenGpuiGraphFocusTarget> {
        (!self.terminal())
            .then(|| self.graph_focus.exact_focused_target(window))
            .flatten()
    }

    pub(super) fn remember_graph_focus_return(&mut self, target: OpenGpuiGraphFocusTarget) {
        if !self.terminal() {
            self.graph_focus.remember_return_target(target);
        }
    }

    pub(super) fn reconcile_graph_focus(
        &mut self,
        current: Vec<OpenGpuiGraphFocusEntry>,
        root: &FocusHandle,
        allow_return: bool,
        window: &mut Window,
        cx: &mut Context<OpenGpuiSceneHost>,
    ) {
        if self.terminal() {
            return;
        }
        self.graph_focus
            .reconcile(current, root, allow_return, window, cx);
    }

    pub(super) fn press_connection(
        &mut self,
        start: ConnectionInteractionStart,
        sample: ViewportPointerSample,
        admission: NativeConnectionAdmission,
    ) -> Result<Option<CanvasInteractionUpdate>, CanvasInteractionError> {
        if self.terminal() || self.connection.is_some() || !admission.matches_start(&start) {
            return Ok(None);
        }
        let update = self.press(CanvasPressTarget::Connection(start), sample)?;
        self.connection = Some(NativeConnectionContext::from_admission(admission));
        Ok(Some(update))
    }

    pub(super) fn activate_connection(
        &mut self,
        start: ConnectionInteractionStart,
        sample: ViewportPointerSample,
        admission: NativeConnectionAdmission,
    ) -> Result<Option<CanvasInteractionUpdate>, CanvasInteractionError> {
        if self.terminal() || self.connection.is_some() || !admission.matches_start(&start) {
            return Ok(None);
        }
        let update = self.session.activate_connection_search(start, sample)?;
        self.connection = Some(NativeConnectionContext::from_admission(admission));
        Ok(Some(update))
    }

    pub(super) fn activate_connection_port(
        &mut self,
        port: KeyboardConnectionPort,
        prospective_edge: EdgeId,
        mode: NodeGraphConnectionMode,
        admission: NativeConnectionAdmission,
    ) -> Result<Option<CanvasInteractionUpdate>, CanvasInteractionError> {
        if self.terminal() {
            return Ok(None);
        }
        if let Some(source) = self.session.keyboard_connection_source() {
            if !self
                .connection
                .as_ref()
                .is_some_and(|current| current.owner.port() == source.port())
            {
                return Ok(None);
            }
            return self
                .session
                .activate_connection_port(port, prospective_edge, mode)
                .map(Some);
        }
        if self.connection.is_some() || !admission.matches_keyboard_port(port) {
            return Ok(None);
        }
        let update = self
            .session
            .activate_connection_port(port, prospective_edge, mode)?;
        if self.session.keyboard_connection_source() == Some(port) {
            self.connection = Some(NativeConnectionContext::from_admission(admission));
        }
        Ok(Some(update))
    }

    pub(super) fn keyboard_connection_source(&self) -> Option<KeyboardConnectionPort> {
        self.session.keyboard_connection_source()
    }

    pub(super) fn keyboard_connection_feedback(&self) -> Option<&KeyboardConnectionFeedback> {
        self.session.keyboard_connection_feedback()
    }

    pub(super) fn nudge_selection(
        &mut self,
        direction: NodeNudgeDirection,
        accelerated: bool,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.session.nudge_selection(direction, accelerated)
    }

    pub(super) fn reveal_node(
        &mut self,
        node: NodeLifetimeKey,
        viewport: ViewSize,
        padding_in_view: f32,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.session.reveal_node(node, viewport, padding_in_view)
    }

    pub(super) fn clear_connection(&mut self) -> bool {
        self.connection.take().is_some()
    }

    #[cfg(test)]
    pub(super) fn has_connection(&self) -> bool {
        self.connection.is_some()
    }

    #[cfg(test)]
    pub(super) fn test_connection_native_generation(&self) -> Option<u64> {
        self.connection
            .as_ref()
            .and_then(|connection| connection.native_generation)
            .map(NativeInteractionGeneration::test_value)
    }

    pub(super) fn connection_identity(
        &self,
    ) -> Option<(OpenGpuiPortDragOwner, OpenGpuiMountCapability)> {
        self.connection
            .as_ref()
            .map(|current| (current.owner.clone(), current.mount.clone()))
    }

    pub(super) fn connection_matches_pointer(
        &self,
        owner: &OpenGpuiPortDragOwner,
        mount: &OpenGpuiMountCapability,
        native_generation: Option<NativeInteractionGeneration>,
    ) -> bool {
        self.connection
            .as_ref()
            .is_some_and(|current| current.matches_pointer(owner, mount, native_generation))
    }

    pub(super) fn connection_matches_binding(&self, binding: &ConnectionGestureBinding) -> bool {
        self.connection
            .as_ref()
            .is_some_and(|current| current.matches_binding(binding))
    }

    pub(super) fn connection_retains_activity(&self) -> bool {
        self.connection
            .as_ref()
            .is_some_and(NativeConnectionContext::retains_connection)
    }

    pub(super) fn retain_connection_activity(
        &mut self,
        owner: &OpenGpuiPortDragOwner,
        mount: &OpenGpuiMountCapability,
        lease: OpenGpuiActivityLease,
    ) -> bool {
        let Some(current) = self
            .connection
            .as_mut()
            .filter(|current| current.matches(owner, mount))
        else {
            return false;
        };
        current.retain_connection(lease);
        true
    }

    fn epoch(&self) -> u64 {
        self.epoch.get()
    }

    fn callback_gate(&self) -> (Weak<Cell<u64>>, CallbackToken) {
        (Rc::downgrade(&self.epoch), self.callback_token())
    }

    fn semantic_generation(&self) -> Option<InteractionGeneration> {
        self.session.presentation().generation()
    }

    fn callback_token(&self) -> CallbackToken {
        CallbackToken {
            epoch: self.epoch(),
            semantic_generation: self.semantic_generation(),
        }
    }

    fn callback_is_current(&self, token: CallbackToken) -> bool {
        !self.terminal()
            && self.epoch() == token.epoch
            && self.semantic_generation() == token.semantic_generation
    }

    pub(super) fn query_work_pending(&self) -> bool {
        self.session.query_work_pending()
    }

    pub(super) fn query_resolution_active(&self) -> bool {
        self.query_resolution_active.get()
    }

    fn begin_query_resolution(&self) -> Option<QueryResolutionGuard> {
        if self.terminal() || self.query_resolution_active.replace(true) {
            return None;
        }
        Some(QueryResolutionGuard(Rc::clone(
            &self.query_resolution_active,
        )))
    }

    pub(super) fn prepare_query_frame_callback(
        &mut self,
    ) -> Option<(Weak<Cell<u64>>, CallbackToken)> {
        if !self.query_work_pending() || self.terminal() {
            return None;
        }
        let token = self.callback_token();
        if self.query_frame_scheduled == Some(token) {
            return None;
        }
        self.query_frame_scheduled = Some(token);
        Some((Rc::downgrade(&self.epoch), token))
    }

    pub(super) fn consume_query_frame_callback(&mut self, token: CallbackToken) -> bool {
        if self.query_frame_scheduled != Some(token) {
            return false;
        }
        self.query_frame_scheduled = None;
        #[cfg(target_os = "macos")]
        {
            self.native_query_frame_wake = None;
        }
        self.callback_is_current(token) && self.query_work_pending()
    }

    #[cfg(target_os = "macos")]
    fn native_frame_fallback_is_current(&self, token: NativeFrameFallbackToken) -> bool {
        match token {
            NativeFrameFallbackToken::Tick(token) => self.current_scheduled_tick() == Some(token),
            NativeFrameFallbackToken::Query(token) => {
                self.query_frame_scheduled == Some(token)
                    && self.callback_is_current(token)
                    && self.query_work_pending()
            }
        }
    }

    fn has_pending_submission(&self) -> bool {
        self.pending_submission.is_some()
    }

    fn install_pending_submission(&mut self, pending: PendingInteractionSubmission) {
        self.pending_submission = Some(pending);
    }

    fn take_pending_submission(&mut self) -> Option<PendingInteractionSubmission> {
        self.pending_submission.take()
    }

    fn restore_pending_submission(&mut self, pending: PendingInteractionSubmission) {
        self.pending_submission = Some(pending);
    }

    fn attach_prefix_task(
        &mut self,
        identity: PendingInteractionSubmissionIdentity,
        task: Task<()>,
    ) -> PrefixTaskAttachOutcome {
        let Some(pending) = self.pending_submission.take() else {
            return PrefixTaskAttachOutcome::MissingSubmission;
        };
        let (pending, outcome) = pending.attach_prefix_task(identity, task);
        self.pending_submission = Some(pending);
        outcome
    }

    fn publish_ready_prefix(
        &mut self,
        token: CallbackToken,
        ticket: InteractionTicketId,
        result: OpenGpuiCommandPrefixBarrierResult,
    ) -> bool {
        if !self.callback_is_current(token) {
            drop(result);
            return false;
        }
        self.ready_prefixes.push_back(ReadyInteractionPrefix {
            token,
            ticket,
            result,
        });
        true
    }

    fn take_ready_prefix(&mut self) -> Option<ReadyInteractionPrefix> {
        self.ready_prefixes.pop_front()
    }

    fn clear_submission_for_ticket(&mut self, ticket: InteractionTicketId) -> bool {
        if !self
            .pending_submission
            .as_ref()
            .is_some_and(|pending| pending.ticket == ticket)
        {
            return false;
        }
        self.pending_submission = None;
        true
    }

    fn clear_pending_submission_work(&mut self) {
        self.pending_submission = None;
        self.ready_prefixes.clear();
    }

    #[cfg(test)]
    fn has_pending_prefix_task(&self) -> bool {
        self.pending_submission
            .as_ref()
            .is_some_and(PendingInteractionSubmission::has_prefix_task)
    }

    fn discard_orphaned_revalidation(&mut self) -> bool {
        let Some(pending) = self.pending_submission.as_ref() else {
            return false;
        };
        if !pending.stage.is_revalidating() {
            return false;
        }
        let presentation = self.presentation();
        let active_ticket = presentation
            .connection()
            .and_then(|connection| connection.ticket());
        if presentation.phase() == CanvasInteractionPhase::Committing
            && active_ticket == Some(pending.ticket)
        {
            return false;
        }
        self.pending_submission = None;
        true
    }

    fn prepare_tick_callback(&mut self) -> Option<(Weak<Cell<u64>>, ScheduledTick)> {
        if self.session.tick_demand() != InteractionTickDemand::WhileActive {
            self.stop_ticking();
            return None;
        }
        let callback = self.callback_token();
        if self
            .tick_scheduled
            .is_some_and(|scheduled| scheduled.callback == callback)
        {
            return None;
        }
        let token = ScheduledTick {
            callback,
            sequence: self.next_tick_sequence,
        };
        self.next_tick_sequence = self.next_tick_sequence.wrapping_add(1);
        self.tick_scheduled = Some(token);
        self.last_tick_at.get_or_insert_with(Instant::now);
        self.tick_callback_registrations = self.tick_callback_registrations.saturating_add(1);
        Some((Rc::downgrade(&self.epoch), token))
    }

    fn current_scheduled_tick(&self) -> Option<ScheduledTick> {
        self.tick_scheduled
    }

    pub(super) fn prepare_animation_frame_request(&mut self) -> bool {
        let Some(scheduled) = self.tick_scheduled else {
            return false;
        };
        if self.tick_frame_requested == Some(scheduled) {
            return false;
        }
        self.tick_frame_requested = Some(scheduled);
        self.tick_frame_requests = self.tick_frame_requests.saturating_add(1);
        true
    }

    fn begin_scheduled_tick(&mut self, token: ScheduledTick, now: Instant) -> Option<f32> {
        if self.tick_scheduled != Some(token) {
            return None;
        }
        self.tick_scheduled = None;
        if !self.callback_is_current(token.callback) {
            return None;
        }
        Some(interaction_elapsed_seconds(&mut self.last_tick_at, now))
    }

    fn tick_callback_entries(&self) -> Rc<Cell<u64>> {
        Rc::clone(&self.tick_callback_entries)
    }

    pub(super) fn tick_diagnostics(&self) -> InteractionTickDiagnostics {
        #[cfg(target_os = "macos")]
        let native_tick_wake_active = self.native_tick_frame_wake.is_some();
        #[cfg(not(target_os = "macos"))]
        let native_tick_wake_active = false;
        #[cfg(target_os = "macos")]
        let native_query_wake_active = self.native_query_frame_wake.is_some();
        #[cfg(not(target_os = "macos"))]
        let native_query_wake_active = false;
        InteractionTickDiagnostics {
            callback_registrations: self.tick_callback_registrations,
            callback_entries: self.tick_callback_entries.get(),
            frame_requests: self.tick_frame_requests,
            callback_epoch: self.epoch(),
            pending_queries: usize::from(self.session.query_work_pending()),
            pending_submission: self.pending_submission.is_some(),
            ready_prefixes: self.ready_prefixes.len(),
            tick_scheduled: self.tick_scheduled.is_some(),
            tick_frame_requested: self.tick_frame_requested.is_some(),
            query_frame_scheduled: self.query_frame_scheduled.is_some(),
            query_resolution_active: self.query_resolution_active.get(),
            native_tick_wake_active,
            native_query_wake_active,
        }
    }

    fn stop_ticking(&mut self) {
        self.tick_scheduled = None;
        self.tick_frame_requested = None;
        #[cfg(target_os = "macos")]
        {
            self.native_tick_frame_wake = None;
        }
        self.last_tick_at = None;
    }

    fn stop_query_scheduling_if_idle(&mut self) {
        if self.query_work_pending() {
            return;
        }
        self.query_frame_scheduled = None;
        #[cfg(target_os = "macos")]
        {
            self.native_query_frame_wake = None;
        }
    }

    #[cfg(target_os = "macos")]
    fn replace_native_frame_wake(&mut self, token: NativeFrameFallbackToken, wake: Task<()>) {
        match token {
            NativeFrameFallbackToken::Tick(_) => self.native_tick_frame_wake = Some(wake),
            NativeFrameFallbackToken::Query(_) => self.native_query_frame_wake = Some(wake),
        }
    }

    pub(super) fn terminal(&self) -> bool {
        !matches!(&self.lifecycle, NativeInteractionOwnerLifecycle::Active)
    }

    pub(super) fn requires_window_cleanup(&self) -> bool {
        match &self.lifecycle {
            NativeInteractionOwnerLifecycle::Active => {
                self.native.requires_window_cleanup()
                    || self.connection.is_some()
                    || self.focus.is_some()
                    || self.graph_focus.has_entries()
            }
            NativeInteractionOwnerLifecycle::Prepared(_) => true,
            NativeInteractionOwnerLifecycle::Settled => false,
        }
    }

    pub(super) fn prepare_terminal(&mut self, window_cleanup_required: bool) -> bool {
        match &self.lifecycle {
            NativeInteractionOwnerLifecycle::Settled => return false,
            NativeInteractionOwnerLifecycle::Prepared(_) => return false,
            NativeInteractionOwnerLifecycle::Active => {}
        }
        let window_cleanup_required = window_cleanup_required || self.requires_window_cleanup();
        self.epoch.set(self.epoch().saturating_add(1));
        self.query_frame_scheduled = None;
        #[cfg(target_os = "macos")]
        {
            self.native_query_frame_wake = None;
        }
        self.pending_submission = None;
        self.ready_prefixes.clear();
        self.node_drag_residency = None;
        self.stop_ticking();
        self.connection = None;
        let root_focus = self.focus.take();
        self.graph_focus.retire();
        self.session.close();
        self.native.prepare_host_close_deferred();
        self.lifecycle = if window_cleanup_required {
            NativeInteractionOwnerLifecycle::Prepared(PendingTerminalWindowCleanup { root_focus })
        } else {
            NativeInteractionOwnerLifecycle::Settled
        };
        true
    }

    pub(super) fn finish_terminal_window_transition(
        &mut self,
        window: &mut Window,
        cx: &mut open_gpui::App,
    ) -> bool {
        if matches!(&self.lifecycle, NativeInteractionOwnerLifecycle::Active) {
            self.prepare_terminal(false);
        }
        let lifecycle = std::mem::replace(
            &mut self.lifecycle,
            NativeInteractionOwnerLifecycle::Settled,
        );
        let NativeInteractionOwnerLifecycle::Prepared(cleanup) = lifecycle else {
            return false;
        };
        #[cfg(test)]
        {
            self.terminal_window_transitions = self.terminal_window_transitions.saturating_add(1);
        }

        self.native.cancel_for_host_close(window, cx);
        let focus_was_owned = cleanup
            .root_focus
            .is_some_and(|focus| focus.contains_focused(window, cx));
        if focus_was_owned {
            window.blur();
            window.refresh();
        }
        true
    }

    #[cfg(test)]
    pub(super) fn terminal_window_transition_count(&self) -> u64 {
        self.terminal_window_transitions
    }

    #[cfg(test)]
    pub(super) fn has_focus_state(&self) -> bool {
        self.focus.is_some()
            || self.graph_focus.has_entries()
            || matches!(
                &self.lifecycle,
                NativeInteractionOwnerLifecycle::Prepared(PendingTerminalWindowCleanup {
                    root_focus: Some(_),
                })
            )
    }

    #[cfg(test)]
    pub(super) fn test_terminal_cleanup_pending(&self) -> bool {
        matches!(
            &self.lifecycle,
            NativeInteractionOwnerLifecycle::Prepared(_)
        )
    }

    #[cfg(test)]
    pub(super) fn test_replace_session(&mut self, session: CanvasInteractionSession) {
        self.epoch.set(self.epoch().saturating_add(1));
        self.session = session;
        self.query_frame_scheduled = None;
        #[cfg(target_os = "macos")]
        {
            self.native_query_frame_wake = None;
        }
        self.pending_submission = None;
        self.ready_prefixes.clear();
        self.node_drag_residency = None;
        self.stop_ticking();
        self.connection = None;
    }

    #[cfg(test)]
    pub(super) fn test_query_work_pending(&self) -> bool {
        self.session.query_work_pending()
    }

    #[cfg(test)]
    pub(super) fn test_has_pending_submission(&self) -> bool {
        self.pending_submission.is_some()
    }

    #[cfg(test)]
    pub(super) fn test_stale_prefix_task_attach_is_isolated(&mut self) -> bool {
        let Some(identity) = self
            .pending_submission
            .as_ref()
            .map(PendingInteractionSubmission::identity)
        else {
            return false;
        };
        let stale = PendingInteractionSubmissionIdentity {
            ticket: identity.ticket,
            token: CallbackToken {
                epoch: identity.token.epoch.wrapping_add(1),
                semantic_generation: identity.token.semantic_generation,
            },
        };
        let outcome = self.attach_prefix_task(stale, Task::ready(()));
        outcome == PrefixTaskAttachOutcome::IdentityMismatch
            && self
                .pending_submission
                .as_ref()
                .is_some_and(|pending| pending.identity() == identity && pending.has_prefix_task())
    }

    #[cfg(test)]
    pub(super) fn test_prefix_task_attach_transition_matrix(&self) -> bool {
        self.pending_submission
            .as_ref()
            .is_some_and(|pending| pending.test_prefix_task_attach_transition_matrix())
    }

    #[cfg(test)]
    pub(super) fn test_duplicate_prefix_task_attach_is_rejected(&mut self) -> bool {
        let Some(identity) = self
            .pending_submission
            .as_ref()
            .map(PendingInteractionSubmission::identity)
        else {
            return false;
        };
        let outcome = self.attach_prefix_task(identity, Task::ready(()));
        outcome == PrefixTaskAttachOutcome::InvalidStage
            && self
                .pending_submission
                .as_ref()
                .is_some_and(|pending| pending.identity() == identity && pending.has_prefix_task())
    }

    #[cfg(test)]
    pub(super) fn test_pending_work_empty(&self) -> bool {
        !self.session.query_work_pending()
            && self.pending_submission.is_none()
            && self.ready_prefixes.is_empty()
    }

    #[cfg(test)]
    pub(super) fn test_native_requires_window_cleanup(&self) -> bool {
        self.native.requires_window_cleanup()
    }

    #[cfg(test)]
    pub(super) fn test_tick_scheduled(&self) -> bool {
        self.tick_scheduled.is_some()
    }

    #[cfg(test)]
    pub(super) fn test_tick_callback_attempts(&self) -> u64 {
        self.tick_diagnostics().callback_entries
    }

    #[cfg(test)]
    pub(super) fn test_tick_callback_registrations(&self) -> u64 {
        self.tick_diagnostics().callback_registrations
    }

    #[cfg(test)]
    pub(super) fn test_tick_frame_requests(&self) -> u64 {
        self.tick_diagnostics().frame_requests
    }
}

struct PendingInteractionSubmission {
    ticket: InteractionTicketId,
    stage: PendingInteractionSubmissionStage,
}

enum PendingInteractionSubmissionStage {
    PreparingPrefixTask {
        token: CallbackToken,
        submission: InteractionGraphSubmission,
        cancellation: OpenGpuiCommandPrefixBarrierCancellation,
    },
    WaitingForPrefix {
        token: CallbackToken,
        submission: InteractionGraphSubmission,
        task: Task<()>,
        cancellation: OpenGpuiCommandPrefixBarrierCancellation,
    },
    Revalidating {
        token: CallbackToken,
        permit: OpenGpuiCommandPrefixPermit,
    },
    Enqueued {
        token: CallbackToken,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PendingInteractionSubmissionIdentity {
    ticket: InteractionTicketId,
    token: CallbackToken,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PrefixTaskAttachOutcome {
    Attached,
    MissingSubmission,
    IdentityMismatch,
    InvalidStage,
}

impl PendingInteractionSubmission {
    fn preparing_prefix(
        ticket: InteractionTicketId,
        token: CallbackToken,
        submission: InteractionGraphSubmission,
        cancellation: OpenGpuiCommandPrefixBarrierCancellation,
    ) -> Self {
        Self {
            ticket,
            stage: PendingInteractionSubmissionStage::PreparingPrefixTask {
                token,
                submission,
                cancellation,
            },
        }
    }

    fn revalidating(
        ticket: InteractionTicketId,
        token: CallbackToken,
        permit: OpenGpuiCommandPrefixPermit,
    ) -> Self {
        Self {
            ticket,
            stage: PendingInteractionSubmissionStage::Revalidating { token, permit },
        }
    }

    fn enqueued(ticket: InteractionTicketId, token: CallbackToken) -> Self {
        Self {
            ticket,
            stage: PendingInteractionSubmissionStage::Enqueued { token },
        }
    }

    fn identity(&self) -> PendingInteractionSubmissionIdentity {
        PendingInteractionSubmissionIdentity {
            ticket: self.ticket,
            token: self.stage.token(),
        }
    }

    #[cfg(test)]
    fn has_prefix_task(&self) -> bool {
        matches!(
            self.stage,
            PendingInteractionSubmissionStage::WaitingForPrefix { .. }
        )
    }

    fn attach_prefix_task(
        self,
        identity: PendingInteractionSubmissionIdentity,
        task: Task<()>,
    ) -> (Self, PrefixTaskAttachOutcome) {
        if self.identity() != identity {
            return (self, PrefixTaskAttachOutcome::IdentityMismatch);
        }
        let Self { ticket, stage } = self;
        match stage {
            PendingInteractionSubmissionStage::PreparingPrefixTask {
                token,
                submission,
                cancellation,
            } => (
                Self {
                    ticket,
                    stage: PendingInteractionSubmissionStage::WaitingForPrefix {
                        token,
                        submission,
                        task,
                        cancellation,
                    },
                },
                PrefixTaskAttachOutcome::Attached,
            ),
            stage @ (PendingInteractionSubmissionStage::WaitingForPrefix { .. }
            | PendingInteractionSubmissionStage::Revalidating { .. }
            | PendingInteractionSubmissionStage::Enqueued { .. }) => (
                Self { ticket, stage },
                PrefixTaskAttachOutcome::InvalidStage,
            ),
        }
    }

    #[cfg(test)]
    fn test_prefix_task_attach_transition_matrix(&self) -> bool {
        let identity = self.identity();
        let waiting_rejected = self.has_prefix_task();

        let permit_journal = OpenGpuiCommandJournal::new();
        let mut permit_barrier = permit_journal
            .capture_prefix_barrier()
            .expect("test prefix permit barrier");
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        let permit = match std::future::Future::poll(
            std::pin::Pin::new(&mut permit_barrier),
            &mut context,
        ) {
            std::task::Poll::Ready(Ok(permit)) => permit,
            std::task::Poll::Ready(Err(error)) => panic!("test prefix permit failed: {error}"),
            std::task::Poll::Pending => panic!("empty test prefix barrier did not settle"),
        };
        let revalidating = Self::revalidating(identity.ticket, identity.token, permit);
        let (revalidating, revalidating_outcome) =
            revalidating.attach_prefix_task(identity, Task::ready(()));
        let revalidating_rejected = revalidating_outcome == PrefixTaskAttachOutcome::InvalidStage
            && revalidating.identity() == identity
            && matches!(
                &revalidating.stage,
                PendingInteractionSubmissionStage::Revalidating { .. }
            );

        let enqueued = Self::enqueued(identity.ticket, identity.token);
        let (enqueued, enqueued_outcome) = enqueued.attach_prefix_task(identity, Task::ready(()));
        let enqueued_rejected = enqueued_outcome == PrefixTaskAttachOutcome::InvalidStage
            && enqueued.identity() == identity
            && matches!(
                &enqueued.stage,
                PendingInteractionSubmissionStage::Enqueued { .. }
            );

        drop((revalidating, enqueued));
        waiting_rejected && revalidating_rejected && enqueued_rejected
    }
}

impl PendingInteractionSubmissionStage {
    fn token(&self) -> CallbackToken {
        match self {
            Self::PreparingPrefixTask { token, .. }
            | Self::WaitingForPrefix { token, .. }
            | Self::Revalidating { token, .. }
            | Self::Enqueued { token } => *token,
        }
    }

    fn is_revalidating(&self) -> bool {
        matches!(self, Self::Revalidating { .. })
    }
}

impl OpenGpuiSceneHost {
    /// Captures a native point through the bound view so interaction input cannot be forged from
    /// an adapter-owned transform.
    pub(super) fn viewport_pointer_sample(
        &self,
        position: Point<Pixels>,
    ) -> Option<ViewportPointerSample> {
        let screen = self.scene_screen_position(position)?;
        self.view
            .viewport_pointer_sample(ViewPoint::new(screen.x, screen.y))
            .ok()
    }

    pub(super) fn apply_canvas_interaction_update(
        &mut self,
        update: CanvasInteractionUpdate,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.interaction_owner.terminal() {
            return;
        }
        let dispatch = update.into_dispatch();
        #[cfg(test)]
        if let Some(work) = dispatch.port_connection_query_work {
            self.scene_evidence
                .last_port_connection_query_work
                .set(Some(work));
        }
        let effects = dispatch.effects;
        let damage = dispatch.damage;
        let tick_demand = dispatch.tick_demand;
        debug_assert_eq!(
            dispatch.query_work_pending,
            self.interaction_owner.query_work_pending()
        );
        let has_damage = damage != Default::default();
        for effect in effects {
            if let Err(error) = self.apply_canvas_interaction_effect(effect, cx) {
                self.record_command_diagnostic(error);
                if matches!(
                    self.interaction_owner.presentation().phase(),
                    CanvasInteractionPhase::Armed
                        | CanvasInteractionPhase::NodeDrag
                        | CanvasInteractionPhase::Pan
                        | CanvasInteractionPhase::Marquee
                        | CanvasInteractionPhase::ConnectionDrag
                        | CanvasInteractionPhase::EdgeActive
                ) {
                    let cancelled = self.interaction_owner.cancel();
                    self.discard_pending_interaction_submission();
                    match cancelled {
                        Ok(cancelled) => {
                            self.apply_canvas_interaction_update(cancelled, window, cx)
                        }
                        Err(error) => self.record_command_diagnostic(error.to_string()),
                    }
                }
                break;
            }
        }
        self.discard_orphaned_interaction_revalidation();
        self.sync_native_connection_state(window, cx);
        if tick_demand == InteractionTickDemand::WhileActive {
            self.schedule_interaction_tick(window, cx);
        } else {
            self.interaction_owner.stop_ticking();
        }
        if self.interaction_owner.query_work_pending()
            && !self.interaction_owner.query_resolution_active()
        {
            self.process_one_interaction_query(window, cx);
        }
        let query_work_pending = self.interaction_owner.query_work_pending();
        if query_work_pending {
            self.schedule_interaction_query_frame(window, cx);
        } else {
            self.interaction_owner.stop_query_scheduling_if_idle();
        }
        if has_damage || query_work_pending {
            cx.notify();
        }
        self.finish_blank_canvas_context_query(window, cx);
    }

    pub(super) fn apply_canvas_interaction_result(
        &mut self,
        result: Result<CanvasInteractionUpdate, CanvasInteractionError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match result {
            Ok(update) => {
                self.apply_canvas_interaction_update(update, window, cx);
                true
            }
            Err(error) => {
                self.record_command_diagnostic(error.to_string());
                self.sync_native_connection_state(window, cx);
                cx.notify();
                false
            }
        }
    }

    fn apply_canvas_interaction_effect(
        &mut self,
        effect: CanvasInteractionEffect,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        match effect {
            CanvasInteractionEffect::Search(_) => Ok(()),
            CanvasInteractionEffect::SubmitGraph { submission } => {
                self.submit_interaction_submission(submission, cx)
            }
        }
    }

    pub(super) fn process_one_interaction_query(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.interaction_owner.terminal() {
            return;
        }
        let Some(_resolution) = self.interaction_owner.begin_query_resolution() else {
            return;
        };
        if !self.interaction_owner.query_work_pending() {
            return;
        }
        let (update, needs_tick, receipt, terminal_error) =
            self.interaction_owner.advance_query_work(1).into_parts();
        debug_assert_eq!(receipt.pages_granted(), 1);
        debug_assert!(receipt.pages_attempted() <= 1);
        debug_assert_eq!(needs_tick, self.interaction_owner.query_work_pending());
        if let Some(error) = terminal_error {
            self.record_command_diagnostic(format!(
                "native canvas interaction query failed: {error}"
            ));
            self.interaction_owner
                .cancel_native_for_query_failure(window, cx);
            self.discard_pending_interaction_submission();
        }
        self.apply_canvas_interaction_update(update, window, cx);
    }

    fn submit_interaction_submission(
        &mut self,
        submission: InteractionGraphSubmission,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let ticket = submission.ticket();
        if self.interaction_owner.has_pending_submission() {
            return self.enqueue_revalidated_interaction_submission(submission, cx);
        }

        let token = self.interaction_owner.callback_token();
        let barrier = self.commands.capture_prefix_barrier().map_err(|error| {
            self.interaction_submission_not_dispatched(ticket);
            format!("failed to capture native command prefix: {error}")
        })?;
        let identity = PendingInteractionSubmissionIdentity { ticket, token };
        let pending = PendingInteractionSubmission::preparing_prefix(
            ticket,
            token,
            submission,
            barrier.cancellation(),
        );
        self.interaction_owner.install_pending_submission(pending);
        let host = cx.weak_entity();
        let (epoch_gate, gate_token) = self.interaction_owner.callback_gate();
        let prefix_task = cx.spawn(async move |_, cx| {
            let result = barrier.await;
            if !callback_epoch_is_current(&epoch_gate, gate_token) {
                drop(result);
                return;
            }
            let _ = host.update(cx, |host, cx| {
                if host
                    .interaction_owner
                    .publish_ready_prefix(token, ticket, result)
                {
                    cx.notify();
                }
            });
        });
        let _ = self
            .interaction_owner
            .attach_prefix_task(identity, prefix_task);
        Ok(())
    }

    fn enqueue_revalidated_interaction_submission(
        &mut self,
        submission: InteractionGraphSubmission,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let ticket = submission.ticket();
        let Some(pending) = self.interaction_owner.take_pending_submission() else {
            return Err("interaction command prefix state disappeared".to_owned());
        };
        let identity = pending.identity();
        if !self.interaction_owner.callback_is_current(identity.token) {
            return Err("interaction command belongs to a retired generation".to_owned());
        }
        if identity.ticket != ticket {
            self.interaction_owner.restore_pending_submission(pending);
            return Err("interaction command ticket does not match its prefix permit".to_owned());
        }
        let PendingInteractionSubmission {
            ticket: pending_ticket,
            stage,
        } = pending;
        let (token, permit) = match stage {
            PendingInteractionSubmissionStage::Revalidating { token, permit } => (token, permit),
            stage => {
                self.interaction_owner
                    .restore_pending_submission(PendingInteractionSubmission {
                        ticket: pending_ticket,
                        stage,
                    });
                return Err("interaction mutation arrived before its prefix settled".to_owned());
            }
        };
        match self.bind_and_enqueue_interaction(submission, permit) {
            Ok(true) => {
                self.interaction_owner.restore_pending_submission(
                    PendingInteractionSubmission::enqueued(pending_ticket, token),
                );
                cx.notify();
                Ok(())
            }
            Ok(false) => {
                self.complete_interaction_without_command(ticket);
                Ok(())
            }
            Err(error) => {
                self.interaction_submission_not_dispatched(ticket);
                Err(error.message)
            }
        }
    }

    pub(super) fn process_one_ready_interaction_prefix(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(ready) = self.interaction_owner.take_ready_prefix() else {
            return;
        };
        if !self.interaction_owner.callback_is_current(ready.token) {
            drop(ready.result);
            return;
        }
        let ReadyInteractionPrefix {
            token,
            ticket,
            result,
        } = ready;
        let Some(pending) = self.interaction_owner.take_pending_submission() else {
            drop(result);
            return;
        };
        if pending.identity() != (PendingInteractionSubmissionIdentity { ticket, token }) {
            self.interaction_owner.restore_pending_submission(pending);
            drop(result);
            return;
        }
        let PendingInteractionSubmission {
            ticket: pending_ticket,
            stage,
        } = pending;
        let (token, submission) = match stage {
            PendingInteractionSubmissionStage::WaitingForPrefix {
                token,
                submission,
                task,
                cancellation,
            } => {
                drop((task, cancellation));
                (token, submission)
            }
            stage => {
                self.interaction_owner
                    .restore_pending_submission(PendingInteractionSubmission {
                        ticket: pending_ticket,
                        stage,
                    });
                drop(result);
                return;
            }
        };
        let permit = match result {
            Ok(permit) => permit,
            Err(error) => {
                self.interaction_submission_not_dispatched(ticket);
                self.record_prefix_barrier_failure(error);
                self.sync_native_connection_state(window, cx);
                return;
            }
        };

        self.interaction_owner.restore_pending_submission(
            PendingInteractionSubmission::revalidating(pending_ticket, token, permit),
        );
        let update = self
            .interaction_owner
            .resume_submission_after_prefix(submission);
        match update {
            Ok(update) => self.apply_canvas_interaction_update(update, window, cx),
            Err(error) => {
                self.discard_pending_interaction_submission();
                self.interaction_submission_not_dispatched(ticket);
                self.record_command_diagnostic(format!(
                    "failed to refresh native interaction command: {error}"
                ));
                self.sync_native_connection_state(window, cx);
            }
        }
    }

    fn bind_and_enqueue_interaction(
        &mut self,
        submission: InteractionGraphSubmission,
        permit: OpenGpuiCommandPrefixPermit,
    ) -> Result<bool, InteractionEnqueueFailure> {
        let ticket = submission.ticket();
        let command = self
            .interaction_owner
            .bind_submission(submission)
            .map_err(|error| InteractionEnqueueFailure::new(error.to_string()))?;
        let Some(command) = command else {
            permit.abandon();
            return Ok(false);
        };
        permit
            .enqueue_detached(command, OpenGpuiCommandOrigin::Interaction { ticket })
            .map_err(|error| InteractionEnqueueFailure::new(error.to_string()))?;
        Ok(true)
    }

    pub(super) fn settle_interaction_command(
        &mut self,
        ticket: InteractionTicketId,
        completion: CommandExecutionCompletion,
    ) {
        self.interaction_owner.clear_submission_for_ticket(ticket);
        let result = self
            .interaction_owner
            .complete_submission(ticket, completion);
        self.finish_interaction_completion(result);
    }

    pub(super) fn interaction_submission_not_dispatched(&mut self, ticket: InteractionTicketId) {
        self.interaction_owner.clear_submission_for_ticket(ticket);
        let result = self.interaction_owner.submission_not_dispatched(ticket);
        self.finish_interaction_completion(result);
    }

    pub(super) fn interaction_submission_outcome_unknown(&mut self, ticket: InteractionTicketId) {
        self.interaction_owner.clear_submission_for_ticket(ticket);
        let result = self.interaction_owner.submission_outcome_unknown(ticket);
        self.finish_interaction_completion(result);
    }

    fn complete_interaction_without_command(&mut self, ticket: InteractionTicketId) {
        self.interaction_owner.clear_submission_for_ticket(ticket);
        let result = self
            .interaction_owner
            .complete_submission_without_command(ticket);
        self.finish_interaction_completion(result);
    }

    fn finish_interaction_completion(
        &mut self,
        result: Result<CanvasInteractionUpdate, CanvasInteractionError>,
    ) {
        match result {
            Ok(_update) => {}
            Err(error) => self.record_command_diagnostic(format!(
                "failed to settle native canvas interaction: {error}"
            )),
        }
    }

    #[cfg(test)]
    pub(super) fn bind_pending_interaction_submission_for_execution(
        &mut self,
    ) -> Result<(InteractionTicketId, Option<GraphCommand>), String> {
        let pending = self
            .interaction_owner
            .take_pending_submission()
            .ok_or_else(|| "native interaction has no pending submission".to_owned())?;
        let PendingInteractionSubmission { ticket, stage } = pending;
        let submission = match stage {
            PendingInteractionSubmissionStage::PreparingPrefixTask {
                submission,
                cancellation,
                ..
            } => {
                drop(cancellation);
                submission
            }
            PendingInteractionSubmissionStage::WaitingForPrefix {
                submission,
                task,
                cancellation,
                ..
            } => {
                drop((task, cancellation));
                submission
            }
            stage @ (PendingInteractionSubmissionStage::Revalidating { .. }
            | PendingInteractionSubmissionStage::Enqueued { .. }) => {
                self.interaction_owner
                    .restore_pending_submission(PendingInteractionSubmission { ticket, stage });
                return Err("native interaction submission is no longer bindable".to_owned());
            }
        };
        let command = self
            .interaction_owner
            .bind_submission(submission)
            .map_err(|error| error.to_string())?;
        Ok((ticket, command))
    }

    pub(super) fn discard_pending_interaction_submission(&mut self) {
        self.interaction_owner.clear_pending_submission_work();
    }

    #[cfg(test)]
    pub(super) fn has_pending_interaction_prefix_task(&self) -> bool {
        self.interaction_owner.has_pending_prefix_task()
    }

    fn discard_orphaned_interaction_revalidation(&mut self) {
        self.interaction_owner.discard_orphaned_revalidation();
    }

    pub(super) fn retire_canvas_interaction_owner(&mut self, owner: InteractionOwner) {
        let _update = self.interaction_owner.owner_retired(owner);
        if self.interaction_owner.presentation().terminal()
            == Some(InteractionTerminalReason::OwnerRetired)
        {
            self.discard_pending_interaction_submission();
        }
    }

    pub(super) fn cancel_invalid_node_drag_residency(&mut self) -> bool {
        let validation = self
            .view
            .graph_snapshot()
            .ok()
            .map(|snapshot| self.interaction_owner.required_node_drag_nodes(&snapshot));
        let Some(Err(error)) = validation else {
            return false;
        };
        match error {
            NodeDragRequiredNodesError::MemberRetired { member, current } => {
                self.interaction_owner
                    .cancel_scene_deferred(OpenGpuiSceneGestureCancelReason::OwnerRetired);
                self.retire_canvas_interaction_owner(InteractionOwner::Node(member));
                self.record_command_diagnostic(format!(
                    "node drag member {:?} retired before required materialization (expected {:?}, current {:?})",
                    member.node(),
                    member,
                    current,
                ));
            }
            NodeDragRequiredNodesError::CapacityExceeded { count } => {
                self.interaction_owner.cancel_scene_deferred(
                    OpenGpuiSceneGestureCancelReason::ProtectedCapacityExceeded,
                );
                let update = self.interaction_owner.cancel();
                self.discard_pending_interaction_submission();
                match update {
                    Ok(_update) => {}
                    Err(error) => self.record_command_diagnostic(error.to_string()),
                }
                self.record_command_diagnostic(format!(
                    "node drag cohort of {count} members exceeds the native required-node capacity"
                ));
            }
        }
        true
    }

    fn record_prefix_barrier_failure(&mut self, error: OpenGpuiCommandPrefixBarrierError) {
        self.record_command_diagnostic(format!("native command prefix failed: {error}"));
    }

    /// Advances one queued continuous interaction from an externally owned frame clock.
    ///
    /// The standard Open GPUI host path invokes the same advancement from
    /// [`Window::on_next_frame`]. Call this only when an integration owns the frame driver and
    /// must explicitly advance the host, such as an off-screen test harness. It consumes at most
    /// one already scheduled tick and returns whether a tick was consumed.
    #[must_use]
    pub fn advance_interaction_frame(
        &mut self,
        now: Instant,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(token) = self.interaction_owner.current_scheduled_tick() else {
            return false;
        };
        self.advance_scheduled_interaction_tick(token, now, window, cx)
    }

    pub(crate) fn schedule_interaction_tick(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((epoch_gate, token)) = self.interaction_owner.prepare_tick_callback() else {
            return;
        };
        let host = cx.weak_entity();
        let tick_callback_entries = self.interaction_owner.tick_callback_entries();
        window.on_next_frame(move |window, cx| {
            tick_callback_entries.set(tick_callback_entries.get().saturating_add(1));
            if !callback_epoch_is_current(&epoch_gate, token.callback) {
                return;
            }
            let Some(host) = host.upgrade() else {
                return;
            };
            host.update(cx, |host, cx| {
                let _ = host.advance_scheduled_interaction_tick(token, Instant::now(), window, cx);
            });
        });
        #[cfg(target_os = "macos")]
        self.schedule_native_frame_fallback(NativeFrameFallbackToken::Tick(token), window, cx);
        cx.notify();
    }

    fn schedule_interaction_query_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((epoch_gate, token)) = self.interaction_owner.prepare_query_frame_callback()
        else {
            return;
        };
        let host = cx.weak_entity();
        window.on_next_frame(move |_, cx| {
            if !callback_epoch_is_current(&epoch_gate, token) {
                return;
            }
            let Some(host) = host.upgrade() else {
                return;
            };
            host.update(cx, |host, cx| {
                if host.interaction_owner.consume_query_frame_callback(token) {
                    cx.notify();
                }
            });
        });
        #[cfg(target_os = "macos")]
        self.schedule_native_frame_fallback(NativeFrameFallbackToken::Query(token), window, cx);
        cx.notify();
    }

    /// When the platform display link does not start, request one AppKit redraw after the current
    /// display callback has unwound. The task only wakes the platform. It cannot advance or mutate
    /// interaction state, and it becomes inert once a real frame consumes this token.
    #[cfg(target_os = "macos")]
    fn schedule_native_frame_fallback(
        &mut self,
        token: NativeFrameFallbackToken,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let wake = cx.spawn_in(
            window,
            async move |host: WeakEntity<OpenGpuiSceneHost>, cx: &mut AsyncWindowContext| {
                for _ in 0..NATIVE_FRAME_FALLBACK_ATTEMPTS {
                    cx.background_executor()
                        .timer(NATIVE_FRAME_FALLBACK_DELAY)
                        .await;
                    let should_wake = host
                        .update(cx, |host, _| {
                            host.interaction_owner
                                .native_frame_fallback_is_current(token)
                        })
                        .unwrap_or(false);
                    if !should_wake {
                        return;
                    }
                    let platform_wake = cx
                        .update(|window, _| super::native_frame_wake::prepare(window))
                        .ok()
                        .flatten();
                    if let Some(platform_wake) = platform_wake {
                        platform_wake.request();
                    }
                }
            },
        );
        self.interaction_owner
            .replace_native_frame_wake(token, wake);
    }

    fn advance_scheduled_interaction_tick(
        &mut self,
        token: ScheduledTick,
        now: Instant,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(elapsed_seconds) = self.interaction_owner.begin_scheduled_tick(token, now) else {
            return false;
        };
        let Some(viewport) = self.surface_size else {
            self.interaction_owner.stop_ticking();
            return true;
        };
        match self.interaction_owner.tick(elapsed_seconds, viewport) {
            Ok(update) => self.apply_canvas_interaction_update(update, window, cx),
            Err(error) => {
                self.interaction_owner.stop_ticking();
                self.record_command_diagnostic(format!(
                    "native canvas interaction tick failed: {error}"
                ));
            }
        }
        true
    }

    pub(super) fn interaction_node_drag_offsets(&mut self) -> BTreeMap<NodeId, CanvasPoint> {
        self.stats.node_drag_offset_map_builds =
            self.stats.node_drag_offset_map_builds.saturating_add(1);
        self.interaction_owner
            .presentation()
            .node_drag()
            .map(|drag| {
                drag.nodes()
                    .iter()
                    .copied()
                    .map(|node| (node, drag.delta()))
                    .collect()
            })
            .unwrap_or_default()
    }

    #[cfg(test)]
    pub(super) fn interaction_node_drag_delta(&self, node: NodeId) -> Option<CanvasPoint> {
        self.interaction_owner
            .presentation()
            .node_drag()
            .filter(|drag| drag.nodes().contains(&node))
            .map(|drag| drag.delta())
    }

    pub(super) fn interaction_marquee_view_bounds(
        &self,
        transform: ViewportTransform,
    ) -> Option<ViewRect> {
        let bounds = self.interaction_owner.presentation().marquee()?;
        projected_view_rect(bounds, transform)
    }
}

fn interaction_elapsed_seconds(last_tick: &mut Option<Instant>, now: Instant) -> f32 {
    let previous = last_tick.replace(now).unwrap_or(now);
    now.saturating_duration_since(previous)
        .as_secs_f32()
        .max(f32::EPSILON)
}

fn callback_epoch_is_current(epoch: &Weak<Cell<u64>>, token: CallbackToken) -> bool {
    epoch
        .upgrade()
        .is_some_and(|epoch| epoch.get() == token.epoch)
}

struct InteractionEnqueueFailure {
    message: String,
}

impl InteractionEnqueueFailure {
    fn new(message: String) -> Self {
        Self { message }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use jellyflow_core::{CanvasSize, GraphBuilder, GraphId, Node, NodeKindKey};
    use jellyflow_runtime::{
        commands::GraphCommand,
        commands::IncidentEdgePolicy,
        commands::TopologyEdit,
        graph::GraphRuntime,
        view::{ViewSessionError, ViewSessionOptions},
    };

    use super::*;

    fn set_selection_by_id(view: &ViewSession, nodes: &[NodeId]) {
        let snapshot = view.graph_snapshot().expect("test view snapshot");
        let nodes = nodes
            .iter()
            .copied()
            .map(|node| {
                snapshot
                    .node_lifetime_key(node)
                    .expect("selected test node is current")
            })
            .collect();
        view.set_selection(nodes, Vec::new(), Vec::new())
            .expect("test selection is accepted");
    }

    fn node_drag_start_by_id(
        view: &ViewSession,
        node: NodeId,
    ) -> Result<Option<NodeDragStart>, ViewSessionError> {
        let lifetime = view
            .node_lifetime_key(node)?
            .expect("dragged test node is current");
        view.node_drag_start(lifetime, SelectionMode::Replace)
    }

    #[test]
    fn interaction_clock_preserves_elapsed_time_at_common_refresh_rates() {
        let start = Instant::now();
        for frames_per_second in [30_u32, 60, 120] {
            let mut last_tick = Some(start);
            let mut accumulated = 0.0_f32;
            for frame in 1..=frames_per_second {
                let now = start + Duration::from_secs_f64(frame as f64 / frames_per_second as f64);
                accumulated += interaction_elapsed_seconds(&mut last_tick, now);
            }
            assert!(
                (accumulated - 1.0).abs() < 0.000_1,
                "{frames_per_second}Hz accumulated {accumulated}s"
            );
        }
    }

    #[test]
    fn pending_query_frame_is_deduplicated_and_lifetime_bound() {
        let mut fixture = node_drag_fixture(1);
        assert!(fixture.owner.prepare_query_frame_callback().is_none());

        let update = fixture
            .owner
            .request_edge_pick(pointer_sample(&fixture.view, 20.0, 20.0))
            .unwrap();
        assert!(update.query_work_pending());
        let (_, token) = fixture
            .owner
            .prepare_query_frame_callback()
            .expect("pending edge pick must own a frame callback");
        assert!(fixture.owner.prepare_query_frame_callback().is_none());
        assert!(fixture.owner.consume_query_frame_callback(token));
        assert!(fixture.owner.query_frame_scheduled.is_none());

        let (_, retired) = fixture
            .owner
            .prepare_query_frame_callback()
            .expect("unconsumed query can request its next page");
        fixture.owner.prepare_terminal(false);
        assert!(!fixture.owner.consume_query_frame_callback(retired));
        assert!(fixture.owner.query_frame_scheduled.is_none());
    }

    #[test]
    fn completed_query_lifetime_retires_its_native_callback_without_waiting_for_entry() {
        let mut fixture = node_drag_fixture(1);
        let update = fixture
            .owner
            .request_edge_pick(pointer_sample(&fixture.view, 20.0, 20.0))
            .unwrap();
        assert!(update.query_work_pending());
        let (_, stale) = fixture
            .owner
            .prepare_query_frame_callback()
            .expect("pending edge pick owns a native callback");

        fixture.owner.cancel().expect("cancel pending edge pick");

        assert!(!fixture.owner.query_work_pending());
        assert!(fixture.owner.query_frame_scheduled.is_none());
        assert!(!fixture.owner.consume_query_frame_callback(stale));
    }

    #[test]
    fn retired_query_owner_releases_its_native_callback_at_the_owner_boundary() {
        let mut fixture = node_drag_fixture(2);
        fixture
            .owner
            .set_config(CanvasInteractionConfig {
                alignment_snap: jellyflow_runtime::interaction::AlignmentSnapConfig::new(6.0, 9.0),
                query_work_budget: 1,
                ..CanvasInteractionConfig::default()
            })
            .expect("alignment query config");
        arm_node_drag(&mut fixture, 1);
        fixture
            .owner
            .move_to(pointer_sample(&fixture.view, 40.0, 24.0))
            .expect("node drag queues alignment work");
        assert!(fixture.owner.query_work_pending());
        let (_, stale) = fixture
            .owner
            .prepare_query_frame_callback()
            .expect("alignment work owns a native callback");
        let lifetime = fixture
            .runtime
            .snapshot()
            .expect("current graph snapshot")
            .node_lifetime_key(fixture.nodes[0])
            .expect("drag owner remains current");

        fixture
            .owner
            .owner_retired(InteractionOwner::Node(lifetime));

        assert!(!fixture.owner.query_work_pending());
        assert!(fixture.owner.query_frame_scheduled.is_none());
        assert!(!fixture.owner.consume_query_frame_callback(stale));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn tick_and_query_native_frame_fallbacks_have_independent_owners() {
        let mut fixture = node_drag_fixture(1);
        let callback = fixture.owner.callback_token();
        let tick = ScheduledTick {
            callback,
            sequence: 1,
        };

        fixture
            .owner
            .replace_native_frame_wake(NativeFrameFallbackToken::Tick(tick), Task::ready(()));
        assert!(fixture.owner.native_tick_frame_wake.is_some());
        assert!(fixture.owner.native_query_frame_wake.is_none());

        fixture
            .owner
            .replace_native_frame_wake(NativeFrameFallbackToken::Query(callback), Task::ready(()));
        assert!(fixture.owner.native_tick_frame_wake.is_some());
        assert!(fixture.owner.native_query_frame_wake.is_some());

        fixture.owner.stop_ticking();
        assert!(fixture.owner.native_tick_frame_wake.is_none());
        assert!(fixture.owner.native_query_frame_wake.is_some());

        fixture.owner.prepare_terminal(false);
        assert!(fixture.owner.native_query_frame_wake.is_none());
    }

    #[test]
    fn node_drag_residency_covers_auto_pan_prefix_and_receipt_wait_atomically() {
        let mut fixture = node_drag_fixture(2);
        arm_node_drag(&mut fixture, 2);
        assert_required_nodes(&fixture, 2);

        let end = pointer_sample(&fixture.view, 195.0, 40.0);
        fixture.owner.move_to(end).unwrap();
        fixture
            .owner
            .tick(1.0 / 60.0, ViewSize::new(200.0, 120.0))
            .unwrap();
        assert_required_nodes(&fixture, 2);

        let (ticket, submission) = release_node_drag(&mut fixture, end);
        assert_required_nodes(&fixture, 2);
        let journal = OpenGpuiCommandJournal::new();
        let barrier = journal.capture_prefix_barrier().unwrap();
        let token = fixture.owner.callback_token();
        let identity = PendingInteractionSubmissionIdentity { ticket, token };
        fixture
            .owner
            .install_pending_submission(PendingInteractionSubmission::preparing_prefix(
                ticket,
                token,
                submission,
                barrier.cancellation(),
            ));
        assert_eq!(
            fixture.owner.attach_prefix_task(identity, Task::ready(())),
            PrefixTaskAttachOutcome::Attached
        );
        assert_required_nodes(&fixture, 2);

        fixture.owner.pending_submission =
            Some(PendingInteractionSubmission::enqueued(ticket, token));
        assert_required_nodes(&fixture, 2);
        fixture.owner.submission_not_dispatched(ticket).unwrap();
        assert!(!fixture.owner.has_node_drag_residency());
    }

    #[test]
    fn applied_residency_waits_for_its_revision_and_late_g1_cannot_release_g2() {
        let mut fixture = node_drag_fixture(2);
        arm_node_drag(&mut fixture, 2);
        fixture
            .owner
            .move_to(pointer_sample(&fixture.view, 40.0, 24.0))
            .unwrap();
        let g1_release = pointer_sample(&fixture.view, 40.0, 24.0);
        let (g1_ticket, g1_submission) = release_node_drag(&mut fixture, g1_release);
        let g1_command = fixture
            .owner
            .bind_submission(g1_submission)
            .unwrap()
            .expect("G1 node drag binds a command");
        let g1_completion = fixture.runtime.execute(g1_command);
        let g1_revision = g1_completion.applied_revision().unwrap();
        fixture
            .owner
            .complete_submission(g1_ticket, g1_completion)
            .unwrap();
        assert_required_nodes(&fixture, 2);
        assert!(
            !fixture
                .owner
                .observe_materialized_frame(g1_revision.saturating_sub(1), [])
        );
        assert_required_nodes(&fixture, 2);

        arm_node_drag(&mut fixture, 2);
        assert!(!fixture.owner.observe_materialized_frame(g1_revision, []));
        assert_required_nodes(&fixture, 2);
        let late_snapshot = fixture.runtime.snapshot().unwrap();
        let late_completion = fixture.runtime.execute(GraphCommand::set_node_data(
            late_snapshot.version(),
            fixture.nodes[0],
            serde_json::json!({ "late": true }),
        ));
        fixture
            .owner
            .complete_submission(g1_ticket, late_completion)
            .unwrap();
        assert_required_nodes(&fixture, 2);

        fixture.owner.cancel().expect("cancel G1 drag");
        arm_node_drag(&mut fixture, 2);
        fixture
            .owner
            .move_to(pointer_sample(&fixture.view, 40.0, 24.0))
            .unwrap();
        let g2_release = pointer_sample(&fixture.view, 40.0, 24.0);
        let (g2_ticket, g2_submission) = release_node_drag(&mut fixture, g2_release);
        let g2_command = fixture
            .owner
            .bind_submission(g2_submission)
            .unwrap()
            .expect("G2 node drag binds a command");
        let g2_completion = fixture.runtime.execute(g2_command);
        let g2_revision = g2_completion.applied_revision().unwrap();
        fixture
            .owner
            .complete_submission(g2_ticket, g2_completion)
            .unwrap();
        let complete_cohort = fixture
            .nodes
            .iter()
            .map(|node| {
                fixture
                    .runtime
                    .snapshot()
                    .unwrap()
                    .node_lifetime_key(*node)
                    .unwrap()
            })
            .collect::<Vec<_>>();
        assert!(!fixture.owner.observe_materialized_frame(
            g2_revision.saturating_sub(1),
            complete_cohort.iter().copied(),
        ));
        assert!(
            !fixture
                .owner
                .observe_materialized_frame(g2_revision, complete_cohort[..1].iter().copied())
        );
        assert!(
            fixture
                .owner
                .observe_materialized_frame(g2_revision, complete_cohort.iter().copied())
        );
        assert!(!fixture.owner.has_node_drag_residency());
    }

    #[test]
    fn required_node_drag_admission_rejects_remove_readd_aba_and_peer_retirement() {
        let mut fixture = node_drag_fixture(2);
        arm_node_drag(&mut fixture, 2);
        let peer = fixture.nodes[1];
        let admitted = fixture.runtime.snapshot().unwrap();
        let expected = admitted.node_lifetime_key(peer).unwrap();
        fixture
            .runtime
            .execute(GraphCommand::topology(
                admitted.version(),
                TopologyEdit::new().remove_node(peer, IncidentEdgePolicy::Reject),
            ))
            .into_result()
            .unwrap()
            .applied()
            .unwrap();
        let retired = fixture.runtime.snapshot().unwrap();
        fixture
            .runtime
            .execute(GraphCommand::undo(retired.version()))
            .into_result()
            .unwrap()
            .applied()
            .unwrap();
        let rebuilt = fixture.runtime.snapshot().unwrap();
        assert_ne!(rebuilt.node_lifetime_key(peer), Some(expected));
        assert!(matches!(
            fixture.owner.required_node_drag_nodes(&rebuilt),
            Err(NodeDragRequiredNodesError::MemberRetired { member, .. })
                if member == expected
        ));

        fixture
            .owner
            .owner_retired(InteractionOwner::Node(expected));
        assert!(!fixture.owner.has_node_drag_residency());
        assert_eq!(
            fixture.owner.presentation().terminal(),
            Some(InteractionTerminalReason::OwnerRetired)
        );
    }

    #[test]
    fn native_node_drag_residency_is_atomic_at_64_and_65_members() {
        assert_eq!(MAX_NODE_MOVE_COHORT_SIZE, 64);
        let mut fixture = node_drag_fixture(MAX_NODE_MOVE_COHORT_SIZE + 1);
        arm_node_drag(&mut fixture, MAX_NODE_MOVE_COHORT_SIZE);
        assert_required_nodes(&fixture, MAX_NODE_MOVE_COHORT_SIZE);
        fixture.owner.cancel().expect("cancel 64-node drag");

        set_selection_by_id(&fixture.view, &fixture.nodes);
        assert!(
            node_drag_start_by_id(&fixture.view, fixture.nodes[0])
                .unwrap()
                .is_none()
        );
        assert!(!fixture.owner.has_node_drag_residency());
    }

    #[test]
    fn cancellation_and_close_release_node_drag_residency_immediately() {
        let mut fixture = node_drag_fixture(2);
        arm_node_drag(&mut fixture, 2);
        fixture.owner.cancel().expect("cancel native node drag");
        assert!(!fixture.owner.has_node_drag_residency());

        arm_node_drag(&mut fixture, 2);
        fixture.owner.prepare_terminal(false);
        assert!(!fixture.owner.has_node_drag_residency());
    }

    #[test]
    fn cancelled_submission_cannot_enter_the_native_command_transport() {
        let mut fixture = node_drag_fixture(1);
        arm_node_drag(&mut fixture, 1);
        let end = pointer_sample(&fixture.view, 40.0, 24.0);
        fixture.owner.move_to(end).unwrap();
        let (ticket, submission) = release_node_drag(&mut fixture, end);

        fixture
            .owner
            .cancel()
            .expect("cancel submission before native transport");

        assert!(matches!(
            fixture
                .owner
                .bind_submission(submission),
            Err(InteractionGraphSubmissionBindError::Superseded {
                ticket: rejected,
                current: None,
            }) if rejected == ticket
        ));
        assert_eq!(
            fixture
                .runtime
                .snapshot()
                .unwrap()
                .graph()
                .node(&fixture.nodes[0])
                .unwrap()
                .pos,
            CanvasPoint::default()
        );
    }

    #[test]
    fn native_owner_uses_the_runtime_final_release_sample_without_a_synthetic_move() {
        let mut fixture = node_drag_fixture(1);
        arm_node_drag(&mut fixture, 1);
        assert_eq!(
            fixture.owner.presentation().phase(),
            CanvasInteractionPhase::Armed
        );

        let end = pointer_sample(&fixture.view, 40.0, 24.0);
        let (_ticket, _submission) = release_node_drag(&mut fixture, end);

        assert_eq!(
            fixture.owner.presentation().phase(),
            CanvasInteractionPhase::Committing
        );
    }

    struct NodeDragFixture {
        runtime: GraphRuntime,
        view: ViewSession,
        owner: OpenGpuiNativeInteractionOwner,
        nodes: Vec<NodeId>,
    }

    fn node_drag_fixture(count: usize) -> NodeDragFixture {
        let nodes = (0..count)
            .map(|index| NodeId::from_u128(index as u128 + 1))
            .collect::<Vec<_>>();
        let mut builder = GraphBuilder::new(GraphId::from_u128(0x4452_4147));
        for (index, node) in nodes.iter().copied().enumerate() {
            builder.insert_node(node, test_node(index as f32 * 24.0));
        }
        let runtime = GraphRuntime::builder(Default::default())
            .build(builder.build().unwrap())
            .unwrap();
        let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
        let session =
            CanvasInteractionSession::attach(&view, CanvasInteractionConfig::default()).unwrap();
        NodeDragFixture {
            runtime,
            view,
            owner: OpenGpuiNativeInteractionOwner::new(session),
            nodes,
        }
    }

    fn arm_node_drag(fixture: &mut NodeDragFixture, selected: usize) {
        set_selection_by_id(&fixture.view, &fixture.nodes[..selected]);
        let start = node_drag_start_by_id(&fixture.view, fixture.nodes[0])
            .unwrap()
            .expect("in-budget node drag");
        fixture
            .owner
            .press(
                CanvasPressTarget::NodeDrag(start),
                pointer_sample(&fixture.view, 10.0, 10.0),
            )
            .unwrap();
    }

    fn release_node_drag(
        fixture: &mut NodeDragFixture,
        sample: ViewportPointerSample,
    ) -> (InteractionTicketId, InteractionGraphSubmission) {
        let update = fixture.owner.release(sample).unwrap();
        update
            .into_dispatch()
            .effects
            .into_iter()
            .find_map(|effect| match effect {
                CanvasInteractionEffect::SubmitGraph { submission } => {
                    Some((submission.ticket(), submission))
                }
                _ => None,
            })
            .expect("node drag submission")
    }

    fn assert_required_nodes(fixture: &NodeDragFixture, expected: usize) {
        let snapshot = fixture.runtime.snapshot().unwrap();
        assert_eq!(
            fixture
                .owner
                .required_node_drag_nodes(&snapshot)
                .unwrap()
                .len(),
            expected
        );
    }

    fn pointer_sample(view: &ViewSession, x: f32, y: f32) -> ViewportPointerSample {
        view.viewport_pointer_sample(ViewPoint::new(x, y))
            .expect("fixture pointer sample")
    }

    fn test_node(x: f32) -> Node {
        Node {
            kind: NodeKindKey::new("test.node"),
            kind_version: 1,
            pos: CanvasPoint { x, y: 0.0 },
            origin: None,
            selectable: None,
            draggable: None,
            connectable: None,
            deletable: None,
            parent: None,
            extent: None,
            expand_parent: None,
            size: Some(CanvasSize {
                width: 160.0,
                height: 96.0,
            }),
            hidden: false,
            collapsed: false,
            ports: Vec::new(),
            data: serde_json::Value::Null,
        }
    }
}
