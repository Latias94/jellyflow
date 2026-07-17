use std::cell::Cell;
use std::rc::Rc;

use jellyflow::core::{CanvasSize, Node, NodeId, PortCapacity, PortDirection, PortKind};
use jellyflow::runtime::schema::{NodeKindSpec, NodePortSpec};
use jellyflow::runtime::{GraphCommand, GraphCommandOutcome, GraphRevision};
use jellyflow_open_gpui::{
    DraftAttempt, DraftConflictResolution, DraftIntentRejection, DraftPhase, DraftReceiptDecision,
    DraftReconcileDecision, DraftRecovery, DraftResolutionDecision, DraftSubmissionDecision,
    DraftSubmissionIntent, ExternalizedDraftStore, OpenGpuiActivityAcquireError,
    OpenGpuiActivityKind, OpenGpuiActivityLease, OpenGpuiCommandTicket,
    OpenGpuiCommandTicketResult, OpenGpuiNodeActivityRegistry, OpenGpuiNodeBinding,
    OpenGpuiNodeBindingRead, OpenGpuiNodeInteractionGuard, OpenGpuiNodeRegistryBuilder,
    RevisionedDraft,
};
use open_gpui::{
    App, AppContext, Context, Entity, Focusable, InteractiveElement, ParentElement, Render, Styled,
    Subscription, Task, Window, div, px, rgb,
};
use open_gpui_ui_components::{
    Button, ButtonVariant, ListboxOption, Select, TextInput, gpui_adapter::TextInputController,
};

use crate::model_options::{
    WorkflowModelOptionsCompleteness, WorkflowModelOptionsState, WorkflowModelOptionsStatus,
};

use super::super::sync_activity_lease;
use super::{
    WorkflowNodeError, WorkflowNodeStatus, WorkflowPort, workflow_alert,
    workflow_fixed_port_elements, workflow_header,
};

pub const WORKFLOW_LLM_KIND: &str = "workflow.llm";
pub const WORKFLOW_LLM_RENDERER: &str = "jellyflow.open-gpui.workflow.llm";
pub const WORKFLOW_LLM_DEFAULT_SIZE: CanvasSize = CanvasSize {
    width: 340.0,
    height: 350.0,
};
pub const WORKFLOW_PROMPT_FIELD: &str = "prompt";
pub const WORKFLOW_MODEL_FIELD: &str = "model";

const WORKFLOW_LLM_NODE_ID: u128 = 0xd1fa_0001;
pub(super) const DEFAULT_MODEL: &str = "gpt-4.1";

pub type WorkflowDraftStore = ExternalizedDraftStore<String>;

pub struct WorkflowLlmNode {
    binding: Entity<OpenGpuiNodeBinding>,
    activity: OpenGpuiNodeActivityRegistry,
    draft_store: WorkflowDraftStore,
    model_options: Entity<WorkflowModelOptionsState>,
    _model_options_subscription: Subscription,
    prompt_controller: Entity<TextInputController>,
    prompt: RevisionedDraft<String>,
    model: RevisionedDraft<String>,
    model_menu_base: Option<(GraphRevision, String)>,
    model_menu_open: Rc<Cell<bool>>,
    model_overlay_lease: Option<OpenGpuiActivityLease>,
    focus_lease: Option<OpenGpuiActivityLease>,
    ime_lease: Option<OpenGpuiActivityLease>,
    draft_lease: Option<OpenGpuiActivityLease>,
    draft_capacity_overflowed: bool,
    _prompt_command_task: Option<Task<()>>,
    _model_command_task: Option<Task<()>>,
    _prompt_command_lease: Option<OpenGpuiActivityLease>,
    _model_command_lease: Option<OpenGpuiActivityLease>,
    diagnostic: Option<String>,
}

#[derive(Clone)]
struct LlmData {
    value: serde_json::Value,
    title: String,
    prompt: String,
    model: String,
}

#[derive(Clone, Copy)]
enum LlmField {
    Prompt,
    Model,
}

struct PendingLlmCommand {
    attempt: DraftAttempt,
    ticket: OpenGpuiCommandTicket,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ModelOptionsNotice {
    Loading,
    Empty,
    Failed { message: String, retryable: bool },
    Partial,
}

impl ModelOptionsNotice {
    fn from_status(status: &WorkflowModelOptionsStatus) -> Option<Self> {
        match status {
            WorkflowModelOptionsStatus::Loading => Some(Self::Loading),
            WorkflowModelOptionsStatus::Empty => Some(Self::Empty),
            WorkflowModelOptionsStatus::Failed(failure) => Some(Self::Failed {
                message: failure.message().to_owned(),
                retryable: failure.is_retryable(),
            }),
            WorkflowModelOptionsStatus::Ready {
                completeness: WorkflowModelOptionsCompleteness::Partial,
                ..
            } => Some(Self::Partial),
            WorkflowModelOptionsStatus::Ready {
                completeness: WorkflowModelOptionsCompleteness::Complete,
                ..
            } => None,
        }
    }
}

fn native_model_options(status: &WorkflowModelOptionsStatus) -> Vec<ListboxOption> {
    let options = status.options().unwrap_or_default();
    options
        .iter()
        .map(|option| {
            let label = option.disabled_reason().map_or_else(
                || option.label().to_owned(),
                |reason| format!("{} - {reason}", option.label()),
            );
            ListboxOption::new(option.value(), label).disabled(!option.is_enabled())
        })
        .collect()
}

impl LlmField {
    const fn key(self) -> &'static str {
        match self {
            Self::Prompt => WORKFLOW_PROMPT_FIELD,
            Self::Model => WORKFLOW_MODEL_FIELD,
        }
    }
}

