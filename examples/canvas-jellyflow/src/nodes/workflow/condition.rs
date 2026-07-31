use std::cell::Cell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::Duration;

use jellyflow_core::{
    CanvasSize, NodeId, NodeKindKey, PortCapacity, PortDirection, PortId, PortKind, TypeDesc,
};
use jellyflow_open_gpui::activity::{
    OpenGpuiActivityKind, OpenGpuiActivityLease, OpenGpuiNodeActivityHandle,
};
use jellyflow_open_gpui::authoring::{
    MutationLayoutEffect, MutationLayoutSettlement, MutationRequest, MutationSettlement,
    OpenGpuiManagedMutation, OpenGpuiNodeAuthoring, OpenGpuiNodeBinding, OpenGpuiPortHandle,
};
use jellyflow_open_gpui::chrome::OpenGpuiPortTone;
use jellyflow_open_gpui::registry::{
    OpenGpuiNodeAdapterPresentation, OpenGpuiNodeRegistrationPolicy, OpenGpuiNodeRegistryBuilder,
    OpenGpuiNodeRendererDescriptor,
};
use jellyflow_open_gpui::size_policy::{OpenGpuiContentHeightPolicy, OpenGpuiNodeSizePolicy};
use jellyflow_runtime::commands::IncidentEdgePolicy;
use jellyflow_runtime::schema::{NodeKindSpec, NodePortSpec};
use jellyflow_runtime::view::NodeInstanceKey;
use jellyflow_runtime::view::RendererStamp;
use open_gpui::{
    App, AppContext, Context, Entity, Focusable, InteractiveElement, ParentElement, Render,
    StatefulInteractiveElement, Styled, Task, Window, div, px,
};
use open_gpui_ui_components::{
    Button, ButtonVariant, ListboxOption, Select, TextInput, gpui_adapter::TextInputController,
};

use super::{
    WorkflowNodeStatus, WorkflowPort, WorkflowPortElement, open_gpui_side, workflow_alert,
    workflow_header, workflow_port_element,
};
use crate::nodes::sync_activity_lease;
use crate::product_catalog::{NativeProductCatalog, NativeProductNodePresentation};
use crate::product_domain::condition::{
    CONDITION_ELSE_PORT_KEY, CONDITION_KIND, ConditionCase, ConditionError, ConditionIntent,
    ConditionObservation, ConditionPredicate, condition_default_data,
};
use crate::product_theme::NativeProductTheme;

pub const WORKFLOW_CONDITION_KIND: &str = CONDITION_KIND;
pub const WORKFLOW_CONDITION_RENDERER: &str = "jellyflow.open-gpui.workflow.condition";
pub const WORKFLOW_CONDITION_DEFAULT_SIZE: CanvasSize = CanvasSize {
    width: 360.0,
    height: 350.0,
};

const WORKFLOW_CONDITION_NODE_ID: u128 = 0xd1fa_0002;
const CONDITION_BODY_HORIZONTAL_PADDING: f32 = 14.0;
const CONDITION_CASE_HORIZONTAL_PADDING: f32 = 9.0;
const CONDITION_CASE_ROW_HEIGHT: f32 = 37.0;
const CONDITION_CASE_LIST_MAX_HEIGHT: f32 = 148.0;
const CONDITION_MIN_HEIGHT: f32 = 250.0;
const CONDITION_MAX_HEIGHT: f32 = 470.0;
const CONDITION_OPERAND_PROVIDER_DELAY: Duration = Duration::from_millis(1);

#[derive(Clone, Debug, PartialEq)]
struct ConditionOperand {
    value: String,
    label: String,
    ty: TypeDesc,
}

