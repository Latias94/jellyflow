use std::cell::Cell;
use std::rc::Rc;

use jellyflow_core::{
    CanvasSize, Node, NodeId, NodeKindKey, PortCapacity, PortDirection, PortKind,
};
use jellyflow_open_gpui::activity::{
    OpenGpuiActivityKind, OpenGpuiActivityLease, OpenGpuiNodeActivityHandle,
};
use jellyflow_open_gpui::authoring::{OpenGpuiNodeAuthoring, OpenGpuiNodeBinding};
use jellyflow_open_gpui::registry::{
    OpenGpuiNodeAdapterPresentation, OpenGpuiNodeRegistrationPolicy, OpenGpuiNodeRegistryBuilder,
    OpenGpuiNodeRendererDescriptor,
};
use jellyflow_open_gpui::retained_field::{
    RetainedField, RetainedFieldConflictChoice, RetainedFieldError, RetainedFieldKey,
    RetainedFieldPhase, RetainedFieldRecovery, RetainedFieldResolution, RetainedFieldStore,
    RetainedFieldSubmitMode, RetainedFieldUpdate,
};
use jellyflow_open_gpui::size_policy::{OpenGpuiContentHeightPolicy, OpenGpuiNodeSizePolicy};
use jellyflow_runtime::commands::GraphCommand;
use jellyflow_runtime::schema::{NodeKindSpec, NodePortSpec};
use jellyflow_runtime::view::RendererStamp;
use open_gpui::{
    App, AppContext, Context, Entity, Focusable, InteractiveElement, ParentElement, Render, Styled,
    Subscription, Window, div, px,
};
use open_gpui_ui_components::listbox::ListboxOption;
use open_gpui_ui_components::{
    Button, ButtonVariant, Select, TextInput, gpui_adapter::TextInputController,
};

use crate::model_options::{
    WorkflowModelOptionsCompleteness, WorkflowModelOptionsState, WorkflowModelOptionsStatus,
};
use crate::product_catalog::{NativeProductCatalog, NativeProductNodePresentation};
use crate::product_theme::NativeProductTheme;

