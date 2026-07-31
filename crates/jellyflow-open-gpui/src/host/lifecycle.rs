use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::future::poll_fn;
use std::rc::Rc;
use std::task::{Poll, Waker};

use jellyflow_core::NodeId;
use jellyflow_runtime::{
    graph::GraphRevision, graph::NodeLifetimeKey, graph::RuntimePatch, view::NodeInstanceKey,
    view::ViewCloseOutcome, view::ViewCloseReason, view::ViewCloseRequest, view::ViewSession,
    view::ViewSessionError, view::ViewSessionEvent, view::ViewSubscriptionToken,
};
use open_gpui::{AsyncWindowContext, Context, KeystrokeEvent, Task, WeakEntity, Window};

use super::OpenGpuiSceneHost;
use super::diagnostics::{OpenGpuiSceneHostDiagnostic, OpenGpuiSceneHostDiagnosticKind};
use super::native_input::{OpenGpuiGraphKeyOwner, OpenGpuiNativeKeyOwner};
use super::node_lifecycle::{MountedNode, suppress_host_panic};
use super::retained_node_owner::retire_mounted_nodes;
use crate::activity::OpenGpuiActivityKind;
use crate::command_journal::{
    OpenGpuiCommandCloseError, OpenGpuiCompletedCommand, resolve_command_tickets,
};

fn release_mounted_pointer_capture(nodes: &[MountedNode], window: &mut Window) -> bool {
    nodes
        .iter()
        .any(|node| node.activity.release_pointer_capture_if_owned(window))
}

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
    current_lifetime: CurrentNodeLifetime,
}

impl PendingNodeChange {
    pub(super) fn revision(self) -> GraphRevision {
        self.revision
    }

    pub(super) fn is_stale_for(self, instance: &NodeInstanceKey) -> bool {
        self.current_lifetime.is_stale_for(instance)
    }