impl ConditionOperand {
    fn new(value: impl Into<String>, label: impl Into<String>, ty: TypeDesc) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            ty,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum ConditionOperandStatus {
    Loading,
    Ready(Vec<ConditionOperand>),
    Failed(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConditionControlDiagnosticSource {
    Literal,
    Product,
    Overlay,
    InputActivity,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ConditionControlDiagnostic {
    source: ConditionControlDiagnosticSource,
    message: String,
}

pub struct WorkflowConditionNode {
    binding: Entity<OpenGpuiNodeBinding>,
    node_id: NodeId,
    mutation: OpenGpuiManagedMutation<ConditionIntent>,
    activity: OpenGpuiNodeActivityHandle,
    operand_status: ConditionOperandStatus,
    selected_operand: String,
    operand_menu_open: Rc<Cell<bool>>,
    operand_overlay_lease: Option<OpenGpuiActivityLease>,
    case_value: String,
    case_value_controller: Entity<TextInputController>,
    focus_lease: Option<OpenGpuiActivityLease>,
    ime_lease: Option<OpenGpuiActivityLease>,
    provider_activity_lease: Option<OpenGpuiActivityLease>,
    provider_task: Option<Task<()>>,
    product_theme: NativeProductTheme,
    product_presentation: NativeProductNodePresentation,
    mutation_diagnostic: Option<String>,
    control_diagnostic: Option<ConditionControlDiagnostic>,
}

struct ConditionRenderData {
    observation: Option<ConditionObservation>,
    title: String,
    cases: Vec<ConditionRenderCase>,
    else_branch: Option<(
        crate::product_domain::condition::ConditionElseBranch,
        OpenGpuiPortHandle,
    )>,
    inputs: Vec<WorkflowPort>,
}

struct ConditionRenderCase {
    domain: ConditionCase,
    route_handle: OpenGpuiPortHandle,
    value_handle: OpenGpuiPortHandle,
}

impl WorkflowConditionNode {
    fn new(
        binding: Entity<OpenGpuiNodeBinding>,
        instance: NodeInstanceKey,
        mutation: OpenGpuiManagedMutation<ConditionIntent>,
        activity: OpenGpuiNodeActivityHandle,
        case_value_controller: Entity<TextInputController>,
        product_theme: NativeProductTheme,
        product_presentation: NativeProductNodePresentation,
    ) -> Self {
        Self {
            binding,
            node_id: instance.node(),
            mutation,
            activity,
            operand_status: ConditionOperandStatus::Loading,
            selected_operand: "customer_intent".to_owned(),
            operand_menu_open: Rc::new(Cell::new(false)),
            operand_overlay_lease: None,
            case_value: String::new(),
            case_value_controller,
            focus_lease: None,
            ime_lease: None,
            provider_activity_lease: None,
            provider_task: None,
            product_theme,
            product_presentation,
            mutation_diagnostic: None,
            control_diagnostic: None,
        }
    }

    fn apply_intent(&mut self, intent: ConditionIntent, cx: &mut Context<Self>) -> bool {
        if !matches!(self.mutation.settlement(), MutationSettlement::Ready) {
            return false;
        }
        self.enqueue_request(MutationRequest::new(intent), cx);
        cx.notify();
        matches!(
            self.mutation.settlement(),
            MutationSettlement::Pending { .. } | MutationSettlement::Ready
        )
    }

    fn add_case(&mut self, cx: &mut Context<Self>) {
        let value = self.case_value.trim();
        if value.is_empty() {
            self.set_control_diagnostic(
                ConditionControlDiagnosticSource::Literal,
                "condition case value must not be empty",
            );
            cx.notify();
            return;
        }
        let Some(operand) = self.selected_operand() else {
            self.set_control_diagnostic(
                ConditionControlDiagnosticSource::Product,
                "condition fields are not available",
            );
            cx.notify();
            return;
        };
        let route_port_id = PortId::new();
        let value_port_id = PortId::new();
        let literal = match condition_literal(value, &operand.ty) {
            Ok(literal) => literal,
            Err(error) => {
                self.set_control_diagnostic(ConditionControlDiagnosticSource::Literal, error);
                cx.notify();
                return;
            }
        };
        let label = format!("{} equals {value}", operand.label);
        let predicate = ConditionPredicate::equals(operand.value, literal, operand.ty)
            .expect("validated native condition literals form valid predicates");
        let case = ConditionCase::new(
            format!("case-{}", route_port_id.0),
            label,
            route_port_id,
            value_port_id,
            predicate,
        )
        .expect("generated condition cases are valid");
        if self.apply_intent(ConditionIntent::add(case), cx) {
            self.case_value.clear();
            self.case_value_controller
                .update(cx, |controller, cx| controller.set_value("", cx));
        }
    }

    fn retry(&mut self, cx: &mut Context<Self>) {
        let retry = match self.mutation.settlement() {
            MutationSettlement::Retryable { .. } => self.mutation.retry(),
            MutationSettlement::Uncertain { .. } => self.mutation.confirm_uncertain_retry(),
            _ => None,
        };
        let Some(retry) = retry else {
            return;
        };
        self.enqueue_request(MutationRequest::retry(retry), cx);
        cx.notify();
    }

    fn discard_unobservable(&mut self, cx: &mut Context<Self>) {
        let Some(abandonment) = self.mutation.unobservable_abandonment() else {
            return;
        };
        if self.mutation.abandon_unobservable(abandonment) {
            self.mutation_diagnostic = None;
            cx.notify();
        }
    }

    fn enqueue_request(
        &mut self,
        request: MutationRequest<ConditionIntent>,
        cx: &mut Context<Self>,
    ) {
        let authoring = match self.binding.read(cx).authoring() {
            Ok(authoring) => authoring,
            Err(error) => {
                let settlement = self.mutation.observe_authoring_error(&error);
                self.apply_mutation_settlement(&settlement);
                return;
            }
        };
        let observation = match condition_observation(&authoring) {
            Ok(observation) => observation,
            Err(error) => {
                self.mutation_diagnostic = Some(error.to_string());
                return;
            }
        };
        if observation.satisfies(request.intent()) {
            self.clear_product_diagnostic();
            let settlement =
                self.mutation
                    .observe(&authoring, |intent| observation.satisfies(intent), cx);
            self.apply_mutation_settlement(&settlement);
            return;
        }
        let command = match observation
            .plan_with_port_occupancy(request.intent(), |port| authoring.port_id_is_occupied(port))
        {
            Ok(command) => command,
            Err(error) => {
                self.reject_request(request, error.to_string());
                return;
            }
        };
        self.clear_product_diagnostic();
        match self.mutation.dispatch(
            authoring,
            command,
            request,
            MutationLayoutEffect::InvalidateWhenSatisfied,
            cx,
        ) {
            Ok(settlement) => self.apply_mutation_settlement(&settlement),
            Err(error) => self.mutation_diagnostic = Some(error.to_string()),
        }
    }

    fn reject_request(&mut self, request: MutationRequest<ConditionIntent>, diagnostic: String) {
        if let MutationRequest::Retry(retry) = request
            && !self.mutation.abandon_retry(retry)
        {
            self.mutation_diagnostic = Some(diagnostic);
            return;
        }
        self.set_control_diagnostic(ConditionControlDiagnosticSource::Product, diagnostic);
    }

    fn reconcile_command(
        &mut self,
        authoring: &OpenGpuiNodeAuthoring,
        observation: &ConditionObservation,
        cx: &mut Context<Self>,
    ) -> MutationSettlement {
        self.mutation
            .observe(authoring, |intent| observation.satisfies(intent), cx)
    }

    fn apply_mutation_settlement(&mut self, settlement: &MutationSettlement) {
        self.mutation_diagnostic = match settlement {
            MutationSettlement::Retryable { diagnostic, .. }
            | MutationSettlement::Uncertain { diagnostic, .. }
            | MutationSettlement::Retired { diagnostic }
            | MutationSettlement::Closed { diagnostic, .. } => Some(diagnostic.to_string()),
            MutationSettlement::Satisfied {
                layout: MutationLayoutSettlement::Failed { diagnostic },
            } => Some(diagnostic.to_string()),
            MutationSettlement::Ready
            | MutationSettlement::Pending { .. }
            | MutationSettlement::Satisfied { .. } => None,
        };
    }

    fn current_diagnostic(&self) -> Option<&str> {
        self.control_diagnostic
            .as_ref()
            .map(|diagnostic| diagnostic.message.as_str())
            .or(self.mutation_diagnostic.as_deref())
            .or_else(|| {
                self.mutation
                    .layout_failure()
                    .map(jellyflow_open_gpui::authoring::MutationDiagnostic::message)
            })
    }

    fn command_status(&self, settlement: &MutationSettlement) -> WorkflowNodeStatus {
        match settlement {
            MutationSettlement::Ready | MutationSettlement::Satisfied { .. }
                if self.current_diagnostic().is_none() =>
            {
                match self.operand_status {
                    ConditionOperandStatus::Loading => WorkflowNodeStatus::Draft,
                    ConditionOperandStatus::Ready(_) => WorkflowNodeStatus::Ready,
                    ConditionOperandStatus::Failed(_) => WorkflowNodeStatus::Error,
                }
            }
            MutationSettlement::Pending { .. } => WorkflowNodeStatus::Draft,
            MutationSettlement::Retryable { .. } => WorkflowNodeStatus::Conflict,
            MutationSettlement::Ready
            | MutationSettlement::Satisfied { .. }
            | MutationSettlement::Uncertain { .. }
            | MutationSettlement::Retired { .. }
            | MutationSettlement::Closed { .. } => WorkflowNodeStatus::Error,
        }
    }

    fn edit_case_value(&mut self, value: String, cx: &mut Context<Self>) {
        if self.case_value == value {
            return;
        }
        self.case_value = value;
        self.revalidate_literal_diagnostic();
        cx.notify();
    }

    fn select_operand(&mut self, value: String, cx: &mut Context<Self>) {
        let selection_changed = self.selected_operand != value
            && self
                .operand_options()
                .iter()
                .any(|option| option.value == value);
        if selection_changed {
            self.selected_operand = value;
            self.revalidate_literal_diagnostic();
        }
        self.operand_menu_open.set(false);
        self.operand_overlay_lease.take();
        cx.notify();
    }

    fn set_operand_menu_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if !open {
            self.operand_menu_open.set(false);
            self.operand_overlay_lease.take();
            cx.notify();
            return;
        }
        if self.operand_menu_open.get() || self.operand_options().is_empty() {
            return;
        }
        let revoked_open = self.operand_menu_open.clone();
        match self
            .activity
            .acquire_with_revocation(OpenGpuiActivityKind::Overlay, move || {
                revoked_open.set(false)
            }) {
            Ok(lease) => {
                self.operand_menu_open.set(true);
                self.operand_overlay_lease = Some(lease);
            }
            Err(error) => {
                self.operand_menu_open.set(false);
                self.set_control_diagnostic(
                    ConditionControlDiagnosticSource::Overlay,
                    format!("condition field overlay failed: {error}"),
                );
            }
        }
        cx.notify();
    }

    fn operand_options(&self) -> &[ConditionOperand] {
        match &self.operand_status {
            ConditionOperandStatus::Ready(options) => options,
            ConditionOperandStatus::Loading | ConditionOperandStatus::Failed(_) => &[],
        }
    }

    fn selected_operand(&self) -> Option<ConditionOperand> {
        self.operand_options()
            .iter()
            .find(|option| option.value == self.selected_operand)
            .cloned()
    }

    fn publish_operand_options(&mut self, options: Vec<ConditionOperand>, cx: &mut Context<Self>) {
        self.provider_activity_lease.take();
        if options.is_empty() {
            self.operand_status = ConditionOperandStatus::Failed(
                "condition field provider returned no fields".to_owned(),
            );
        } else {
            if !options
                .iter()
                .any(|option| option.value == self.selected_operand)
            {
                self.selected_operand = options[0].value.clone();
            }
            self.operand_status = ConditionOperandStatus::Ready(options);
        }
        cx.notify();
    }

    fn fail_operand_provider(&mut self, message: String, cx: &mut Context<Self>) {
        self.provider_activity_lease.take();
        self.operand_status = ConditionOperandStatus::Failed(message);
        cx.notify();
    }

    fn sync_activity(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (focused, composing) = {
            let controller = self.case_value_controller.read(cx);
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
        if let Some(error) = focus.err().or_else(|| ime.err()) {
            self.reject_transient_input(window, cx);
            self.set_control_diagnostic(
                ConditionControlDiagnosticSource::InputActivity,
                format!("condition input activity failed: {error}"),
            );
            cx.notify();
        }
    }

    fn set_control_diagnostic(
        &mut self,
        source: ConditionControlDiagnosticSource,
        message: impl Into<String>,
    ) {
        self.control_diagnostic = Some(ConditionControlDiagnostic {
            source,
            message: message.into(),
        });
    }

    fn clear_product_diagnostic(&mut self) {
        if self.control_diagnostic.as_ref().is_some_and(|diagnostic| {
            matches!(
                diagnostic.source,
                ConditionControlDiagnosticSource::Literal
                    | ConditionControlDiagnosticSource::Product
            )
        }) {
            self.control_diagnostic = None;
        }
    }

    fn revalidate_literal_diagnostic(&mut self) {
        if !self.control_diagnostic.as_ref().is_some_and(|diagnostic| {
            diagnostic.source == ConditionControlDiagnosticSource::Literal
        }) {
            return;
        }
        let message = if self.case_value.trim().is_empty() {
            Some("condition case value must not be empty".to_owned())
        } else {
            self.selected_operand()
                .and_then(|operand| condition_literal(self.case_value.trim(), &operand.ty).err())
        };
        self.control_diagnostic = message.map(|message| ConditionControlDiagnostic {
            source: ConditionControlDiagnosticSource::Literal,
            message,
        });
    }

    fn reject_transient_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let value = self.case_value_controller.read(cx).value().to_owned();
        self.case_value = value.clone();
        self.case_value_controller
            .update(cx, |controller, cx| controller.set_value(value, cx));
        self.focus_lease.take();
        self.ime_lease.take();
        window.blur();
    }
}

impl Render for WorkflowConditionNode {
    fn render(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        let theme = self.product_theme;
        let else_label = self
            .product_presentation
            .port(&jellyflow_core::PortKey::new(CONDITION_ELSE_PORT_KEY))
            .expect("validated Condition product presentation contains else")
            .label()
            .to_owned();
        let node_id = self.node_id;
        let drag_handle = self.binding.read(cx).drag_handle();
        let authoring = self.binding.read(cx).authoring();
        let settlement;
        let data = match authoring {
            Ok(authoring) => match read_condition_authoring(&authoring, &self.product_presentation)
            {
                Ok(data) => {
                    settlement = self.reconcile_command(
                        &authoring,
                        data.observation
                            .as_ref()
                            .expect("live condition render data has an observation"),
                        cx,
                    );
                    self.apply_mutation_settlement(&settlement);
                    Ok(data)
                }
                Err(error) => {
                    settlement = self.mutation.observe_unavailable(&authoring, cx);
                    self.apply_mutation_settlement(&settlement);
                    Err(error.to_string())
                }
            },
            Err(error) => {
                settlement = self.mutation.observe_authoring_error(&error);
                self.apply_mutation_settlement(&settlement);
                Err(error.to_string())
            }
        };
        let binding_valid = data.is_ok();
        let data = match data {
            Ok(value) => value,
            Err(error) => {
                self.mutation_diagnostic = Some(error);
                retired_render_data()
            }
        };
        self.sync_activity(window, cx);

        let commands_locked = !binding_valid
            || !matches!(
                settlement,
                MutationSettlement::Ready | MutationSettlement::Satisfied { .. }
            );
        let pending_label = matches!(settlement, MutationSettlement::Pending { .. })
            .then(|| self.mutation.active_intent())
            .flatten()
            .filter(|intent| {
                !data
                    .observation
                    .as_ref()
                    .is_some_and(|observation| observation.satisfies(intent))
            })
            .map(condition_intent_label);

        let operand_options = self
            .operand_options()
            .iter()
            .map(|option| ListboxOption::new(option.value.clone(), option.label.clone()))
            .collect::<Vec<_>>();
        let provider_loading = matches!(self.operand_status, ConditionOperandStatus::Loading);
        let provider_failed = match &self.operand_status {
            ConditionOperandStatus::Failed(message) => Some(message.clone()),
            ConditionOperandStatus::Loading | ConditionOperandStatus::Ready(_) => None,
        };
        let edit_node = cx.weak_entity();
        let select_node = cx.weak_entity();
        let open_node = cx.weak_entity();
        let add_node = cx.weak_entity();
        let composer = div()
            .px(px(CONDITION_BODY_HORIZONTAL_PADDING))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                Select::new(workflow_condition_field_id(node_id), "Field")
                    .options(operand_options)
                    .placeholder(if provider_loading {
                        "Loading fields..."
                    } else {
                        "Select a field"
                    })
                    .selected(self.selected_operand.clone())
                    .open(self.operand_menu_open.get())
                    .full_width(true)
                    .disabled(commands_locked || provider_loading || provider_failed.is_some())
                    .on_open_change(move |open, window, cx| {
                        let _ =
                            open_node.update(cx, |node, cx| node.set_operand_menu_open(open, cx));
                        window.refresh();
                    })
                    .on_select(move |selection, window, cx| {
                        let value = selection.value().to_owned();
                        let _ = select_node.update(cx, |node, cx| node.select_operand(value, cx));
                        window.refresh();
                    }),
            )
            .child(
                TextInput::new(workflow_condition_value_id(node_id), "Case value")
                    .controller(self.case_value_controller.clone())
                    .value(self.case_value.clone())
                    .placeholder("Comparison value")
                    .disabled(commands_locked || provider_loading || provider_failed.is_some())
                    .invalid(self.control_diagnostic.as_ref().is_some_and(|diagnostic| {
                        diagnostic.source == ConditionControlDiagnosticSource::Literal
                    }))
                    .on_change(move |value, window, cx| {
                        let edit_node = edit_node.clone();
                        window.defer(cx, move |window, cx| {
                            let _ =
                                edit_node.update(cx, |node, cx| node.edit_case_value(value, cx));
                            window.refresh();
                        });
                    }),
            )
            .child(
                div()
                    .debug_selector(move || workflow_condition_add_selector(node_id))
                    .child(
                        Button::new(workflow_condition_add_id(node_id), "Add case")
                            .variant(ButtonVariant::Secondary)
                            .disabled(
                                commands_locked
                                    || provider_loading
                                    || provider_failed.is_some()
                                    || self.case_value.trim().is_empty(),
                            )
                            .on_click(move |_, window, cx| {
                                let _ = add_node.update(cx, |node, cx| node.add_case(cx));
                                window.refresh();
                            }),
                    ),
            );

        let mut rows = div()
            .px(px(CONDITION_BODY_HORIZONTAL_PADDING))
            .flex()
            .flex_col()
            .gap(px(theme.spacing.sm));
        for (
            index,
            ConditionRenderCase {
                domain: case,
                route_handle,
                value_handle,
            },
        ) in data.cases.into_iter().enumerate()
        {
            let port_id = case.port_id();
            let row_selector = workflow_condition_case_selector(node_id, port_id);
            let WorkflowPortElement {
                marker: route_marker,
                layout_diameter: route_diameter,
            } = workflow_port_element(
                route_handle,
                OpenGpuiPortTone::Control,
                row_selector.clone(),
                theme,
            );
            let route_marker = div()
                .absolute()
                .top(px((CONDITION_CASE_ROW_HEIGHT - route_diameter) / 2.0))
                .right(px(-(CONDITION_BODY_HORIZONTAL_PADDING
                    + CONDITION_CASE_HORIZONTAL_PADDING
                    + route_diameter / 2.0)))
                .child(route_marker);
            let value_port_id = case.value_port_id();
            let value_selector = workflow_condition_case_value_selector(node_id, value_port_id);
            let WorkflowPortElement {
                marker: value_marker,
                layout_diameter: value_diameter,
            } = workflow_port_element(value_handle, OpenGpuiPortTone::Data, value_selector, theme);
            let value_marker = div()
                .absolute()
                .top(px((CONDITION_CASE_ROW_HEIGHT - value_diameter) / 2.0))
                .left(px(-(CONDITION_BODY_HORIZONTAL_PADDING
                    + CONDITION_CASE_HORIZONTAL_PADDING
                    + value_diameter / 2.0)))
                .child(value_marker);
            let actions_selector = workflow_condition_case_actions_selector(node_id, port_id);
            let mut actions = div()
                .debug_selector(move || actions_selector.clone())
                .flex_none()
                .flex()
                .gap(px(4.0));
            if index > 0 {
                let move_node = cx.weak_entity();
                let case_id = case.id().to_owned();
                let up_selector = workflow_condition_up_selector(node_id, port_id);
                actions = actions.child(
                    div().debug_selector(move || up_selector.clone()).child(
                        Button::new(
                            format!("jellyflow-workflow-condition-up-{}", port_id.0),
                            "Up",
                        )
                        .variant(ButtonVariant::Secondary)
                        .disabled(commands_locked)
                        .on_click(move |_, window, cx| {
                            let intent = ConditionIntent::reorder(case_id.clone(), index - 1);
                            let _ = move_node.update(cx, |node, cx| {
                                node.apply_intent(intent, cx);
                            });
                            window.refresh();
                        }),
                    ),
                );
            }
            let remove_node = cx.weak_entity();
            let case_id = case.id().to_owned();
            let remove_selector = workflow_condition_remove_selector(node_id, port_id);
            actions = actions.child(
                div().debug_selector(move || remove_selector.clone()).child(
                    Button::new(
                        format!("jellyflow-workflow-condition-remove-{}", port_id.0),
                        "Remove",
                    )
                    .variant(ButtonVariant::Secondary)
                    .disabled(commands_locked)
                    .on_click(move |_, window, cx| {
                        let intent =
                            ConditionIntent::remove(case_id.clone(), IncidentEdgePolicy::Cascade);
                        let _ = remove_node.update(cx, |node, cx| {
                            node.apply_intent(intent, cx);
                        });
                        window.refresh();
                    }),
                ),
            );
            let label_selector = workflow_condition_case_label_selector(node_id, port_id);
            let value_type = condition_type_label(case.value_type());
            rows = rows.child(
                div()
                    .relative()
                    .h(px(CONDITION_CASE_ROW_HEIGHT))
                    .px(px(CONDITION_CASE_HORIZONTAL_PADDING))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(theme.spacing.md))
                    .rounded(px(theme.radius.control))
                    .bg(theme.control.surface)
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .flex()
                            .items_center()
                            .gap(px(theme.spacing.sm))
                            .child(
                                div()
                                    .debug_selector(move || label_selector.clone())
                                    .min_w(px(0.0))
                                    .flex_1()
                                    .truncate()
                                    .text_sm()
                                    .child(case.label().to_owned()),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .text_xs()
                                    .text_color(theme.text.muted)
                                    .child(value_type),
                            ),
                    )
                    .child(actions)
                    .child(value_marker)
                    .child(route_marker),
            );
        }
        if let Some(label) = pending_label {
            let pending_selector = workflow_condition_pending_selector(node_id);
            rows = rows.child(
                div()
                    .debug_selector(move || pending_selector.clone())
                    .h(px(CONDITION_CASE_ROW_HEIGHT))
                    .px(px(CONDITION_CASE_HORIZONTAL_PADDING))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(theme.spacing.md))
                    .rounded(px(theme.radius.control))
                    .border_1()
                    .border_color(theme.status.info.border)
                    .text_color(theme.status.info.text)
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .truncate()
                            .text_sm()
                            .child(label),
                    )
                    .child(div().flex_none().text_xs().child("Pending")),
            );
        }
        if let Some((else_branch, handle)) = data.else_branch {
            let port_id = else_branch.port_id();
            let row_selector = workflow_condition_else_selector(node_id, port_id);
            let label_selector = workflow_condition_else_label_selector(node_id, port_id);
            let WorkflowPortElement {
                marker,
                layout_diameter,
            } = workflow_port_element(handle, OpenGpuiPortTone::Control, row_selector, theme);
            rows = rows.child(
                div()
                    .relative()
                    .h(px(CONDITION_CASE_ROW_HEIGHT))
                    .px(px(CONDITION_CASE_HORIZONTAL_PADDING))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(theme.spacing.md))
                    .rounded(px(theme.radius.control))
                    .bg(theme.surface.inset)
                    .child(
                        div()
                            .debug_selector(move || label_selector.clone())
                            .min_w(px(0.0))
                            .flex_1()
                            .truncate()
                            .text_sm()
                            .child(else_label),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_xs()
                            .text_color(theme.text.muted)
                            .child("Fallback"),
                    )
                    .child(
                        div()
                            .absolute()
                            .top(px((CONDITION_CASE_ROW_HEIGHT - layout_diameter) / 2.0))
                            .right(px(-(CONDITION_BODY_HORIZONTAL_PADDING
                                + CONDITION_CASE_HORIZONTAL_PADDING
                                + layout_diameter / 2.0)))
                            .child(marker),
                    ),
            );
        }
        let scroll_selector = workflow_condition_scroll_selector(node_id);
        let rows = div()
            .id(format!("jellyflow-workflow-condition-scroll-{}", node_id.0))
            .debug_selector(move || scroll_selector.clone())
            .max_h(px(CONDITION_CASE_LIST_MAX_HEIGHT))
            .overflow_y_scroll()
            .scrollbar_width(px(7.0))
            .child(rows);

        let retry_allowed = binding_valid
            && matches!(
                settlement,
                MutationSettlement::Retryable { .. }
                    | MutationSettlement::Uncertain {
                        retry_confirmable: true,
                        ..
                    }
            );
        let retry = retry_allowed.then(|| {
            let retry_node = cx.weak_entity();
            let retry_selector = workflow_condition_retry_selector(node_id);
            let label = if matches!(settlement, MutationSettlement::Uncertain { .. }) {
                "Confirm retry"
            } else {
                "Retry"
            };
            div().px(px(CONDITION_BODY_HORIZONTAL_PADDING)).child(
                div().debug_selector(move || retry_selector.clone()).child(
                    Button::new(
                        format!("jellyflow-workflow-condition-retry-button-{}", node_id.0),
                        label,
                    )
                    .variant(ButtonVariant::Secondary)
                    .on_click(move |_, window, cx| {
                        let _ = retry_node.update(cx, |node, cx| node.retry(cx));
                        window.refresh();
                    }),
                ),
            )
        });
        let discard =
            (!binding_valid && self.mutation.unobservable_abandonment().is_some()).then(|| {
                let discard_node = cx.weak_entity();
                let discard_selector = workflow_condition_discard_selector(node_id);
                div().px(px(CONDITION_BODY_HORIZONTAL_PADDING)).child(
                    div()
                        .debug_selector(move || discard_selector.clone())
                        .child(
                            Button::new(
                                format!(
                                    "jellyflow-workflow-condition-discard-button-{}",
                                    node_id.0
                                ),
                                "Discard change",
                            )
                            .variant(ButtonVariant::Secondary)
                            .on_click(move |_, window, cx| {
                                let _ = discard_node
                                    .update(cx, |node, cx| node.discard_unobservable(cx));
                                window.refresh();
                            }),
                        ),
                )
            });
        let provider_notice = provider_failed.map(|message| {
            let selector = workflow_condition_provider_failed_selector(node_id);
            workflow_alert(selector.clone(), "Condition field provider failed")
                .debug_selector(move || selector)
                .mx(px(CONDITION_BODY_HORIZONTAL_PADDING))
                .p(px(8.0))
                .text_xs()
                .text_color(theme.status.warning.text)
                .child(message)
        });
        let body_selector = workflow_condition_body_selector(node_id);
        let controls = div()
            .debug_selector(move || body_selector.clone())
            .py(px(12.0))
            .flex()
            .flex_col()
            .gap(px(9.0))
            .child(composer)
            .children(provider_notice)
            .child(rows)
            .children(retry)
            .children(discard);
        let status = self.command_status(&settlement);
        let diagnostic = self
            .current_diagnostic()
            .map(ToOwned::to_owned)
            .map(|message| {
                let selector = workflow_condition_diagnostic_selector(node_id);
                workflow_alert(
                    selector.clone(),
                    format!("Workflow condition error: {message}"),
                )
                .debug_selector(move || selector)
                .px(px(14.0))
                .text_xs()
                .text_color(theme.status.danger.text)
                .child(message)
            });

        let node_selector = workflow_condition_node_selector(node_id);
        let content = div()
            .debug_selector(move || node_selector.clone())
            .relative()
            .w_full()
            .rounded(px(theme.radius.node))
            .border_1()
            .border_color(theme.node.alternate_border)
            .bg(theme.node.alternate_surface)
            .text_color(theme.text.primary)
            .child(drag_handle.wrap(workflow_header(
                node_id,
                data.title,
                "CONDITION",
                status,
                data.inputs,
                theme,
            )))
            .child(controls)
            .children(diagnostic);
        self.binding.read(cx).measure_content(content)
    }
}