use super::super::sync_activity_lease;
use super::{
    WORKFLOW_HEADER_HEIGHT, WorkflowNodeError, WorkflowNodeStatus, WorkflowPort, open_gpui_side,
    workflow_alert, workflow_header,
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
const WORKFLOW_LLM_MIN_HEIGHT: f32 = 190.0;
const WORKFLOW_LLM_MAX_HEIGHT: f32 = 560.0;
const WORKFLOW_LLM_BODY_MAX_HEIGHT: f32 = WORKFLOW_LLM_MAX_HEIGHT - WORKFLOW_HEADER_HEIGHT;
pub(super) const DEFAULT_MODEL: &str = "gpt-4.1";

pub type WorkflowFieldStore = RetainedFieldStore<String>;

pub struct WorkflowLlmNode {
    binding: Entity<OpenGpuiNodeBinding>,
    activity: OpenGpuiNodeActivityHandle,
    model_options: Entity<WorkflowModelOptionsState>,
    _model_options_subscription: Subscription,
    prompt_controller: Entity<TextInputController>,
    prompt: RetainedField<String>,
    model: RetainedField<String>,
    product_theme: NativeProductTheme,
    product_presentation: NativeProductNodePresentation,
    model_menu_base: Option<String>,
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
        let result = read_llm_render_binding(&self.binding, &self.product_presentation, cx);
        match result {
            Ok((authoring, data, ports)) => {
                let prompt_observation = authoring.observe_field(|_| data.prompt.clone());
                let prompt_update = match self.prompt.observe(prompt_observation, cx) {
                    Ok(update) => update,
                    Err(error) => {
                        self.diagnostic =
                            Some(Self::field_error(LlmField::Prompt, error).to_string());
                        return None;
                    }
                };
                if matches!(prompt_update, RetainedFieldUpdate::RemoteApplied) {
                    let value = self.prompt.local().clone();
                    self.prompt_controller
                        .update(cx, |controller, cx| controller.set_value(value, cx));
                }
                if !self.model_menu_open.get()
                    && let Err(error) = self
                        .model
                        .observe(authoring.observe_field(|_| data.model.clone()), cx)
                {
                    self.diagnostic = Some(Self::field_error(LlmField::Model, error).to_string());
                    return None;
                }
                Some((data, ports))
            }
            Err(error) => {
                self.diagnostic = Some(error.to_string());
                None
            }
        }
    }

    fn edit_prompt(&mut self, value: String, cx: &mut Context<Self>) {
        if let Err(error) = self.prompt.edit(value) {
            self.diagnostic = Some(Self::field_error(LlmField::Prompt, error).to_string());
            cx.notify();
            return;
        }
        let composing = self
            .prompt_controller
            .read(cx)
            .marked_range_utf16()
            .is_some();
        if let Err(error) = self.prompt.set_composing(composing) {
            self.diagnostic = Some(Self::field_error(LlmField::Prompt, error).to_string());
        }
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
        self.model_menu_base.take();
        if let Err(error) = self.model.edit(value) {
            self.diagnostic = Some(Self::field_error(LlmField::Model, error).to_string());
            cx.notify();
            return;
        }
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
            || self.model.conflict().is_some()
            || self.model.attempt_state().blocks_submission()
        {
            return;
        }
        let (authoring, data) = match read_llm_binding(&self.binding, cx) {
            Ok(binding) => binding,
            Err(error) => {
                self.diagnostic = Some(error.to_string());
                cx.notify();
                return;
            }
        };
        if let Err(error) = self
            .model
            .observe(authoring.observe_field(|_| data.model.clone()), cx)
        {
            self.diagnostic = Some(Self::field_error(LlmField::Model, error).to_string());
            cx.notify();
            return;
        }
        if self.model.conflict().is_some() || self.model.attempt_state().blocks_submission() {
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
                self.model_menu_base = Some(data.model);
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
        let result = read_llm_binding(&self.binding, cx).and_then(|(authoring, data)| {
            self.model
                .observe(authoring.observe_field(|_| data.model), cx)
                .map_err(|error| Self::field_error(LlmField::Model, error))?;
            match self
                .model
                .resolve(&authoring, RetainedFieldConflictChoice::AcceptRemote, cx)
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
        let result = read_llm_binding(&self.binding, cx).and_then(|(authoring, data)| {
            self.prompt
                .observe(authoring.observe_field(|_| data.prompt), cx)
                .map_err(|error| Self::field_error(LlmField::Prompt, error))?;
            match self
                .prompt
                .resolve(&authoring, RetainedFieldConflictChoice::AcceptRemote, cx)
            {
                Ok(RetainedFieldResolution::AcceptedRemote { .. }) => Ok(()),
                Ok(_) => Err(WorkflowNodeError::DraftConflict { field: "prompt" }),
                Err(error) => Err(Self::field_error(LlmField::Prompt, error)),
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
        self.retained_field_mut(field)
            .observe(
                authoring.observe_field(|_| current.field(field).to_owned()),
                cx,
            )
            .map_err(|error| Self::field_error(field, error))?;
        let plan = match self.retained_field_mut(field).plan(authoring, mode, cx) {
            Ok(plan) => plan,
            Err(RetainedFieldError::Clean | RetainedFieldError::CompositionActive) => return Ok(()),
            Err(error) => return Err(Self::field_error(field, error)),
        };
        let version = plan.authoring().version();
        let next_data = current.with_field(node_id, field, plan.value().clone())?;
        let command = GraphCommand::set_node_data(version, node_id, next_data)
            .with_label(format!("edit workflow {}", field.key()));
        let result = plan.dispatch(command, cx);
        if result.is_ok() {
            self.diagnostic = None;
        }
        result.map_err(|error| Self::field_error(field, error))
    }

    fn retained_field_mut(&mut self, field: LlmField) -> &mut RetainedField<String> {
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
    }

    fn reject_transient_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let value = self.prompt_controller.read(cx).value().to_owned();
        if let Err(error) = self.prompt.edit(value.clone()) {
            self.diagnostic = Some(Self::field_error(LlmField::Prompt, error).to_string());
        }
        if let Err(error) = self.prompt.set_composing(false) {
            self.diagnostic = Some(Self::field_error(LlmField::Prompt, error).to_string());
        }
        self.prompt_controller
            .update(cx, |controller, cx| controller.set_value(value, cx));
        self.focus_lease.take();
        self.ime_lease.take();
        window.blur(cx);
        cx.notify();
    }

    fn sync_prompt_composition(&mut self, cx: &mut Context<Self>) {
        let composing = self
            .prompt_controller
            .read(cx)
            .marked_range_utf16()
            .is_some();
        let should_update = composing
            || matches!(
                self.prompt.phase(),
                RetainedFieldPhase::Composing | RetainedFieldPhase::ComposingConflict
            );
        if should_update && let Err(error) = self.prompt.set_composing(composing) {
            self.diagnostic = Some(Self::field_error(LlmField::Prompt, error).to_string());
        }
    }

    fn status(&self) -> WorkflowNodeStatus {
        let phases = [self.prompt.phase(), self.model.phase()];
        if phases.iter().any(|phase| {
            matches!(
                phase,
                RetainedFieldPhase::Conflict | RetainedFieldPhase::ComposingConflict
            )
        }) {
            WorkflowNodeStatus::Conflict
        } else if self.diagnostic.is_some()
            || self.prompt.notice().is_some()
            || self.model.notice().is_some()
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

impl Render for WorkflowLlmNode {
    fn render(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        let theme = self.product_theme;
        let node_id = self.prompt.instance().node();
        let drag_handle = self.binding.read(cx).drag_handle();
        let (data, ports) = self.reconcile_graph(cx).unwrap_or_else(|| {
            (
                LlmData {
                    value: serde_json::Value::Null,
                    title: "Retired LLM".to_owned(),
                    prompt: self.prompt.local().clone(),
                    model: self.model.local().clone(),
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
        let model_select_disabled = self.model.conflict().is_some()
            || self.model.attempt_state().blocks_submission()
            || !model_options_status.is_ready();
        if model_select_disabled && self.model_menu_open.get() {
            self.model_menu_open.set(false);
            self.model_menu_base.take();
            self.model_overlay_lease.take();
        }
        let model_options = native_model_options(&model_options_status);
        let body_selector = workflow_llm_body_selector(node_id);
        let mut control_body = div()
            .id(body_selector.clone())
            .debug_selector(move || body_selector.clone())
            .px(px(16.0))
            .py(px(14.0))
            .min_h(px(0.0))
            .max_h(px(WORKFLOW_LLM_BODY_MAX_HEIGHT))
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .overflow_y_scroll()
            .scrollbar_width(px(7.0))
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
                        .disabled(!self.prompt.can_submit())
                        .on_activate(move |_, window, cx| {
                            let _ = apply_node.update(cx, |node, cx| node.apply_prompt(cx));
                            window.refresh();
                        }),
                    ),
            )
            .child(
                Select::new(select_id, "Model")
                    .options(model_options)
                    .placeholder(self.model.local().clone())
                    .selected(Some(self.model.local().clone()))
                    .open(self.model_menu_open.get())
                    .full_width(true)
                    .disabled(model_select_disabled)
                    .on_open_change(move |intent, window, cx| {
                        let open = intent.desired_open();
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
                .rounded(px(theme.radius.control))
                .border_1()
                .border_color(theme.status.warning.border)
                .bg(theme.status.warning.surface)
                .text_xs()
                .text_color(theme.status.warning.text)
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
                        .on_activate(move |_, window, cx| {
                            let _retry = retry_options.update(cx, |state, cx| state.retry(cx));
                            window.refresh();
                        }),
                    ),
                );
            }
            control_body = control_body.child(alert);
        }
        if let Some(message) = self.prompt.notice() {
            control_body = control_body.child(
                workflow_alert(
                    format!("jellyflow-workflow-prompt-command-notice-{}", node_id.0),
                    "Prompt command state",
                )
                .p(px(8.0))
                .rounded(px(theme.radius.control))
                .border_1()
                .border_color(theme.status.warning.border)
                .bg(theme.status.warning.surface)
                .text_xs()
                .text_color(theme.status.warning.text)
                .child(message.to_owned()),
            );
        }
        if let Some(message) = self.model.notice() {
            control_body = control_body.child(
                workflow_alert(
                    format!("jellyflow-workflow-model-command-notice-{}", node_id.0),
                    "Model command state",
                )
                .p(px(8.0))
                .rounded(px(theme.radius.control))
                .border_1()
                .border_color(theme.status.warning.border)
                .bg(theme.status.warning.surface)
                .text_xs()
                .text_color(theme.status.warning.text)
                .child(message.to_owned()),
            );
        }
        if self.prompt.conflict().is_some() {
            let accept_node = cx.weak_entity();
            let reapply_node = cx.weak_entity();
            let reapply_disabled =
                matches!(self.prompt.phase(), RetainedFieldPhase::ComposingConflict)
                    || self.prompt.attempt_state().blocks_submission();
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
                .rounded(px(theme.radius.control))
                .border_1()
                .border_color(theme.status.conflict.border)
                .bg(theme.status.conflict.surface)
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.status.conflict.text)
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
                                .on_activate(
                                    move |_, window, cx| {
                                        let _ = accept_node
                                            .update(cx, |node, cx| node.accept_remote_prompt(cx));
                                        window.refresh();
                                    },
                                ),
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
                                    .on_activate(
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
                .rounded(px(theme.radius.control))
                .border_1()
                .border_color(theme.status.conflict.border)
                .bg(theme.status.conflict.surface)
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.status.conflict.text)
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
                                .on_activate(
                                    move |_, window, cx| {
                                        let _ = accept_node
                                            .update(cx, |node, cx| node.accept_remote_model(cx));
                                        window.refresh();
                                    },
                                ),
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
                                    .disabled(self.model.attempt_state().blocks_submission())
                                    .on_activate(
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
        let diagnostic = self.diagnostic.clone().map(|message| {
            let selector = workflow_llm_diagnostic_selector(node_id);
            workflow_alert(selector.clone(), format!("Workflow LLM error: {message}"))
                .debug_selector(move || selector.clone())
                .text_xs()
                .text_color(theme.status.danger.text)
                .child(message)
        });
        let controls = control_body.children(diagnostic);

        let node_selector = workflow_llm_node_selector(node_id);
        let content = div()
            .debug_selector(move || node_selector.clone())
            .relative()
            .w_full()
            .max_h(px(WORKFLOW_LLM_MAX_HEIGHT))
            .flex()
            .flex_col()
            .rounded(px(theme.radius.node))
            .border_1()
            .border_color(theme.node.border)
            .bg(theme.node.surface)
            .text_color(theme.text.primary)
            .child(div().flex_none().child(drag_handle.wrap(workflow_header(
                node_id, data.title, "LLM", status, ports, theme,
            ))))
            .child(controls);
        self.binding.read(cx).measure_content(content)
    }
}

pub(super) fn catalog_spec() -> NodeKindSpec {
    NodeKindSpec::builder(WORKFLOW_LLM_KIND)
        .ports([
            NodePortSpec::new(
                "input",
                PortDirection::In,
                PortKind::Exec,
                PortCapacity::Single,
            ),
            NodePortSpec::new(
                "success",
                PortDirection::Out,
                PortKind::Exec,
                PortCapacity::Multi,
            ),
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
    product_theme: NativeProductTheme,
    product_catalog: &NativeProductCatalog,
) {
    let product_presentation = product_catalog
        .get(&NodeKindKey::new(WORKFLOW_LLM_KIND), 1)
        .expect("LLM renderer requires its product presentation")
        .clone();
    let accessible_name = product_presentation.title().to_owned();
    registry
        .register(
            OpenGpuiNodeRendererDescriptor::new(
                WORKFLOW_LLM_KIND,
                1,
                RendererStamp::new(WORKFLOW_LLM_RENDERER, 1),
                OpenGpuiNodeAdapterPresentation::new(accessible_name),
                move |binding, _window, cx| {
                    Ok(mount_workflow_llm(
                        binding,
                        field_store.clone(),
                        model_options.clone(),
                        product_theme,
                        product_presentation.clone(),
                        cx,
                    ))
                },
            )
            .with_policy(workflow_llm_registration_policy()),
        )
        .expect("unique workflow LLM renderer descriptor");
}

fn workflow_llm_registration_policy() -> OpenGpuiNodeRegistrationPolicy {
    OpenGpuiNodeRegistrationPolicy::default().with_size_policy(
        OpenGpuiNodeSizePolicy::ContentHeight(
            OpenGpuiContentHeightPolicy::new(WORKFLOW_LLM_MIN_HEIGHT, WORKFLOW_LLM_MAX_HEIGHT)
                .expect("workflow LLM content-height bounds must be valid"),
        ),
    )
}

fn mount_workflow_llm(
    binding: Entity<OpenGpuiNodeBinding>,
    field_store: WorkflowFieldStore,
    model_options: Entity<WorkflowModelOptionsState>,
    product_theme: NativeProductTheme,
    product_presentation: NativeProductNodePresentation,
    cx: &mut App,
) -> Entity<WorkflowLlmNode> {
    let activity = binding.read(cx).activity().clone();
    let initial = binding
        .read(cx)
        .authoring()
        .expect("an LLM node factory must receive a live authoring binding");
    let data = LlmData::parse(initial.node_id(), initial.node()).unwrap_or_else(|_| LlmData {
        value: serde_json::Value::Null,
        title: "LLM".to_owned(),
        prompt: String::new(),
        model: DEFAULT_MODEL.to_owned(),
    });
    let prompt_key = RetainedFieldKey::new(WORKFLOW_PROMPT_FIELD)
        .expect("workflow prompt field key is a non-empty constant");
    let prompt = RetainedField::mount(
        initial.observe_field(|_| data.prompt.clone()),
        prompt_key,
        field_store.clone(),
    )
    .expect("workflow prompt field must mount while its binding is live");
    let model_key = RetainedFieldKey::new(WORKFLOW_MODEL_FIELD)
        .expect("workflow model field key is a non-empty constant");
    let model = RetainedField::mount(
        initial.observe_field(|_| data.model.clone()),
        model_key,
        field_store,
    )
    .expect("workflow model field must mount while its binding is live");
    let recoveries = vec![
        (WORKFLOW_PROMPT_FIELD, prompt.recovery()),
        (WORKFLOW_MODEL_FIELD, model.recovery()),
    ];
    let controller = cx.new(|cx| TextInputController::with_value(prompt.local().clone(), cx));
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
            product_theme,
            product_presentation,
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
pub fn workflow_llm_body_selector(node: NodeId) -> String {
    format!("jellyflow-workflow-llm-body-{}", node.0)
}

#[must_use]
pub fn workflow_llm_node_selector(node: NodeId) -> String {
    format!("jellyflow-workflow-llm-node-{}", node.0)
}

fn workflow_llm_diagnostic_selector(node: NodeId) -> String {
    format!("jellyflow-workflow-llm-diagnostic-{}", node.0)
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
) -> Result<(OpenGpuiNodeAuthoring, LlmData), WorkflowNodeError> {
    let authoring =
        binding
            .read(cx)
            .authoring()
            .map_err(|error| WorkflowNodeError::BindingUnavailable {
                message: error.to_string(),
            })?;
    let data = LlmData::parse(authoring.node_id(), authoring.node())?;
    Ok((authoring, data))
}

fn read_llm_render_binding(
    binding: &Entity<OpenGpuiNodeBinding>,
    product_presentation: &NativeProductNodePresentation,
    cx: &App,
) -> Result<(OpenGpuiNodeAuthoring, LlmData, Vec<WorkflowPort>), WorkflowNodeError> {
    let binding = binding.read(cx);
    let authoring = binding
        .authoring()
        .map_err(|error| WorkflowNodeError::BindingUnavailable {
            message: error.to_string(),
        })?;
    let data = LlmData::parse(authoring.node_id(), authoring.node())?;
    let ports = authoring
        .ports()
        .map(|port| {
            let handle = product_presentation
                .port(&port.facts().key)
                .map(|presentation| {
                    port.handle()
                        .clone()
                        .with_side(open_gpui_side(presentation.side()))
                })
                .unwrap_or_else(|| port.handle().clone());
            WorkflowPort {
                id: port.id(),
                direction: port.facts().dir,
                kind: port.facts().kind,
                handle,
            }
        })
        .collect();
    Ok((authoring, data, ports))
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

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use jellyflow_core::{CanvasPoint, CanvasRect, GraphBuilder, GraphId, NodeKindKey, PortId};
    use jellyflow_open_gpui::host::{
        OPEN_GPUI_DEFAULT_ELIGIBLE_PARKED_CAPACITY, OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY,
        OpenGpuiSceneHost,
    };
    use jellyflow_open_gpui::registry::OpenGpuiNodeRegistry;
    use jellyflow_runtime::graph::GraphRuntime;
    use jellyflow_runtime::view::ViewSessionOptions;
    use open_gpui::{
        AppContext, EntityInputHandler, ScrollDelta, ScrollEvent, ScrollSource, TouchPhase, point,
    };
    use open_gpui_ui_components::gpui_adapter::init_text_input;

    use super::*;
    use crate::model_options::{WorkflowModelOptionsFailure, WorkflowModelOptionsResponse};

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

    #[open_gpui::test]
    fn saturated_llm_states_scroll_inside_fixed_node_geometry(cx: &mut open_gpui::TestAppContext) {
        cx.update(init_text_input);
        let catalog = transient_overflow_catalog();
        let runtime = Rc::new(
            GraphRuntime::builder(catalog.clone())
                .build(single_llm_graph(&catalog))
                .expect("valid saturated workflow LLM graph"),
        );
        let host_view = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("saturated workflow host view");
        let weak_view = host_view.downgrade();
        let remote_view = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("saturated workflow remote view");
        let model_options = cx.update(|cx| cx.new(|_| WorkflowModelOptionsState::ready_default()));
        let captured = Rc::new(RefCell::new(None::<Entity<WorkflowLlmNode>>));
        let mut registry = OpenGpuiNodeRegistry::builder();
        let factory_captured = Rc::clone(&captured);
        let factory_options = model_options.clone();
        let factory_store = WorkflowFieldStore::new();
        let product_presentation = crate::product_catalog::native_product_catalog()
            .expect("native product catalog")
            .get(&NodeKindKey::new(WORKFLOW_LLM_KIND), 1)
            .expect("LLM product presentation")
            .clone();
        registry
            .register(
                OpenGpuiNodeRendererDescriptor::new(
                    WORKFLOW_LLM_KIND,
                    1,
                    RendererStamp::new(WORKFLOW_LLM_RENDERER, 1),
                    OpenGpuiNodeAdapterPresentation::new("LLM"),
                    move |binding, _window, cx| {
                        let node = mount_workflow_llm(
                            binding,
                            factory_store.clone(),
                            factory_options.clone(),
                            NativeProductTheme::workflow_dify(),
                            product_presentation.clone(),
                            cx,
                        );
                        factory_captured.borrow_mut().replace(node.clone());
                        Ok(node)
                    },
                )
                .with_policy(workflow_llm_registration_policy()),
            )
            .expect("unique saturated LLM renderer descriptor");
        let (_host, cx) = cx.add_window_view(move |_, _| {
            OpenGpuiSceneHost::new(
                host_view,
                registry
                    .seal(&catalog)
                    .expect("valid saturated LLM registry"),
                saturated_llm_bounds(),
            )
            .expect("valid Open GPUI scene host")
        });
        draw_transient_host(cx);

        let node = captured
            .borrow()
            .clone()
            .expect("saturated workflow LLM mounted");
        cx.update_entity(&node, |node, cx| {
            node.edit_prompt("local prompt draft".to_owned(), cx);
        });
        apply_remote_llm_field(&remote_view, LlmField::Prompt, "remote prompt");
        draw_transient_host(cx);
        assert!(
            cx.debug_bounds(&workflow_prompt_conflict_selector(transient_llm_id()))
                .is_some()
        );

        cx.update_entity(&node, |node, cx| node.set_model_menu_open(true, cx));
        apply_remote_llm_field(&remote_view, LlmField::Model, "remote-provider-model");
        cx.update_entity(&node, |node, cx| {
            node.select_model("claude-3.7-sonnet".to_owned(), cx);
        });
        draw_transient_host(cx);
        assert!(
            cx.debug_bounds(&workflow_model_conflict_selector(transient_llm_id()))
                .is_some()
        );

        let token = cx
            .update_entity(&model_options, |options, cx| options.begin_request(cx))
            .expect("model option refresh token");
        cx.update_entity(&model_options, |options, cx| {
            let outcome = options.publish(
                token,
                WorkflowModelOptionsResponse::Failed(WorkflowModelOptionsFailure::retryable(
                    "The provider catalog is temporarily unavailable after validating every configured endpoint and credential source.",
                )),
                cx,
            );
            assert_eq!(
                outcome,
                crate::model_options::WorkflowModelOptionsPublishOutcome::Published
            );
        });
        cx.update_entity(&node, |node, cx| {
            node.diagnostic = Some(
                "The last workflow command could not be reconciled with the active authoring snapshot; inspect the remote changes before retrying."
                    .to_owned(),
            );
            cx.notify();
        });
        for _ in 0..4 {
            draw_transient_host(cx);
        }

        let node_id = transient_llm_id();
        let snapshot = runtime.snapshot().expect("saturated LLM snapshot");
        let ports = snapshot
            .graph()
            .node(&node_id)
            .expect("saturated LLM node")
            .ports
            .clone();
        drop(snapshot);
        let node_bounds = cx
            .debug_bounds(&workflow_llm_node_selector(node_id))
            .expect("saturated LLM node bounds");
        let body_bounds = cx
            .debug_bounds(&workflow_llm_body_selector(node_id))
            .expect("saturated LLM scroll body");
        let title_bounds = cx
            .debug_bounds(&super::super::workflow_header_title_selector(node_id))
            .expect("fixed saturated LLM title");
        let metadata_bounds = cx
            .debug_bounds(&super::super::workflow_header_metadata_selector(node_id))
            .expect("fixed saturated LLM metadata");
        let port_bounds = ports
            .iter()
            .map(|port| {
                cx.debug_bounds(&super::super::workflow_port_selector(*port))
                    .expect("fixed saturated LLM port")
            })
            .collect::<Vec<_>>();
        assert!(node_bounds.size.height.as_f32() <= WORKFLOW_LLM_MAX_HEIGHT + 1.0);
        assert!(body_bounds.size.height.as_f32() <= WORKFLOW_LLM_BODY_MAX_HEIGHT + 1.0);
        assert!(body_bounds.is_contained_within(&node_bounds));
        assert!(!title_bounds.intersects(&body_bounds));
        assert!(!metadata_bounds.intersects(&body_bounds));
        for port in &port_bounds {
            assert!(!port.intersects(&title_bounds));
            assert!(!port.intersects(&metadata_bounds));
            assert!(!port.intersects(&body_bounds));
        }

        let selectors = [
            workflow_model_options_failed_selector(node_id),
            workflow_prompt_conflict_selector(node_id),
            workflow_model_conflict_selector(node_id),
            workflow_llm_diagnostic_selector(node_id),
        ];
        let mut reached = [false; 4];
        let mut previous_bounds =
            std::array::from_fn::<_, 4, _>(|index| cx.debug_bounds(&selectors[index]));
        let mut content_moved = false;
        let viewport_before = weak_view
            .viewport_transform()
            .expect("saturated LLM viewport before scrolling");
        for _ in 0..24 {
            for (index, selector) in selectors.iter().enumerate() {
                if let Some(bounds) = cx.debug_bounds(selector)
                    && bounds.is_contained_within(&body_bounds)
                {
                    reached[index] = true;
                    assert!(bounds.is_contained_within(&node_bounds));
                }
            }
            if reached.into_iter().all(|visible| visible) {
                break;
            }
            cx.simulate_event(ScrollEvent {
                position: body_bounds.center(),
                delta: ScrollDelta::Pixels(point(px(0.0), px(-120.0))),
                modifiers: Default::default(),
                touch_phase: TouchPhase::Moved,
                source: ScrollSource::Pointer,
            });
            draw_transient_host(cx);
            let current_bounds =
                std::array::from_fn::<_, 4, _>(|index| cx.debug_bounds(&selectors[index]));
            content_moved |= current_bounds != previous_bounds;
            previous_bounds = current_bounds;
        }
        assert_eq!(
            reached, [true; 4],
            "every saturated failure, conflict, and diagnostic control must be reachable by scrolling"
        );
        assert!(
            content_moved,
            "the saturation fixture must exercise real body scrolling"
        );
        assert_eq!(
            cx.debug_bounds(&workflow_llm_node_selector(node_id)),
            Some(node_bounds)
        );
        assert_eq!(
            cx.debug_bounds(&workflow_llm_body_selector(node_id)),
            Some(body_bounds)
        );
        assert_eq!(
            cx.debug_bounds(&super::super::workflow_header_title_selector(node_id)),
            Some(title_bounds)
        );
        assert_eq!(
            cx.debug_bounds(&super::super::workflow_header_metadata_selector(node_id)),
            Some(metadata_bounds)
        );
        for (port, before) in ports.iter().zip(port_bounds) {
            assert_eq!(
                cx.debug_bounds(&super::super::workflow_port_selector(*port)),
                Some(before)
            );
        }
        assert_eq!(
            weak_view
                .viewport_transform()
                .expect("saturated LLM viewport after scrolling"),
            viewport_before,
            "node-local scrolling must not pan or zoom the canvas",
        );
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
        let product_presentation = crate::product_catalog::native_product_catalog()
            .expect("native product catalog")
            .get(&NodeKindKey::new(WORKFLOW_LLM_KIND), 1)
            .expect("LLM product presentation")
            .clone();
        registry
            .register(OpenGpuiNodeRendererDescriptor::new(
                WORKFLOW_LLM_KIND,
                1,
                RendererStamp::new(WORKFLOW_LLM_RENDERER, 1),
                OpenGpuiNodeAdapterPresentation::new("LLM"),
                move |binding, _window, cx| {
                    let node = mount_workflow_llm(
                        binding,
                        factory_store.clone(),
                        model_options.clone(),
                        NativeProductTheme::workflow_dify(),
                        product_presentation.clone(),
                        cx,
                    );
                    captured_for_factory.borrow_mut().replace(node.clone());
                    mounts_for_factory.set(mounts_for_factory.get().saturating_add(1));
                    Ok(node)
                },
            ))
            .expect("unique overflow LLM renderer descriptor");
        registry
            .register(OpenGpuiNodeRendererDescriptor::new(
                PROTECTOR_KIND,
                1,
                RendererStamp::new(PROTECTOR_RENDERER, 1),
                OpenGpuiNodeAdapterPresentation::new("Workflow transient protector"),
                |binding, _window, cx| {
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
                    Ok(cx.new(|_| ProtectorNode { _lease: lease }))
                },
            ))
            .expect("unique transient protector renderer descriptor");
        let (host, cx) = cx.add_window_view(move |_, _| {
            OpenGpuiSceneHost::new(
                view,
                registry
                    .seal(&catalog)
                    .expect("valid workflow transient overflow registry"),
                transient_initial_bounds(),
            )
            .expect("valid Open GPUI scene host")
        });
        draw_transient_host(cx);
        cx.read_entity(&host, |host, _| {
            assert_eq!(
                host.resource_snapshot().protected_instances(),
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
            assert_eq!(node.prompt.phase(), RetainedFieldPhase::Dirty);
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
            assert!(host.resource_snapshot().eligible_parked_nodes() >= 1);
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
            assert_eq!(
                host.resource_snapshot().protected_instances(),
                OPEN_GPUI_DEFAULT_PROTECTED_CAPACITY
            );
            assert!(
                host.resource_snapshot().eligible_parked_nodes()
                    <= OPEN_GPUI_DEFAULT_ELIGIBLE_PARKED_CAPACITY
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
            assert_eq!(node.prompt.phase(), RetainedFieldPhase::Dirty);
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
            .register(NodeKindSpec::builder(PROTECTOR_KIND).build())
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
                CanvasRect {
                    origin: CanvasPoint::default(),
                    size: WORKFLOW_LLM_DEFAULT_SIZE,
                },
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

    fn single_llm_graph(catalog: &jellyflow_runtime::schema::NodeCatalog) -> jellyflow_core::Graph {
        let creation = catalog
            .instantiate_node_with_ids(
                &NodeKindKey::new(WORKFLOW_LLM_KIND),
                transient_llm_id(),
                CanvasRect {
                    origin: CanvasPoint::default(),
                    size: WORKFLOW_LLM_DEFAULT_SIZE,
                },
                [
                    PortId::from_u128(0x7a11_2101),
                    PortId::from_u128(0x7a11_2102),
                ],
            )
            .expect("instantiate saturated workflow LLM");
        let (node_id, node, ports) = creation.into_parts();
        let mut graph = GraphBuilder::new(GraphId::from_u128(0x7a11_4000));
        graph.insert_node(node_id, node);
        for (port_id, port) in ports {
            graph.insert_port(port_id, port);
        }
        graph.build().expect("saturated workflow LLM graph")
    }

    fn apply_remote_llm_field(
        view: &jellyflow_runtime::view::ViewSession,
        field: LlmField,
        value: &str,
    ) {
        let snapshot = view
            .graph_snapshot()
            .expect("remote saturated workflow snapshot");
        let node_id = transient_llm_id();
        let data = LlmData::parse(
            node_id,
            snapshot
                .graph()
                .node(&node_id)
                .expect("remote saturated workflow LLM"),
        )
        .expect("valid saturated workflow LLM data")
        .with_field(node_id, field, value.to_owned())
        .expect("valid saturated workflow field update");
        view.execute(GraphCommand::set_node_data(
            snapshot.version(),
            node_id,
            data,
        ))
        .expect("execute remote saturated workflow field update")
        .applied_revision()
        .expect("remote saturated workflow field update applies");
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

    fn saturated_llm_bounds() -> CanvasRect {
        CanvasRect {
            origin: CanvasPoint { x: -10.0, y: -10.0 },
            size: CanvasSize {
                width: 380.0,
                height: 620.0,
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
            window.draw(cx).clear(cx);
        });
        cx.run_until_parked();
    }
}
