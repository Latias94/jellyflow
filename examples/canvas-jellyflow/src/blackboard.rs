use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::future::poll_fn;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;
use std::task::{Poll, Waker};

use jellyflow_core::{Symbol, SymbolId};
use jellyflow_open_gpui::retained_field::{
    ApplicationRetainedField, ApplicationRetainedFieldDispatchReceipt,
    ApplicationRetainedFieldResolution, RetainedFieldConflictChoice, RetainedFieldPhase,
};
use jellyflow_runtime::commands::{GraphCommand, GraphCommandOutcome, SymbolEdit};
use jellyflow_runtime::graph::{
    FactChange, GraphRuntime, GraphRuntimeEvent, GraphSnapshot, RuntimeFactKey, RuntimePatchKind,
};
use open_gpui::{
    Context, InteractiveElement, ParentElement, Render, StatefulInteractiveElement, Styled, Task,
    Window, div, px,
};
use open_gpui_ui_components::prelude::Sizable;
use open_gpui_ui_components::{Button, ButtonVariant, TextInput};

use crate::product_theme::NativeProductTheme;

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

/// Lifecycle evidence delivered with one Blackboard source invalidation.
///
/// A source must retain removals until delivery even when a later snapshot contains the same
/// domain ID again. Snapshot equality alone cannot distinguish an update from identity reuse.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BlackboardSourceInvalidation {
    reset_lifetimes: bool,
    retired_symbols: BTreeSet<SymbolId>,
}

impl BlackboardSourceInvalidation {
    #[must_use]
    pub fn refresh() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn reset_lifetimes() -> Self {
        Self {
            reset_lifetimes: true,
            retired_symbols: BTreeSet::new(),
        }
    }

    #[must_use]
    pub fn retire_symbols(symbols: impl IntoIterator<Item = SymbolId>) -> Self {
        Self {
            reset_lifetimes: false,
            retired_symbols: symbols.into_iter().collect(),
        }
    }

    fn merge(&mut self, newer: Self) {
        self.reset_lifetimes |= newer.reset_lifetimes;
        self.retired_symbols.extend(newer.retired_symbols);
    }
}

pub type BlackboardSourceCallback = Rc<dyn Fn(BlackboardSourceInvalidation)>;

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

    fn cancel(&mut self) -> Result<(), BlackboardSourceError> {
        let Some(cancel) = self.cancel.take() else {
            return Ok(());
        };
        catch_unwind(AssertUnwindSafe(cancel)).map_err(|_| {
            BlackboardSourceError::new("blackboard source subscription cancellation panicked")
        })
    }
}

impl Drop for BlackboardSourceSubscription {
    fn drop(&mut self) {
        let _ = self.cancel();
    }
}