pub(super) fn catalog_spec() -> NodeKindSpec {
    NodeKindSpec::builder(WORKFLOW_CONDITION_KIND)
        .port(NodePortSpec::new(
            "input",
            PortDirection::In,
            PortKind::Exec,
            PortCapacity::Single,
        ))
        .port(NodePortSpec::new(
            CONDITION_ELSE_PORT_KEY,
            PortDirection::Out,
            PortKind::Exec,
            PortCapacity::Multi,
        ))
        .default_data(condition_default_data())
        .build()
}

pub(super) fn register(
    registry: &mut OpenGpuiNodeRegistryBuilder,
    product_theme: NativeProductTheme,
    product_catalog: &NativeProductCatalog,
) {
    let product_presentation = product_catalog
        .get(&NodeKindKey::new(WORKFLOW_CONDITION_KIND), 1)
        .expect("Condition renderer requires its product presentation")
        .clone();
    let accessible_name = product_presentation.title().to_owned();
    let policy = OpenGpuiNodeRegistrationPolicy::default().with_size_policy(
        OpenGpuiNodeSizePolicy::ContentHeight(
            OpenGpuiContentHeightPolicy::new(CONDITION_MIN_HEIGHT, CONDITION_MAX_HEIGHT)
                .expect("workflow condition content-height bounds must be valid"),
        ),
    );
    registry
        .register(
            OpenGpuiNodeRendererDescriptor::new(
                WORKFLOW_CONDITION_KIND,
                1,
                RendererStamp::new(WORKFLOW_CONDITION_RENDERER, 1),
                OpenGpuiNodeAdapterPresentation::new(accessible_name),
                move |binding, _window, cx| {
                    Ok(mount_condition(
                        binding,
                        product_theme,
                        product_presentation.clone(),
                        cx,
                    ))
                },
            )
            .with_policy(policy),
        )
        .expect("unique workflow condition renderer descriptor");
}