fn recovery_diagnostic(recoveries: &[(&'static str, DraftRecovery)]) -> Option<String> {
    let recovery_details = recoveries
        .iter()
        .filter(|(_, recovery)| !recovery.is_empty())
        .flat_map(|(field, recovery)| {
            [
                recovery
                    .interrupted_attempt()
                    .map(|attempt| format!("{field}#{}", attempt.get())),
                recovery
                    .composition_interrupted()
                    .then(|| format!("{field}:composition")),
            ]
            .into_iter()
            .flatten()
        })
        .collect::<Vec<_>>();
    (!recovery_details.is_empty()).then(|| {
        let fields = recovery_details
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "Recovered interrupted draft state ({fields}); explicit resubmission or composition is required"
        )
    })
}

impl WorkflowLlmNode {
    fn reconcile_graph(&mut self, cx: &mut Context<Self>) -> Option<LlmData> {
        let result = read_llm_binding(&self.binding, cx);
        match result {
            Ok((revision, data)) => {
                if matches!(
                    self.prompt.reconcile_remote(revision, data.prompt.clone()),
                    DraftReconcileDecision::RemoteApplied { .. }
                ) {
                    let value = self.prompt.local().clone();
                    self.prompt_controller
                        .update(cx, |controller, cx| controller.set_value(value, cx));
                }
                self.model.reconcile_remote(revision, data.model.clone());
                Some(data)
            }
            Err(error) => {
                self.diagnostic = Some(error.to_string());
                None
            }
        }
    }

    fn edit_prompt(&mut self, value: String, cx: &mut Context<Self>) {
        self.prompt.edit_local(value);
        let composing = self
            .prompt_controller
            .read(cx)
            .marked_range_utf16()
            .is_some();
        if composing {
            self.prompt.mark_composing();
        } else {
            self.prompt.unmark_composing();
        }
        self.diagnostic = None;
        cx.notify();
    }

    fn apply_prompt(&mut self, cx: &mut Context<Self>) {
        let node_id = self.binding.read(cx).snapshot().node_id();
        let pending = submit_llm_draft(
            &self.binding,
            node_id,
            LlmField::Prompt,
            &mut self.prompt,
            cx,
        );
        self.start_command(LlmField::Prompt, pending, cx);
        cx.notify();
    }

    fn select_model(&mut self, value: String, cx: &mut Context<Self>) {
        if !self.model_options.read(cx).status().is_selectable(&value) {
            self.model_menu_open.set(false);
            self.model_menu_base.take();
            self.model_overlay_lease.take();
            cx.notify();
            return;
        }
        self.model_menu_open.set(false);
        self.model_overlay_lease.take();
        let node_id = self.binding.read(cx).snapshot().node_id();
        if let Some((revision, model)) = self.model_menu_base.take() {
            self.model = RevisionedDraft::new(revision, model);
        }
        self.model.edit_local(value);
        let pending =
            submit_llm_draft(&self.binding, node_id, LlmField::Model, &mut self.model, cx);
        self.start_command(LlmField::Model, pending, cx);
        cx.notify();
    }

    fn set_model_menu_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if !open {
            self.model_menu_open.set(false);
            self.model_overlay_lease.take();
            cx.notify();
            return;
        }
        if self.model_menu_open.get() {
            return;
        }
        if !self.model_options.read(cx).status().is_ready() || self.model.conflict().is_some() {
            return;
        }
        let (revision, data) = match read_llm_binding(&self.binding, cx) {
            Ok(binding) => binding,
            Err(error) => {
                self.diagnostic = Some(error.to_string());
                cx.notify();
                return;
            }
        };
        let revoked_open = self.model_menu_open.clone();
        match self
            .activity
            .acquire_with_revocation(OpenGpuiActivityKind::Overlay, move || {
                revoked_open.set(false)
            }) {
            Ok(lease) => {
                self.model.reconcile_remote(revision, data.model.clone());
                self.model_menu_base = Some((revision, data.model));
                self.model_menu_open.set(true);
                self.model_overlay_lease = Some(lease);
                self.diagnostic = None;
            }
            Err(error) => {
                self.model_menu_open.set(false);
                self.model_overlay_lease.take();
                self.diagnostic = Some(format!("workflow model overlay lease failed: {error}"));
            }
        }
        cx.notify();
    }

    fn accept_remote_model(&mut self, cx: &mut Context<Self>) {
        let result = read_llm_binding(&self.binding, cx).and_then(|(revision, data)| {
            self.model.reconcile_remote(revision, data.model);
            match self
                .model
                .resolve_at(DraftConflictResolution::AcceptRemote, revision)
            {
                DraftResolutionDecision::AcceptedRemote { .. } => Ok(()),
                _ => Err(WorkflowNodeError::DraftConflict { field: "model" }),
            }
        });
        self.diagnostic = result.err().map(|error| error.to_string());
        cx.notify();
    }

    fn reapply_local_model(&mut self, cx: &mut Context<Self>) {
        let node_id = self.binding.read(cx).snapshot().node_id();
        let pending =
            reapply_llm_draft(&self.binding, node_id, LlmField::Model, &mut self.model, cx);
        self.start_command(LlmField::Model, pending, cx);
        cx.notify();
    }

    fn accept_remote_prompt(&mut self, cx: &mut Context<Self>) {
        let result = read_llm_binding(&self.binding, cx).and_then(|(revision, data)| {
            self.prompt.reconcile_remote(revision, data.prompt);
            match self
                .prompt
                .resolve_at(DraftConflictResolution::AcceptRemote, revision)
            {
                DraftResolutionDecision::AcceptedRemote { .. } => Ok(()),
                _ => Err(WorkflowNodeError::DraftConflict { field: "prompt" }),
            }
        });
        if result.is_ok() {
            let value = self.prompt.local().clone();
            self.prompt_controller
                .update(cx, |controller, cx| controller.set_value(value, cx));
        }
        self.diagnostic = result.err().map(|error| error.to_string());
        cx.notify();
    }

    fn reapply_local_prompt(&mut self, cx: &mut Context<Self>) {
        let node_id = self.binding.read(cx).snapshot().node_id();
        let pending = reapply_llm_draft(
            &self.binding,
            node_id,
            LlmField::Prompt,
            &mut self.prompt,
            cx,
        );
        self.start_command(LlmField::Prompt, pending, cx);
        cx.notify();
    }

    fn start_command(
        &mut self,
        field: LlmField,
        pending: Result<Option<PendingLlmCommand>, WorkflowNodeError>,
        cx: &mut Context<Self>,
    ) {
        match pending {
            Ok(Some(PendingLlmCommand { attempt, ticket })) => {
                let task_lease = match self.activity.acquire(OpenGpuiActivityKind::Task) {
                    Ok(lease) => {
                        self.diagnostic = None;
                        Some(lease)
                    }
                    Err(error) => {
                        self.diagnostic = Some(format!(
                            "workflow {} command task lease failed: {error}",
                            field.key()
                        ));
                        None
                    }
                };
                let task = cx.spawn(async move |node, cx| {
                    let result = ticket.await;
                    let _ = node.update(cx, |node, cx| {
                        node.finish_command(field, attempt, result, cx);
                    });
                });
                match field {
                    LlmField::Prompt => {
                        self._prompt_command_lease = task_lease;
                        self._prompt_command_task = Some(task);
                    }
                    LlmField::Model => {
                        self._model_command_lease = task_lease;
                        self._model_command_task = Some(task);
                    }
                }
            }
            Ok(None) => self.diagnostic = None,
            Err(error) => self.diagnostic = Some(error.to_string()),
        }
    }

    fn finish_command(
        &mut self,
        field: LlmField,
        attempt: DraftAttempt,
        result: OpenGpuiCommandTicketResult,
        cx: &mut Context<Self>,
    ) {
        match field {
            LlmField::Prompt => {
                self._prompt_command_task = None;
                self._prompt_command_lease = None;
            }
            LlmField::Model => {
                self._model_command_task = None;
                self._model_command_lease = None;
            }
        }
        let draft = match field {
            LlmField::Prompt => &mut self.prompt,
            LlmField::Model => &mut self.model,
        };
        if draft.pending_attempt() != Some(attempt) {
            return;
        }
        self.diagnostic = finish_llm_ticket(draft, field, attempt, result);
        cx.notify();
    }

    fn sync_activity(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (focused, composing) = {
            let controller = self.prompt_controller.read(cx);
            (
                controller.focus_handle(cx).is_focused(window),
                controller.marked_range_utf16().is_some(),
            )
        };
        let focus = sync_activity_lease(
            &self.activity,
            &mut self.focus_lease,
            OpenGpuiActivityKind::Focus,
            focused,
        );
        let ime = sync_activity_lease(
            &self.activity,
            &mut self.ime_lease,
            OpenGpuiActivityKind::Ime,
            composing,
        );
        let transient_error = focus.err().or_else(|| ime.err());
        if let Some(error) = transient_error {
            self.reject_transient_input(window, cx);
            self.diagnostic = Some(format!("workflow activity lease failed: {error}"));
        }

        let dirty =
            self.prompt.phase() != DraftPhase::Clean || self.model.phase() != DraftPhase::Clean;
        let draft = sync_activity_lease(
            &self.activity,
            &mut self.draft_lease,
            OpenGpuiActivityKind::Draft,
            dirty,
        );
        if let Err(error) = draft {
            if matches!(
                error,
                OpenGpuiActivityAcquireError::ProtectedCapacityExceeded { .. }
            ) && dirty
            {
                self.externalize_overflow_drafts(cx);
            }
            self.diagnostic = Some(format!("workflow activity lease failed: {error}"));
        } else if self.draft_capacity_overflowed {
            let instance = self.binding.read(cx).snapshot().instance().clone();
            self.draft_store
                .discard_externalized(&instance, WORKFLOW_PROMPT_FIELD);
            self.draft_store
                .discard_externalized(&instance, WORKFLOW_MODEL_FIELD);
            self.draft_capacity_overflowed = false;
        }
    }

    fn reject_transient_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let value = self.prompt_controller.read(cx).value().to_owned();
        self.prompt.edit_local(value.clone());
        self.prompt.unmark_composing();
        self.prompt_controller
            .update(cx, |controller, cx| controller.set_value(value, cx));
        self.focus_lease.take();
        self.ime_lease.take();
        window.blur();
        cx.notify();
    }

    fn externalize_overflow_drafts(&mut self, cx: &mut Context<Self>) {
        let instance = self.binding.read(cx).snapshot().instance().clone();
        self.draft_store
            .externalize_overflow(&instance, WORKFLOW_PROMPT_FIELD, &self.prompt);
        self.draft_store
            .externalize_overflow(&instance, WORKFLOW_MODEL_FIELD, &self.model);
        self.draft_capacity_overflowed = true;
    }

    fn sync_prompt_composition(&mut self, cx: &mut Context<Self>) {
        let composing = self
            .prompt_controller
            .read(cx)
            .marked_range_utf16()
            .is_some();
        reconcile_prompt_composition(&mut self.prompt, composing);
    }

    fn status(&self) -> WorkflowNodeStatus {
        let phases = [self.prompt.phase(), self.model.phase()];
        if phases
            .iter()
            .any(|phase| matches!(phase, DraftPhase::Conflict | DraftPhase::ComposingConflict))
        {
            WorkflowNodeStatus::Conflict
        } else if self.diagnostic.is_some() {
            WorkflowNodeStatus::Error
        } else if phases.iter().any(|phase| *phase != DraftPhase::Clean) {
            WorkflowNodeStatus::Draft
        } else {
            WorkflowNodeStatus::Ready
        }
    }
}

