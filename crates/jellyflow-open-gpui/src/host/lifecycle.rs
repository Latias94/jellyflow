use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::future::poll_fn;
use std::rc::Rc;
use std::task::{Poll, Waker};

use jellyflow::core::NodeId;
use jellyflow::runtime::{
    GraphRevision, NodeIncarnation, NodeInstanceKey, ViewCloseOutcome, ViewCloseReason,
    ViewCloseRequest, ViewSession, ViewSessionError, ViewSessionEvent, ViewSubscriptionToken,
};
use open_gpui::{Context, Task, Window};

use crate::command_journal::{
    OpenGpuiCommandCloseError, OpenGpuiCompletedCommand, resolve_command_tickets,
};
use crate::interaction::{OpenGpuiPortDragCancelReason, OpenGpuiSceneGestureCancelReason};

use super::OpenGpuiSceneHost;
use super::diagnostics::{OpenGpuiSceneHostDiagnostic, OpenGpuiSceneHostDiagnosticKind};
use super::node_lifecycle::{
    MountedNode, catch_host_cleanup, resume_mounted_node_panic, revoke_mounted_nodes,
    suppress_host_panic,
};

#[derive(Debug)]
pub(super) enum HostLifecycleEvent {
    NodesChanged(NodeChanges),
    ViewChanged,
    Closed(ViewCloseReason),
}

#[derive(Debug)]
pub(super) struct NodeChanges(BTreeMap<NodeId, PendingNodeChange>);

impl NodeChanges {
    pub(super) fn get(&self, node: &NodeId) -> Option<&PendingNodeChange> {
        self.0.get(node)
    }
}

impl<'a> IntoIterator for &'a NodeChanges {
    type Item = (&'a NodeId, &'a PendingNodeChange);
    type IntoIter = std::collections::btree_map::Iter<'a, NodeId, PendingNodeChange>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct PendingNodeChange {
    revision: GraphRevision,
    current_incarnation: CurrentNodeIncarnation,
}

impl PendingNodeChange {
    pub(super) fn revision(self) -> GraphRevision {
        self.revision
    }

    pub(super) fn is_stale_for(self, instance: &NodeInstanceKey) -> bool {
        self.current_incarnation.is_stale_for(instance)
    }