/// Application-owned boundary for Blackboard loading, observation, and commands.
pub trait BlackboardSource {
    fn subscribe(
        &self,
        callback: BlackboardSourceCallback,
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
        callback: BlackboardSourceCallback,
    ) -> Result<BlackboardSourceSubscription, BlackboardSourceError> {
        let token = self
            .runtime
            .subscribe(move |event| {
                if let Some(invalidation) = runtime_blackboard_invalidation(&event) {
                    callback(invalidation);
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

fn runtime_blackboard_invalidation(
    event: &GraphRuntimeEvent,
) -> Option<BlackboardSourceInvalidation> {
    match event {
        GraphRuntimeEvent::Patched { patch } => match patch.kind() {
            RuntimePatchKind::DocumentReset => {
                Some(BlackboardSourceInvalidation::reset_lifetimes())
            }
            RuntimePatchKind::Incremental => {
                let mut affects_symbols = false;
                let retired_symbols = patch.facts().iter().filter_map(|fact| {
                    let RuntimeFactKey::Symbol(symbol) = fact.key() else {
                        return None;
                    };
                    affects_symbols = true;
                    (fact.change() == FactChange::Removed).then_some(symbol)
                });
                let invalidation = BlackboardSourceInvalidation::retire_symbols(retired_symbols);
                affects_symbols.then_some(invalidation)
            }
        },
        GraphRuntimeEvent::HistoryChanged { .. } => None,
    }
}

/// Application-owned Open GPUI panel over graph-scoped symbols.
///
/// This is deliberately not part of the adapter: other UI frameworks can build an equivalent
/// panel from the same application-owned source contract and revision-checked commands.
pub struct NativeBlackboard {
    source: Rc<dyn BlackboardSource>,
    product_theme: NativeProductTheme,
    resource: BlackboardResourceState,
    source_subscription: Option<BlackboardSourceSubscription>,
    source_signal: BlackboardSourceSignal,
    source_task: Option<Task<()>>,
    source_attempt: u64,
    active_attempt: Option<u64>,
    drafts: BTreeMap<SymbolId, ApplicationRetainedField<String>>,
    draft_diagnostic: Option<String>,
}

impl NativeBlackboard {
    #[must_use]
    pub fn new(source: Rc<dyn BlackboardSource>) -> Self {
        Self::new_with_theme(source, NativeProductTheme::workflow_dify())
    }

    /// Creates a Blackboard using the same product tokens as its graph surface.
    #[must_use]
    pub fn new_with_theme(
        source: Rc<dyn BlackboardSource>,
        product_theme: NativeProductTheme,
    ) -> Self {
        Self {
            source,
            product_theme,
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
                let pending = signal.wait().await;
                if panel
                    .update(cx, |panel, cx| {
                        if panel.active_attempt == Some(pending.attempt) {
                            panel.apply_source_invalidation(pending.invalidation);
                            panel.refresh_source(pending.attempt);
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
        let callback: BlackboardSourceCallback =
            Rc::new(move |invalidation| signal.publish(attempt, invalidation));
        match self.subscribe_source(callback) {
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
        match self.snapshot_source() {
            Ok(BlackboardSourceSnapshot::Loading) => {
                self.resource = BlackboardResourceState::Loading;
            }
            Ok(BlackboardSourceSnapshot::Ready(snapshot)) => self.publish_snapshot(snapshot),
            Err(error) => self.fail_source(error),
        }
    }

    fn publish_snapshot(&mut self, snapshot: GraphSnapshot) {
        let mut authority_error = None;
        self.drafts.retain(|id, field| {
            let Some(symbol) = snapshot.graph().symbol(id) else {
                return false;
            };
            match field.observe(snapshot.version(), symbol.name.clone()) {
                Ok(_) => field.is_active(),
                Err(error) => {
                    authority_error = Some(format!(
                        "blackboard rejected foreign authoritative snapshot: {error}"
                    ));
                    true
                }
            }
        });
        if let Some(error) = authority_error {
            self.draft_diagnostic = Some(error);
        }
        self.resource = if snapshot.graph().symbols().is_empty() {
            BlackboardResourceState::Empty
        } else {
            BlackboardResourceState::Ready(snapshot)
        };
    }

    fn apply_source_invalidation(&mut self, invalidation: BlackboardSourceInvalidation) {
        if invalidation.reset_lifetimes {
            self.drafts.clear();
        } else {
            for symbol in invalidation.retired_symbols {
                self.drafts.remove(&symbol);
            }
        }
    }

    fn fail_source(&mut self, error: BlackboardSourceError) {
        self.active_attempt = None;
        let cancellation_error = self
            .source_subscription
            .take()
            .and_then(|mut subscription| subscription.cancel().err());
        let error = if let Some(cancellation_error) = cancellation_error {
            BlackboardSourceError::new(format!("{error}; {cancellation_error}"))
        } else {
            error
        };
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
        match self.snapshot_source() {
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
                        ApplicationRetainedField::new(
                            snapshot.version(),
                            authoritative.name.clone(),
                        )
                    });
                    match field.observe(snapshot.version(), authoritative.name.clone()) {
                        Ok(_) => {
                            field.edit(value);
                            self.draft_diagnostic = None;
                        }
                        Err(error) => {
                            self.draft_diagnostic = Some(format!(
                                "blackboard rejected foreign authoritative snapshot: {error}"
                            ));
                        }
                    }
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
            Ok((receipt, command)) => {
                let outcome = self.execute_source(command);
                self.handle_submission_outcome(symbol, receipt, outcome);
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
    ) -> Result<(ApplicationRetainedFieldDispatchReceipt, GraphCommand), BlackboardPlanError> {
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
        field
            .observe(snapshot.version(), current.name.clone())
            .map_err(|error| {
                BlackboardPlanError::Draft(format!(
                    "blackboard authoritative snapshot rejected: {error}"
                ))
            })?;
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
                snapshot.version(),
            ) {
                Ok(ApplicationRetainedFieldResolution::Reapply(intent)) => intent,
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
            field.plan(snapshot.version()).map_err(|error| {
                BlackboardPlanError::Draft(format!("blackboard apply rejected: {error}"))
            })?
        };
        let updated = Symbol {
            name: intent.value().clone(),
            ..current
        };
        let command = GraphCommand::symbols(
            intent.expected_version(),
            SymbolEdit::new().update(symbol, updated),
        )
        .with_label("edit native blackboard symbol");
        let receipt = match intent.bind_command(&command) {
            Ok(receipt) => receipt,
            Err(error) => {
                let rollback = field.not_dispatched(intent.attempt());
                let suffix = rollback.err().map_or_else(String::new, |rollback| {
                    format!("; rollback failed: {rollback}")
                });
                return Err(BlackboardPlanError::Draft(format!(
                    "blackboard command binding rejected: {error}{suffix}"
                )));
            }
        };
        Ok((receipt, command))
    }

    fn handle_submission_outcome(
        &mut self,
        symbol: SymbolId,
        receipt: ApplicationRetainedFieldDispatchReceipt,
        outcome: Result<GraphCommandOutcome, BlackboardCommandError>,
    ) {
        match outcome {
            Ok(outcome) => self.settle_submission(symbol, receipt, outcome),
            Err(error) => {
                let result = self.drafts.get_mut(&symbol).map(|field| {
                    field.mark_uncertain(
                        receipt,
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
        receipt: ApplicationRetainedFieldDispatchReceipt,
        outcome: GraphCommandOutcome,
    ) {
        let Some(field) = self.drafts.get_mut(&symbol) else {
            return;
        };
        if let Err(error) = field.settle_command(receipt, outcome) {
            self.draft_diagnostic = Some(format!(
                "blackboard rejected an unbound command outcome: {error}"
            ));
            return;
        }
        self.load_latest_snapshot();
        if self
            .drafts
            .get(&symbol)
            .is_some_and(|field| !field.is_active())
        {
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
                field
                    .observe(snapshot.version(), current.name.clone())
                    .map_err(|error| {
                        format!("blackboard authoritative snapshot rejected: {error}")
                    })?;
                match field.resolve(
                    RetainedFieldConflictChoice::AcceptRemote,
                    snapshot.version(),
                ) {
                    Ok(ApplicationRetainedFieldResolution::AcceptedRemote { .. }) => Ok(()),
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

    fn subscribe_source(
        &self,
        callback: BlackboardSourceCallback,
    ) -> Result<BlackboardSourceSubscription, BlackboardSourceError> {
        catch_unwind(AssertUnwindSafe(|| self.source.subscribe(callback))).unwrap_or_else(|_| {
            Err(BlackboardSourceError::new(
                "blackboard source subscription panicked",
            ))
        })
    }

    fn snapshot_source(&self) -> Result<BlackboardSourceSnapshot, BlackboardSourceError> {
        catch_unwind(AssertUnwindSafe(|| self.source.snapshot())).unwrap_or_else(|_| {
            Err(BlackboardSourceError::new(
                "blackboard source snapshot panicked",
            ))
        })
    }

    fn execute_source(
        &self,
        command: GraphCommand,
    ) -> Result<GraphCommandOutcome, BlackboardCommandError> {
        catch_unwind(AssertUnwindSafe(|| self.source.execute(command))).unwrap_or_else(|_| {
            Err(BlackboardCommandError::new(
                "blackboard source command panicked",
            ))
        })
    }
}

impl Render for NativeBlackboard {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        let theme = self.product_theme;
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
                    .map_or(RetainedFieldPhase::Clean, ApplicationRetainedField::phase);
                let dirty = phase != RetainedFieldPhase::Clean;
                let conflicted = matches!(phase, RetainedFieldPhase::Conflict);
                let can_submit = self.drafts.get(&id).is_some_and(|field| {
                    !field.attempt_state().blocks_submission() && (field.can_submit() || conflicted)
                });
                let field_notice = self
                    .drafts
                    .get(&id)
                    .and_then(ApplicationRetainedField::notice)
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
                        .p(px(theme.spacing.lg))
                        .flex()
                        .flex_col()
                        .gap(px(theme.spacing.md))
                        .rounded(px(theme.radius.control))
                        .border_1()
                        .border_color(theme.border.normal)
                        .bg(theme.surface.raised)
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .text_xs()
                                .text_color(theme.text.secondary)
                                .child(type_label.to_ascii_uppercase())
                                .children(dirty.then(|| {
                                    div()
                                        .text_color(if conflicted {
                                            theme.status.conflict.text
                                        } else {
                                            theme.status.info.text
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
                            div()
                                .text_xs()
                                .text_color(theme.status.danger.text)
                                .child(message)
                        })),
                );
            }
        }

        let resource_notice = match &self.resource {
            BlackboardResourceState::Loading => Some(
                div()
                    .debug_selector(|| NATIVE_BLACKBOARD_LOADING_SELECTOR.to_owned())
                    .text_sm()
                    .text_color(theme.text.muted)
                    .child("Loading symbols..."),
            ),
            BlackboardResourceState::Empty => Some(
                div()
                    .debug_selector(|| NATIVE_BLACKBOARD_EMPTY_SELECTOR.to_owned())
                    .text_sm()
                    .text_color(theme.text.muted)
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
                        .gap(px(theme.spacing.md))
                        .text_sm()
                        .text_color(theme.status.danger.text)
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
            .border_color(theme.border.normal)
            .bg(theme.surface.panel)
            .text_color(theme.text.primary)
            .child(
                div()
                    .h(px(48.0))
                    .px(px(14.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(theme.border.normal)
                    .child(div().text_base().child("Blackboard"))
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.text.muted)
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
                    .children(self.draft_diagnostic.clone().map(|message| {
                        div()
                            .text_xs()
                            .text_color(theme.status.danger.text)
                            .child(message)
                    })),
            )
    }
}

#[derive(Clone, Default)]
struct BlackboardSourceSignal {
    pending: Rc<std::cell::RefCell<Option<BlackboardPendingSignal>>>,
    waker: Rc<std::cell::RefCell<Option<Waker>>>,
}

struct BlackboardPendingSignal {
    attempt: u64,
    invalidation: BlackboardSourceInvalidation,
}

impl BlackboardSourceSignal {
    fn publish(&self, attempt: u64, invalidation: BlackboardSourceInvalidation) {
        let mut pending = self.pending.borrow_mut();
        match pending.as_mut() {
            Some(current) if current.attempt > attempt => return,
            Some(current) if current.attempt == attempt => {
                current.invalidation.merge(invalidation);
            }
            _ => {
                *pending = Some(BlackboardPendingSignal {
                    attempt,
                    invalidation,
                });
            }
        }
        drop(pending);
        if let Some(waker) = self.waker.borrow_mut().take() {
            waker.wake();
        }
    }

    async fn wait(&self) -> BlackboardPendingSignal {
        poll_fn(|context| {
            if let Some(pending) = self.pending.borrow_mut().take() {
                Poll::Ready(pending)
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