impl Render for WorkflowLlmNode {
    fn render(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        let node_id = self.binding.read(cx).snapshot().node_id();
        let data = self.reconcile_graph(cx).unwrap_or_else(|| LlmData {
            value: serde_json::Value::Null,
            title: "Retired LLM".to_owned(),
            prompt: self.prompt.local().clone(),
            model: self.model.local().clone(),
        });
        self.sync_prompt_composition(cx);
        self.sync_activity(window, cx);
        let status = self.status();
        let ports = bound_ports(&self.binding, cx);
        let prompt_node = cx.weak_entity();
        let apply_node = cx.weak_entity();
        let model_open_node = cx.weak_entity();
        let model_node = cx.weak_entity();
        let prompt_id = workflow_prompt_id(node_id);
        let select_id = workflow_model_id(node_id);
        let model_options_status = self.model_options.read(cx).status().clone();
        let model_options_notice = ModelOptionsNotice::from_status(&model_options_status);
        let model_select_disabled = self.model.conflict().is_some()
            || self.model.pending_attempt().is_some()
            || !model_options_status.is_ready();
        if model_select_disabled && self.model_menu_open.get() {
            self.model_menu_open.set(false);
            self.model_menu_base.take();
            self.model_overlay_lease.take();
        }
        let model_options = native_model_options(&model_options_status);
        let mut control_body = div()
            .px(px(16.0))
            .py(px(14.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                TextInput::new(prompt_id, "Prompt")
                    .controller(self.prompt_controller.clone())
                    .value(self.prompt.local().clone())
                    .placeholder("Describe the task")
                    .on_change(move |value, window, cx| {
                        let prompt_node = prompt_node.clone();
                        window.defer(cx, move |window, cx| {
                            let _ = prompt_node.update(cx, |node, cx| node.edit_prompt(value, cx));
                            window.refresh();
                        });
                    }),
            )
            .child(
                div()
                    .debug_selector(move || workflow_prompt_apply_selector(node_id))
                    .child(
                        Button::new(
                            format!("jellyflow-workflow-prompt-apply-{}", node_id.0),
                            "Apply prompt",
                        )
                        .variant(ButtonVariant::Secondary)
                        .disabled(
                            !matches!(self.prompt.phase(), DraftPhase::Dirty)
                                || self.prompt.pending_attempt().is_some(),
                        )
                        .on_click(move |_, window, cx| {
                            let _ = apply_node.update(cx, |node, cx| node.apply_prompt(cx));
                            window.refresh();
                        }),
                    ),
            )
            .child(
                Select::new(select_id, "Model")
                    .options(model_options)
                    .placeholder(self.model.local().clone())
                    .selected(self.model.local().clone())
                    .open(self.model_menu_open.get())
                    .full_width(true)
                    .disabled(model_select_disabled)
                    .on_open_change(move |open, window, cx| {
                        let _ = model_open_node
                            .update(cx, |node, cx| node.set_model_menu_open(open, cx));
                        window.refresh();
                    })
                    .on_select(move |selection, window, cx| {
                        let value = selection.value().to_owned();
                        let _ = model_node.update(cx, |node, cx| node.select_model(value, cx));
                        window.refresh();
                    }),
            );
        if let Some(notice) = model_options_notice {
            let (selector, label, message, retryable) = match notice {
                ModelOptionsNotice::Loading => (
                    workflow_model_options_loading_selector(node_id),
                    "Model options loading",
                    "Loading model options...".to_owned(),
                    false,
                ),
                ModelOptionsNotice::Empty => (
                    workflow_model_options_empty_selector(node_id),
                    "Model options empty",
                    "No models are available.".to_owned(),
                    false,
                ),
                ModelOptionsNotice::Failed { message, retryable } => (
                    workflow_model_options_failed_selector(node_id),
                    "Model options failed",
                    message,
                    retryable,
                ),
                ModelOptionsNotice::Partial => (
                    workflow_model_options_partial_selector(node_id),
                    "Model options partial",
                    "Some model options are unavailable.".to_owned(),
                    false,
                ),
            };
            let mut alert = workflow_alert(selector.clone(), label)
                .debug_selector(move || selector.clone())
                .p(px(8.0))
                .flex()
                .flex_col()
                .gap(px(6.0))
                .rounded(px(4.0))
                .border_1()
                .border_color(rgb(0x725f42))
                .bg(rgb(0x302b22))
                .text_xs()
                .text_color(rgb(0xe6cf9e))
                .child(message);
            if retryable {
                let retry_options = self.model_options.clone();
                let retry_selector = workflow_model_options_retry_selector(node_id);
                alert = alert.child(
                    div().debug_selector(move || retry_selector.clone()).child(
                        Button::new(
                            format!("jellyflow-workflow-model-options-retry-{}", node_id.0),
                            "Retry",
                        )
                        .variant(ButtonVariant::Secondary)
                        .on_click(move |_, window, cx| {
                            let _retry = retry_options.update(cx, |state, cx| state.retry(cx));
                            window.refresh();
                        }),
                    ),
                );
            }
            control_body = control_body.child(alert);
        }
        if self.prompt.conflict().is_some() {
            let accept_node = cx.weak_entity();
            let reapply_node = cx.weak_entity();
            let reapply_disabled = matches!(self.prompt.phase(), DraftPhase::ComposingConflict)
                || self.prompt.pending_attempt().is_some();
            let accept_selector = workflow_prompt_accept_remote_selector(node_id);
            let reapply_selector = workflow_prompt_reapply_selector(node_id);
            let conflict_selector = workflow_prompt_conflict_selector(node_id);
            control_body = control_body.child(
                workflow_alert(
                    conflict_selector.clone(),
                    "Prompt conflict: Prompt changed in another view",
                )
                .debug_selector(move || conflict_selector)
                .p(px(8.0))
                .flex()
                .flex_col()
                .gap(px(7.0))
                .rounded(px(4.0))
                .border_1()
                .border_color(rgb(0x9d654d))
                .bg(rgb(0x382a28))
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(0xf0c3aa))
                        .child("Prompt changed in another view"),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(6.0))
                        .child(
                            div().debug_selector(move || accept_selector.clone()).child(
                                Button::new(
                                    format!("jellyflow-workflow-prompt-accept-{}", node_id.0),
                                    "Use remote",
                                )
                                .variant(ButtonVariant::Secondary)
                                .on_click(move |_, window, cx| {
                                    let _ = accept_node
                                        .update(cx, |node, cx| node.accept_remote_prompt(cx));
                                    window.refresh();
                                }),
                            ),
                        )
                        .child(
                            div()
                                .debug_selector(move || reapply_selector.clone())
                                .child(
                                    Button::new(
                                        format!("jellyflow-workflow-prompt-reapply-{}", node_id.0),
                                        "Reapply local",
                                    )
                                    .variant(ButtonVariant::Secondary)
                                    .disabled(reapply_disabled)
                                    .on_click(
                                        move |_, window, cx| {
                                            let _ = reapply_node.update(cx, |node, cx| {
                                                node.reapply_local_prompt(cx)
                                            });
                                            window.refresh();
                                        },
                                    ),
                                ),
                        ),
                ),
            );
        }
        if self.model.conflict().is_some() {
            let accept_node = cx.weak_entity();
            let reapply_node = cx.weak_entity();
            let accept_selector = workflow_model_accept_remote_selector(node_id);
            let reapply_selector = workflow_model_reapply_selector(node_id);
            let conflict_selector = workflow_model_conflict_selector(node_id);
            control_body = control_body.child(
                workflow_alert(
                    conflict_selector.clone(),
                    "Model conflict: Model changed in another view",
                )
                .debug_selector(move || conflict_selector)
                .p(px(8.0))
                .flex()
                .flex_col()
                .gap(px(7.0))
                .rounded(px(4.0))
                .border_1()
                .border_color(rgb(0x9d654d))
                .bg(rgb(0x382a28))
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(0xf0c3aa))
                        .child("Model changed in another view"),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(6.0))
                        .child(
                            div().debug_selector(move || accept_selector.clone()).child(
                                Button::new(
                                    format!("jellyflow-workflow-model-accept-{}", node_id.0),
                                    "Use remote",
                                )
                                .variant(ButtonVariant::Secondary)
                                .on_click(move |_, window, cx| {
                                    let _ = accept_node
                                        .update(cx, |node, cx| node.accept_remote_model(cx));
                                    window.refresh();
                                }),
                            ),
                        )
                        .child(
                            div()
                                .debug_selector(move || reapply_selector.clone())
                                .child(
                                    Button::new(
                                        format!("jellyflow-workflow-model-reapply-{}", node_id.0),
                                        "Reapply local",
                                    )
                                    .variant(ButtonVariant::Secondary)
                                    .disabled(self.model.pending_attempt().is_some())
                                    .on_click(
                                        move |_, window, cx| {
                                            let _ = reapply_node.update(cx, |node, cx| {
                                                node.reapply_local_model(cx)
                                            });
                                            window.refresh();
                                        },
                                    ),
                                ),
                        ),
                ),
            );
        }
        let controls = OpenGpuiNodeInteractionGuard::control(control_body);
        let diagnostic = self.diagnostic.clone().map(|message| {
            workflow_alert(
                format!("jellyflow-workflow-llm-diagnostic-{}", node_id.0),
                format!("Workflow LLM error: {message}"),
            )
            .px(px(16.0))
            .text_xs()
            .text_color(rgb(0xe58b7b))
            .child(message)
        });

        div()
            .relative()
            .size_full()
            .rounded(px(6.0))
            .border_1()
            .border_color(rgb(0x3d556f))
            .bg(rgb(0x1d2733))
            .text_color(rgb(0xe8edf3))
            .child(workflow_header(node_id, data.title, "LLM", status))
            .child(controls)
            .children(diagnostic)
            .children(workflow_fixed_port_elements(ports))
    }
}

