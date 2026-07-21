use std::cell::Cell;
use std::collections::{BTreeMap, VecDeque};
use std::rc::{Rc, Weak};
use std::time::Instant;

use jellyflow_core::{CanvasPoint, NodeId};
use jellyflow_runtime::runtime::viewport::ViewportTransform;
use jellyflow_runtime::{
    AlignmentSnapQueryOutcome, CanvasInteractionConfig, CanvasInteractionEffect,
    CanvasInteractionError, CanvasInteractionPhase, CanvasInteractionPresentation,
    CanvasInteractionQuery, CanvasInteractionSession, CanvasInteractionUpdate, CanvasPressTarget,
    CommandSettlement, ConnectionInteractionStart, ConnectionProbeQueryError,
    ConnectionProbeQueryOutcome, ConnectionSearchRequest, ConnectionSearchSelection,
    EdgePickQueryError, EdgePickQueryOutcome, FreshGraphMutation, GraphRevision,
    InteractionGeneration, InteractionOwner, InteractionPosition, InteractionQueryId,
    InteractionTerminalReason, InteractionTickDemand, InteractionTicketId, NodeInstanceKey,
    PublishedEdgeRoute, SelectionMode, ViewPoint, ViewRect, ViewSize,
};
use open_gpui::{Context, FocusHandle, Pixels, Point, Task, Window};

use crate::activity::OpenGpuiActivityLease;
use crate::binding::OpenGpuiMountCapability;
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
    pending_queries: VecDeque<QueuedInteractionQuery>,
    pending_submission: Option<PendingInteractionSubmission>,
    ready_prefixes: VecDeque<ReadyInteractionPrefix>,
    connection: Option<NativeConnectionContext>,
    focus: Option<FocusHandle>,
    graph_focus: OpenGpuiGraphFocusCoordinator,
    tick_scheduled: Option<CallbackToken>,
    last_tick_at: Option<Instant>,
    epoch: Rc<Cell<u64>>,
    lifecycle: NativeInteractionOwnerLifecycle,
    #[cfg(test)]
    terminal_window_transitions: u64,
    #[cfg(test)]
    tick_callback_attempts: Rc<Cell<u64>>,
}

enum NativeInteractionOwnerLifecycle {
    Active,
    Prepared(PendingTerminalWindowCleanup),
    Settled,
}

struct PendingTerminalWindowCleanup {
    root_focus: Option<FocusHandle>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CallbackToken {
    epoch: u64,
    semantic_generation: Option<InteractionGeneration>,
}

struct QueuedInteractionQuery {
    token: CallbackToken,
    query: CanvasInteractionQuery,
}

#[cfg(test)]
pub(super) struct QueuedInteractionQueryProbe(QueuedInteractionQuery);

struct ReadyInteractionPrefix {
    token: CallbackToken,
    ticket: InteractionTicketId,
    result: OpenGpuiCommandPrefixBarrierResult,
}

struct NativeConnectionContext {
    owner: OpenGpuiPortDragOwner,
    mount: OpenGpuiMountCapability,
    native_generation: Option<NativeInteractionGeneration>,
    continuation_lease: Option<OpenGpuiActivityLease>,
}

impl NativeConnectionContext {
    fn for_pointer(
        owner: OpenGpuiPortDragOwner,
        mount: OpenGpuiMountCapability,
        native_generation: NativeInteractionGeneration,
    ) -> Self {
        Self {
            owner,
            mount,
            native_generation: Some(native_generation),
            continuation_lease: None,
        }
    }