    fn merge(&mut self, next: Self) {
        if self.revision > next.revision {
            return;
        }
        self.revision = next.revision;
        if next.current_incarnation.is_definitive() {
            self.current_incarnation = next.current_incarnation;
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum CurrentNodeIncarnation {
    Unavailable,
    Retired,
    Live(NodeIncarnation),
}

impl CurrentNodeIncarnation {
    fn is_definitive(self) -> bool {
        !matches!(self, Self::Unavailable)
    }

    fn is_stale_for(self, instance: &NodeInstanceKey) -> bool {
        match self {
            Self::Unavailable => false,
            Self::Retired => true,
            Self::Live(current) => current != instance.incarnation(),
        }
    }
}

#[derive(Clone)]
pub(super) struct HostLifecycleSignal {
    state: Rc<HostLifecycleSignalState>,
}

struct HostLifecycleSignalState {
    node_changes: RefCell<BTreeMap<NodeId, PendingNodeChange>>,
    patch_pending: Cell<bool>,
    view_pending: Cell<bool>,
    terminal_reason: Cell<Option<ViewCloseReason>>,
    waker: RefCell<Option<Waker>>,
}

impl HostLifecycleSignal {
    pub(super) fn new() -> Self {
        Self {
            state: Rc::new(HostLifecycleSignalState {
                node_changes: RefCell::new(BTreeMap::new()),
                patch_pending: Cell::new(false),
                view_pending: Cell::new(false),
                terminal_reason: Cell::new(None),
                waker: RefCell::new(None),
            }),
        }
    }

    pub(super) fn publish(&self, event: ViewSessionEvent) {
        match event {
            ViewSessionEvent::RuntimePatched { patch } => {
                let nodes = patch.affected_nodes().iter().copied().map(|node| {
                    let current_incarnation = match patch.node_incarnation_change(node) {
                        Some(Some(incarnation)) => CurrentNodeIncarnation::Live(incarnation),
                        Some(None) => CurrentNodeIncarnation::Retired,
                        None => CurrentNodeIncarnation::Unavailable,
                    };
                    (node, current_incarnation)
                });
                self.publish_patch(patch.to_revision(), nodes);
            }
            ViewSessionEvent::ViewportChanged { .. }
            | ViewSessionEvent::SelectionChanged { .. } => self.publish_view_changed(),
            ViewSessionEvent::LayoutPublished { .. } => {}
            ViewSessionEvent::Closed { reason } => self.publish_terminal(reason),
        }
    }

    pub(super) async fn wait(&self) -> HostLifecycleEvent {
        poll_fn(|context| {
            if let Some(reason) = self.state.terminal_reason.get() {
                return Poll::Ready(HostLifecycleEvent::Closed(reason));
            }
            if let Some(changes) = self.take_node_changes() {
                return Poll::Ready(HostLifecycleEvent::NodesChanged(changes));
            }
            if self.state.view_pending.replace(false) {
                return Poll::Ready(HostLifecycleEvent::ViewChanged);
            }
            *self.state.waker.borrow_mut() = Some(context.waker().clone());
            Poll::Pending
        })
        .await
    }

    pub(super) fn take_node_changes(&self) -> Option<NodeChanges> {
        self.state
            .patch_pending
            .replace(false)
            .then(|| NodeChanges(std::mem::take(&mut *self.state.node_changes.borrow_mut())))
    }

    fn publish_patch(
        &self,
        revision: GraphRevision,
        nodes: impl IntoIterator<Item = (NodeId, CurrentNodeIncarnation)>,
    ) {
        if self.state.terminal_reason.get().is_some() {
            return;
        }
        let mut changes = self.state.node_changes.borrow_mut();
        for (node, current_incarnation) in nodes {
            let next = PendingNodeChange {
                revision,
                current_incarnation,
            };
            changes
                .entry(node)
                .and_modify(|current| current.merge(next))
                .or_insert(next);
        }
        drop(changes);
        self.state.patch_pending.set(true);
        self.wake();
    }

    fn publish_terminal(&self, reason: ViewCloseReason) {
        if self.state.terminal_reason.replace(Some(reason)).is_some() {
            return;
        }
        self.state.node_changes.borrow_mut().clear();
        self.state.patch_pending.set(false);
        self.state.view_pending.set(false);
        self.wake();
    }

    fn publish_view_changed(&self) {
        if self.state.terminal_reason.get().is_some() {
            return;
        }
        self.state.view_pending.set(true);
        self.wake();
    }

    fn wake(&self) {
        if let Some(waker) = self.state.waker.borrow_mut().take() {
            waker.wake();
        }
    }
}

pub(super) struct HostLifecycleBridge {
    subscription: ViewSubscriptionToken,
    _task: Task<()>,
}

impl HostLifecycleBridge {
    pub(super) fn new(subscription: ViewSubscriptionToken, task: Task<()>) -> Self {
        Self {
            subscription,
            _task: task,
        }
    }

    pub(super) fn unsubscribe(self, view: &ViewSession) {
        let _ = view.unsubscribe(self.subscription);
    }
}

fn command_close_error(error: OpenGpuiCommandCloseError) -> OpenGpuiSceneHostCloseError {
    match error {
        OpenGpuiCommandCloseError::BatchSubmitting => {
            OpenGpuiSceneHostCloseError::CommandBatchSubmitting
        }
        error => OpenGpuiSceneHostCloseError::CommandJournal(error.to_string()),
    }
}

/// Failure to close a native host without access to its owning window.
#[derive(Debug, thiserror::Error)]
pub enum OpenGpuiSceneHostCloseError {
    #[error(
        "native interactions or pending commands require close_in_window for synchronous cleanup"
    )]
    WindowCleanupRequired,
    #[error("native command batch is currently submitting")]
    CommandBatchSubmitting,
    #[error("native command journal could not close: {0}")]
    CommandJournal(String),
    #[error("native host close encountered a panicking callback")]
    PanicDuringClose,
    #[error(transparent)]
    Session(#[from] ViewSessionError),
}

impl OpenGpuiSceneHost {
    fn owns_window_resources(&self) -> bool {
        self.interactions.requires_window_cleanup()
            || self.scene_interactions.requires_window_cleanup()
            || self.connection_owner.is_some()
            || self.activity_coordinator.requires_window_cleanup()
    }

    /// Closes an idle view capability and releases all retained native state.
    ///
    /// Active or queued window-owned interaction must be closed through [`Self::close_in_window`].
    pub fn close(&mut self) -> Result<ViewCloseOutcome, OpenGpuiSceneHostCloseError> {
        self.close_with_request(ViewCloseRequest::default())
    }

    /// Drains an optional final command when no native interaction requires window cleanup.
    pub fn close_with_request(
        &mut self,
        request: ViewCloseRequest,
    ) -> Result<ViewCloseOutcome, OpenGpuiSceneHostCloseError> {
        if self.owns_window_resources() || self.commands.has_pending() {
            return Err(OpenGpuiSceneHostCloseError::WindowCleanupRequired);
        }
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.view.close_with_request(request)
        }));
        let outcome = match outcome {
            Ok(outcome) => outcome,
            Err(payload) => {
                if let Some(reason) = self.view.close_reason() {
                    self.interactions
                        .cancel_deferred(OpenGpuiPortDragCancelReason::HostClosed);
                    self.scene_interactions
                        .cancel_deferred(OpenGpuiSceneGestureCancelReason::HostClosed);
                    let _ = catch_host_cleanup(|| self.enter_terminal(reason));
                }
                suppress_host_panic(Some(payload));
                return Err(OpenGpuiSceneHostCloseError::PanicDuringClose);
            }
        };
        let mut cleanup_panicked = false;
        match &outcome {
            Ok(outcome) => {
                self.interactions
                    .cancel_deferred(OpenGpuiPortDragCancelReason::HostClosed);
                self.scene_interactions
                    .cancel_deferred(OpenGpuiSceneGestureCancelReason::HostClosed);
                cleanup_panicked |= catch_host_cleanup(|| self.enter_terminal(outcome.reason()));
            }
            Err(ViewSessionError::Closed { reason }) => {
                self.interactions
                    .cancel_deferred(OpenGpuiPortDragCancelReason::HostClosed);
                self.scene_interactions
                    .cancel_deferred(OpenGpuiSceneGestureCancelReason::HostClosed);
                cleanup_panicked |= catch_host_cleanup(|| self.enter_terminal(*reason));
            }
            Err(_) => {}
        }
        if cleanup_panicked {
            return Err(OpenGpuiSceneHostCloseError::PanicDuringClose);
        }
        outcome.map_err(Into::into)
    }

    /// Closes the view and synchronously releases platform pointer capture before returning.
    pub fn close_in_window(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<ViewCloseOutcome, OpenGpuiSceneHostCloseError> {
        self.close_with_request_in_window(ViewCloseRequest::default(), window, cx)
    }

    /// Drains a final command and synchronously cancels native interaction ownership on close.
    pub fn close_with_request_in_window(
        &mut self,
        request: ViewCloseRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<ViewCloseOutcome, OpenGpuiSceneHostCloseError> {
        let drain = self.commands.begin_close().map_err(command_close_error)?;
        let journal_count = drain.len();
        let request = drain.combined_request(&request);
        let close = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.view.close_with_request(request)
        }));
        match close {
            Err(payload) => {
                let result = if let Some(reason) = self.view.close_reason() {
                    self.commands
                        .finish_close_after_panic(drain, reason)
                        .map(|completed| {
                            self.finish_window_close_cleanup(completed, reason, window, cx);
                        })
                } else {
                    self.commands.abort_close(drain)
                };
                suppress_host_panic(Some(payload));
                result.map_err(command_close_error)?;
                Err(OpenGpuiSceneHostCloseError::PanicDuringClose)
            }
            Ok(Ok(outcome)) => {
                let reason = outcome.reason();
                let (journal_receipts, caller_outcome) =
                    outcome.split_receipt_prefix(journal_count);
                let completed = self
                    .commands
                    .finish_close(drain, reason, journal_receipts)
                    .map_err(command_close_error)?;
                let cleanup_panicked =
                    self.finish_window_close_cleanup(completed, reason, window, cx);
                if cleanup_panicked {
                    Err(OpenGpuiSceneHostCloseError::PanicDuringClose)
                } else {
                    Ok(caller_outcome)
                }
            }
            Ok(Err(ViewSessionError::Closed { reason })) => {
                let completed = self
                    .commands
                    .finish_close_without_execution(drain, reason)
                    .map_err(command_close_error)?;
                let cleanup_panicked =
                    self.finish_window_close_cleanup(completed, reason, window, cx);
                if cleanup_panicked {
                    Err(OpenGpuiSceneHostCloseError::PanicDuringClose)
                } else {
                    Err(ViewSessionError::Closed { reason }.into())
                }
            }
            Ok(Err(error)) => {
                self.commands
                    .abort_close(drain)
                    .map_err(command_close_error)?;
                Err(error.into())
            }
        }
    }

    fn finish_window_close_cleanup(
        &mut self,
        completed: Vec<OpenGpuiCompletedCommand>,
        reason: ViewCloseReason,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let mut panicked = false;
        panicked |= catch_host_cleanup(|| {
            self.interactions
                .cancel_now(OpenGpuiPortDragCancelReason::HostClosed, window, cx);
            self.interactions.settle_port_focus_for_host_close(window);
        });
        panicked |= catch_host_cleanup(|| {
            self.canvas_interaction.close();
            self.discard_pending_interaction_submission();
        });
        panicked |= catch_host_cleanup(|| {
            self.scene_interactions.cancel_for_host_close(window, cx);
        });
        window.release_pointer();
        panicked |= catch_host_cleanup(|| self.enter_terminal(reason));
        self.terminal_window_cleanup_pending = false;
        let resolutions = self.handle_completed_commands(completed);
        window.defer(cx, move |_, _| resolve_command_tickets(resolutions));
        panicked
    }

    pub(super) fn finish_terminal_render_cleanup(
        &mut self,
        reason: ViewCloseReason,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let needs_window_cleanup = self.terminal_window_cleanup_pending
            || (self.terminal_reason.is_none() && self.owns_window_resources());
        if needs_window_cleanup {
            self.interactions.settle_port_focus_for_host_close(window);
            window.release_pointer();
        }
        let completed = self.commands.terminate(reason);
        self.enter_terminal(reason);
        self.terminal_window_cleanup_pending = false;
        let resolutions = self.handle_completed_commands(completed);
        cx.defer(move |_| resolve_command_tickets(resolutions));
        self.interactions.schedule_pending_drain(window, cx);
        self.scene_interactions.schedule_pending_drain(window, cx);
    }

    #[must_use]
    pub fn close_reason(&self) -> Option<ViewCloseReason> {
        self.terminal_reason.or_else(|| self.view.close_reason())
    }

    pub(super) fn ensure_lifecycle_bridge(&mut self, cx: &mut Context<Self>) {
        if self.lifecycle_bridge.is_some() || self.terminal_reason.is_some() {
            return;
        }

        let signal = HostLifecycleSignal::new();
        let callback_signal = signal.clone();
        let subscription = match self
            .view
            .subscribe(move |event| callback_signal.publish(event))
        {
            Ok(subscription) => subscription,
            Err(ViewSessionError::Closed { reason }) => {
                let completed = self.commands.terminate(reason);
                self.enter_terminal(reason);
                let resolutions = self.handle_completed_commands(completed);
                cx.defer(move |_| resolve_command_tickets(resolutions));
                return;
            }
            Err(error) => {
                self.last_diagnostic = Some(OpenGpuiSceneHostDiagnostic {
                    kind: OpenGpuiSceneHostDiagnosticKind::FrameMaterialization,
                    instance: None,
                    node: None,
                    renderer_key: None,
                    message: format!("failed to subscribe to view lifecycle: {error}"),
                });
                return;
            }
        };
        let task = cx.spawn(async move |host, cx| {
            loop {
                let event = signal.wait().await;
                let terminal = matches!(&event, HostLifecycleEvent::Closed(_));
                let result = host.update(cx, |host, cx| match event {
                    HostLifecycleEvent::NodesChanged(changes) => {
                        let retired = host.apply_node_changes(&changes, cx);
                        cx.notify();
                        (retired, Vec::new())
                    }
                    HostLifecycleEvent::ViewChanged => {
                        cx.notify();
                        (Vec::new(), Vec::new())
                    }
                    HostLifecycleEvent::Closed(reason) => {
                        let completed = host.commands.terminate(reason);
                        let retired = host.detach_for_terminal(reason);
                        cx.notify();
                        let resolutions = host.handle_completed_commands(completed);
                        (retired, resolutions)
                    }
                });
                let Ok((retired, resolutions)) = result else {
                    break;
                };
                for mounted in retired {
                    mounted.revoke();
                }
                resolve_command_tickets(resolutions);
                if terminal {
                    break;
                }
            }
        });
        self.lifecycle_bridge = Some(HostLifecycleBridge::new(subscription, task));
    }

    fn detach_for_terminal(&mut self, reason: ViewCloseReason) -> Vec<MountedNode> {
        if self.terminal_reason.is_some() {
            return Vec::new();
        }
        self.terminal_window_cleanup_pending = self.owns_window_resources();
        self.interactions
            .cancel_deferred(OpenGpuiPortDragCancelReason::HostClosed);
        self.scene_interactions
            .cancel_deferred(OpenGpuiSceneGestureCancelReason::HostClosed);
        self.canvas_interaction.close();
        self.discard_pending_interaction_submission();
        self.connection_owner = None;
        self.scene_origin = None;
        self.scene_transform = None;
        self.terminal_reason = Some(reason);
        self.document_epoch = None;
        self.detach_all_instances()
    }

    pub(super) fn enter_terminal(&mut self, reason: ViewCloseReason) {
        let panic = revoke_mounted_nodes(self.detach_for_terminal(reason));
        resume_mounted_node_panic(panic);
    }
}

impl Drop for OpenGpuiSceneHost {
    fn drop(&mut self) {
        let resolutions = self.commands.terminate_dropped();
        let panic = revoke_mounted_nodes(self.detach_all_instances());
        suppress_host_panic(panic);
        if let Some(bridge) = self.lifecycle_bridge.take() {
            bridge.unsubscribe(&self.view);
        }
        resolve_command_tickets(resolutions);
    }
}
