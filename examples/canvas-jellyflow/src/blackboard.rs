use std::collections::BTreeMap;
use std::fmt;
use std::future::poll_fn;
use std::rc::Rc;
use std::task::{Poll, Waker};

use jellyflow::core::{Symbol, SymbolId};
use jellyflow::runtime::{
    GraphCommand, GraphCommandOutcome, GraphRuntime, GraphRuntimeEvent, GraphSnapshot,
    RuntimeFactKey, RuntimePatchKind, SymbolEdit,
};
use jellyflow_open_gpui::{
    RetainedFieldAttempt, RetainedFieldAttemptState, RetainedFieldConflictChoice,
    RetainedFieldIntent, RetainedFieldPhase, RetainedFieldResolution, RevisionedField,
};
use open_gpui::{
    Context, InteractiveElement, ParentElement, Render, StatefulInteractiveElement, Styled, Task,
    Window, div, px, rgb,
};
use open_gpui_ui_components::prelude::Sizable;
use open_gpui_ui_components::{Button, ButtonVariant, TextInput};

pub const NATIVE_BLACKBOARD_WIDTH: f32 = 280.0;
const NATIVE_BLACKBOARD_LOADING_SELECTOR: &str = "jellyflow-native-blackboard-loading";
const NATIVE_BLACKBOARD_EMPTY_SELECTOR: &str = "jellyflow-native-blackboard-empty";
const NATIVE_BLACKBOARD_FAILED_SELECTOR: &str = "jellyflow-native-blackboard-failed";
const NATIVE_BLACKBOARD_RETRY_SELECTOR: &str = "jellyflow-native-blackboard-retry";
const NATIVE_BLACKBOARD_SCROLL_SELECTOR: &str = "jellyflow-native-blackboard-scroll";

/// Application-level failure while loading or observing graph-scoped symbols.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlackboardSourceError {
    message: String,
}

impl BlackboardSourceError {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for BlackboardSourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.message.fmt(formatter)
    }
}

impl std::error::Error for BlackboardSourceError {}

/// Application-level failure while dispatching a Blackboard graph command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlackboardCommandError {
    message: String,
}

impl BlackboardCommandError {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for BlackboardCommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.message.fmt(formatter)
    }
}

impl std::error::Error for BlackboardCommandError {}

/// Result of polling one application-owned Blackboard load attempt.
#[derive(Clone, Debug)]
pub enum BlackboardSourceSnapshot {
    Loading,
    Ready(GraphSnapshot),
}

/// Cancels one source observation when dropped.
pub struct BlackboardSourceSubscription {
    cancel: Option<Box<dyn FnOnce()>>,
}

impl BlackboardSourceSubscription {
    #[must_use]
    pub fn new(cancel: impl FnOnce() + 'static) -> Self {
        Self {
            cancel: Some(Box::new(cancel)),
        }
    }
}

impl Drop for BlackboardSourceSubscription {
    fn drop(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            cancel();
        }
    }
}

/// Application-owned boundary for Blackboard loading, observation, and commands.
pub trait BlackboardSource {
    fn subscribe(
        &self,
        callback: Rc<dyn Fn()>,
    ) -> Result<BlackboardSourceSubscription, BlackboardSourceError>;

    fn snapshot(&self) -> Result<BlackboardSourceSnapshot, BlackboardSourceError>;

    fn execute(&self, command: GraphCommand)
    -> Result<GraphCommandOutcome, BlackboardCommandError>;
}

/// Production Blackboard source backed by one authoritative graph runtime.
pub struct GraphRuntimeBlackboardSource {
    runtime: Rc<GraphRuntime>,
}

impl GraphRuntimeBlackboardSource {
    #[must_use]
    pub fn new(runtime: Rc<GraphRuntime>) -> Self {
        Self { runtime }
    }
}

impl BlackboardSource for GraphRuntimeBlackboardSource {
    fn subscribe(
        &self,
        callback: Rc<dyn Fn()>,
    ) -> Result<BlackboardSourceSubscription, BlackboardSourceError> {
        let token = self
            .runtime
            .subscribe(move |event| {
                if runtime_event_affects_symbols(&event) {
                    callback();
                }
            })
            .map_err(|error| BlackboardSourceError::new(error.to_string()))?;
        let runtime = Rc::clone(&self.runtime);
        Ok(BlackboardSourceSubscription::new(move || {
            runtime.unsubscribe(token);
        }))
    }