    fn for_activation(owner: OpenGpuiPortDragOwner, mount: OpenGpuiMountCapability) -> Self {
        Self {
            owner,
            mount,
            native_generation: None,
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
            pending_queries: VecDeque::new(),
            pending_submission: None,
            ready_prefixes: VecDeque::new(),
            connection: None,
            focus: None,
            graph_focus: OpenGpuiGraphFocusCoordinator::default(),
            tick_scheduled: None,
            last_tick_at: None,
            epoch: Rc::new(Cell::new(1)),
            lifecycle: NativeInteractionOwnerLifecycle::Active,
            #[cfg(test)]
            terminal_window_transitions: 0,
            #[cfg(test)]
            tick_callback_attempts: Rc::new(Cell::new(0)),
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
        position: InteractionPosition,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.session.press(target, position)
    }

    pub(super) fn activate_node(
        &mut self,
        node: NodeId,
        selection_mode: SelectionMode,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.session.activate_node(node, selection_mode)
    }

    pub(super) fn activate_connection_search(
        &mut self,
        start: ConnectionInteractionStart,
        position: InteractionPosition,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.session.activate_connection_search(start, position)
    }

    pub(super) fn move_to(
        &mut self,
        position: InteractionPosition,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.session.move_to(position)
    }

    pub(super) fn release(
        &mut self,
        position: InteractionPosition,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.session.release(position)
    }

    pub(super) fn cancel(&mut self) -> CanvasInteractionUpdate {
        self.session.cancel()
    }

    pub(super) fn close_semantic(&mut self) -> CanvasInteractionUpdate {
        self.session.close()
    }

    pub(super) fn owner_retired(&mut self, owner: InteractionOwner) -> CanvasInteractionUpdate {
        self.session.owner_retired(owner)
    }

    pub(super) fn uses_owner(&self, owner: InteractionOwner) -> bool {
        self.session.uses_owner(owner)
    }

    pub(super) fn delete_edge(
        &mut self,
        route: &PublishedEdgeRoute,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.session.delete_edge(route)
    }

    pub(super) fn request_edge_pick(
        &mut self,
        position: InteractionPosition,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.session.request_edge_pick(position)
    }

    pub(super) fn clear_edge_hover(
        &mut self,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.session.clear_edge_hover()
    }

    pub(super) fn resolve_edge_pick(
        &mut self,
        id: InteractionQueryId,
        outcome: EdgePickQueryOutcome,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.session.resolve_edge_pick(id, outcome)
    }

    pub(super) fn resolve_connection_probe(
        &mut self,
        id: InteractionQueryId,
        outcome: ConnectionProbeQueryOutcome,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.session.resolve_connection_probe(id, outcome)
    }

    pub(super) fn resolve_alignment_snap(
        &mut self,
        id: InteractionQueryId,
        outcome: AlignmentSnapQueryOutcome,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.session.resolve_alignment_snap(id, outcome)
    }

    pub(super) fn restart_stale_query(
        &mut self,
        id: InteractionQueryId,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.session.restart_stale_query(id)
    }

    pub(super) fn search_request(&self) -> Option<ConnectionSearchRequest> {
        self.session.search_request()
    }

    pub(super) fn search_ready(
        &mut self,
        generation: InteractionGeneration,
        result_count: usize,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.session.search_ready(generation, result_count)
    }

    pub(super) fn search_failed(
        &mut self,
        generation: InteractionGeneration,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.session.search_failed(generation)
    }

    pub(super) fn retry_search(
        &mut self,
        generation: InteractionGeneration,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.session.retry_search(generation)
    }

    pub(super) fn select_search_result(
        &mut self,
        generation: InteractionGeneration,
        selection: ConnectionSearchSelection,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.session.select_search_result(generation, selection)
    }

    pub(super) fn settle(
        &mut self,
        ticket: InteractionTicketId,
        settlement: CommandSettlement,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.session.settle(ticket, settlement)
    }

    pub(super) fn request_connection_commit_revalidation(
        &mut self,
        ticket: InteractionTicketId,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.session.request_connection_commit_revalidation(ticket)
    }

    pub(super) fn refresh_create_from_wire_commit_revision(
        &mut self,
        ticket: InteractionTicketId,
        revision: GraphRevision,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.session
            .refresh_create_from_wire_commit_revision(ticket, revision)
    }

    pub(super) fn tick(
        &mut self,
        elapsed_seconds: f32,
        viewport: ViewSize,
        transform: ViewportTransform,
    ) -> Result<CanvasInteractionUpdate, CanvasInteractionError> {
        self.session.tick(elapsed_seconds, viewport, transform)
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

    pub(super) fn begin_pointer_connection(
        &mut self,
        owner: OpenGpuiPortDragOwner,
        mount: OpenGpuiMountCapability,
        native_generation: NativeInteractionGeneration,
    ) -> bool {
        if self.terminal() {
            return false;
        }
        self.connection = Some(NativeConnectionContext::for_pointer(
            owner,
            mount,
            native_generation,
        ));
        true
    }

    pub(super) fn begin_activation_connection(
        &mut self,
        owner: OpenGpuiPortDragOwner,
        mount: OpenGpuiMountCapability,
    ) -> bool {
        if self.terminal() {
            return false;
        }
        self.connection = Some(NativeConnectionContext::for_activation(owner, mount));
        true
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

    fn enqueue_queries(
        &mut self,
        queries: impl IntoIterator<Item = CanvasInteractionQuery>,
    ) -> bool {
        if self.terminal() {
            return false;
        }
        let epoch = self.epoch();
        let mut queued_any = false;
        for query in queries {
            let semantic_generation = match &query {
                CanvasInteractionQuery::AlignmentSnap { generation, .. }
                | CanvasInteractionQuery::ConnectionProbe { generation, .. } => Some(*generation),
                CanvasInteractionQuery::EdgePick { .. } => self.semantic_generation(),
            };
            let token = CallbackToken {
                epoch,
                semantic_generation,
            };
            self.pending_queries.retain(|queued| {
                !matches!(
                    (&queued.query, &query),
                    (
                        CanvasInteractionQuery::AlignmentSnap { .. },
                        CanvasInteractionQuery::AlignmentSnap { .. }
                    ) | (
                        CanvasInteractionQuery::ConnectionProbe { .. },
                        CanvasInteractionQuery::ConnectionProbe { .. }
                    ) | (
                        CanvasInteractionQuery::EdgePick { .. },
                        CanvasInteractionQuery::EdgePick { .. }
                    )
                )
            });
            self.pending_queries
                .push_back(QueuedInteractionQuery { token, query });
            queued_any = true;
        }
        queued_any
    }

    fn take_next_query(&mut self) -> Option<QueuedInteractionQuery> {
        self.pending_queries.pop_front()
    }

    fn has_pending_queries(&self) -> bool {
        !self.pending_queries.is_empty()
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

    fn attach_prefix_task(&mut self, task: Task<()>) -> bool {
        let Some(pending) = self.pending_submission.as_mut() else {
            return false;
        };
        pending.prefix_task = Some(task);
        true
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

    fn pending_submission_ticket(&self) -> Option<InteractionTicketId> {
        self.pending_submission
            .as_ref()
            .map(|pending| pending.ticket)
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
            .is_some_and(|pending| pending.prefix_task.is_some())
    }

    fn discard_orphaned_revalidation(&mut self) -> bool {
        let Some(pending) = self.pending_submission.as_ref() else {
            return false;
        };
        if !matches!(
            pending.stage,
            PendingInteractionSubmissionStage::Revalidating { .. }
        ) {
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

    fn prepare_tick_callback(&mut self) -> Option<(Weak<Cell<u64>>, CallbackToken)> {
        let token = self.callback_token();
        if self.tick_scheduled == Some(token) {
            return None;
        }
        self.tick_scheduled = Some(token);
        self.last_tick_at.get_or_insert_with(Instant::now);
        Some((Rc::downgrade(&self.epoch), token))
    }

    fn begin_scheduled_tick(&mut self, token: CallbackToken, now: Instant) -> Option<f32> {
        if self.tick_scheduled != Some(token) {
            return None;
        }
        self.tick_scheduled = None;
        if !self.callback_is_current(token) {
            return None;
        }
        Some(interaction_elapsed_seconds(&mut self.last_tick_at, now))
    }

    fn stop_ticking(&mut self) {
        self.last_tick_at = None;
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
        self.pending_queries.clear();
        self.pending_submission = None;
        self.ready_prefixes.clear();
        self.tick_scheduled = None;
        self.last_tick_at = None;
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
        self.pending_queries.clear();
        self.pending_submission = None;
        self.ready_prefixes.clear();
        self.tick_scheduled = None;
        self.last_tick_at = None;
        self.connection = None;
    }

    #[cfg(test)]
    pub(super) fn test_pending_query_count(&self) -> usize {
        self.pending_queries.len()
    }

    #[cfg(test)]
    pub(super) fn test_pending_queries_empty(&self) -> bool {
        self.pending_queries.is_empty()
    }

    #[cfg(test)]
    pub(super) fn test_front_query(&self) -> Option<&CanvasInteractionQuery> {
        self.pending_queries.front().map(|queued| &queued.query)
    }

    #[cfg(test)]
    pub(super) fn test_take_query_probe(&mut self) -> Option<QueuedInteractionQueryProbe> {
        self.pending_queries
            .pop_front()
            .map(QueuedInteractionQueryProbe)
    }

    #[cfg(test)]
    pub(super) fn test_restore_query_front(&mut self, probe: QueuedInteractionQueryProbe) {
        self.pending_queries.push_front(probe.0);
    }

    #[cfg(test)]
    pub(super) fn test_restore_query_back(&mut self, probe: QueuedInteractionQueryProbe) {
        self.pending_queries.push_back(probe.0);
    }

    #[cfg(test)]
    pub(super) fn test_take_query(&mut self) -> Option<CanvasInteractionQuery> {
        self.pending_queries.pop_front().map(|queued| queued.query)
    }

    #[cfg(test)]
    pub(super) fn test_clear_queries(&mut self) {
        self.pending_queries.clear();
    }

    #[cfg(test)]
    pub(super) fn test_has_pending_submission(&self) -> bool {
        self.pending_submission.is_some()
    }

    #[cfg(test)]
    pub(super) fn test_pending_work_empty(&self) -> bool {
        self.pending_queries.is_empty()
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
        self.tick_callback_attempts.get()
    }
}

struct PendingInteractionSubmission {
    ticket: InteractionTicketId,
    token: CallbackToken,
    stage: PendingInteractionSubmissionStage,
    prefix_task: Option<Task<()>>,
    prefix_cancellation: Option<OpenGpuiCommandPrefixBarrierCancellation>,
}

enum PendingInteractionSubmissionStage {
    WaitingForPrefix(PendingInteractionMutation),
    Revalidating { permit: OpenGpuiCommandPrefixPermit },
    Enqueued,
}

enum PendingInteractionMutation {
    Direct(FreshGraphMutation),
    Connection(PendingConnectionMutation),
}

#[derive(Clone, Copy)]
enum PendingConnectionMutation {
    Ordinary,
    CreateFromWire,
}

impl OpenGpuiSceneHost {
    pub(super) fn interaction_position(
        &self,
        position: Point<Pixels>,
    ) -> Option<InteractionPosition> {
        let screen = self.scene_screen_position(position)?;
        let transform = self
            .scene_transform
            .or_else(|| self.view.viewport_transform().ok())?;
        InteractionPosition::from_view(ViewPoint::new(screen.x, screen.y), transform)
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
        let effects = dispatch.pre_query_effects;
        let queries = dispatch.queries;
        let damage = dispatch.damage;
        let tick_demand = dispatch.tick_demand;
        let has_queries = !queries.is_empty();
        let has_effects = !effects.is_empty();
        let has_damage = damage != Default::default();
        let mut effects_succeeded = true;
        for effect in effects {
            if let Err(error) = self.apply_canvas_interaction_effect(effect, cx) {
                effects_succeeded = false;
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
                    self.queue_interaction_queries(cancelled.into_dispatch().queries);
                }
                break;
            }
        }
        if effects_succeeded {
            self.queue_interaction_queries(queries);
        }
        self.discard_orphaned_interaction_revalidation();
        self.sync_native_connection_state(window, cx);
        let should_continue_ticking = tick_demand == InteractionTickDemand::WhileActive
            && (has_queries || has_effects || has_damage);
        if should_continue_ticking {
            self.schedule_interaction_tick(window, cx);
        } else {
            self.interaction_owner.stop_ticking();
        }
        if has_damage || self.interaction_owner.has_pending_queries() {
            cx.notify();
        }
    }

    fn apply_canvas_interaction_effect(
        &mut self,
        effect: CanvasInteractionEffect,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        match effect {
            CanvasInteractionEffect::Selection(selection) => self
                .view
                .apply_selection_mutation(selection)
                .map_err(|error| error.to_string()),
            CanvasInteractionEffect::Viewport(viewport) => self
                .view
                .apply_viewport_mutation(viewport)
                .map_err(|error| error.to_string()),
            CanvasInteractionEffect::Search(_) => Ok(()),
            CanvasInteractionEffect::SubmitGraph { ticket, mutation } => {
                self.submit_interaction_mutation(ticket, mutation, cx)
            }
        }
    }

    pub(super) fn queue_interaction_queries(
        &mut self,
        queries: impl IntoIterator<Item = CanvasInteractionQuery>,
    ) {
        self.interaction_owner.enqueue_queries(queries);
    }

    pub(super) fn process_one_interaction_query(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(queued) = self.interaction_owner.take_next_query() else {
            return;
        };
        if !self.interaction_owner.callback_is_current(queued.token) {
            if self.interaction_owner.has_pending_queries() {
                cx.notify();
            }
            return;
        }
        let query = queued.query;
        let update = match query {
            CanvasInteractionQuery::AlignmentSnap { id, query, .. } => self
                .view
                .alignment_snap(query)
                .map_err(|error| error.to_string())
                .and_then(|outcome| {
                    self.interaction_owner
                        .resolve_alignment_snap(id, outcome)
                        .map_err(|error| error.to_string())
                }),
            CanvasInteractionQuery::ConnectionProbe { id, query, .. } => {
                match self.view.connection_probe(query) {
                    Ok(outcome) => self
                        .interaction_owner
                        .resolve_connection_probe(id, outcome)
                        .map_err(|error| error.to_string()),
                    Err(ConnectionProbeQueryError::StaleCursor(reason))
                        if reason.is_restartable() =>
                    {
                        self.interaction_owner
                            .restart_stale_query(id)
                            .map_err(|error| error.to_string())
                    }
                    Err(error) => Err(error.to_string()),
                }
            }
            CanvasInteractionQuery::EdgePick { id, query } => match self.view.edge_pick(query) {
                Ok(outcome) => self
                    .interaction_owner
                    .resolve_edge_pick(id, outcome)
                    .map_err(|error| error.to_string()),
                Err(EdgePickQueryError::StaleCursor(reason)) if reason.is_restartable() => self
                    .interaction_owner
                    .restart_stale_query(id)
                    .map_err(|error| error.to_string()),
                Err(error) => Err(error.to_string()),
            },
        };
        match update {
            Ok(update) => self.apply_canvas_interaction_update(update, window, cx),
            Err(error) => {
                self.record_command_diagnostic(format!(
                    "native canvas interaction query failed: {error}"
                ));
                if self.interaction_owner.presentation().phase()
                    == CanvasInteractionPhase::Committing
                    && let Some(ticket) = self.interaction_owner.pending_submission_ticket()
                {
                    self.discard_pending_interaction_submission();
                    self.settle_interaction_without_native(ticket, CommandSettlement::Conflict);
                    self.sync_native_connection_state(window, cx);
                } else {
                    self.interaction_owner
                        .cancel_native_for_query_failure(window, cx);
                    let update = self.interaction_owner.cancel();
                    self.discard_pending_interaction_submission();
                    self.apply_canvas_interaction_update(update, window, cx);
                }
            }
        }
    }

    fn submit_interaction_mutation(
        &mut self,
        ticket: InteractionTicketId,
        mutation: FreshGraphMutation,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.interaction_owner.has_pending_submission() {
            return self.enqueue_revalidated_interaction_mutation(ticket, mutation, cx);
        }

        let token = self.interaction_owner.callback_token();
        let pending = match mutation {
            mutation @ (FreshGraphMutation::MoveNodes(_) | FreshGraphMutation::DeleteEdge(_)) => {
                PendingInteractionSubmission {
                    ticket,
                    token,
                    stage: PendingInteractionSubmissionStage::WaitingForPrefix(
                        PendingInteractionMutation::Direct(mutation),
                    ),
                    prefix_task: None,
                    prefix_cancellation: None,
                }
            }
            FreshGraphMutation::Connection { topology, .. } => PendingInteractionSubmission {
                ticket,
                token,
                stage: PendingInteractionSubmissionStage::WaitingForPrefix(
                    PendingInteractionMutation::Connection(if topology.is_empty() {
                        PendingConnectionMutation::Ordinary
                    } else {
                        PendingConnectionMutation::CreateFromWire
                    }),
                ),
                prefix_task: None,
                prefix_cancellation: None,
            },
        };
        let barrier = self.commands.capture_prefix_barrier().map_err(|error| {
            self.settle_interaction_without_native(ticket, CommandSettlement::TransportFailure);
            format!("failed to capture native command prefix: {error}")
        })?;
        let prefix_cancellation = barrier.cancellation();
        let mut pending = pending;
        pending.prefix_cancellation = Some(prefix_cancellation);
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
        assert!(
            self.interaction_owner.attach_prefix_task(prefix_task),
            "the prefix task is installed before control returns",
        );
        Ok(())
    }

    fn enqueue_revalidated_interaction_mutation(
        &mut self,
        ticket: InteractionTicketId,
        mutation: FreshGraphMutation,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(mut pending) = self.interaction_owner.take_pending_submission() else {
            return Err("interaction command prefix state disappeared".to_owned());
        };
        if !self.interaction_owner.callback_is_current(pending.token) {
            return Err("interaction command belongs to a retired generation".to_owned());
        }
        if pending.ticket != ticket {
            self.interaction_owner.restore_pending_submission(pending);
            return Err("interaction command ticket does not match its prefix permit".to_owned());
        }
        let permit = match std::mem::replace(
            &mut pending.stage,
            PendingInteractionSubmissionStage::Enqueued,
        ) {
            PendingInteractionSubmissionStage::Revalidating { permit } => permit,
            stage => {
                pending.stage = stage;
                self.interaction_owner.restore_pending_submission(pending);
                return Err("interaction mutation arrived before its prefix settled".to_owned());
            }
        };
        match self.bind_and_enqueue_interaction(ticket, mutation, permit) {
            Ok(true) => {
                pending.stage = PendingInteractionSubmissionStage::Enqueued;
                self.interaction_owner.restore_pending_submission(pending);
                cx.notify();
                Ok(())
            }
            Ok(false) => {
                self.settle_interaction_without_native(ticket, CommandSettlement::Noop);
                Ok(())
            }
            Err(error) => {
                self.settle_interaction_without_native(ticket, error.settlement());
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
        let Some(mut pending) = self.interaction_owner.take_pending_submission() else {
            drop(result);
            return;
        };
        if pending.ticket != ticket || pending.token != token {
            self.interaction_owner.restore_pending_submission(pending);
            drop(result);
            return;
        }
        pending.prefix_task = None;
        pending.prefix_cancellation = None;
        let mutation = match std::mem::replace(
            &mut pending.stage,
            PendingInteractionSubmissionStage::Enqueued,
        ) {
            PendingInteractionSubmissionStage::WaitingForPrefix(mutation) => mutation,
            stage => {
                pending.stage = stage;
                self.interaction_owner.restore_pending_submission(pending);
                drop(result);
                return;
            }
        };
        let permit = match result {
            Ok(permit) => permit,
            Err(error) => {
                self.settle_interaction_without_native(ticket, CommandSettlement::TransportFailure);
                self.record_prefix_barrier_failure(error);
                self.sync_native_connection_state(window, cx);
                return;
            }
        };

        match mutation {
            PendingInteractionMutation::Direct(mutation) => {
                match self.bind_and_enqueue_interaction(ticket, mutation, permit) {
                    Ok(true) => {
                        pending.stage = PendingInteractionSubmissionStage::Enqueued;
                        self.interaction_owner.restore_pending_submission(pending);
                        cx.notify();
                    }
                    Ok(false) => {
                        self.settle_interaction_without_native(ticket, CommandSettlement::Noop);
                    }
                    Err(error) => {
                        self.settle_interaction_without_native(ticket, error.settlement());
                        self.record_command_diagnostic(error.message);
                    }
                }
            }
            PendingInteractionMutation::Connection(kind) => {
                pending.stage = PendingInteractionSubmissionStage::Revalidating { permit };
                self.interaction_owner.restore_pending_submission(pending);
                let update = match kind {
                    PendingConnectionMutation::Ordinary => self
                        .interaction_owner
                        .request_connection_commit_revalidation(ticket),
                    PendingConnectionMutation::CreateFromWire => self
                        .view
                        .graph_snapshot()
                        .map_err(|_| CanvasInteractionError::ConnectionCommitRequiresFreshRevision)
                        .and_then(|snapshot| {
                            self.interaction_owner
                                .refresh_create_from_wire_commit_revision(
                                    ticket,
                                    snapshot.revision(),
                                )
                        }),
                };
                match update {
                    Ok(update) => self.apply_canvas_interaction_update(update, window, cx),
                    Err(error) => {
                        self.discard_pending_interaction_submission();
                        self.settle_interaction_without_native(
                            ticket,
                            CommandSettlement::TransportFailure,
                        );
                        self.record_command_diagnostic(format!(
                            "failed to refresh native interaction command: {error}"
                        ));
                        self.sync_native_connection_state(window, cx);
                    }
                }
            }
        }
    }

    fn bind_and_enqueue_interaction(
        &self,
        ticket: InteractionTicketId,
        mutation: FreshGraphMutation,
        permit: OpenGpuiCommandPrefixPermit,
    ) -> Result<bool, InteractionEnqueueFailure> {
        let snapshot = self
            .view
            .graph_snapshot()
            .map_err(|error| InteractionEnqueueFailure::transport(error.to_string()))?;
        let command = mutation
            .into_command(&snapshot)
            .map_err(|error| InteractionEnqueueFailure::conflict(error.to_string()))?;
        let Some(command) = command else {
            permit.abandon();
            return Ok(false);
        };
        permit
            .enqueue_detached(command, OpenGpuiCommandOrigin::Interaction { ticket })
            .map_err(|error| InteractionEnqueueFailure::transport(error.to_string()))?;
        Ok(true)
    }

    pub(super) fn settle_interaction_command(
        &mut self,
        ticket: InteractionTicketId,
        settlement: CommandSettlement,
    ) {
        self.interaction_owner.clear_submission_for_ticket(ticket);
        self.settle_interaction_without_native(ticket, settlement);
    }

    fn settle_interaction_without_native(
        &mut self,
        ticket: InteractionTicketId,
        settlement: CommandSettlement,
    ) {
        match self.interaction_owner.settle(ticket, settlement) {
            Ok(update) => self.queue_interaction_queries(update.into_dispatch().queries),
            Err(error) => self.record_command_diagnostic(format!(
                "failed to settle native canvas interaction: {error}"
            )),
        }
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
        let update = self.interaction_owner.owner_retired(owner);
        if self.interaction_owner.presentation().terminal()
            == Some(InteractionTerminalReason::OwnerRetired)
        {
            self.discard_pending_interaction_submission();
        }
        self.queue_interaction_queries(update.into_dispatch().queries);
    }

    fn record_prefix_barrier_failure(&mut self, error: OpenGpuiCommandPrefixBarrierError) {
        self.record_command_diagnostic(format!("native command prefix failed: {error}"));
    }

    fn schedule_interaction_tick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((epoch_gate, token)) = self.interaction_owner.prepare_tick_callback() else {
            return;
        };
        let host = cx.weak_entity();
        #[cfg(test)]
        let tick_callback_attempts = Rc::clone(&self.interaction_owner.tick_callback_attempts);
        window.on_next_frame(move |window, cx| {
            #[cfg(test)]
            tick_callback_attempts.set(tick_callback_attempts.get().saturating_add(1));
            if !callback_epoch_is_current(&epoch_gate, token) {
                return;
            }
            let Some(host) = host.upgrade() else {
                return;
            };
            host.update(cx, |host, cx| {
                let Some(elapsed_seconds) = host
                    .interaction_owner
                    .begin_scheduled_tick(token, Instant::now())
                else {
                    return;
                };
                let Some(viewport) = host.surface_size else {
                    host.interaction_owner.stop_ticking();
                    return;
                };
                let Ok(transform) = host.view.viewport_transform() else {
                    host.interaction_owner.stop_ticking();
                    return;
                };
                match host
                    .interaction_owner
                    .tick(elapsed_seconds, viewport, transform)
                {
                    Ok(update) => host.apply_canvas_interaction_update(update, window, cx),
                    Err(error) => {
                        host.interaction_owner.stop_ticking();
                        host.record_command_diagnostic(format!(
                            "native canvas interaction tick failed: {error}"
                        ));
                    }
                }
            });
        });
    }

    pub(super) fn interaction_node_drag_offsets(&self) -> BTreeMap<NodeId, CanvasPoint> {
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
    settlement: CommandSettlement,
    message: String,
}

impl InteractionEnqueueFailure {
    fn conflict(message: String) -> Self {
        Self {
            settlement: CommandSettlement::Conflict,
            message,
        }
    }

    fn transport(message: String) -> Self {
        Self {
            settlement: CommandSettlement::TransportFailure,
            message,
        }
    }

    fn settlement(&self) -> CommandSettlement {
        self.settlement
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

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
}