fn mount_condition(
    binding: Entity<OpenGpuiNodeBinding>,
    product_theme: NativeProductTheme,
    product_presentation: NativeProductNodePresentation,
    cx: &mut App,
) -> Entity<WorkflowConditionNode> {
    let (instance, mutation, activity) = {
        let binding = binding.read(cx);
        let instance = binding
            .authoring()
            .expect("a condition node factory must receive a live authoring binding")
            .instance()
            .clone();
        (instance, binding.managed_mutation(), binding.activity())
    };
    let controller = cx.new(|cx| TextInputController::with_value("", cx));
    let node = cx.new(|_| {
        WorkflowConditionNode::new(
            binding,
            instance,
            mutation,
            activity.clone(),
            controller,
            product_theme,
            product_presentation,
        )
    });
    let provider_cancelled = Rc::new(Cell::new(false));
    let revoke_provider = Rc::clone(&provider_cancelled);
    let provider_lease = activity.acquire_with_revocation(OpenGpuiActivityKind::Task, move || {
        revoke_provider.set(true);
    });
    match provider_lease {
        Ok(lease) => {
            node.update(cx, |node, _| node.provider_activity_lease = Some(lease));
            let weak_node = node.downgrade();
            let task = cx.spawn(async move |cx| {
                cx.background_executor()
                    .timer(CONDITION_OPERAND_PROVIDER_DELAY)
                    .await;
                if provider_cancelled.get() {
                    return;
                }
                let options = vec![
                    ConditionOperand::new("customer_intent", "Customer intent", TypeDesc::String),
                    ConditionOperand::new("risk_score", "Risk score", TypeDesc::Float),
                    ConditionOperand::new("region", "Region", TypeDesc::String),
                ];
                weak_node
                    .update(cx, |node, cx| node.publish_operand_options(options, cx))
                    .ok();
            });
            node.update(cx, |node, _| node.provider_task = Some(task));
        }
        Err(error) => {
            node.update(cx, |node, cx| {
                node.fail_operand_provider(
                    format!("condition field provider task failed: {error}"),
                    cx,
                );
            });
        }
    }
    node
}

