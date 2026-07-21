use std::cell::Cell;
use std::rc::Rc;

use jellyflow_core::{CanvasSize, Node, NodeId, PortCapacity, PortDirection, PortKind};
use jellyflow_open_gpui::authoring::OpenGpuiNodeBinding;
use jellyflow_open_gpui::{
    OpenGpuiActivityKind, OpenGpuiActivityLease, OpenGpuiCommandTicketResult,
    OpenGpuiNodeActivityRegistry, OpenGpuiNodeRegistryBuilder, RetainedField, RetainedFieldAttempt,
    RetainedFieldConflictChoice, RetainedFieldError, RetainedFieldKey, RetainedFieldPhase,
    RetainedFieldRecovery, RetainedFieldResolution, RetainedFieldStore, RetainedFieldSubmitMode,
    RetainedFieldUpdate, RevisionedField,
};
use jellyflow_runtime::schema::{NodeKindSpec, NodePortSpec};
use jellyflow_runtime::{GraphCommand, GraphRevision};
use open_gpui::{
    App, AppContext, Context, Entity, Focusable, InteractiveElement, ParentElement, Render, Styled,
    Subscription, Window, div, px, rgb,
};
use open_gpui_ui_components::{
    Button, ButtonVariant, ListboxOption, Select, TextInput, gpui_adapter::TextInputController,
};

use crate::model_options::{
    WorkflowModelOptionsCompleteness, WorkflowModelOptionsState, WorkflowModelOptionsStatus,
};

use super::super::sync_activity_lease;
use super::{WorkflowNodeError, WorkflowNodeStatus, WorkflowPort, workflow_alert, workflow_header};

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

pub type WorkflowFieldStore = RetainedFieldStore<String>;