pub(super) fn catalog_spec() -> NodeKindSpec {
    NodeKindSpec::builder(WORKFLOW_LLM_KIND, "LLM", WORKFLOW_LLM_RENDERER)
        .category(["Workflow", "AI"])
        .keywords(["llm", "model", "prompt", "generate"])
        .default_size(WORKFLOW_LLM_DEFAULT_SIZE)
        .ports([
            NodePortSpec::new(
                "input",
                PortDirection::In,
                PortKind::Exec,
                PortCapacity::Single,
            )
            .with_label("Input"),
            NodePortSpec::new(
                "success",
                PortDirection::Out,
                PortKind::Exec,
                PortCapacity::Multi,
            )
            .with_label("Success"),
        ])
        .default_data(serde_json::json!({
            "title": "LLM",
            "prompt": "",
            "model": DEFAULT_MODEL,
        }))
        .build()
}

pub(super) fn register(
    registry: &mut OpenGpuiNodeRegistryBuilder,
    draft_store: WorkflowDraftStore,
    model_options: Entity<WorkflowModelOptionsState>,
) {
    registry.register(WORKFLOW_LLM_RENDERER, move |binding, _window, cx| {
        mount_workflow_llm(binding, draft_store.clone(), model_options.clone(), cx)
    });
}

fn mount_workflow_llm(
    binding: Entity<OpenGpuiNodeBinding>,
    draft_store: WorkflowDraftStore,
    model_options: Entity<WorkflowModelOptionsState>,
    cx: &mut App,
) -> Entity<WorkflowLlmNode> {
    let activity = binding.read(cx).activity().clone();
    let instance = binding.read(cx).snapshot().instance().clone();
    let (revision, data, can_restore) = match read_llm_binding(&binding, cx) {
        Ok((revision, data)) => (revision, data, true),
        Err(_) => (
            0,
            LlmData {
                value: serde_json::Value::Null,
                title: "LLM".to_owned(),
                prompt: String::new(),
                model: DEFAULT_MODEL.to_owned(),
            },
            false,
        ),
    };
    let (prompt, prompt_recovery) = if can_restore {
        let restored = draft_store.restore(
            &instance,
            WORKFLOW_PROMPT_FIELD,
            revision,
            data.prompt.clone(),
        );
        let recovery = restored.recovery();
        (restored.into_draft(), recovery)
    } else {
        (
            RevisionedDraft::new(revision, data.prompt.clone()),
            DraftRecovery::default(),
        )
    };
    let (model, model_recovery) = if can_restore {
        let restored = draft_store.restore(
            &instance,
            WORKFLOW_MODEL_FIELD,
            revision,
            data.model.clone(),
        );
        let recovery = restored.recovery();
        (restored.into_draft(), recovery)
    } else {
        (
            RevisionedDraft::new(revision, data.model.clone()),
            DraftRecovery::default(),
        )
    };
    let recoveries = vec![
        (WORKFLOW_PROMPT_FIELD, prompt_recovery),
        (WORKFLOW_MODEL_FIELD, model_recovery),
    ];
    let controller = cx.new(|cx| TextInputController::with_value(prompt.local().clone(), cx));
    cx.new(|cx| {
        let model_options_subscription = cx.observe(&model_options, |_, _, cx| cx.notify());
        WorkflowLlmNode {
            binding,
            activity,
            draft_store,
            model_options,
            _model_options_subscription: model_options_subscription,
            prompt_controller: controller,
            prompt,
            model,
            model_menu_base: None,
            model_menu_open: Rc::new(Cell::new(false)),
            model_overlay_lease: None,
            focus_lease: None,
            ime_lease: None,
            draft_lease: None,
            draft_capacity_overflowed: false,
            _prompt_command_task: None,
            _model_command_task: None,
            _prompt_command_lease: None,
            _model_command_lease: None,
            diagnostic: recovery_diagnostic(&recoveries),
        }
    })
}

