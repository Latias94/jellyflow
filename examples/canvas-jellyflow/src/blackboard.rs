use std::future::poll_fn;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;
use std::task::{Poll, Waker};

use jellyflow_core::SymbolId;
use jellyflow_runtime::commands::GraphCommand;
use jellyflow_runtime::graph::GraphSnapshot;
use open_gpui::{
    Context, InteractiveElement, ParentElement, Render, StatefulInteractiveElement, Styled, Task,
    Window, div, px,
};
use open_gpui_ui_components::prelude::Sizable;
use open_gpui_ui_components::{Button, ButtonVariant, TextInput};

use crate::product_domain::blackboard::{
    BlackboardApplyAction, BlackboardAuthoring, BlackboardDispatchOutcome, BlackboardDraftPhase,
    BlackboardObservation, BlackboardRetryReason, BlackboardSource, BlackboardSourceCallback,
    BlackboardSourceError, BlackboardSourceInvalidation, BlackboardSourceSnapshot,
    BlackboardSourceSubscription, BlackboardSubmissionPhase,
};
use crate::product_theme::NativeProductTheme;

pub const NATIVE_BLACKBOARD_WIDTH: f32 = 280.0;
const NATIVE_BLACKBOARD_LOADING_SELECTOR: &str = "jellyflow-native-blackboard-loading";
const NATIVE_BLACKBOARD_EMPTY_SELECTOR: &str = "jellyflow-native-blackboard-empty";
const NATIVE_BLACKBOARD_FAILED_SELECTOR: &str = "jellyflow-native-blackboard-failed";
const NATIVE_BLACKBOARD_RETRY_SELECTOR: &str = "jellyflow-native-blackboard-retry";
const NATIVE_BLACKBOARD_SCROLL_SELECTOR: &str = "jellyflow-native-blackboard-scroll";

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
    Ready(BlackboardObservation),
    Failed(BlackboardSourceError),
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
    authoring: BlackboardAuthoring,
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
            authoring: BlackboardAuthoring::new(),
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
        let observation = match BlackboardObservation::from_snapshot(&snapshot) {
            Ok(observation) => observation,
            Err(error) => {
                self.fail_source(BlackboardSourceError::new(format!(
                    "invalid blackboard snapshot: {error}"
                )));
                return;
            }
        };
        match self.authoring.observe(&observation) {
            Ok(transition) if transition.ignored_stale_observation() => return,
            Ok(_) => {}
            Err(error) => {
                self.fail_source(BlackboardSourceError::new(format!(
                    "blackboard snapshot rejected: {error}"
                )));
                return;
            }
        }
        self.resource = if observation.symbols().is_empty() {
            BlackboardResourceState::Empty
        } else {
            BlackboardResourceState::Ready(observation)
        };
    }

    fn apply_source_invalidation(&mut self, invalidation: BlackboardSourceInvalidation) {
        invalidation.apply_to(&mut self.authoring);
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

    fn load_latest_observation(&mut self) -> Option<BlackboardObservation> {
        match self.snapshot_source() {
            Ok(BlackboardSourceSnapshot::Loading) => {
                self.resource = BlackboardResourceState::Loading;
                None
            }
            Ok(BlackboardSourceSnapshot::Ready(snapshot)) => {
                self.publish_snapshot(snapshot);
                match &self.resource {
                    BlackboardResourceState::Ready(observation) => Some(observation.clone()),
                    BlackboardResourceState::Loading
                    | BlackboardResourceState::Empty
                    | BlackboardResourceState::Failed(_) => None,
                }
            }
            Err(error) => {
                self.fail_source(error);
                None
            }
        }
    }

    fn edit_name(&mut self, symbol: SymbolId, value: String, cx: &mut Context<Self>) -> bool {
        let changed = if let BlackboardResourceState::Ready(observation) = &self.resource {
            match self.authoring.edit_name(observation, symbol, value) {
                Ok(transition) => {
                    let diagnostic_cleared = self.draft_diagnostic.take().is_some();
                    transition.state_changed() || diagnostic_cleared
                }
                Err(error) => {
                    let diagnostic = error.to_string();
                    if self.draft_diagnostic.as_deref() == Some(&diagnostic) {
                        false
                    } else {
                        self.draft_diagnostic = Some(diagnostic);
                        true
                    }
                }
            }
        } else {
            false
        };
        if changed {
            cx.notify();
        }
        changed
    }

    fn apply_name(&mut self, symbol: SymbolId, cx: &mut Context<Self>) {
        let Some(observation) = self.load_latest_observation() else {
            cx.notify();
            return;
        };
        let Some(action) = self
            .authoring
            .projection(&observation, symbol)
            .and_then(|projection| projection.apply_action())
        else {
            self.draft_diagnostic = Some("symbol has no available apply action".to_owned());
            cx.notify();
            return;
        };
        match self.authoring.plan_name(&observation, symbol, action) {
            Ok(submission) => {
                let outcome = self.execute_source(submission.command().clone());
                match self.authoring.complete(submission, outcome) {
                    Ok(_) => self.draft_diagnostic = None,
                    Err(error) => self.draft_diagnostic = Some(error.to_string()),
                }
                self.load_latest_observation();
            }
            Err(error) => self.draft_diagnostic = Some(error.to_string()),
        }
        cx.notify();
    }

    fn accept_remote_name(&mut self, symbol: SymbolId, cx: &mut Context<Self>) {
        let Some(observation) = self.load_latest_observation() else {
            cx.notify();
            return;
        };
        match self.authoring.accept_remote(&observation, symbol) {
            Ok(()) => self.draft_diagnostic = None,
            Err(error) => self.draft_diagnostic = Some(error.to_string()),
        }
        cx.notify();
    }

    fn discard_name_submission(&mut self, symbol: SymbolId, cx: &mut Context<Self>) {
        let Some(observation) = self.load_latest_observation() else {
            cx.notify();
            return;
        };
        match self.authoring.discard_submission(&observation, symbol) {
            Ok(()) => self.draft_diagnostic = None,
            Err(error) => self.draft_diagnostic = Some(error.to_string()),
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

    fn execute_source(&self, command: GraphCommand) -> BlackboardDispatchOutcome {
        catch_unwind(AssertUnwindSafe(|| self.source.execute(command))).unwrap_or_else(|_| {
            BlackboardDispatchOutcome::uncertain(
                "blackboard source command panicked after entering the source boundary",
            )
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
        let mut symbols = Vec::new();
        if let BlackboardResourceState::Ready(observation) = &self.resource {
            for (id, symbol) in observation.symbols() {
                let id = *id;
                let projection = self
                    .authoring
                    .projection(observation, id)
                    .expect("iterated Blackboard symbol has a projection");
                let phase = projection.phase();
                let submission_phase = projection.submission_phase();
                let dirty = phase.is_dirty() || submission_phase != BlackboardSubmissionPhase::Idle;
                let conflicted = projection.can_accept_remote();
                let apply_action = projection.apply_action();
                let can_discard = projection.can_discard_submission();
                let retry_reason = projection.retry_reason();
                let (value, field_notice) = projection.into_text();
                let input_panel = cx.weak_entity();
                let apply_panel = cx.weak_entity();
                let accept_panel = cx.weak_entity();
                let discard_panel = cx.weak_entity();
                let status_label = match submission_phase {
                    BlackboardSubmissionPhase::Submitting
                    | BlackboardSubmissionPhase::AwaitingAuthoritative => "Applying",
                    BlackboardSubmissionPhase::Retryable
                        if retry_reason == Some(BlackboardRetryReason::RuntimeConflict) =>
                    {
                        "Runtime conflict"
                    }
                    BlackboardSubmissionPhase::Retryable => "Retry required",
                    BlackboardSubmissionPhase::Uncertain => "Uncertain",
                    BlackboardSubmissionPhase::Idle => match phase {
                        BlackboardDraftPhase::Conflict => "Conflict",
                        BlackboardDraftPhase::Clean | BlackboardDraftPhase::Edited => "Edited",
                    },
                };
                let apply_label = match apply_action {
                    Some(BlackboardApplyAction::Reapply) => "Reapply",
                    Some(
                        BlackboardApplyAction::Retry | BlackboardApplyAction::ConfirmUncertainRetry,
                    ) => "Retry",
                    Some(BlackboardApplyAction::Commit) | None => "Apply",
                };
                let type_label = symbol
                    .ty()
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
                                        .child(status_label)
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
                                                    let Some(input_panel) = input_panel.upgrade()
                                                    else {
                                                        return;
                                                    };
                                                    if input_panel.update(cx, |panel, cx| {
                                                        panel.edit_name(id, value, cx)
                                                    }) {
                                                        window.refresh();
                                                    }
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
                                                apply_label,
                                            )
                                            .variant(if dirty {
                                                ButtonVariant::Default
                                            } else {
                                                ButtonVariant::Ghost
                                            })
                                            .xsmall()
                                            .disabled(apply_action.is_none())
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
                        .children((can_discard && !conflicted).then(|| {
                            Button::new(
                                format!("jellyflow-blackboard-discard-{}", id.0),
                                "Dismiss status",
                            )
                            .variant(ButtonVariant::Ghost)
                            .xsmall()
                            .on_click(move |_, window, cx| {
                                let _ = discard_panel
                                    .update(cx, |panel, cx| panel.discard_name_submission(id, cx));
                                window.refresh();
                            })
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