pub struct WorkflowLlmNode {
    binding: Entity<OpenGpuiNodeBinding>,
    activity: OpenGpuiNodeActivityRegistry,
    model_options: Entity<WorkflowModelOptionsState>,
    _model_options_subscription: Subscription,
    prompt_controller: Entity<TextInputController>,
    prompt: RetainedField<String>,
    model: RetainedField<String>,
    model_menu_base: Option<(GraphRevision, String)>,
    model_menu_open: Rc<Cell<bool>>,
    model_overlay_lease: Option<OpenGpuiActivityLease>,
    focus_lease: Option<OpenGpuiActivityLease>,
    ime_lease: Option<OpenGpuiActivityLease>,
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

fn recovery_diagnostic(recoveries: &[(&'static str, RetainedFieldRecovery)]) -> Option<String> {
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
    fn reconcile_graph(&mut self, cx: &mut Context<Self>) -> Option<(LlmData, Vec<WorkflowPort>)> {
        let result = read_llm_render_binding(&self.binding, cx);
        match result {
            Ok((revision, data, ports)) => {
                if matches!(
                    self.prompt
                        .field_mut()
                        .observe(revision, data.prompt.clone()),
                    RetainedFieldUpdate::RemoteApplied
                ) {
                    let value = self.prompt.field().local().clone();
                    self.prompt_controller
                        .update(cx, |controller, cx| controller.set_value(value, cx));
                }
                let _ = observe_model_snapshot(
                    self.model.field_mut(),
                    self.model_menu_open.get(),
                    revision,
                    data.model.clone(),
                );
                Some((data, ports))
            }
            Err(error) => {
                self.diagnostic = Some(error.to_string());
                None
            }
        }
    }

    fn edit_prompt(&mut self, value: String, cx: &mut Context<Self>) {
        self.prompt.field_mut().edit(value);
        let composing = self
            .prompt_controller
            .read(cx)
            .marked_range_utf16()
            .is_some();
        self.prompt.field_mut().set_composing(composing);
        cx.notify();
    }

    fn apply_prompt(&mut self, cx: &mut Context<Self>) {
        self.submit_field(LlmField::Prompt, RetainedFieldSubmitMode::Commit, cx);
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
        if let Some((revision, model)) = self.model_menu_base.take() {
            self.model.field_mut().observe(revision, model);
        }
        self.model.field_mut().edit(value);
        self.submit_field(LlmField::Model, RetainedFieldSubmitMode::Commit, cx);
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
        if !self.model_options.read(cx).status().is_ready()
            || self.model.field().conflict().is_some()
            || self.model.field().attempt_state().blocks_submission()
        {
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
        self.model.field_mut().observe(revision, data.model.clone());
        if self.model.field().conflict().is_some()
            || self.model.field().attempt_state().blocks_submission()
        {
            self.model_menu_base.take();
            self.model_menu_open.set(false);
            cx.notify();
            return;
        }
        let revoked_open = self.model_menu_open.clone();
        match self
            .activity
            .acquire_with_revocation(OpenGpuiActivityKind::Overlay, move || {
                revoked_open.set(false)
            }) {
            Ok(lease) => {
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
            self.model.field_mut().observe(revision, data.model);
            match self
                .model
                .field_mut()
                .resolve(RetainedFieldConflictChoice::AcceptRemote, revision)
            {
                Ok(RetainedFieldResolution::AcceptedRemote { .. }) => Ok(()),
                Ok(_) => Err(WorkflowNodeError::DraftConflict { field: "model" }),
                Err(error) => Err(Self::field_error(LlmField::Model, error)),
            }
        });
        self.diagnostic = result.err().map(|error| error.to_string());
        cx.notify();
    }

    fn reapply_local_model(&mut self, cx: &mut Context<Self>) {
        self.submit_field(LlmField::Model, RetainedFieldSubmitMode::Reapply, cx);
    }

    fn accept_remote_prompt(&mut self, cx: &mut Context<Self>) {
        let result = read_llm_binding(&self.binding, cx).and_then(|(revision, data)| {
            self.prompt.field_mut().observe(revision, data.prompt);
            match self
                .prompt
                .field_mut()
                .resolve(RetainedFieldConflictChoice::AcceptRemote, revision)
            {
                Ok(RetainedFieldResolution::AcceptedRemote { .. }) => Ok(()),
                Ok(_) => Err(WorkflowNodeError::DraftConflict { field: "prompt" }),
                Err(error) => Err(Self::field_error(LlmField::Prompt, error)),
            }
        });
        if result.is_ok() {
            let value = self.prompt.field().local().clone();
            self.prompt_controller
                .update(cx, |controller, cx| controller.set_value(value, cx));
        }
        self.diagnostic = result.err().map(|error| error.to_string());
        cx.notify();
    }

    fn reapply_local_prompt(&mut self, cx: &mut Context<Self>) {
        self.submit_field(LlmField::Prompt, RetainedFieldSubmitMode::Reapply, cx);
    }

    fn submit_field(
        &mut self,
        field: LlmField,
        mode: RetainedFieldSubmitMode,
        cx: &mut Context<Self>,
    ) {
        if let Err(error) = self.enqueue_field(field, mode, cx) {
            self.diagnostic = Some(error.to_string());
        }
        cx.notify();
    }

    fn enqueue_field(
        &mut self,
        field: LlmField,
        mode: RetainedFieldSubmitMode,
        cx: &mut Context<Self>,
    ) -> Result<(), WorkflowNodeError> {
        let authoring = self.binding.read(cx).authoring().map_err(|error| {
            WorkflowNodeError::BindingUnavailable {
                message: error.to_string(),
            }
        })?;
        let node_id = authoring.node_id();
        let current = LlmData::parse(node_id, authoring.node())?;
        let revision = authoring.revision();
        self.field_mut(field)
            .field_mut()
            .observe(revision, current.field(field).to_owned());
        let intent = match mode {
            RetainedFieldSubmitMode::Commit => {
                match self.field_mut(field).field_mut().plan(revision) {
                    Ok(intent) => intent,
                    Err(RetainedFieldError::Clean | RetainedFieldError::CompositionActive) => {
                        return Ok(());
                    }
                    Err(error) => return Err(Self::field_error(field, error)),
                }
            }
            RetainedFieldSubmitMode::Reapply => {
                match self
                    .field_mut(field)
                    .field_mut()
                    .resolve(RetainedFieldConflictChoice::ReapplyLocal, revision)
                {
                    Ok(RetainedFieldResolution::Reapply(intent)) => intent,
                    Ok(_) => return Err(WorkflowNodeError::DraftConflict { field: field.key() }),
                    Err(error) => return Err(Self::field_error(field, error)),
                }
            }
        };
        let attempt = intent.attempt();
        let next_data = match current.with_field(node_id, field, intent.value().clone()) {
            Ok(next_data) => next_data,
            Err(error) => {
                let _ = self.field_mut(field).not_dispatched(attempt);
                return Err(error);
            }
        };
        let command = GraphCommand::set_node_data(revision, node_id, next_data)
            .with_label(format!("edit workflow {}", field.key()));
        if let Err(error) = self.field_mut(field).prepare_task(attempt) {
            let _ = self.field_mut(field).not_dispatched(attempt);
            return Err(Self::field_error(field, error));
        }
        let ticket = match authoring.enqueue(command, cx) {
            Ok(ticket) => ticket,
            Err(error) => {
                let _ = self.field_mut(field).not_dispatched(attempt);
                return Err(WorkflowNodeError::BindingUnavailable {
                    message: error.to_string(),
                });
            }
        };
        self.diagnostic = None;
        let task = cx.spawn(async move |node, cx| {
            let result = ticket.await;
            let _ = node.update(cx, |node, cx| {
                node.finish_field_command(field, attempt, result, cx);
            });
        });
        if let Err(error) = self.field_mut(field).attach_task(attempt, task) {
            let _ = self.field_mut(field).field_mut().mark_uncertain(
                attempt,
                format!(
                    "workflow {} command was dispatched but its task detached: {error}",
                    field.key()
                ),
            );
            return Err(Self::field_error(field, error));
        }
        Ok(())
    }

    fn finish_field_command(
        &mut self,
        field: LlmField,
        attempt: RetainedFieldAttempt,
        result: OpenGpuiCommandTicketResult,
        cx: &mut Context<Self>,
    ) {
        if let Err(error) = self.field_mut(field).settle_ticket(attempt, result) {
            self.diagnostic = Some(Self::field_error(field, error).to_string());
        }
        cx.notify();
    }

    fn field_mut(&mut self, field: LlmField) -> &mut RetainedField<String> {
        match field {
            LlmField::Prompt => &mut self.prompt,
            LlmField::Model => &mut self.model,
        }
    }

    fn field_error(field: LlmField, error: RetainedFieldError) -> WorkflowNodeError {
        match error {
            RetainedFieldError::ConflictRequiresResolution | RetainedFieldError::NoConflict => {
                WorkflowNodeError::DraftConflict { field: field.key() }
            }
            error => WorkflowNodeError::CommandRejected {
                message: format!("workflow {} field: {error}", field.key()),
            },
        }
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

        if let Err(error) = self.prompt.sync_retention() {
            self.diagnostic = Some(format!("workflow prompt retention failed: {error}"));
        }
        if let Err(error) = self.model.sync_retention() {
            self.diagnostic = Some(format!("workflow model retention failed: {error}"));
        }
    }

    fn reject_transient_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let value = self.prompt_controller.read(cx).value().to_owned();
        self.prompt.field_mut().edit(value.clone());
        self.prompt.field_mut().set_composing(false);
        self.prompt_controller
            .update(cx, |controller, cx| controller.set_value(value, cx));
        self.focus_lease.take();
        self.ime_lease.take();
        window.blur();
        cx.notify();
    }

    fn sync_prompt_composition(&mut self, cx: &mut Context<Self>) {
        let composing = self
            .prompt_controller
            .read(cx)
            .marked_range_utf16()
            .is_some();
        reconcile_prompt_composition(self.prompt.field_mut(), composing);
    }

    fn status(&self) -> WorkflowNodeStatus {
        let phases = [self.prompt.field().phase(), self.model.field().phase()];
        if phases.iter().any(|phase| {
            matches!(
                phase,
                RetainedFieldPhase::Conflict | RetainedFieldPhase::ComposingConflict
            )
        }) {
            WorkflowNodeStatus::Conflict
        } else if self.diagnostic.is_some()
            || self.prompt.field().notice().is_some()
            || self.model.field().notice().is_some()
        {
            WorkflowNodeStatus::Error
        } else if phases
            .iter()
            .any(|phase| *phase != RetainedFieldPhase::Clean)
        {
            WorkflowNodeStatus::Draft
        } else {
            WorkflowNodeStatus::Ready
        }
    }
}

fn observe_model_snapshot(
    field: &mut RevisionedField<String>,
    menu_open: bool,
    revision: GraphRevision,
    authoritative: String,
) -> Option<RetainedFieldUpdate> {
    // A selection is authored against the value visible when the menu opened. Keeping that base
    // stable turns a concurrent provider update into an explicit conflict instead of overwriting it.
    (!menu_open).then(|| field.observe(revision, authoritative))
}

impl Render for WorkflowLlmNode {
    fn render(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        let node_id = self.prompt.instance().node();
        let drag_handle = self.binding.read(cx).drag_handle();
        let (data, ports) = self.reconcile_graph(cx).unwrap_or_else(|| {
            (
                LlmData {
                    value: serde_json::Value::Null,
                    title: "Retired LLM".to_owned(),
                    prompt: self.prompt.field().local().clone(),
                    model: self.model.field().local().clone(),
                },
                Vec::new(),
            )
        });
        self.sync_prompt_composition(cx);
        self.sync_activity(window, cx);
        let status = self.status();
        let prompt_node = cx.weak_entity();
        let apply_node = cx.weak_entity();
        let model_open_node = cx.weak_entity();
        let model_node = cx.weak_entity();
        let prompt_id = workflow_prompt_id(node_id);
        let select_id = workflow_model_id(node_id);
        let model_options_status = self.model_options.read(cx).status().clone();
        let model_options_notice = ModelOptionsNotice::from_status(&model_options_status);
        let model_select_disabled = self.model.field().conflict().is_some()
            || self.model.field().attempt_state().blocks_submission()
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
                    .value(self.prompt.field().local().clone())
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
                        .disabled(!self.prompt.field().can_submit())
                        .on_click(move |_, window, cx| {
                            let _ = apply_node.update(cx, |node, cx| node.apply_prompt(cx));
                            window.refresh();
                        }),
                    ),
            )
            .child(
                Select::new(select_id, "Model")
                    .options(model_options)
                    .placeholder(self.model.field().local().clone())
                    .selected(self.model.field().local().clone())
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
        if let Some(message) = self.prompt.field().notice() {
            control_body = control_body.child(
                workflow_alert(
                    format!("jellyflow-workflow-prompt-command-notice-{}", node_id.0),
                    "Prompt command state",
                )
                .p(px(8.0))
                .rounded(px(4.0))
                .border_1()
                .border_color(rgb(0x725f42))
                .bg(rgb(0x302b22))
                .text_xs()
                .text_color(rgb(0xe6cf9e))
                .child(message.to_owned()),
            );
        }
        if let Some(message) = self.model.field().notice() {
            control_body = control_body.child(
                workflow_alert(
                    format!("jellyflow-workflow-model-command-notice-{}", node_id.0),
                    "Model command state",
                )
                .p(px(8.0))
                .rounded(px(4.0))
                .border_1()
                .border_color(rgb(0x725f42))
                .bg(rgb(0x302b22))
                .text_xs()
                .text_color(rgb(0xe6cf9e))
                .child(message.to_owned()),
            );
        }
        if self.prompt.field().conflict().is_some() {
            let accept_node = cx.weak_entity();
            let reapply_node = cx.weak_entity();
            let reapply_disabled = matches!(
                self.prompt.field().phase(),
                RetainedFieldPhase::ComposingConflict
            ) || self.prompt.field().attempt_state().blocks_submission();
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
        if self.model.field().conflict().is_some() {
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
                                    .disabled(
                                        self.model.field().attempt_state().blocks_submission(),
                                    )
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
        let controls = control_body;
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
            .child(drag_handle.wrap(workflow_header(node_id, data.title, "LLM", status, ports)))
            .child(controls)
            .children(diagnostic)
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
    field_store: WorkflowFieldStore,
    model_options: Entity<WorkflowModelOptionsState>,
) {
    registry.register(WORKFLOW_LLM_RENDERER, move |binding, _window, cx| {
        mount_workflow_llm(binding, field_store.clone(), model_options.clone(), cx)
    });
}

fn mount_workflow_llm(
    binding: Entity<OpenGpuiNodeBinding>,
    field_store: WorkflowFieldStore,
    model_options: Entity<WorkflowModelOptionsState>,
    cx: &mut App,
) -> Entity<WorkflowLlmNode> {
    let activity = binding.read(cx).activity().clone();
    let initial = binding
        .read(cx)
        .authoring()
        .expect("an LLM node factory must receive a live authoring binding");
    let revision = initial.revision();
    let data = LlmData::parse(initial.node_id(), initial.node()).unwrap_or_else(|_| LlmData {
        value: serde_json::Value::Null,
        title: "LLM".to_owned(),
        prompt: String::new(),
        model: DEFAULT_MODEL.to_owned(),
    });
    let prompt_key = RetainedFieldKey::new(WORKFLOW_PROMPT_FIELD)
        .expect("workflow prompt field key is a non-empty constant");
    let prompt = RetainedField::mount(
        binding.read(cx),
        prompt_key,
        field_store.clone(),
        revision,
        data.prompt.clone(),
    )
    .expect("workflow prompt field must mount while its binding is live");
    let model_key = RetainedFieldKey::new(WORKFLOW_MODEL_FIELD)
        .expect("workflow model field key is a non-empty constant");
    let model = RetainedField::mount(
        binding.read(cx),
        model_key,
        field_store,
        revision,
        data.model.clone(),
    )
    .expect("workflow model field must mount while its binding is live");
    let recoveries = vec![
        (WORKFLOW_PROMPT_FIELD, prompt.recovery()),
        (WORKFLOW_MODEL_FIELD, model.recovery()),
    ];
    let controller =
        cx.new(|cx| TextInputController::with_value(prompt.field().local().clone(), cx));
    cx.new(|cx| {
        let model_options_subscription = cx.observe(&model_options, |_, _, cx| cx.notify());
        WorkflowLlmNode {
            binding,
            activity,
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

fn read_llm_binding(
    binding: &Entity<OpenGpuiNodeBinding>,
    cx: &App,
) -> Result<(GraphRevision, LlmData), WorkflowNodeError> {
    let authoring =
        binding
            .read(cx)
            .authoring()
            .map_err(|error| WorkflowNodeError::BindingUnavailable {
                message: error.to_string(),
            })?;
    Ok((
        authoring.revision(),
        LlmData::parse(authoring.node_id(), authoring.node())?,
    ))
}

fn read_llm_render_binding(
    binding: &Entity<OpenGpuiNodeBinding>,
    cx: &App,
) -> Result<(GraphRevision, LlmData, Vec<WorkflowPort>), WorkflowNodeError> {
    let binding = binding.read(cx);
    let authoring = binding
        .authoring()
        .map_err(|error| WorkflowNodeError::BindingUnavailable {
            message: error.to_string(),
        })?;
    let data = LlmData::parse(authoring.node_id(), authoring.node())?;
    let ports = authoring
        .ports()
        .map(|port| WorkflowPort {
            id: port.id(),
            direction: port.facts().dir,
            handle: port.handle().clone(),
        })
        .collect();
    Ok((authoring.revision(), data, ports))
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

fn reconcile_prompt_composition(field: &mut RevisionedField<String>, composing: bool) {
    if composing {
        field.set_composing(true);
    } else if matches!(
        field.phase(),
        RetainedFieldPhase::Composing | RetainedFieldPhase::ComposingConflict
    ) {
        // The native text controller clears its marked range during IME commit without emitting a
        // second application-level change callback. Reconcile that edge on render.
        field.set_composing(false);
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use jellyflow_core::{CanvasPoint, CanvasRect, GraphBuilder, GraphId, NodeKindKey, PortId};
    use jellyflow_open_gpui::{
        OPEN_GPUI_DEFAULT_ELIGIBLE_PARKED_CAPACITY, OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY,
        OpenGpuiNodeRegistry, OpenGpuiSceneHost, RetainedFieldAttemptState,
    };
    use jellyflow_runtime::{GraphCommandOutcome, GraphRuntime, ViewSessionOptions};
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
        let mut field = RevisionedField::new(0, String::from("before"));
        field.edit(String::from("composing"));

        reconcile_prompt_composition(&mut field, true);
        assert_eq!(field.phase(), RetainedFieldPhase::Composing);

        reconcile_prompt_composition(&mut field, false);
        assert_eq!(field.phase(), RetainedFieldPhase::Dirty);
    }

    #[test]
    fn ime_commit_preserves_a_remote_conflict_for_explicit_resolution() {
        let mut field = RevisionedField::new(0, String::from("before"));
        field.edit(String::from("local"));
        reconcile_prompt_composition(&mut field, true);
        field.observe(1, String::from("remote"));
        assert_eq!(field.phase(), RetainedFieldPhase::ComposingConflict);

        reconcile_prompt_composition(&mut field, false);
        assert_eq!(field.phase(), RetainedFieldPhase::Conflict);
    }

    #[test]
    fn model_transport_error_survives_prompt_edit_and_blocks_duplicate_model_submission() {
        let mut prompt = RevisionedField::new(7, String::from("before prompt"));
        let mut model = RevisionedField::new(7, String::from("gpt-4.1"));
        model.edit(String::from("gpt-5"));
        let intent = model.plan(7).expect("dirty model intent");
        assert!(matches!(
            model.settle_ticket(
                intent.attempt(),
                Err(jellyflow_open_gpui::OpenGpuiCommandTicketError::HostDropped),
            ),
            jellyflow_open_gpui::RetainedFieldSettlement::Uncertain { .. }
        ));
        let model_notice = model.notice().expect("uncertain model notice").to_owned();

        prompt.edit(String::from("edited prompt"));

        assert!(prompt.can_submit());
        assert!(prompt.notice().is_none());
        assert!(matches!(
            model.attempt_state(),
            RetainedFieldAttemptState::Uncertain { .. }
        ));
        assert_eq!(model.notice(), Some(model_notice.as_str()));
        assert!(matches!(
            model.plan(7),
            Err(RetainedFieldError::AttemptPending { .. })
        ));
    }

    #[test]
    fn applied_prompt_receipt_waits_for_the_authoritative_snapshot() {
        let mut field = RevisionedField::new(0, String::from("before"));
        field.edit(String::from("local"));
        let intent = field.plan(0).expect("dirty prompt intent");
        assert!(matches!(
            field.settle_ticket(intent.attempt(), Ok(applied_prompt_outcome("local"))),
            jellyflow_open_gpui::RetainedFieldSettlement::AwaitingSnapshot { .. }
        ));
        assert!(matches!(
            field.attempt_state(),
            RetainedFieldAttemptState::ReceiptFirst { .. }
        ));
        assert!(matches!(
            field.observe(1, String::from("local")),
            RetainedFieldUpdate::Acknowledged { .. }
        ));
        assert_eq!(field.phase(), RetainedFieldPhase::Clean);
    }

    #[test]
    fn authoritative_prompt_snapshot_can_arrive_before_its_receipt() {
        let mut field = RevisionedField::new(0, String::from("before"));
        field.edit(String::from("local"));
        let intent = field.plan(0).expect("dirty prompt intent");
        assert!(matches!(
            field.observe(1, String::from("local")),
            RetainedFieldUpdate::SnapshotFirst { .. }
        ));
        assert!(matches!(
            field.settle_ticket(intent.attempt(), Ok(applied_prompt_outcome("local"))),
            jellyflow_open_gpui::RetainedFieldSettlement::Acknowledged { .. }
        ));
        assert_eq!(field.phase(), RetainedFieldPhase::Clean);
    }

    #[test]
    fn open_model_menu_keeps_its_authoring_base_until_selection() {
        let mut field = RevisionedField::new(7, String::from("gpt-4.1"));

        assert_eq!(
            observe_model_snapshot(&mut field, true, 8, String::from("remote-provider-model")),
            None
        );
        assert_eq!(field.remote_revision(), 7);
        assert_eq!(field.remote(), "gpt-4.1");

        field.edit(String::from("claude-3.7-sonnet"));
        assert!(matches!(
            field.observe(8, String::from("remote-provider-model")),
            RetainedFieldUpdate::Conflict
        ));
        assert_eq!(field.phase(), RetainedFieldPhase::Conflict);
        assert_eq!(field.local(), "claude-3.7-sonnet");
        assert_eq!(field.remote(), "remote-provider-model");
    }

    fn applied_prompt_outcome(value: &str) -> GraphCommandOutcome {
        let catalog = transient_overflow_catalog();
        let runtime = GraphRuntime::builder(catalog.clone())
            .build(transient_overflow_graph(&catalog))
            .expect("valid workflow prompt receipt graph");
        let snapshot = runtime.snapshot().expect("initial workflow snapshot");
        let node = snapshot
            .graph()
            .node(&transient_llm_id())
            .expect("workflow receipt LLM node");
        let data = LlmData::parse(transient_llm_id(), node)
            .expect("valid workflow receipt data")
            .with_field(transient_llm_id(), LlmField::Prompt, value.to_owned())
            .expect("valid workflow prompt update");
        runtime
            .execute(GraphCommand::set_node_data(
                snapshot.revision(),
                transient_llm_id(),
                data,
            ))
            .expect("apply workflow prompt command")
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
        let draft_store = WorkflowFieldStore::new();
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
            let node = binding
                .read(cx)
                .authoring()
                .expect("a protector factory must receive a live authoring binding")
                .node_id();
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
            assert_eq!(node.prompt.field().local(), "partial composition");
            assert_eq!(node.prompt.field().phase(), RetainedFieldPhase::Dirty);
            assert!(node.focus_lease.is_none());
            assert!(node.ime_lease.is_none());
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
            assert_eq!(node.prompt.field().local(), "partial composition");
            assert_eq!(node.prompt.field().phase(), RetainedFieldPhase::Dirty);
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

    fn transient_overflow_catalog() -> jellyflow_runtime::schema::NodeCatalog {
        let mut catalog = jellyflow_runtime::schema::NodeCatalog::new();
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
        catalog: &jellyflow_runtime::schema::NodeCatalog,
    ) -> jellyflow_core::Graph {
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