#[must_use]
pub fn workflow_llm_node_id() -> NodeId {
    NodeId::from_u128(WORKFLOW_LLM_NODE_ID)
}

#[must_use]
pub fn workflow_prompt_selector(node: NodeId) -> String {
    format!("text-input:{}:root", workflow_prompt_id(node))
}

#[must_use]
pub fn workflow_model_selector(node: NodeId) -> String {
    format!("select:{}:trigger", workflow_model_id(node))
}

#[must_use]
pub fn workflow_model_option_selector(node: NodeId, model: &str) -> String {
    format!("listbox:{}-listbox:option:{model}", workflow_model_id(node))
}

#[must_use]
pub fn workflow_model_options_loading_selector(node: NodeId) -> String {
    format!("jellyflow-workflow-model-options-loading-{}", node.0)
}

#[must_use]
pub fn workflow_model_options_empty_selector(node: NodeId) -> String {
    format!("jellyflow-workflow-model-options-empty-{}", node.0)
}

#[must_use]
pub fn workflow_model_options_failed_selector(node: NodeId) -> String {
    format!("jellyflow-workflow-model-options-failed-{}", node.0)
}

#[must_use]
pub fn workflow_model_options_partial_selector(node: NodeId) -> String {
    format!("jellyflow-workflow-model-options-partial-{}", node.0)
}

#[must_use]
pub fn workflow_model_options_retry_selector(node: NodeId) -> String {
    format!("jellyflow-workflow-model-options-retry-{}", node.0)
}

#[must_use]
pub fn workflow_model_conflict_selector(node: NodeId) -> String {
    format!("jellyflow-workflow-model-conflict-{}", node.0)
}

#[must_use]
pub fn workflow_model_accept_remote_selector(node: NodeId) -> String {
    format!("jellyflow-workflow-model-accept-control-{}", node.0)
}

#[must_use]
pub fn workflow_model_reapply_selector(node: NodeId) -> String {
    format!("jellyflow-workflow-model-reapply-control-{}", node.0)
}

#[must_use]
pub fn workflow_prompt_conflict_selector(node: NodeId) -> String {
    format!("jellyflow-workflow-prompt-conflict-{}", node.0)
}

#[must_use]
pub fn workflow_prompt_apply_selector(node: NodeId) -> String {
    format!("jellyflow-workflow-prompt-apply-control-{}", node.0)
}

#[must_use]
pub fn workflow_prompt_accept_remote_selector(node: NodeId) -> String {
    format!("jellyflow-workflow-prompt-accept-control-{}", node.0)
}

#[must_use]
pub fn workflow_prompt_reapply_selector(node: NodeId) -> String {
    format!("jellyflow-workflow-prompt-reapply-control-{}", node.0)
}

fn workflow_prompt_id(node: NodeId) -> String {
    format!("jellyflow-workflow-prompt-{}", node.0)
}

fn workflow_model_id(node: NodeId) -> String {
    format!("jellyflow-workflow-model-{}", node.0)
}

fn submit_llm_draft(
    binding: &Entity<OpenGpuiNodeBinding>,
    node_id: NodeId,
    field: LlmField,
    draft: &mut RevisionedDraft<String>,
    cx: &mut App,
) -> Result<Option<PendingLlmCommand>, WorkflowNodeError> {
    let facts = binding.read(cx).snapshot().read().map_err(|error| {
        WorkflowNodeError::BindingUnavailable {
            message: error.to_string(),
        }
    })?;
    let current = LlmData::parse(node_id, facts.node())?;
    let remote = current.field(field).to_owned();
    draft.reconcile_remote(facts.graph_revision(), remote);
    let intent = match draft.plan_submission_at(facts.graph_revision()) {
        DraftSubmissionDecision::Submit(intent) => intent,
        DraftSubmissionDecision::Rejected(
            DraftIntentRejection::Clean | DraftIntentRejection::CompositionActive,
        ) => return Ok(None),
        DraftSubmissionDecision::Rejected(_) => {
            return Err(WorkflowNodeError::DraftConflict { field: field.key() });
        }
    };
    dispatch_llm_intent(facts, current, node_id, field, draft, intent, cx).map(Some)
}