fn read_condition_authoring(
    authoring: &OpenGpuiNodeAuthoring,
    product_presentation: &NativeProductNodePresentation,
) -> Result<ConditionRenderData, ConditionError> {
    let observation = condition_observation(authoring)?;
    let mut handles = authoring
        .ports()
        .map(|port| (port.id(), port.handle().clone()))
        .collect::<BTreeMap<_, _>>();
    let cases = observation
        .cases()
        .iter()
        .map(|case| {
            let route_port = case.port_id();
            let route_handle =
                handles
                    .remove(&route_port)
                    .ok_or(ConditionError::MissingCasePort {
                        node: authoring.node_id(),
                        port: route_port,
                    })?;
            let value_port = case.value_port_id();
            let value_handle =
                handles
                    .remove(&value_port)
                    .ok_or(ConditionError::MissingCasePort {
                        node: authoring.node_id(),
                        port: value_port,
                    })?;
            Ok(ConditionRenderCase {
                domain: case.clone(),
                route_handle,
                value_handle,
            })
        })
        .collect::<Result<Vec<_>, ConditionError>>()?;
    let else_branch = observation.else_branch().clone();
    let else_port = else_branch.port_id();
    let else_handle = handles
        .remove(&else_port)
        .ok_or(ConditionError::MissingElsePort {
            node: authoring.node_id(),
        })?;
    let else_handle = product_presentation
        .port(&jellyflow_core::PortKey::new(CONDITION_ELSE_PORT_KEY))
        .map(|presentation| {
            else_handle
                .clone()
                .with_side(open_gpui_side(presentation.side()))
        })
        .unwrap_or(else_handle);
    let input_port = observation.input_port();
    let input_handle = handles
        .remove(&input_port)
        .ok_or(ConditionError::MissingInputPort {
            node: authoring.node_id(),
        })?;
    let input_handle = product_presentation
        .port(&jellyflow_core::PortKey::new("input"))
        .map(|presentation| {
            input_handle
                .clone()
                .with_side(open_gpui_side(presentation.side()))
        })
        .unwrap_or(input_handle);
    Ok(ConditionRenderData {
        title: observation.title().to_owned(),
        cases,
        else_branch: Some((else_branch, else_handle)),
        inputs: vec![WorkflowPort {
            id: input_port,
            direction: PortDirection::In,
            kind: PortKind::Exec,
            handle: input_handle,
        }],
        observation: Some(observation),
    })
}