    fn merge(&mut self, next: Self) {
        if self.revision > next.revision {
            return;
        }
        self.revision = next.revision;
        if next.current_lifetime.is_definitive() {
            self.current_lifetime = next.current_lifetime;
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum CurrentNodeLifetime {
    Unavailable,
    Retired,
    Live(NodeLifetimeKey),
}

impl CurrentNodeLifetime {
    fn is_definitive(self) -> bool {
        !matches!(self, Self::Unavailable)
    }

    fn is_stale_for(self, instance: &NodeInstanceKey) -> bool {
        match self {
            Self::Unavailable => false,
            Self::Retired => true,
            Self::Live(current) => current != instance.lifetime(),
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
            ViewSessionEvent::RuntimePatched { patch, .. } => {
                self.publish_runtime_patch(patch);
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

    fn publish_runtime_patch(&self, patch: RuntimePatch) {
        let nodes = patch.affected_nodes().iter().copied().map(|node| {
            let current_lifetime = match patch.node_lifetime_change(node) {
                Some(_) => patch
                    .snapshot()
                    .node_lifetime_key(node)
                    .map_or(CurrentNodeLifetime::Retired, CurrentNodeLifetime::Live),
                None => CurrentNodeLifetime::Unavailable,
            };
            (node, current_lifetime)
        });
        self.publish_patch(patch.to_revision(), nodes);
    }

    #[cfg(test)]
    pub(crate) fn publish_runtime_patch_for_test(&self, patch: RuntimePatch) {
        self.publish_runtime_patch(patch);
    }

    fn publish_patch(
        &self,
        revision: GraphRevision,
        nodes: impl IntoIterator<Item = (NodeId, CurrentNodeLifetime)>,
    ) {
        if self.state.terminal_reason.get().is_some() {
            return;
        }
        let mut changes = self.state.node_changes.borrow_mut();
        for (node, current_lifetime) in nodes {
            let next = PendingNodeChange {
                revision,
                current_lifetime,
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
        "a rendered native focus domain, active window interaction, or pending command requires close_in_window"
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
    pub(super) fn ensure_window_owner(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let window_id = window.window_handle().window_id();
        if let Some(owner) = &self.window_owner {
            assert_eq!(
                owner.window, window_id,
                "an OpenGpuiSceneHost entity cannot be rendered in more than one window",
            );
            return;
        }

        let host = cx.entity();
        let release_subscription = window.observe_release(&host, cx, |host, window, cx| {
            host.release_in_window(window, cx);
        });
        self.native_input.bind_window(window_id);
        let input_host = cx.weak_entity();
        let key_interceptor = cx.intercept_keystrokes(move |event, window, cx| {
            let Some(host) = input_host.upgrade() else {
                return;
            };
            host.update(cx, |host, cx| {
                host.freeze_native_key_owner(event, window, cx)
            });
        });
        self.window_owner = Some(super::OpenGpuiSceneHostWindowOwner {
            window: window_id,
            _release_subscription: release_subscription,
            _key_interceptor: key_interceptor,
        });
    }

    fn freeze_native_key_owner(
        &self,
        event: &KeystrokeEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let window_id = window.window_handle().window_id();
        if !self.native_input.owns_window(window_id) || self.interaction_owner.terminal() {
            return;
        }

        let root = self.interaction_owner.focus_handle();
        let focus_in_graph_domain = root
            .as_ref()
            .is_some_and(|focus| focus.contains_focused(window, cx));
        let ime_active = self.retained_nodes.has_activity(OpenGpuiActivityKind::Ime);
        let overlay_active = self
            .retained_nodes
            .has_activity(OpenGpuiActivityKind::Overlay)
            || self.interaction_owner.native_capability().overlay_active();
        let owner = if ime_active
            || (event.keystroke.is_ime_in_progress() && (focus_in_graph_domain || overlay_active))
        {
            OpenGpuiNativeKeyOwner::Ime
        } else if overlay_active {
            OpenGpuiNativeKeyOwner::Overlay
        } else if let Some(target) = self.interaction_owner.exact_focused_graph_target(window) {
            OpenGpuiNativeKeyOwner::Graph(OpenGpuiGraphKeyOwner::Exact(target))
        } else if root.as_ref().is_some_and(|focus| focus.is_focused(window)) {
            OpenGpuiNativeKeyOwner::Graph(OpenGpuiGraphKeyOwner::Root)
        } else if focus_in_graph_domain {
            OpenGpuiNativeKeyOwner::NativeControl
        } else {
            OpenGpuiNativeKeyOwner::None
        };

        self.native_input
            .freeze_key_down(event.keystroke.clone(), owner);
    }

    fn owns_window_resources(&self) -> bool {
        self.interaction_owner.requires_window_cleanup()
            || self.retained_nodes.requires_window_cleanup()
    }

    fn release_registered_pointer_capture(&self, window: &mut Window) -> bool {
        self.retained_nodes.release_pointer_capture_if_owned(window)
    }

    /// Closes a never-rendered view capability with no pending window-owned state.
    ///
    /// A rendered native focus domain, active interaction, or queued command must be closed through
    /// [`Self::close_in_window`].
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
                    self.pending_terminal_panics.push(Some(payload));
                    let resolutions = self.prepare_terminal_cleanup(reason);
                    resolve_command_tickets(resolutions);
                    self.suppress_pending_terminal_panics();
                } else {
                    suppress_host_panic(Some(payload));
                }
                return Err(OpenGpuiSceneHostCloseError::PanicDuringClose);
            }
        };
        let cleanup_panicked = match &outcome {
            Ok(outcome) => self.finish_terminal_without_window(outcome.reason()),
            Err(ViewSessionError::Closed { reason }) => {
                self.finish_terminal_without_window(*reason)
            }
            Err(_) => false,
        };
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
        self.close_with_request_in_app(request, window, cx)
    }

    fn close_with_request_in_app(
        &mut self,
        request: ViewCloseRequest,
        window: &mut Window,
        cx: &mut open_gpui::App,
    ) -> Result<ViewCloseOutcome, OpenGpuiSceneHostCloseError> {
        let drain = self.commands.begin_close().map_err(command_close_error)?;
        let journal_identities = drain.command_identities();
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
            Ok(Ok(mut outcome)) => {
                let reason = outcome.reason();
                let journal_completions =
                    match outcome.take_command_completions(&journal_identities) {
                        Ok(completions) => completions,
                        Err(error) => {
                            let completed = self
                                .commands
                                .finish_close_after_panic(drain, reason)
                                .map_err(command_close_error)?;
                            self.finish_window_close_cleanup(completed, reason, window, cx);
                            return Err(command_close_error(error.into()));
                        }
                    };
                let completed = self
                    .commands
                    .finish_close(drain, reason, journal_completions)
                    .map_err(command_close_error)?;
                let cleanup_panicked =
                    self.finish_window_close_cleanup(completed, reason, window, cx);
                if cleanup_panicked {
                    Err(OpenGpuiSceneHostCloseError::PanicDuringClose)
                } else {
                    Ok(outcome)
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
        cx: &mut open_gpui::App,
    ) -> bool {
        self.release_registered_pointer_capture(window);
        let resolutions = self.prepare_terminal_cleanup_with_completed(completed, reason);
        self.capture_terminal_window_cleanup(window, cx);
        window.defer(cx, move |_, _| resolve_command_tickets(resolutions));
        self.suppress_pending_terminal_panics()
    }

    fn finish_terminal_without_window(&mut self, reason: ViewCloseReason) -> bool {
        let resolutions = self.prepare_terminal_cleanup(reason);
        resolve_command_tickets(resolutions);
        self.suppress_pending_terminal_panics()
    }

    pub(super) fn prepare_terminal_cleanup(
        &mut self,
        reason: ViewCloseReason,
    ) -> Vec<crate::command_journal::OpenGpuiPendingCommandResolution> {
        let completed = self.commands.terminate(reason);
        self.prepare_terminal_cleanup_with_completed(completed, reason)
    }

    fn prepare_terminal_cleanup_with_completed(
        &mut self,
        completed: Vec<OpenGpuiCompletedCommand>,
        reason: ViewCloseReason,
    ) -> Vec<crate::command_journal::OpenGpuiPendingCommandResolution> {
        self.native_input.terminate();
        let resolutions = self.handle_completed_commands(completed);
        let panics = retire_mounted_nodes(self.detach_for_terminal(reason));
        self.pending_terminal_panics.merge(panics);
        resolutions
    }

    fn suppress_pending_terminal_panics(&mut self) -> bool {
        let panics = std::mem::take(&mut self.pending_terminal_panics);
        let panicked = !panics.is_empty() || panics.leaked_recursive_sentinel();
        let leaked_recursive_sentinel = panics.suppress();
        if leaked_recursive_sentinel {
            self.record_feedback_failure(
                None,
                "native lifecycle retained one bounded recursive panic payload sentinel".to_owned(),
            );
        }
        panicked
    }

    pub(super) fn resume_pending_terminal_panics(&mut self) {
        let panics = std::mem::take(&mut self.pending_terminal_panics);
        if panics.leaked_recursive_sentinel() {
            self.record_feedback_failure(
                None,
                "native lifecycle retained one bounded recursive panic payload sentinel".to_owned(),
            );
        }
        panics.resume();
    }

    pub(super) fn capture_terminal_window_cleanup(
        &mut self,
        window: &mut Window,
        cx: &mut open_gpui::App,
    ) {
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.interaction_owner
                .finish_terminal_window_transition(window, cx);
        }))
        .err();
        self.pending_terminal_panics.push(panic);
    }

    pub(super) fn finish_terminal_render_cleanup(
        &mut self,
        reason: ViewCloseReason,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.release_registered_pointer_capture(window);
        let resolutions = self.prepare_terminal_cleanup(reason);
        self.capture_terminal_window_cleanup(window, cx);
        resolve_command_tickets(resolutions);
        self.resume_pending_terminal_panics();
    }

    fn release_in_window(&mut self, window: &mut Window, cx: &mut open_gpui::App) {
        let close = self.close_with_request_in_app(ViewCloseRequest::default(), window, cx);
        if close.is_ok() || (self.terminal_reason.is_some() && !self.owns_window_resources()) {
            return;
        }

        let reason = self.close_reason().unwrap_or(ViewCloseReason::HostClosed);
        if self.view.close_reason().is_none() {
            let close =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.view.close()));
            if let Err(payload) = close {
                suppress_host_panic(Some(payload));
            }
        }
        self.release_registered_pointer_capture(window);
        let resolutions = self.prepare_terminal_cleanup(reason);
        self.capture_terminal_window_cleanup(window, cx);
        window.defer(cx, move |_, _| resolve_command_tickets(resolutions));
        self.suppress_pending_terminal_panics();
    }

    #[must_use]
    pub fn close_reason(&self) -> Option<ViewCloseReason> {
        self.terminal_reason.or_else(|| self.view.close_reason())
    }

    pub(super) fn ensure_lifecycle_bridge(&mut self, window: &Window, cx: &mut Context<Self>) {
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
            Err(ViewSessionError::Closed { .. }) => return,
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
        let task = cx.spawn_in(
            window,
            async move |host: WeakEntity<OpenGpuiSceneHost>, cx: &mut AsyncWindowContext| {
            loop {
                let event = signal.wait().await;
                let terminal = matches!(&event, HostLifecycleEvent::Closed(_));
                let result = cx.update(|window, app| {
                    let host = host.upgrade()?;
                    Some(host.update(app, |host, cx| match event {
                        HostLifecycleEvent::NodesChanged(changes) => {
                            let retired = host.apply_node_changes(&changes, cx);
                            release_mounted_pointer_capture(&retired, window);
                            match host.interaction_owner.reconcile_authoritative_state() {
                                Ok(update) => {
                                    host.apply_canvas_interaction_update(update, window, cx)
                                }
                                Err(error) => host.record_command_diagnostic(format!(
                                    "failed to reconcile native interaction authority: {error}"
                                )),
                            }
                            host.interaction_owner
                                .schedule_pending_native_drain(window, cx);
                            cx.notify();
                            (retired, Vec::new())
                        }
                        HostLifecycleEvent::ViewChanged => {
                            cx.notify();
                            (Vec::new(), Vec::new())
                        }
                        HostLifecycleEvent::Closed(reason) => {
                            let completed = host.commands.terminate(reason);
                            let resolutions = host.handle_completed_commands(completed);
                            let retired = host.detach_for_terminal(reason);
                            release_mounted_pointer_capture(&retired, window);
                            host.capture_terminal_window_cleanup(window, cx);
                            cx.notify();
                            (retired, resolutions)
                        }
                    }))
                });
                let Ok(Some((retired, resolutions))) = result else {
                    break;
                };
                let panics = retire_mounted_nodes(retired);
                resolve_command_tickets(resolutions);
                let panicked = !panics.is_empty() || panics.leaked_recursive_sentinel();
                if panicked {
                    let leaked_recursive_sentinel = panics.suppress();
                    let message = match (terminal, leaked_recursive_sentinel) {
                        (true, true) => {
                            "terminal node retirement callback panicked; lifecycle bridge retained one bounded recursive panic payload sentinel"
                        }
                        (true, false) => {
                            "terminal node retirement callback panicked; lifecycle bridge completed cleanup"
                        }
                        (false, true) => {
                            "node retirement callback panicked; lifecycle bridge continued and retained one bounded recursive panic payload sentinel"
                        }
                        (false, false) => {
                            "node retirement callback panicked; lifecycle bridge continued"
                        }
                    };
                    let recorded = cx
                        .update(|_, app| {
                            let host = host.upgrade()?;
                            host.update(app, |host, cx| {
                                host.record_feedback_failure(None, message.to_owned());
                                cx.notify();
                            });
                            Some(())
                        })
                        .ok()
                        .flatten()
                        .is_some();
                    if !recorded {
                        break;
                    }
                }
                if terminal {
                    break;
                }
            }
        },
        );
        self.lifecycle_bridge = Some(HostLifecycleBridge::new(subscription, task));
    }

    fn detach_for_terminal(&mut self, reason: ViewCloseReason) -> Vec<MountedNode> {
        if self.terminal_reason.is_some() {
            return Vec::new();
        }
        let window_cleanup_required = self.owns_window_resources();
        self.interaction_owner
            .prepare_terminal(window_cleanup_required);
        self.content_sizes.clear();
        self.discard_frame_transaction();
        self.scene_origin = None;
        self.scene_transform = None;
        self.terminal_reason = Some(reason);
        self.retained_nodes.clear_document();
        self.close_all_instances(reason)
    }

    #[cfg(test)]
    pub(super) fn enter_terminal(&mut self, reason: ViewCloseReason) {
        let resolutions = self.prepare_terminal_cleanup(reason);
        resolve_command_tickets(resolutions);
        self.resume_pending_terminal_panics();
    }
}

impl Drop for OpenGpuiSceneHost {
    fn drop(&mut self) {
        self.native_input.terminate();
        let completed = self.commands.terminate_dropped();
        self.content_sizes.clear();
        self.discard_frame_transaction();
        let resolutions = self.handle_completed_commands(completed);
        let mounted = self.close_all_instances(ViewCloseReason::HostClosed);
        self.pending_terminal_panics
            .merge(retire_mounted_nodes(mounted));
        if let Some(bridge) = self.lifecycle_bridge.take() {
            bridge.unsubscribe(&self.view);
        }
        resolve_command_tickets(resolutions);
        self.suppress_pending_terminal_panics();
    }
}