fn reapply_llm_draft(
    binding: &Entity<OpenGpuiNodeBinding>,
    node_id: NodeId,
    field: LlmField,
    draft: &mut RevisionedDraft<String>,
    cx: &mut App,
) -> Result<Option<PendingLlmCommand>, WorkflowNodeError> {
    let facts = binding.read(cx).snapshot().read().map_err(|error| {
        WorkflowNodeError::BindingUnavailable {
            message: error.to_string(),
        }
    })?;
    let current = LlmData::parse(node_id, facts.node())?;
    let remote = current.field(field).to_owned();
    draft.reconcile_remote(facts.graph_revision(), remote);
    let intent = match draft.resolve_at(
        DraftConflictResolution::ReapplyLocal,
        facts.graph_revision(),
    ) {
        DraftResolutionDecision::Reapply(intent) => intent,
        _ => return Err(WorkflowNodeError::DraftConflict { field: field.key() }),
    };
    dispatch_llm_intent(facts, current, node_id, field, draft, intent, cx).map(Some)
}

fn dispatch_llm_intent(
    facts: OpenGpuiNodeBindingRead,
    current: LlmData,
    node_id: NodeId,
    field: LlmField,
    draft: &mut RevisionedDraft<String>,
    intent: DraftSubmissionIntent<String>,
    cx: &mut App,
) -> Result<PendingLlmCommand, WorkflowNodeError> {
    let attempt = intent.attempt();
    let expected_revision = intent.expected_revision();
    let committed_value = intent.value().clone();
    let next_data = match current.with_field(node_id, field, committed_value) {
        Ok(next_data) => next_data,
        Err(error) => {
            draft.reject_submission(attempt);
            return Err(error);
        }
    };
    let ticket = match facts.enqueue_command(cx, move |_| {
        GraphCommand::set_node_data(expected_revision, node_id, next_data)
            .with_label(format!("edit workflow {}", field.key()))
    }) {
        Ok(ticket) => ticket,
        Err(error) => {
            draft.reject_submission(attempt);
            return Err(WorkflowNodeError::BindingUnavailable {
                message: error.to_string(),
            });
        }
    };
    Ok(PendingLlmCommand { attempt, ticket })
}

fn finish_llm_ticket(
    draft: &mut RevisionedDraft<String>,
    field: LlmField,
    attempt: DraftAttempt,
    result: OpenGpuiCommandTicketResult,
) -> Option<String> {
    match result {
        Ok(GraphCommandOutcome::Applied(receipt)) => {
            match draft.record_self_receipt(attempt, receipt.revision()) {
                DraftReceiptDecision::Recorded { .. }
                | DraftReceiptDecision::Acknowledged { .. }
                | DraftReceiptDecision::AlreadyRecorded { .. } => None,
                decision => Some(format!(
                    "workflow {} receipt needs a fresh binding snapshot: {decision:?}",
                    field.key()
                )),
            }
        }
        Ok(GraphCommandOutcome::Noop(_)) => {
            draft.reject_submission(attempt);
            (draft.phase() != DraftPhase::Clean).then(|| {
                format!(
                    "workflow {} command made no graph change; review the fresh value before retrying",
                    field.key()
                )
            })
        }
        Ok(GraphCommandOutcome::Conflict(conflict)) => {
            draft.reject_submission(attempt);
            Some(format!(
                "workflow {} changed at revision {}; refresh, then reapply explicitly",
                field.key(),
                conflict.current().revision()
            ))
        }
        Ok(GraphCommandOutcome::Rejected(rejection)) => {
            draft.reject_submission(attempt);
            Some(format!(
                "workflow {} command was rejected: {}",
                field.key(),
                rejection.message()
            ))
        }
        Err(error) => {
            draft.reject_submission(attempt);
            Some(format!(
                "workflow {} command result is uncertain: {error}; refresh before retrying",
                field.key()
            ))
        }
    }
}

fn read_llm_binding(
    binding: &Entity<OpenGpuiNodeBinding>,
    cx: &App,
) -> Result<(GraphRevision, LlmData), WorkflowNodeError> {
    let facts = binding.read(cx).snapshot().read().map_err(|error| {
        WorkflowNodeError::BindingUnavailable {
            message: error.to_string(),
        }
    })?;
    let node_id = binding.read(cx).snapshot().node_id();
    Ok((
        facts.graph_revision(),
        LlmData::parse(node_id, facts.node())?,
    ))
}

fn bound_ports(binding: &Entity<OpenGpuiNodeBinding>, cx: &App) -> Vec<WorkflowPort> {
    let binding = binding.read(cx);
    binding
        .snapshot()
        .read()
        .and_then(|facts| {
            Ok(facts
                .ports()
                .zip(binding.port_handles(&facts)?)
                .map(|(port, handle)| WorkflowPort {
                    id: port.id(),
                    direction: port.port().dir,
                    label: port.port().key.0.clone(),
                    handle,
                })
                .collect::<Vec<_>>())
        })
        .unwrap_or_default()
}

impl LlmData {
    fn parse(node_id: NodeId, node: &Node) -> Result<Self, WorkflowNodeError> {
        ensure_kind(node_id, node)?;
        let object = node
            .data
            .as_object()
            .ok_or(WorkflowNodeError::InvalidData {
                node: node_id,
                field: "object",
            })?;
        let title = object
            .get("title")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("LLM")
            .to_owned();
        let prompt = object
            .get("prompt")
            .and_then(serde_json::Value::as_str)
            .ok_or(WorkflowNodeError::InvalidData {
                node: node_id,
                field: "prompt",
            })?
            .to_owned();
        let model = object
            .get("model")
            .and_then(serde_json::Value::as_str)
            .ok_or(WorkflowNodeError::InvalidData {
                node: node_id,
                field: "model",
            })?
            .to_owned();
        Ok(Self {
            value: node.data.clone(),
            title,
            prompt,
            model,
        })
    }

    fn field(&self, field: LlmField) -> &str {
        match field {
            LlmField::Prompt => &self.prompt,
            LlmField::Model => &self.model,
        }
    }

    fn with_field(
        mut self,
        node_id: NodeId,
        field: LlmField,
        value: String,
    ) -> Result<serde_json::Value, WorkflowNodeError> {
        let object = self
            .value
            .as_object_mut()
            .ok_or(WorkflowNodeError::InvalidData {
                node: node_id,
                field: "object",
            })?;
        object.insert(field.key().to_owned(), serde_json::Value::String(value));
        Ok(self.value)
    }
}

fn ensure_kind(node_id: NodeId, node: &Node) -> Result<(), WorkflowNodeError> {
    if node.kind.0 == WORKFLOW_LLM_KIND {
        Ok(())
    } else {
        Err(WorkflowNodeError::WrongNodeKind {
            node: node_id,
            expected: WORKFLOW_LLM_KIND,
            actual: node.kind.0.clone(),
        })
    }
}