fn condition_observation(
    authoring: &OpenGpuiNodeAuthoring,
) -> Result<ConditionObservation, ConditionError> {
    let ports = authoring
        .ports()
        .map(|port| (port.id(), port.facts().clone()))
        .collect();
    ConditionObservation::from_parts(
        authoring.version(),
        authoring.node_id(),
        authoring.node(),
        ports,
    )
}

fn retired_render_data() -> ConditionRenderData {
    ConditionRenderData {
        observation: None,
        title: "Retired condition".to_owned(),
        cases: Vec::new(),
        else_branch: None,
        inputs: Vec::new(),
    }
}

fn condition_intent_label(intent: &ConditionIntent) -> String {
    match intent {
        ConditionIntent::Add(case) => case.label().to_owned(),
        ConditionIntent::Reorder { case_id, index } => {
            format!("Move {case_id} to position {}", index + 1)
        }
        ConditionIntent::Remove { case_id, .. } => format!("Remove {case_id}"),
    }
}

#[must_use]
pub fn workflow_condition_node_id() -> NodeId {
    NodeId::from_u128(WORKFLOW_CONDITION_NODE_ID)
}

#[must_use]
pub fn workflow_condition_add_selector(node: NodeId) -> String {
    format!("jellyflow-workflow-condition-add-{}", node.0)
}