    fn snapshot(&self) -> Result<BlackboardSourceSnapshot, BlackboardSourceError> {
        self.runtime
            .snapshot()
            .map(BlackboardSourceSnapshot::Ready)
            .map_err(|error| BlackboardSourceError::new(error.to_string()))
    }

    fn execute(
        &self,
        command: GraphCommand,
    ) -> Result<GraphCommandOutcome, BlackboardCommandError> {
        self.runtime
            .execute(command)
            .map_err(|error| BlackboardCommandError::new(error.to_string()))
    }
}

/// Explicit application resource state rendered by the native Blackboard.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BlackboardResourceStatus {
    Loading,
    Empty,
    Ready,
    Failed(BlackboardSourceError),
}

enum BlackboardResourceState {
    Loading,
    Empty,
    Ready(GraphSnapshot),
    Failed(BlackboardSourceError),
}

enum BlackboardPlanError {
    Resource,
    Draft(String),
}

fn runtime_event_affects_symbols(event: &GraphRuntimeEvent) -> bool {
    match event {
        GraphRuntimeEvent::Patched { patch } => {
            patch.kind() == RuntimePatchKind::DocumentReset
                || patch
                    .facts()
                    .iter()
                    .any(|fact| matches!(fact.key(), RuntimeFactKey::Symbol(_)))
        }
        GraphRuntimeEvent::HistoryChanged { .. } => false,
    }
}

/// Application-owned Open GPUI panel over graph-scoped symbols.
///
/// This is deliberately not part of the adapter: other UI frameworks can build an equivalent
/// panel from the same application-owned source contract and revision-checked commands.
pub struct NativeBlackboard {
    source: Rc<dyn BlackboardSource>,
    resource: BlackboardResourceState,
    source_subscription: Option<BlackboardSourceSubscription>,
    source_signal: BlackboardSourceSignal,
    source_task: Option<Task<()>>,
    source_attempt: u64,
    active_attempt: Option<u64>,
    drafts: BTreeMap<SymbolId, RevisionedField<String>>,
    draft_diagnostic: Option<String>,
}

impl NativeBlackboard {
    #[must_use]
    pub fn new(source: Rc<dyn BlackboardSource>) -> Self {
        Self {
            source,
            resource: BlackboardResourceState::Loading,
            source_subscription: None,
            source_signal: BlackboardSourceSignal::default(),
            source_task: None,
            source_attempt: 0,
            active_attempt: None,
            drafts: BTreeMap::new(),
            draft_diagnostic: None,
        }
    }

    #[must_use]
    pub fn resource_status(&self) -> BlackboardResourceStatus {
        match &self.resource {
            BlackboardResourceState::Loading => BlackboardResourceStatus::Loading,
            BlackboardResourceState::Empty => BlackboardResourceStatus::Empty,
            BlackboardResourceState::Ready(_) => BlackboardResourceStatus::Ready,
            BlackboardResourceState::Failed(error) => {
                BlackboardResourceStatus::Failed(error.clone())
            }
        }
    }