fn reconcile_prompt_composition(draft: &mut RevisionedDraft<String>, composing: bool) {
    if composing {
        draft.mark_composing();
    } else if matches!(
        draft.phase(),
        DraftPhase::Composing | DraftPhase::ComposingConflict
    ) {
        // The native text controller clears its marked range during IME commit without emitting a
        // second application-level change callback. Reconcile that edge on render.
        draft.unmark_composing();
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use jellyflow::core::{CanvasPoint, CanvasRect, GraphBuilder, GraphId, NodeKindKey, PortId};
    use jellyflow::runtime::{GraphRuntime, ViewSessionOptions};
    use jellyflow_open_gpui::{
        OPEN_GPUI_DEFAULT_ELIGIBLE_PARKED_CAPACITY, OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY,
        OpenGpuiNodeRegistry, OpenGpuiSceneHost,
    };
    use open_gpui::{AppContext, EntityInputHandler};
    use open_gpui_ui_components::gpui_adapter::init_text_input;

    use super::*;

    const PROTECTOR_KIND: &str = "test.workflow-transient-protector";
    const PROTECTOR_RENDERER: &str = "test.native.workflow-transient-protector";
    const TEST_NODE_SPACING: f32 = 240.0;

    struct ProtectorNode {
        _lease: Option<OpenGpuiActivityLease>,
    }

    impl Render for ProtectorNode {
        fn render(
            &mut self,
            _window: &mut Window,
            _cx: &mut Context<Self>,
        ) -> impl open_gpui::IntoElement {
            div().size_full().child("protected workflow state")
        }
    }

    #[test]
    fn ime_commit_without_a_second_change_callback_returns_prompt_to_dirty() {
        let mut draft = RevisionedDraft::new(0, String::from("before"));
        draft.edit_local(String::from("composing"));

        reconcile_prompt_composition(&mut draft, true);
        assert_eq!(draft.phase(), DraftPhase::Composing);

        reconcile_prompt_composition(&mut draft, false);
        assert_eq!(draft.phase(), DraftPhase::Dirty);
    }

    #[test]
    fn ime_commit_preserves_a_remote_conflict_for_explicit_resolution() {
        let mut draft = RevisionedDraft::new(0, String::from("before"));
        draft.edit_local(String::from("local"));
        reconcile_prompt_composition(&mut draft, true);
        draft.reconcile_remote(1, String::from("remote"));
        assert_eq!(draft.phase(), DraftPhase::ComposingConflict);

        reconcile_prompt_composition(&mut draft, false);
        assert_eq!(draft.phase(), DraftPhase::Conflict);
    }

    #[test]
    fn dropped_host_never_acknowledges_a_pending_prompt_draft() {
        let mut draft = RevisionedDraft::new(7, String::from("remote"));
        draft.edit_local(String::from("local"));
        let intent = match draft.plan_submission_at(7) {
            DraftSubmissionDecision::Submit(intent) => intent,
            decision => panic!("expected pending prompt intent, got {decision:?}"),
        };

        let diagnostic = finish_llm_ticket(
            &mut draft,
            LlmField::Prompt,
            intent.attempt(),
            Err(jellyflow_open_gpui::OpenGpuiCommandTicketError::HostDropped),
        )
        .expect("host drop remains visible to the user");

        assert!(diagnostic.contains("uncertain"));
        assert_eq!(draft.pending_attempt(), None);
        assert_eq!(draft.phase(), DraftPhase::Dirty);
        assert_eq!(draft.local(), "local");
        assert_eq!(draft.remote(), "remote");
    }

    #[test]
    fn applied_receipt_waits_for_the_authoritative_binding_snapshot() {
        let mut draft = RevisionedDraft::new(0, String::from("before"));
        draft.edit_local(String::from("local"));
        draft.reconcile_remote(1, String::from("remote"));
        let intent = match draft.resolve_at(DraftConflictResolution::ReapplyLocal, 1) {
            DraftResolutionDecision::Reapply(intent) => intent,
            decision => panic!("expected a reapply intent, got {decision:?}"),
        };

        assert!(matches!(
            draft.record_self_receipt(intent.attempt(), 2),
            jellyflow_open_gpui::DraftReceiptDecision::Recorded { .. }
        ));
        assert_eq!(draft.phase(), DraftPhase::Conflict);
        assert_eq!(draft.pending_attempt(), Some(intent.attempt()));
        assert_eq!(draft.remote(), "remote");

        assert!(matches!(
            draft.reconcile_remote(2, String::from("local")),
            jellyflow_open_gpui::DraftReconcileDecision::SelfAcknowledged { .. }
        ));
        assert_eq!(draft.phase(), DraftPhase::Clean);
        assert_eq!(draft.pending_attempt(), None);
        assert_eq!(draft.remote(), "local");
    }

    #[open_gpui::test]
    fn rejected_workflow_ime_is_canceled_and_its_draft_survives_lru_churn(
        cx: &mut open_gpui::TestAppContext,
    ) {
        cx.update(init_text_input);
        let catalog = transient_overflow_catalog();
        let runtime = GraphRuntime::builder(catalog.clone())
            .build(transient_overflow_graph(&catalog))
            .expect("valid workflow transient overflow graph");
        let view = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("attach workflow transient overflow view");
        let draft_store = WorkflowDraftStore::new();
        let model_options = cx.update(|cx| cx.new(|_| WorkflowModelOptionsState::ready_default()));
        let captured = Rc::new(RefCell::new(None::<Entity<WorkflowLlmNode>>));
        let mounts = Rc::new(Cell::new(0_u32));
        let mut registry = OpenGpuiNodeRegistry::builder();
        let captured_for_factory = Rc::clone(&captured);
        let mounts_for_factory = Rc::clone(&mounts);
        let factory_store = draft_store.clone();
        registry.register(WORKFLOW_LLM_RENDERER, move |binding, _window, cx| {
            let node =
                mount_workflow_llm(binding, factory_store.clone(), model_options.clone(), cx);
            captured_for_factory.borrow_mut().replace(node.clone());
            mounts_for_factory.set(mounts_for_factory.get().saturating_add(1));
            node
        });
        registry.register(PROTECTOR_RENDERER, |binding, _window, cx| {
            let node = binding.read(cx).snapshot().node_id();
            let lease = is_transient_protector(node).then(|| {
                binding
                    .read(cx)
                    .activity()
                    .acquire(OpenGpuiActivityKind::Draft)
                    .expect("the first 64 instances fit the protected budget")
            });
            cx.new(|_| ProtectorNode { _lease: lease })
        });
        let (host, cx) = cx.add_window_view(move |_, _| {
            OpenGpuiSceneHost::new(
                view,
                registry
                    .seal()
                    .expect("valid workflow transient overflow registry"),
                transient_initial_bounds(),
            )
        });
        draw_transient_host(cx);
        cx.read_entity(&host, |host, _| {
            assert_eq!(
                host.stats().protected_instances(),
                OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY
            );
        });

        let node = captured
            .borrow()
            .clone()
            .expect("mounted overflow workflow LLM");
        let controller = node.read_with(cx, |node, _| node.prompt_controller.clone());
        cx.update(|window, cx| {
            let focus = controller.read(cx).focus_handle(cx);
            focus.focus(window, cx);
            controller.update(cx, |controller, cx| {
                let full_range = 0..controller.value().encode_utf16().count();
                EntityInputHandler::replace_and_mark_text_in_range(
                    controller,
                    Some(full_range),
                    "partial composition",
                    Some(0..0),
                    window,
                    cx,
                );
            });
        });
        draw_transient_host(cx);

        cx.read_entity(&node, |node, _| {
            assert_eq!(node.prompt.local(), "partial composition");
            assert_eq!(node.prompt.phase(), DraftPhase::Dirty);
            assert!(node.focus_lease.is_none());
            assert!(node.ime_lease.is_none());
            assert!(node.draft_lease.is_none());
            assert_eq!(node.activity.protected_count(), 0);
        });
        cx.read_entity(&controller, |controller, _| {
            assert_eq!(controller.value(), "partial composition");
            assert!(controller.marked_range_utf16().is_none());
        });
        cx.update(|window, cx| assert!(window.focused(cx).is_none()));
        assert_eq!(draft_store.len(), 1);

        cx.update_entity(&host, |host, cx| {
            host.set_frame_bounds(transient_churn_bounds(0));
            cx.notify();
        });
        draw_transient_host(cx);
        cx.read_entity(&host, |host, _| {
            assert!(host.stats().eligible_parked_nodes() >= 1);
        });
        let weak_node = node.downgrade();
        let weak_controller = controller.downgrade();
        captured.borrow_mut().take();
        drop(controller);
        drop(node);

        for index in 1..=OPEN_GPUI_DEFAULT_ELIGIBLE_PARKED_CAPACITY {
            cx.update_entity(&host, |host, cx| {
                host.set_frame_bounds(transient_churn_bounds(index));
                cx.notify();
            });
            draw_transient_host(cx);
        }
        cx.read_entity(&host, |host, _| {
            assert!(host.stats().evictions() >= 1);
            assert_eq!(
                host.stats().protected_instances(),
                OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY
            );
            assert!(
                host.stats().eligible_parked_nodes() <= OPEN_GPUI_DEFAULT_ELIGIBLE_PARKED_CAPACITY
            );
        });
        assert!(weak_node.upgrade().is_none());
        assert!(weak_controller.upgrade().is_none());

        cx.update_entity(&host, |host, cx| {
            host.set_frame_bounds(transient_initial_bounds());
            cx.notify();
        });
        draw_transient_host(cx);
        assert_eq!(mounts.get(), 2);
        let restored = captured
            .borrow()
            .clone()
            .expect("remounted overflow workflow LLM");
        let restored_controller = restored.read_with(cx, |node, _| node.prompt_controller.clone());
        cx.read_entity(&restored, |node, _| {
            assert_eq!(node.prompt.local(), "partial composition");
            assert_eq!(node.prompt.phase(), DraftPhase::Dirty);
            assert!(node.focus_lease.is_none());
            assert!(node.ime_lease.is_none());
            assert_eq!(node.activity.protected_count(), 0);
        });
        cx.read_entity(&restored_controller, |controller, _| {
            assert_eq!(controller.value(), "partial composition");
            assert!(controller.marked_range_utf16().is_none());
        });
        cx.update(|window, cx| assert!(window.focused(cx).is_none()));
    }

    fn transient_overflow_catalog() -> jellyflow::runtime::schema::NodeCatalog {
        let mut catalog = jellyflow::runtime::schema::NodeCatalog::new();
        catalog
            .register(catalog_spec())
            .expect("register workflow LLM kind");
        catalog
            .register(
                NodeKindSpec::builder(
                    PROTECTOR_KIND,
                    "Workflow transient protector",
                    PROTECTOR_RENDERER,
                )
                .default_size(CanvasSize {
                    width: 120.0,
                    height: 80.0,
                })
                .build(),
            )
            .expect("register workflow transient protector kind");
        catalog
    }

    fn transient_overflow_graph(
        catalog: &jellyflow::runtime::schema::NodeCatalog,
    ) -> jellyflow::core::Graph {
        let mut graph = GraphBuilder::new(GraphId::from_u128(0x7a11_0000));
        for index in 0..OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY {
            graph.insert_node(
                transient_protector_id(index),
                transient_test_node(CanvasPoint::default()),
            );
        }
        let creation = catalog
            .instantiate_node_with_ids(
                &NodeKindKey::new(WORKFLOW_LLM_KIND),
                transient_llm_id(),
                CanvasPoint::default(),
                [
                    PortId::from_u128(0x7a11_2101),
                    PortId::from_u128(0x7a11_2102),
                ],
            )
            .expect("instantiate transient overflow workflow LLM");
        let (node_id, node, ports) = creation.into_parts();
        graph.insert_node(node_id, node);
        for (port_id, port) in ports {
            graph.insert_port(port_id, port);
        }
        for index in 0..=OPEN_GPUI_DEFAULT_ELIGIBLE_PARKED_CAPACITY {
            graph.insert_node(
                transient_churn_id(index),
                transient_test_node(CanvasPoint {
                    x: transient_churn_x(index),
                    y: 0.0,
                }),
            );
        }
        graph.build().expect("workflow transient overflow graph")
    }

    fn transient_test_node(pos: CanvasPoint) -> Node {
        Node {
            kind: NodeKindKey::new(PROTECTOR_KIND),
            kind_version: 1,
            pos,
            origin: None,
            selectable: None,
            draggable: None,
            connectable: None,
            deletable: None,
            parent: None,
            extent: None,
            expand_parent: None,
            size: Some(CanvasSize {
                width: 120.0,
                height: 80.0,
            }),
            hidden: false,
            collapsed: false,
            ports: Vec::new(),
            data: serde_json::Value::Null,
        }
    }

    fn transient_protector_id(index: usize) -> NodeId {
        NodeId::from_u128(0x7a11_1000 + index as u128)
    }

    fn transient_llm_id() -> NodeId {
        NodeId::from_u128(0x7a11_2000)
    }

    fn transient_churn_id(index: usize) -> NodeId {
        NodeId::from_u128(0x7a11_3000 + index as u128)
    }

    fn is_transient_protector(node: NodeId) -> bool {
        (0..OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY).any(|index| transient_protector_id(index) == node)
    }

    fn transient_churn_x(index: usize) -> f32 {
        1_000.0 + index as f32 * TEST_NODE_SPACING
    }

    fn transient_initial_bounds() -> CanvasRect {
        CanvasRect {
            origin: CanvasPoint { x: -10.0, y: -10.0 },
            size: CanvasSize {
                width: 380.0,
                height: 390.0,
            },
        }
    }

    fn transient_churn_bounds(index: usize) -> CanvasRect {
        CanvasRect {
            origin: CanvasPoint {
                x: transient_churn_x(index) - 10.0,
                y: -10.0,
            },
            size: CanvasSize {
                width: 160.0,
                height: 120.0,
            },
        }
    }

    fn draw_transient_host(cx: &mut open_gpui::VisualTestContext) {
        cx.update(|window, cx| {
            window.draw(cx).clear();
        });
        cx.run_until_parked();
    }
}