#[must_use]
pub fn workflow_condition_body_selector(node: NodeId) -> String {
    format!("jellyflow-workflow-condition-body-{}", node.0)
}

#[must_use]
pub fn workflow_condition_node_selector(node: NodeId) -> String {
    format!("jellyflow-workflow-condition-node-{}", node.0)
}

#[must_use]
pub fn workflow_condition_case_label_selector(node: NodeId, port: PortId) -> String {
    format!(
        "jellyflow-workflow-condition-case-label-{}-{}",
        node.0, port.0
    )
}

#[must_use]
pub fn workflow_condition_case_actions_selector(node: NodeId, port: PortId) -> String {
    format!(
        "jellyflow-workflow-condition-case-actions-{}-{}",
        node.0, port.0
    )
}

#[must_use]
pub fn workflow_condition_else_label_selector(node: NodeId, port: PortId) -> String {
    format!(
        "jellyflow-workflow-condition-else-label-{}-{}",
        node.0, port.0
    )
}

#[must_use]
pub fn workflow_condition_case_selector(node: NodeId, port: PortId) -> String {
    format!("jellyflow-workflow-condition-case-{}-{}", node.0, port.0)
}

#[must_use]
pub fn workflow_condition_case_value_selector(node: NodeId, port: PortId) -> String {
    format!(
        "jellyflow-workflow-condition-case-value-{}-{}",
        node.0, port.0
    )
}