    fn ensure_source_task(&mut self, cx: &mut Context<Self>) {
        if self.source_task.is_some() {
            return;
        }
        let signal = self.source_signal.clone();
        let task = cx.spawn(async move |panel, cx| {
            loop {
                let attempt = signal.wait().await;
                if panel
                    .update(cx, |panel, cx| {
                        if panel.active_attempt == Some(attempt) {
                            panel.refresh_source(attempt);
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        self.source_task = Some(task);
    }

    fn ensure_source_attempt(&mut self, cx: &mut Context<Self>) {
        self.ensure_source_task(cx);
        if !matches!(&self.resource, BlackboardResourceState::Loading)
            || self.source_subscription.is_some()
        {
            return;
        }
        let Some(attempt) = self.source_attempt.checked_add(1) else {
            self.fail_source(BlackboardSourceError::new(
                "blackboard source attempt counter exhausted",
            ));
            return;
        };
        self.source_attempt = attempt;
        self.active_attempt = Some(attempt);
        let signal = self.source_signal.clone();
        let callback: Rc<dyn Fn()> = Rc::new(move || signal.publish(attempt));
        match self.source.subscribe(callback) {
            Ok(subscription) => {
                self.source_subscription = Some(subscription);
                self.refresh_source(attempt);
            }
            Err(error) => self.fail_source(error),
        }
    }

    fn refresh_source(&mut self, attempt: u64) {
        if self.active_attempt != Some(attempt) {
            return;
        }
        match self.source.snapshot() {
            Ok(BlackboardSourceSnapshot::Loading) => {
                self.resource = BlackboardResourceState::Loading;
            }
            Ok(BlackboardSourceSnapshot::Ready(snapshot)) => self.publish_snapshot(snapshot),
            Err(error) => self.fail_source(error),
        }
    }

    fn publish_snapshot(&mut self, snapshot: GraphSnapshot) {
        self.drafts.retain(|id, field| {
            let Some(symbol) = snapshot.graph().symbol(id) else {
                return false;
            };
            field.observe(snapshot.revision(), symbol.name.clone());
            field.phase() != RetainedFieldPhase::Clean
                || !matches!(field.attempt_state(), RetainedFieldAttemptState::Idle)
        });
        self.resource = if snapshot.graph().symbols().is_empty() {
            BlackboardResourceState::Empty
        } else {
            BlackboardResourceState::Ready(snapshot)
        };
    }

    fn fail_source(&mut self, error: BlackboardSourceError) {
        self.source_subscription.take();
        self.active_attempt = None;
        self.resource = BlackboardResourceState::Failed(error);
    }

    fn retry_source(&mut self, cx: &mut Context<Self>) {
        if !matches!(&self.resource, BlackboardResourceState::Failed(_)) {
            return;
        }
        self.resource = BlackboardResourceState::Loading;
        cx.notify();
    }

    fn load_latest_snapshot(&mut self) -> Option<GraphSnapshot> {
        match self.source.snapshot() {
            Ok(BlackboardSourceSnapshot::Loading) => {
                self.resource = BlackboardResourceState::Loading;
                None
            }
            Ok(BlackboardSourceSnapshot::Ready(snapshot)) => {
                self.publish_snapshot(snapshot.clone());
                Some(snapshot)
            }
            Err(error) => {
                self.fail_source(error);
                None
            }
        }
    }

    fn edit_name(&mut self, symbol: SymbolId, value: String, cx: &mut Context<Self>) {
        match &self.resource {
            BlackboardResourceState::Ready(snapshot) => match snapshot.graph().symbol(&symbol) {
                Some(authoritative) => {
                    let field = self.drafts.entry(symbol).or_insert_with(|| {
                        RevisionedField::new(snapshot.revision(), authoritative.name.clone())
                    });
                    field.observe(snapshot.revision(), authoritative.name.clone());
                    field.edit(value);
                    self.draft_diagnostic = None;
                }
                None => {
                    self.drafts.remove(&symbol);
                    self.draft_diagnostic = Some("symbol no longer exists".to_owned());
                }
            },
            BlackboardResourceState::Loading
            | BlackboardResourceState::Empty
            | BlackboardResourceState::Failed(_) => {}
        }
        cx.notify();
    }

    fn apply_name(&mut self, symbol: SymbolId, cx: &mut Context<Self>) {
        let planned = self.plan_name_submission(symbol);
        match planned {
            Ok((intent, updated)) => {
                let attempt = intent.attempt();
                let outcome = self.source.execute(
                    GraphCommand::symbols(
                        intent.expected_revision(),
                        SymbolEdit::new().update(symbol, updated),
                    )
                    .with_label("edit native blackboard symbol"),
                );
                self.handle_submission_outcome(symbol, attempt, outcome);
            }
            Err(BlackboardPlanError::Resource) => {}
            Err(BlackboardPlanError::Draft(message)) => {
                self.draft_diagnostic = Some(message);
            }
        }
        cx.notify();
    }

    fn plan_name_submission(
        &mut self,
        symbol: SymbolId,
    ) -> Result<(RetainedFieldIntent<String>, Symbol), BlackboardPlanError> {
        let snapshot = self
            .load_latest_snapshot()
            .ok_or(BlackboardPlanError::Resource)?;
        let current = snapshot
            .graph()
            .symbol(&symbol)
            .cloned()
            .ok_or_else(|| BlackboardPlanError::Draft("symbol no longer exists".to_owned()))?;
        let field = self.drafts.get_mut(&symbol).ok_or_else(|| {
            BlackboardPlanError::Draft("symbol has no pending blackboard edit".to_owned())
        })?;
        field.observe(snapshot.revision(), current.name.clone());
        let name = field.local().trim().to_owned();
        if name.is_empty() {
            return Err(BlackboardPlanError::Draft(
                "symbol name must not be empty".to_owned(),
            ));
        }
        field.edit(name);
        let intent = if field.conflict().is_some() {
            match field.resolve(
                RetainedFieldConflictChoice::ReapplyLocal,
                snapshot.revision(),
            ) {
                Ok(RetainedFieldResolution::Reapply(intent)) => intent,
                Ok(other) => {
                    return Err(BlackboardPlanError::Draft(format!(
                        "blackboard reapply did not produce an intent: {other:?}"
                    )));
                }
                Err(error) => {
                    return Err(BlackboardPlanError::Draft(format!(
                        "blackboard reapply rejected: {error}"
                    )));
                }
            }
        } else {
            field.plan(snapshot.revision()).map_err(|error| {
                BlackboardPlanError::Draft(format!("blackboard apply rejected: {error}"))
            })?
        };
        let updated = Symbol {
            name: intent.value().clone(),
            ..current
        };
        Ok((intent, updated))
    }

    fn handle_submission_outcome(
        &mut self,
        symbol: SymbolId,
        attempt: RetainedFieldAttempt,
        outcome: Result<GraphCommandOutcome, BlackboardCommandError>,
    ) {
        match outcome {
            Ok(outcome) => self.settle_submission(symbol, attempt, outcome),
            Err(error) => {
                let result = self.drafts.get_mut(&symbol).map(|field| {
                    field.mark_uncertain(
                        attempt,
                        format!(
                            "symbol command outcome is uncertain: {error}; wait for an authoritative refresh"
                        ),
                    )
                });
                if let Some(Err(state_error)) = result {
                    self.draft_diagnostic = Some(format!(
                        "could not retain uncertain symbol attempt: {state_error}"
                    ));
                }
            }
        }
    }

    fn settle_submission(
        &mut self,
        symbol: SymbolId,
        attempt: RetainedFieldAttempt,
        outcome: GraphCommandOutcome,
    ) {
        let Some(field) = self.drafts.get_mut(&symbol) else {
            return;
        };
        field.settle_ticket(attempt, Ok(outcome));
        self.load_latest_snapshot();
        if self.drafts.get(&symbol).is_some_and(|field| {
            field.phase() == RetainedFieldPhase::Clean
                && matches!(field.attempt_state(), RetainedFieldAttemptState::Idle)
        }) {
            self.drafts.remove(&symbol);
        }
        self.draft_diagnostic = None;
    }

    fn accept_remote_name(&mut self, symbol: SymbolId, cx: &mut Context<Self>) {
        let Some(snapshot) = self.load_latest_snapshot() else {
            cx.notify();
            return;
        };
        let result = snapshot
            .graph()
            .symbol(&symbol)
            .ok_or_else(|| "symbol no longer exists".to_owned())
            .and_then(|current| {
                let field = self
                    .drafts
                    .get_mut(&symbol)
                    .ok_or_else(|| "symbol has no pending blackboard edit".to_owned())?;
                field.observe(snapshot.revision(), current.name.clone());
                match field.resolve(
                    RetainedFieldConflictChoice::AcceptRemote,
                    snapshot.revision(),
                ) {
                    Ok(RetainedFieldResolution::AcceptedRemote { .. }) => Ok(()),
                    Ok(other) => Err(format!(
                        "blackboard accept-remote did not accept remote: {other:?}"
                    )),
                    Err(error) => Err(format!("blackboard accept-remote rejected: {error}")),
                }
            });
        match result {
            Ok(()) => {
                self.drafts.remove(&symbol);
                self.draft_diagnostic = None;
            }
            Err(message) => self.draft_diagnostic = Some(message),
        }
        cx.notify();
    }
}

impl Render for NativeBlackboard {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        self.ensure_source_attempt(cx);
        let snapshot = match &self.resource {
            BlackboardResourceState::Ready(snapshot) => Some(snapshot.clone()),
            BlackboardResourceState::Loading
            | BlackboardResourceState::Empty
            | BlackboardResourceState::Failed(_) => None,
        };
        let mut symbols = Vec::new();
        if let Some(snapshot) = snapshot {
            for (id, symbol) in snapshot.graph().symbols() {
                let id = *id;
                let value = self
                    .drafts
                    .get(&id)
                    .map(|draft| draft.local().clone())
                    .unwrap_or_else(|| symbol.name.clone());
                let phase = self
                    .drafts
                    .get(&id)
                    .map_or(RetainedFieldPhase::Clean, RevisionedField::phase);
                let dirty = phase != RetainedFieldPhase::Clean;
                let conflicted = matches!(phase, RetainedFieldPhase::Conflict);
                let can_submit = self.drafts.get(&id).is_some_and(|field| {
                    matches!(field.attempt_state(), RetainedFieldAttemptState::Idle)
                        && (field.can_submit() || conflicted)
                });
                let field_notice = self
                    .drafts
                    .get(&id)
                    .and_then(RevisionedField::notice)
                    .map(str::to_owned);
                let input_panel = cx.weak_entity();
                let apply_panel = cx.weak_entity();
                let accept_panel = cx.weak_entity();
                let type_label = symbol
                    .ty
                    .as_ref()
                    .map(|ty| format!("{ty:?}"))
                    .unwrap_or_else(|| "Any".to_owned());
                symbols.push(
                    div()
                        .p(px(10.0))
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .rounded(px(4.0))
                        .border_1()
                        .border_color(rgb(0x343d48))
                        .bg(rgb(0x20262d))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .text_xs()
                                .text_color(rgb(0xaab4c0))
                                .child(type_label.to_ascii_uppercase())
                                .children(dirty.then(|| {
                                    div()
                                        .text_color(if conflicted {
                                            rgb(0xffb28a)
                                        } else {
                                            rgb(0x7ab8ff)
                                        })
                                        .child(if conflicted { "Conflict" } else { "Edited" })
                                })),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .child(
                                    div()
                                        .debug_selector(move || blackboard_symbol_name_selector(id))
                                        .flex_1()
                                        .min_w(px(0.0))
                                        .child(
                                            TextInput::new(
                                                format!("jellyflow-blackboard-name-{}", id.0),
                                                "Symbol name",
                                            )
                                            .value(value)
                                            .small()
                                            .on_change(move |value, window, cx| {
                                                let input_panel = input_panel.clone();
                                                window.defer(cx, move |window, cx| {
                                                    let _ = input_panel.update(cx, |panel, cx| {
                                                        panel.edit_name(id, value, cx)
                                                    });
                                                    window.refresh();
                                                });
                                            }),
                                        ),
                                )
                                .child(
                                    div()
                                        .debug_selector(move || {
                                            blackboard_symbol_apply_selector(id)
                                        })
                                        .child(
                                            Button::new(
                                                format!("jellyflow-blackboard-apply-{}", id.0),
                                                if conflicted { "Reapply" } else { "Apply" },
                                            )
                                            .variant(if dirty {
                                                ButtonVariant::Default
                                            } else {
                                                ButtonVariant::Ghost
                                            })
                                            .xsmall()
                                            .disabled(!can_submit)
                                            .on_click(
                                                move |_, window, cx| {
                                                    let _ = apply_panel.update(cx, |panel, cx| {
                                                        panel.apply_name(id, cx)
                                                    });
                                                    window.refresh();
                                                },
                                            ),
                                        ),
                                ),
                        )
                        .children(conflicted.then(|| {
                            div()
                                .debug_selector(move || {
                                    blackboard_symbol_accept_remote_selector(id)
                                })
                                .child(
                                    Button::new(
                                        format!("jellyflow-blackboard-accept-remote-{}", id.0),
                                        "Use remote",
                                    )
                                    .variant(ButtonVariant::Ghost)
                                    .xsmall()
                                    .on_click(
                                        move |_, window, cx| {
                                            let _ = accept_panel.update(cx, |panel, cx| {
                                                panel.accept_remote_name(id, cx)
                                            });
                                            window.refresh();
                                        },
                                    ),
                                )
                        }))
                        .children(field_notice.map(|message| {
                            div().text_xs().text_color(rgb(0xff8b84)).child(message)
                        })),
                );
            }
        }

        let resource_notice = match &self.resource {
            BlackboardResourceState::Loading => Some(
                div()
                    .debug_selector(|| NATIVE_BLACKBOARD_LOADING_SELECTOR.to_owned())
                    .text_sm()
                    .text_color(rgb(0x929aa7))
                    .child("Loading symbols..."),
            ),
            BlackboardResourceState::Empty => Some(
                div()
                    .debug_selector(|| NATIVE_BLACKBOARD_EMPTY_SELECTOR.to_owned())
                    .text_sm()
                    .text_color(rgb(0x929aa7))
                    .child("No symbols"),
            ),
            BlackboardResourceState::Failed(error) => {
                let retry_panel = cx.weak_entity();
                let message = format!("Could not load symbols: {}", error.message());
                Some(
                    div()
                        .debug_selector(|| NATIVE_BLACKBOARD_FAILED_SELECTOR.to_owned())
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .text_sm()
                        .text_color(rgb(0xe58b7b))
                        .child(message)
                        .child(
                            div()
                                .debug_selector(|| NATIVE_BLACKBOARD_RETRY_SELECTOR.to_owned())
                                .child(
                                    Button::new("jellyflow-blackboard-retry", "Retry")
                                        .variant(ButtonVariant::Secondary)
                                        .xsmall()
                                        .on_click(move |_, window, cx| {
                                            let _ = retry_panel.update(cx, |panel, cx| {
                                                panel.retry_source(cx);
                                            });
                                            window.refresh();
                                        }),
                                ),
                        ),
                )
            }
            BlackboardResourceState::Ready(_) => None,
        };
        let symbol_count = symbols.len();

        div()
            .debug_selector(|| "jellyflow-native-blackboard".to_owned())
            .w(px(NATIVE_BLACKBOARD_WIDTH))
            .h_full()
            .flex()
            .flex_col()
            .border_l_1()
            .border_color(rgb(0x2d3540))
            .bg(rgb(0x171b20))
            .text_color(rgb(0xf4f7fb))
            .child(
                div()
                    .h(px(48.0))
                    .px(px(14.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(rgb(0x2d3540))
                    .child(div().text_base().child("Blackboard"))
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(0x84909d))
                            .child(format!("{symbol_count} PARAM")),
                    ),
            )
            .child(
                div()
                    .id(NATIVE_BLACKBOARD_SCROLL_SELECTOR)
                    .debug_selector(|| NATIVE_BLACKBOARD_SCROLL_SELECTOR.to_owned())
                    .p(px(12.0))
                    .flex_1()
                    .min_h(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .overflow_y_scroll()
                    .scrollbar_width(px(7.0))
                    .children(resource_notice)
                    .children(symbols)
                    .children(
                        self.draft_diagnostic.clone().map(|message| {
                            div().text_xs().text_color(rgb(0xff8b84)).child(message)
                        }),
                    ),
            )
    }
}

#[derive(Clone, Default)]
struct BlackboardSourceSignal {
    pending: Rc<std::cell::Cell<Option<u64>>>,
    waker: Rc<std::cell::RefCell<Option<Waker>>>,
}

impl BlackboardSourceSignal {
    fn publish(&self, attempt: u64) {
        self.pending.set(Some(attempt));
        if let Some(waker) = self.waker.borrow_mut().take() {
            waker.wake();
        }
    }

    async fn wait(&self) -> u64 {
        poll_fn(|context| {
            if let Some(attempt) = self.pending.take() {
                Poll::Ready(attempt)
            } else {
                *self.waker.borrow_mut() = Some(context.waker().clone());
                Poll::Pending
            }
        })
        .await
    }
}

#[must_use]
pub const fn blackboard_loading_selector() -> &'static str {
    NATIVE_BLACKBOARD_LOADING_SELECTOR
}

#[must_use]
pub const fn blackboard_empty_selector() -> &'static str {
    NATIVE_BLACKBOARD_EMPTY_SELECTOR
}

#[must_use]
pub const fn blackboard_failed_selector() -> &'static str {
    NATIVE_BLACKBOARD_FAILED_SELECTOR
}

#[must_use]
pub const fn blackboard_retry_selector() -> &'static str {
    NATIVE_BLACKBOARD_RETRY_SELECTOR
}

#[must_use]
pub const fn blackboard_scroll_selector() -> &'static str {
    NATIVE_BLACKBOARD_SCROLL_SELECTOR
}

#[must_use]
pub fn blackboard_symbol_name_selector(symbol: SymbolId) -> String {
    format!("jellyflow-native-blackboard-name-{}", symbol.0)
}

#[must_use]
pub fn blackboard_symbol_apply_selector(symbol: SymbolId) -> String {
    format!("jellyflow-native-blackboard-apply-{}", symbol.0)
}

#[must_use]
pub fn blackboard_symbol_accept_remote_selector(symbol: SymbolId) -> String {
    format!("jellyflow-native-blackboard-accept-remote-{}", symbol.0)
}