#[must_use]
pub fn workflow_condition_else_selector(node: NodeId, port: PortId) -> String {
    format!("jellyflow-workflow-condition-else-{}-{}", node.0, port.0)
}

#[must_use]
pub fn workflow_condition_up_selector(node: NodeId, port: PortId) -> String {
    format!("jellyflow-workflow-condition-up-{}-{}", node.0, port.0)
}

#[must_use]
pub fn workflow_condition_remove_selector(node: NodeId, port: PortId) -> String {
    format!("jellyflow-workflow-condition-remove-{}-{}", node.0, port.0)
}

#[must_use]
pub fn workflow_condition_scroll_selector(node: NodeId) -> String {
    format!("jellyflow-workflow-condition-scroll-{}", node.0)
}

#[must_use]
pub fn workflow_condition_pending_selector(node: NodeId) -> String {
    format!("jellyflow-workflow-condition-pending-{}", node.0)
}

#[must_use]
pub fn workflow_condition_value_selector(node: NodeId) -> String {
    format!("text-input:{}:root", workflow_condition_value_id(node))
}

#[must_use]
pub fn workflow_condition_field_selector(node: NodeId) -> String {
    format!("select:{}:trigger", workflow_condition_field_id(node))
}

#[must_use]
pub fn workflow_condition_provider_failed_selector(node: NodeId) -> String {
    format!("jellyflow-workflow-condition-provider-failed-{}", node.0)
}

#[must_use]
pub fn workflow_condition_diagnostic_selector(node: NodeId) -> String {
    format!("jellyflow-workflow-condition-diagnostic-{}", node.0)
}

fn workflow_condition_retry_selector(node: NodeId) -> String {
    format!("jellyflow-workflow-condition-retry-{}", node.0)
}

fn workflow_condition_discard_selector(node: NodeId) -> String {
    format!("jellyflow-workflow-condition-discard-{}", node.0)
}

fn workflow_condition_add_id(node: NodeId) -> String {
    format!("jellyflow-workflow-condition-add-button-{}", node.0)
}

fn workflow_condition_value_id(node: NodeId) -> String {
    format!("jellyflow-workflow-condition-value-{}", node.0)
}

fn workflow_condition_field_id(node: NodeId) -> String {
    format!("jellyflow-workflow-condition-field-{}", node.0)
}

fn condition_literal(value: &str, ty: &TypeDesc) -> Result<serde_json::Value, String> {
    match ty {
        TypeDesc::String => Ok(serde_json::Value::String(value.to_owned())),
        TypeDesc::Float => value
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map(serde_json::Value::Number)
            .ok_or_else(|| format!("condition value {value:?} is not a finite float")),
        TypeDesc::Int => value
            .parse::<i64>()
            .map(serde_json::Value::from)
            .map_err(|_| format!("condition value {value:?} is not an integer")),
        TypeDesc::Bool => value
            .parse::<bool>()
            .map(serde_json::Value::from)
            .map_err(|_| format!("condition value {value:?} is not a boolean")),
        other => Err(format!(
            "condition field type {} has no native literal editor",
            condition_type_label(other)
        )),
    }
}

fn condition_type_label(ty: &TypeDesc) -> &'static str {
    match ty {
        TypeDesc::Any => "ANY",
        TypeDesc::Unknown => "UNKNOWN",
        TypeDesc::Never => "NEVER",
        TypeDesc::Null => "NULL",
        TypeDesc::Bool => "BOOL",
        TypeDesc::Int => "INT",
        TypeDesc::Float => "FLOAT",
        TypeDesc::String => "STRING",
        TypeDesc::Bytes => "BYTES",
        TypeDesc::List { .. } => "LIST",
        TypeDesc::Map { .. } => "MAP",
        TypeDesc::Object { .. } => "OBJECT",
        TypeDesc::Union { .. } => "UNION",
        TypeDesc::Option { .. } => "OPTION",
        TypeDesc::Var { .. } => "VAR",
        TypeDesc::Opaque { .. } => "OPAQUE",
    }
}
