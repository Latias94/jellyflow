use std::collections::{BTreeMap, BTreeSet};

use jellyflow_core::{
    CanvasSize, Node, NodeId, Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
};
use jellyflow_open_gpui::authoring::{
    OpenGpuiNodeAuthoring, OpenGpuiNodeBinding, OpenGpuiPortHandle,
};
use jellyflow_open_gpui::chrome::OpenGpuiPortMarker;
use jellyflow_open_gpui::{
    OpenGpuiCommandTicketResult, OpenGpuiContentHeightPolicy, OpenGpuiNodeRegistrationPolicy,
    OpenGpuiNodeRegistryBuilder, OpenGpuiNodeSizePolicy,
};
use jellyflow_runtime::schema::{NodeKindSpec, NodePortSpec};
use jellyflow_runtime::{
    GraphCommand, GraphRevision, GraphSnapshot, IncidentEdgePolicy, NodeInstanceKey, TopologyEdit,
    TopologyPort,
};
use open_gpui::{
    AppContext, Context, Entity, InteractiveElement, ParentElement, Render,
    StatefulInteractiveElement, Styled, Window, div, px, rgb,
};
use open_gpui_ui_components::{Button, ButtonVariant};

use super::WORKFLOW_PORT_DIAMETER;
use super::{WorkflowNodeError, WorkflowNodeStatus, WorkflowPort, workflow_alert, workflow_header};
use crate::nodes::native_graph_theme;
use crate::nodes::topology_attempt::{
    TopologyAttempt, TopologyAttemptPhase, TopologyDispatchRequest,
};

pub const WORKFLOW_CONDITION_KIND: &str = "workflow.condition";
pub const WORKFLOW_CONDITION_RENDERER: &str = "jellyflow.open-gpui.workflow.condition";
pub const WORKFLOW_CONDITION_DEFAULT_SIZE: CanvasSize = CanvasSize {
    width: 360.0,
    height: 290.0,
};

const WORKFLOW_CONDITION_NODE_ID: u128 = 0xd1fa_0002;
const CONDITION_BODY_HORIZONTAL_PADDING: f32 = 14.0;
const CONDITION_CASE_HORIZONTAL_PADDING: f32 = 9.0;
const CONDITION_CASE_ROW_HEIGHT: f32 = 37.0;
const CONDITION_CASE_LIST_MAX_HEIGHT: f32 = 148.0;
const CONDITION_MIN_HEIGHT: f32 = 180.0;
const CONDITION_MAX_HEIGHT: f32 = 360.0;

const CONDITION_ELSE_PORT_KEY: &str = "else";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkflowCase {
    pub(super) id: String,
    pub(super) label: String,
    pub(super) port_id: PortId,
}

impl WorkflowCase {
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        port_id: PortId,
    ) -> Result<Self, WorkflowNodeError> {
        let id = id.into();
        if id.trim().is_empty() {
            return Err(WorkflowNodeError::EmptyCaseId);
        }
        if id == CONDITION_ELSE_PORT_KEY {
            return Err(WorkflowNodeError::ReservedCaseId { case_id: id });
        }
        let label = label.into();
        if label.trim().is_empty() {
            return Err(WorkflowNodeError::EmptyCaseLabel { case_id: id });
        }
        Ok(Self { id, label, port_id })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub const fn port_id(&self) -> PortId {
        self.port_id
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkflowElseBranch {
    label: String,
    port_id: PortId,
}

impl WorkflowElseBranch {
    pub fn label(&self) -> &str {
        &self.label
    }

    pub const fn port_id(&self) -> PortId {
        self.port_id
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkflowConditionOutputs {
    cases: Vec<WorkflowCase>,
    else_branch: WorkflowElseBranch,
}

impl WorkflowConditionOutputs {
    pub fn cases(&self) -> &[WorkflowCase] {
        &self.cases
    }

    pub const fn else_branch(&self) -> &WorkflowElseBranch {
        &self.else_branch
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConditionCaseAction {
    Add(WorkflowCase),
    Reorder { case_id: String, index: usize },
    Remove { case_id: String },
}

pub struct WorkflowConditionNode {
    binding: Entity<OpenGpuiNodeBinding>,
    node_id: NodeId,
    topology: TopologyAttempt<ConditionCaseIntent>,
    diagnostic: Option<String>,
}

struct ConditionRenderData {
    instance: NodeInstanceKey,
    revision: GraphRevision,
    title: String,
    cases: Vec<(WorkflowCase, OpenGpuiPortHandle)>,
    else_branch: Option<(WorkflowElseBranch, OpenGpuiPortHandle)>,
    inputs: Vec<WorkflowPort>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ConditionCaseIntent {
    action: ConditionCaseAction,
    incident_edges: IncidentEdgePolicy,
}

impl WorkflowConditionNode {
    fn new(binding: Entity<OpenGpuiNodeBinding>, instance: NodeInstanceKey) -> Self {
        Self {
            binding,
            node_id: instance.node(),
            topology: TopologyAttempt::new(instance),
            diagnostic: None,
        }
    }

    fn apply_action(
        &mut self,
        action: ConditionCaseAction,
        incident_edges: IncidentEdgePolicy,
        cx: &mut Context<Self>,
    ) {
        if self.topology.phase() != TopologyAttemptPhase::Idle {
            return;
        }
        self.enqueue_request(
            TopologyDispatchRequest::new(ConditionCaseIntent {
                action,
                incident_edges,
            }),
            cx,
        );
        cx.notify();
    }

    fn add_case(&mut self, cx: &mut Context<Self>) {
        if self.topology.phase() != TopologyAttemptPhase::Idle {
            return;
        }
        let port_id = PortId::new();
        let case_count = self
            .binding
            .read(cx)
            .authoring()
            .ok()
            .and_then(|authoring| parse_condition_cases(self.node_id, &authoring.node().data).ok())
            .map_or(0, |cases| cases.len());
        let case = WorkflowCase {
            id: format!("case-{}", port_id.0),
            label: format!("Case {}", case_count + 1),
            port_id,
        };
        self.apply_action(
            ConditionCaseAction::Add(case),
            IncidentEdgePolicy::Reject,
            cx,
        );
    }

    fn retry(&mut self, cx: &mut Context<Self>) {
        let retry = match self.topology.phase() {
            TopologyAttemptPhase::Recoverable => self.topology.retry(),
            TopologyAttemptPhase::Uncertain => self.topology.confirm_uncertain_retry(),
            _ => None,
        };
        let Some(retry) = retry else {
            return;
        };
        self.enqueue_request(TopologyDispatchRequest::retry(retry), cx);
        cx.notify();
    }

    fn enqueue_request(
        &mut self,
        request: TopologyDispatchRequest<ConditionCaseIntent>,
        cx: &mut Context<Self>,
    ) {
        let authoring = match self.binding.read(cx).authoring() {
            Ok(authoring) => authoring,
            Err(error) => {
                self.topology.observe_authoring_error(&error);
                self.diagnostic = Some(error.to_string());
                return;
            }
        };
        let cases = match parse_condition_cases(self.node_id, &authoring.node().data) {
            Ok(cases) => cases,
            Err(error) => {
                self.diagnostic = Some(error.to_string());
                return;
            }
        };
        if condition_action_satisfied(&cases, &request.intent().action) {
            self.topology
                .observe(authoring.instance(), authoring.revision(), |intent| {
                    condition_action_satisfied(&cases, &intent.action)
                });
            self.diagnostic = None;
            return;
        }
        let ports = authoring
            .ports()
            .map(|port| (port.id(), port.facts().clone()))
            .collect();
        let command = build_condition_case_command(
            authoring.revision(),
            self.node_id,
            authoring.node(),
            &ports,
            request.intent().action.clone(),
            request.intent().incident_edges,
        );
        let command = match command {
            Ok(command) => command,
            Err(error) => {
                self.diagnostic = Some(error.to_string());
                return;
            }
        };
        let activity = self.binding.read(cx).activity().clone();
        let dispatch = self
            .topology
            .dispatch(authoring, command, request, &activity, cx);
        let dispatch = match dispatch {
            Ok(dispatch) => dispatch,
            Err(error) => {
                self.diagnostic = Some(error.to_string());
                return;
            }
        };
        let attempt = dispatch.attempt();
        let ticket = dispatch.into_ticket();
        let task = cx.spawn(async move |node, cx| {
            let result = ticket.await;
            let _ = node.update(cx, |node, cx| node.finish_command(attempt, result, cx));
        });
        self.topology.own_task(attempt, task);
        self.diagnostic = None;
    }

    fn finish_command(
        &mut self,
        attempt: u64,
        result: OpenGpuiCommandTicketResult,
        cx: &mut Context<Self>,
    ) {
        if self.topology.finish(attempt, result)
            && let Err(error) = self
                .binding
                .update(cx, |binding, cx| binding.invalidate_layout(cx))
        {
            self.diagnostic = Some(format!(
                "condition content changed but native layout invalidation failed: {error}"
            ));
        } else {
            self.diagnostic = self.topology.diagnostic().map(str::to_owned);
        }
        cx.notify();
    }

    fn reconcile_command(
        &mut self,
        instance: &NodeInstanceKey,
        revision: GraphRevision,
        cases: &[WorkflowCase],
    ) {
        self.topology.observe(instance, revision, |intent| {
            condition_action_satisfied(cases, &intent.action)
        });
        self.diagnostic = self.topology.diagnostic().map(str::to_owned);
    }

    fn command_status(&self) -> WorkflowNodeStatus {
        match self.topology.phase() {
            TopologyAttemptPhase::Idle if self.diagnostic.is_some() => WorkflowNodeStatus::Error,
            TopologyAttemptPhase::Idle => WorkflowNodeStatus::Ready,
            TopologyAttemptPhase::Pending | TopologyAttemptPhase::AwaitingSnapshot => {
                WorkflowNodeStatus::Draft
            }
            TopologyAttemptPhase::Recoverable => WorkflowNodeStatus::Conflict,
            TopologyAttemptPhase::Uncertain
            | TopologyAttemptPhase::Retired
            | TopologyAttemptPhase::Closed => WorkflowNodeStatus::Error,
        }
    }
}

impl Render for WorkflowConditionNode {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        let node_id = self.node_id;
        let drag_handle = self.binding.read(cx).drag_handle();
        let authoring = self.binding.read(cx).authoring();
        let data = match authoring {
            Ok(authoring) => read_condition_authoring(&authoring),
            Err(error) => {
                self.topology.observe_authoring_error(&error);
                Err(binding_error(error))
            }
        };
        let binding_valid = data.is_ok();
        let data = match data {
            Ok(value) => {
                let cases = value
                    .cases
                    .iter()
                    .map(|(case, _)| case.clone())
                    .collect::<Vec<_>>();
                self.reconcile_command(&value.instance, value.revision, &cases);
                value
            }
            Err(error) => {
                self.diagnostic = Some(error.to_string());
                ConditionRenderData {
                    instance: self.binding.read(cx).drag_handle().instance().clone(),
                    revision: 0,
                    title: "Retired condition".to_owned(),
                    cases: Vec::new(),
                    else_branch: None,
                    inputs: Vec::new(),
                }
            }
        };
        let phase = self.topology.phase();
        let authoritative_cases = data
            .cases
            .iter()
            .map(|(case, _)| case.clone())
            .collect::<Vec<_>>();
        let commands_locked = phase != TopologyAttemptPhase::Idle || !binding_valid;
        let pending_label = if matches!(
            phase,
            TopologyAttemptPhase::Pending | TopologyAttemptPhase::AwaitingSnapshot
        ) {
            self.topology.active_intent().cloned().and_then(|intent| {
                if condition_action_satisfied(&authoritative_cases, &intent.action) {
                    return None;
                }
                Some(match intent.action {
                    ConditionCaseAction::Add(case) => case.label().to_owned(),
                    ConditionCaseAction::Reorder { case_id, index } => {
                        format!("Move {case_id} to position {}", index + 1)
                    }
                    ConditionCaseAction::Remove { case_id } => format!("Remove {case_id}"),
                })
            })
        } else {
            None
        };
        let add_node = cx.weak_entity();
        let add_button = div()
            .px(px(CONDITION_BODY_HORIZONTAL_PADDING))
            .debug_selector(move || workflow_condition_add_selector(node_id))
            .child(
                Button::new(workflow_condition_add_id(node_id), "Add case")
                    .variant(ButtonVariant::Secondary)
                    .disabled(commands_locked)
                    .on_click(move |_, window, cx| {
                        let _ = add_node.update(cx, |node, cx| node.add_case(cx));
                        window.refresh();
                    }),
            );
        let mut rows = div()
            .px(px(CONDITION_BODY_HORIZONTAL_PADDING))
            .flex()
            .flex_col()
            .gap(px(7.0));
        for (index, (case, handle)) in data.cases.into_iter().enumerate() {
            let port_id = case.port_id();
            let row_selector = workflow_condition_case_selector(node_id, port_id);
            let chrome = OpenGpuiPortMarker::new(handle.presentation(), native_graph_theme());
            let minimum_hit_diameter = chrome.minimum_hit_diameter();
            let marker = handle
                .with_minimum_hit_diameter(minimum_hit_diameter)
                .interactive(
                    div()
                        .debug_selector(move || row_selector.clone())
                        .w(px(WORKFLOW_PORT_DIAMETER))
                        .h(px(WORKFLOW_PORT_DIAMETER))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(chrome),
                );
            let marker = div()
                .absolute()
                .top(px(
                    (CONDITION_CASE_ROW_HEIGHT - WORKFLOW_PORT_DIAMETER) / 2.0
                ))
                .right(px(
                    -(CONDITION_BODY_HORIZONTAL_PADDING + CONDITION_CASE_HORIZONTAL_PADDING)
                ))
                .child(marker);
            let mut actions = div().flex().gap(px(4.0));
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
                            let action = ConditionCaseAction::Reorder {
                                case_id: case_id.clone(),
                                index: index - 1,
                            };
                            let _ = move_node.update(cx, |node, cx| {
                                node.apply_action(action, IncidentEdgePolicy::Reject, cx)
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
                        let action = ConditionCaseAction::Remove {
                            case_id: case_id.clone(),
                        };
                        let _ = remove_node.update(cx, |node, cx| {
                            node.apply_action(action, IncidentEdgePolicy::Cascade, cx)
                        });
                        window.refresh();
                    }),
                ),
            );
            rows = rows.child(
                div()
                    .relative()
                    .h(px(CONDITION_CASE_ROW_HEIGHT))
                    .px(px(CONDITION_CASE_HORIZONTAL_PADDING))
                    .flex()
                    .items_center()
                    .justify_between()
                    .rounded(px(4.0))
                    .bg(rgb(0x292d35))
                    .child(div().text_sm().child(case.label().to_owned()))
                    .child(actions)
                    .child(marker),
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
                    .rounded(px(4.0))
                    .border_1()
                    .border_color(rgb(0x59616c))
                    .text_color(rgb(0xaab6c6))
                    .child(div().text_sm().child(label))
                    .child(div().text_xs().child("Pending")),
            );
        }
        if let Some((else_branch, handle)) = data.else_branch {
            let port_id = else_branch.port_id();
            let row_selector = workflow_condition_else_selector(node_id, port_id);
            let chrome = OpenGpuiPortMarker::new(handle.presentation(), native_graph_theme());
            let minimum_hit_diameter = chrome.minimum_hit_diameter();
            let marker = handle
                .with_minimum_hit_diameter(minimum_hit_diameter)
                .interactive(
                    div()
                        .debug_selector(move || row_selector.clone())
                        .w(px(WORKFLOW_PORT_DIAMETER))
                        .h(px(WORKFLOW_PORT_DIAMETER))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(chrome),
                );
            rows = rows.child(
                div()
                    .relative()
                    .h(px(CONDITION_CASE_ROW_HEIGHT))
                    .px(px(CONDITION_CASE_HORIZONTAL_PADDING))
                    .flex()
                    .items_center()
                    .justify_between()
                    .rounded(px(4.0))
                    .bg(rgb(0x23272e))
                    .child(div().text_sm().child(else_branch.label().to_owned()))
                    .child(div().text_xs().text_color(rgb(0x93a0b1)).child("Fallback"))
                    .child(
                        div()
                            .absolute()
                            .top(px(
                                (CONDITION_CASE_ROW_HEIGHT - WORKFLOW_PORT_DIAMETER) / 2.0
                            ))
                            .right(px(-(CONDITION_BODY_HORIZONTAL_PADDING
                                + CONDITION_CASE_HORIZONTAL_PADDING)))
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
            && (phase == TopologyAttemptPhase::Recoverable
                || (phase == TopologyAttemptPhase::Uncertain
                    && self.topology.can_confirm_uncertain_retry()));
        let retry = retry_allowed.then(|| {
            let retry_node = cx.weak_entity();
            let retry_selector = workflow_condition_retry_selector(node_id);
            div().px(px(CONDITION_BODY_HORIZONTAL_PADDING)).child(
                div().debug_selector(move || retry_selector.clone()).child(
                    Button::new(
                        format!("jellyflow-workflow-condition-retry-button-{}", node_id.0),
                        if phase == TopologyAttemptPhase::Uncertain {
                            "Confirm retry"
                        } else {
                            "Retry"
                        },
                    )
                    .variant(ButtonVariant::Secondary)
                    .on_click(move |_, window, cx| {
                        let _ = retry_node.update(cx, |node, cx| node.retry(cx));
                        window.refresh();
                    }),
                ),
            )
        });
        let controls = div()
            .py(px(12.0))
            .flex()
            .flex_col()
            .gap(px(9.0))
            .child(rows)
            .child(add_button)
            .children(retry);
        let status = self.command_status();
        let diagnostic = self.diagnostic.clone().map(|message| {
            workflow_alert(
                format!("jellyflow-workflow-condition-diagnostic-{}", node_id.0),
                format!("Workflow condition error: {message}"),
            )
            .px(px(14.0))
            .text_xs()
            .text_color(rgb(0xe58b7b))
            .child(message)
        });

        let content = div()
            .relative()
            .w_full()
            .rounded(px(6.0))
            .border_1()
            .border_color(rgb(0x66583c))
            .bg(rgb(0x24262c))
            .text_color(rgb(0xf0ece4))
            .child(drag_handle.wrap(workflow_header(
                node_id,
                data.title,
                "CONDITION",
                status,
                data.inputs,
            )))
            .child(controls)
            .children(diagnostic);
        self.binding.read(cx).measure_content(content)
    }
}

pub(super) fn catalog_spec() -> NodeKindSpec {
    NodeKindSpec::builder(
        WORKFLOW_CONDITION_KIND,
        "Condition",
        WORKFLOW_CONDITION_RENDERER,
    )
    .category(["Workflow", "Logic"])
    .keywords(["condition", "case", "route", "if"])
    .default_size(WORKFLOW_CONDITION_DEFAULT_SIZE)
    .port(
        NodePortSpec::new(
            "input",
            PortDirection::In,
            PortKind::Exec,
            PortCapacity::Single,
        )
        .with_label("Input"),
    )
    .port(
        NodePortSpec::new(
            CONDITION_ELSE_PORT_KEY,
            PortDirection::Out,
            PortKind::Exec,
            PortCapacity::Multi,
        )
        .with_label("Else"),
    )
    .default_data(serde_json::json!({
        "title": "Condition",
        "cases": [],
        "else_branch": { "label": "Else" },
    }))
    .build()
}

pub(super) fn register(registry: &mut OpenGpuiNodeRegistryBuilder) {
    let policy = OpenGpuiNodeRegistrationPolicy::default().with_size_policy(
        OpenGpuiNodeSizePolicy::ContentHeight(
            OpenGpuiContentHeightPolicy::new(CONDITION_MIN_HEIGHT, CONDITION_MAX_HEIGHT)
                .expect("workflow condition content-height bounds must be valid"),
        ),
    );
    registry.register_with_policy(
        WORKFLOW_CONDITION_RENDERER,
        policy,
        |binding, _window, cx| {
            let instance = binding
                .read(cx)
                .authoring()
                .expect("a condition node factory must receive a live authoring binding")
                .instance()
                .clone();
            cx.new(|_| WorkflowConditionNode::new(binding, instance))
        },
    );
}

#[must_use]
pub fn workflow_condition_node_id() -> NodeId {
    NodeId::from_u128(WORKFLOW_CONDITION_NODE_ID)
}

pub fn condition_outputs(
    snapshot: &GraphSnapshot,
    node_id: NodeId,
) -> Result<WorkflowConditionOutputs, WorkflowNodeError> {
    let node = snapshot
        .graph()
        .node(&node_id)
        .ok_or(WorkflowNodeError::MissingNode { node: node_id })?;
    ensure_kind(node_id, node)?;
    let cases = parse_condition_cases(node_id, &node.data)?;
    let ports = node
        .ports
        .iter()
        .filter_map(|port_id| {
            snapshot
                .graph()
                .port(port_id)
                .cloned()
                .map(|port| (*port_id, port))
        })
        .collect::<BTreeMap<_, _>>();
    let else_branch = parse_else_branch(node_id, &node.data, &ports)?;
    validate_condition_ports(node_id, node, &cases, &else_branch, &ports)?;
    Ok(WorkflowConditionOutputs { cases, else_branch })
}

pub fn condition_cases(
    snapshot: &GraphSnapshot,
    node_id: NodeId,
) -> Result<Vec<WorkflowCase>, WorkflowNodeError> {
    Ok(condition_outputs(snapshot, node_id)?.cases)
}

pub fn plan_condition_case_action(
    snapshot: &GraphSnapshot,
    node_id: NodeId,
    action: ConditionCaseAction,
    incident_edges: IncidentEdgePolicy,
) -> Result<GraphCommand, WorkflowNodeError> {
    let node = snapshot
        .graph()
        .node(&node_id)
        .ok_or(WorkflowNodeError::MissingNode { node: node_id })?;
    let ports = node
        .ports
        .iter()
        .filter_map(|port_id| {
            snapshot
                .graph()
                .port(port_id)
                .cloned()
                .map(|port| (*port_id, port))
        })
        .collect();
    build_condition_case_command(
        snapshot.revision(),
        node_id,
        node,
        &ports,
        action,
        incident_edges,
    )
}

#[must_use]
pub fn workflow_condition_add_selector(node: NodeId) -> String {
    format!("jellyflow-workflow-condition-add-{}", node.0)
}

#[must_use]
pub fn workflow_condition_case_selector(node: NodeId, port: PortId) -> String {
    format!("jellyflow-workflow-condition-case-{}-{}", node.0, port.0)
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

fn workflow_condition_retry_selector(node: NodeId) -> String {
    format!("jellyflow-workflow-condition-retry-{}", node.0)
}

fn workflow_condition_add_id(node: NodeId) -> String {
    format!("jellyflow-workflow-condition-add-button-{}", node.0)
}

fn build_condition_case_command(
    revision: GraphRevision,
    node_id: NodeId,
    node: &Node,
    ports: &BTreeMap<PortId, Port>,
    action: ConditionCaseAction,
    incident_edges: IncidentEdgePolicy,
) -> Result<GraphCommand, WorkflowNodeError> {
    ensure_kind(node_id, node)?;
    let original_cases = parse_condition_cases(node_id, &node.data)?;
    let else_branch = parse_else_branch(node_id, &node.data, ports)?;
    validate_condition_ports(node_id, node, &original_cases, &else_branch, ports)?;
    let mut cases = original_cases.clone();
    let label = match action {
        ConditionCaseAction::Add(case) => {
            if cases.iter().any(|existing| existing.id == case.id) {
                return Err(WorkflowNodeError::DuplicateCaseId { case_id: case.id });
            }
            if cases
                .iter()
                .any(|existing| existing.port_id == case.port_id)
                || else_branch.port_id == case.port_id
            {
                return Err(WorkflowNodeError::DuplicateCasePort { port: case.port_id });
            }
            cases.push(case);
            "add condition case"
        }
        ConditionCaseAction::Reorder { case_id, index } => {
            if index >= cases.len() {
                return Err(WorkflowNodeError::CaseReorderIndexOutOfBounds {
                    index,
                    case_count: cases.len(),
                });
            }
            let current = cases
                .iter()
                .position(|case| case.id == case_id)
                .ok_or_else(|| WorkflowNodeError::CaseNotFound {
                    case_id: case_id.clone(),
                })?;
            let case = cases.remove(current);
            cases.insert(index, case);
            "reorder condition case"
        }
        ConditionCaseAction::Remove { case_id } => {
            let current = cases
                .iter()
                .position(|case| case.id == case_id)
                .ok_or_else(|| WorkflowNodeError::CaseNotFound {
                    case_id: case_id.clone(),
                })?;
            cases.remove(current);
            "remove condition case"
        }
    };
    validate_unique_cases(&cases)?;
    let case_ids: BTreeSet<_> = original_cases.iter().map(WorkflowCase::port_id).collect();
    let mut topology_ports: Vec<_> = node
        .ports
        .iter()
        .filter(|port_id| !case_ids.contains(port_id) && **port_id != else_branch.port_id)
        .map(|port_id| {
            ports
                .get(port_id)
                .cloned()
                .map(|port| TopologyPort::new(*port_id, port))
                .ok_or(WorkflowNodeError::MissingCasePort {
                    node: node_id,
                    port: *port_id,
                })
        })
        .collect::<Result<_, _>>()?;
    for case in &cases {
        let port = ports
            .get(&case.port_id)
            .cloned()
            .unwrap_or_else(|| case_port(node_id, case));
        topology_ports.push(TopologyPort::new(case.port_id, port));
    }
    let else_port = ports
        .get(&else_branch.port_id)
        .cloned()
        .ok_or(WorkflowNodeError::MissingElsePort { node: node_id })?;
    topology_ports.push(TopologyPort::new(else_branch.port_id, else_port));
    let next_data = replace_condition_cases(node_id, &node.data, &cases)?;
    Ok(GraphCommand::topology(
        revision,
        TopologyEdit::new()
            .set_node_data(node_id, next_data)
            .replace_node_ports(node_id, topology_ports, incident_edges),
    )
    .with_label(label))
}

fn read_condition_authoring(
    authoring: &OpenGpuiNodeAuthoring,
) -> Result<ConditionRenderData, WorkflowNodeError> {
    let node_id = authoring.node_id();
    ensure_kind(node_id, authoring.node())?;
    let cases = parse_condition_cases(node_id, &authoring.node().data)?;
    let ports: BTreeMap<_, _> = authoring
        .ports()
        .map(|port| (port.id(), port.facts().clone()))
        .collect();
    let else_branch = parse_else_branch(node_id, &authoring.node().data, &ports)?;
    let mut handles = authoring
        .ports()
        .map(|port| (port.id(), port.handle().clone()))
        .collect::<BTreeMap<_, _>>();
    validate_condition_ports(node_id, authoring.node(), &cases, &else_branch, &ports)?;
    let case_rows = cases
        .into_iter()
        .map(|case| {
            handles
                .remove(&case.port_id())
                .map(|handle| (case.clone(), handle))
                .ok_or(WorkflowNodeError::MissingCasePort {
                    node: node_id,
                    port: case.port_id(),
                })
        })
        .collect::<Result<_, _>>()?;
    let else_handle = handles
        .remove(&else_branch.port_id())
        .ok_or(WorkflowNodeError::MissingElsePort { node: node_id })?;
    let inputs = authoring
        .ports()
        .filter(|port| port.facts().dir == PortDirection::In)
        .filter_map(|port| {
            handles.remove(&port.id()).map(|handle| WorkflowPort {
                id: port.id(),
                direction: port.facts().dir,
                handle,
            })
        })
        .collect();
    let title = authoring
        .node()
        .data
        .get("title")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("Condition")
        .to_owned();
    Ok(ConditionRenderData {
        instance: authoring.instance().clone(),
        revision: authoring.revision(),
        title,
        cases: case_rows,
        else_branch: Some((else_branch, else_handle)),
        inputs,
    })
}

fn condition_action_satisfied(cases: &[WorkflowCase], action: &ConditionCaseAction) -> bool {
    match action {
        ConditionCaseAction::Add(case) => cases.iter().any(|current| current == case),
        ConditionCaseAction::Reorder { case_id, index } => {
            cases.get(*index).is_some_and(|case| case.id() == case_id)
        }
        ConditionCaseAction::Remove { case_id } => cases.iter().all(|case| case.id() != case_id),
    }
}

fn binding_error(error: impl std::fmt::Display) -> WorkflowNodeError {
    WorkflowNodeError::BindingUnavailable {
        message: error.to_string(),
    }
}

fn parse_condition_cases(
    node_id: NodeId,
    data: &serde_json::Value,
) -> Result<Vec<WorkflowCase>, WorkflowNodeError> {
    let cases = data
        .get("cases")
        .and_then(serde_json::Value::as_array)
        .ok_or(WorkflowNodeError::InvalidData {
            node: node_id,
            field: "cases",
        })?;
    let cases = cases
        .iter()
        .map(|case| {
            let id = case.get("id").and_then(serde_json::Value::as_str).ok_or(
                WorkflowNodeError::InvalidData {
                    node: node_id,
                    field: "cases[].id",
                },
            )?;
            let label = case
                .get("label")
                .and_then(serde_json::Value::as_str)
                .ok_or(WorkflowNodeError::InvalidData {
                    node: node_id,
                    field: "cases[].label",
                })?;
            let port_id = case
                .get("port_id")
                .cloned()
                .ok_or(WorkflowNodeError::InvalidData {
                    node: node_id,
                    field: "cases[].port_id",
                })
                .and_then(|value| {
                    serde_json::from_value(value).map_err(|_| WorkflowNodeError::InvalidData {
                        node: node_id,
                        field: "cases[].port_id",
                    })
                })?;
            WorkflowCase::new(id, label, port_id)
        })
        .collect::<Result<Vec<_>, _>>()?;
    validate_unique_cases(&cases)?;
    Ok(cases)
}

fn parse_else_branch(
    node_id: NodeId,
    data: &serde_json::Value,
    ports: &BTreeMap<PortId, Port>,
) -> Result<WorkflowElseBranch, WorkflowNodeError> {
    let label = data
        .get("else_branch")
        .and_then(|value| value.get("label"))
        .and_then(serde_json::Value::as_str)
        .filter(|label| *label == "Else")
        .ok_or(WorkflowNodeError::InvalidData {
            node: node_id,
            field: "else_branch.label",
        })?;
    let mut matching = ports
        .iter()
        .filter(|(_, port)| port.node == node_id && port.key.0 == CONDITION_ELSE_PORT_KEY);
    let Some((port_id, _)) = matching.next() else {
        return Err(WorkflowNodeError::MissingElsePort { node: node_id });
    };
    if matching.next().is_some() {
        return Err(WorkflowNodeError::InvalidElsePort {
            node: node_id,
            port: *port_id,
        });
    }
    Ok(WorkflowElseBranch {
        label: label.to_owned(),
        port_id: *port_id,
    })
}

fn validate_unique_cases(cases: &[WorkflowCase]) -> Result<(), WorkflowNodeError> {
    let mut ids = BTreeSet::new();
    let mut ports = BTreeSet::new();
    for case in cases {
        if !ids.insert(case.id.as_str()) {
            return Err(WorkflowNodeError::DuplicateCaseId {
                case_id: case.id.clone(),
            });
        }
        if !ports.insert(case.port_id) {
            return Err(WorkflowNodeError::DuplicateCasePort { port: case.port_id });
        }
    }
    Ok(())
}

fn validate_condition_ports(
    node_id: NodeId,
    node: &Node,
    cases: &[WorkflowCase],
    else_branch: &WorkflowElseBranch,
    ports: &BTreeMap<PortId, Port>,
) -> Result<(), WorkflowNodeError> {
    for case in cases {
        let port = ports
            .get(&case.port_id)
            .ok_or(WorkflowNodeError::MissingCasePort {
                node: node_id,
                port: case.port_id,
            })?;
        if !node.ports.contains(&case.port_id)
            || port.node != node_id
            || port.dir != PortDirection::Out
            || port.kind != PortKind::Exec
            || port.capacity != PortCapacity::Multi
            || port.key.0 != case_key(case.id())
        {
            return Err(WorkflowNodeError::InvalidCasePort {
                node: node_id,
                port: case.port_id,
            });
        }
    }
    let else_port = ports
        .get(&else_branch.port_id)
        .ok_or(WorkflowNodeError::MissingElsePort { node: node_id })?;
    if !node.ports.contains(&else_branch.port_id)
        || else_port.node != node_id
        || else_port.dir != PortDirection::Out
        || else_port.kind != PortKind::Exec
        || else_port.capacity != PortCapacity::Multi
        || else_port.key.0 != CONDITION_ELSE_PORT_KEY
        || cases.iter().any(|case| case.port_id == else_branch.port_id)
    {
        return Err(WorkflowNodeError::InvalidElsePort {
            node: node_id,
            port: else_branch.port_id,
        });
    }
    let actual_outputs = node
        .ports
        .iter()
        .filter(|port_id| {
            ports
                .get(port_id)
                .is_some_and(|port| port.dir == PortDirection::Out && port.kind == PortKind::Exec)
        })
        .copied()
        .collect::<Vec<_>>();
    let expected_outputs = cases
        .iter()
        .map(WorkflowCase::port_id)
        .chain(std::iter::once(else_branch.port_id))
        .collect::<Vec<_>>();
    if actual_outputs != expected_outputs {
        return Err(WorkflowNodeError::InvalidConditionPortOrder { node: node_id });
    }
    Ok(())
}

fn ensure_kind(node_id: NodeId, node: &Node) -> Result<(), WorkflowNodeError> {
    if node.kind.0 == WORKFLOW_CONDITION_KIND {
        Ok(())
    } else {
        Err(WorkflowNodeError::WrongNodeKind {
            node: node_id,
            expected: WORKFLOW_CONDITION_KIND,
            actual: node.kind.0.clone(),
        })
    }
}

pub(super) fn condition_data(title: &str, cases: &[WorkflowCase]) -> serde_json::Value {
    serde_json::json!({
        "title": title,
        "cases": cases
            .iter()
            .map(|case| serde_json::json!({
                "id": case.id,
                "label": case.label,
                "port_id": case.port_id,
            }))
            .collect::<Vec<_>>(),
        "else_branch": { "label": "Else" },
    })
}

fn replace_condition_cases(
    node_id: NodeId,
    data: &serde_json::Value,
    cases: &[WorkflowCase],
) -> Result<serde_json::Value, WorkflowNodeError> {
    let mut value = data.clone();
    let object = value
        .as_object_mut()
        .ok_or(WorkflowNodeError::InvalidData {
            node: node_id,
            field: "object",
        })?;
    object.insert(
        "cases".to_owned(),
        serde_json::Value::Array(
            cases
                .iter()
                .map(|case| {
                    serde_json::json!({
                        "id": case.id,
                        "label": case.label,
                        "port_id": case.port_id,
                    })
                })
                .collect(),
        ),
    );
    Ok(value)
}

pub(super) fn case_port(node: NodeId, case: &WorkflowCase) -> Port {
    Port {
        node,
        key: PortKey::new(case_key(case.id())),
        dir: PortDirection::Out,
        kind: PortKind::Exec,
        capacity: PortCapacity::Multi,
        connectable: None,
        connectable_start: None,
        connectable_end: None,
        ty: None,
        data: serde_json::json!({
            "case_id": case.id,
            "label": case.label,
        }),
    }
}

fn case_key(case_id: &str) -> String {
    format!("case.{case_id}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use jellyflow_runtime::GraphRuntime;

    #[test]
    fn legacy_branch_schema_is_rejected_instead_of_silently_migrated() {
        let node = NodeId::from_u128(1);
        let data = serde_json::json!({
            "branches": [],
            "else_branch": { "label": "Else" },
        });
        assert!(matches!(
            parse_condition_cases(node, &data),
            Err(WorkflowNodeError::InvalidData { field: "cases", .. })
        ));
    }

    #[test]
    fn case_model_has_no_editable_else_entry() {
        assert!(matches!(
            WorkflowCase::new("else", "Else", PortId::from_u128(0xd1fa_ff00)),
            Err(WorkflowNodeError::ReservedCaseId { case_id }) if case_id == "else"
        ));
        let case =
            WorkflowCase::new("case-stable", "Stable", PortId::from_u128(0xd1fa_ff01)).unwrap();
        let cases = vec![case.clone()];
        assert!(condition_action_satisfied(
            &cases,
            &ConditionCaseAction::Add(case)
        ));
        assert!(cases.iter().all(|case| case.id() != "else"));
    }

    #[test]
    fn case_edits_keep_the_catalog_else_port_stable_and_last() {
        let runtime = GraphRuntime::builder(crate::nodes::native_node_catalog().unwrap())
            .build(super::super::workflow_demo_graph().unwrap())
            .unwrap();
        let node_id = workflow_condition_node_id();
        let initial = runtime.snapshot().unwrap();
        let initial_outputs = condition_outputs(&initial, node_id).unwrap();
        let else_port = initial_outputs.else_branch().port_id();
        assert_eq!(
            initial.graph().node(&node_id).unwrap().ports.last(),
            Some(&else_port)
        );

        let added = WorkflowCase::new("case-test", "Test", PortId::from_u128(0xd1fa_ff10)).unwrap();
        let add = plan_condition_case_action(
            &initial,
            node_id,
            ConditionCaseAction::Add(added.clone()),
            IncidentEdgePolicy::Reject,
        )
        .unwrap();
        runtime.execute(add).unwrap().applied().unwrap();
        let after_add = runtime.snapshot().unwrap();
        assert_eq!(
            after_add.graph().node(&node_id).unwrap().ports.last(),
            Some(&else_port)
        );

        let reorder = plan_condition_case_action(
            &after_add,
            node_id,
            ConditionCaseAction::Reorder {
                case_id: added.id().to_owned(),
                index: 0,
            },
            IncidentEdgePolicy::Reject,
        )
        .unwrap();
        runtime.execute(reorder).unwrap().applied().unwrap();
        let after_reorder = runtime.snapshot().unwrap();
        assert_eq!(condition_cases(&after_reorder, node_id).unwrap()[0], added);
        assert_eq!(
            after_reorder.graph().node(&node_id).unwrap().ports.last(),
            Some(&else_port)
        );

        let remove = plan_condition_case_action(
            &after_reorder,
            node_id,
            ConditionCaseAction::Remove {
                case_id: "case-test".to_owned(),
            },
            IncidentEdgePolicy::Reject,
        )
        .unwrap();
        runtime.execute(remove).unwrap().applied().unwrap();
        let after_remove = runtime.snapshot().unwrap();
        assert_eq!(
            condition_outputs(&after_remove, node_id)
                .unwrap()
                .else_branch()
                .port_id(),
            else_port
        );
        assert_eq!(
            after_remove.graph().node(&node_id).unwrap().ports.last(),
            Some(&else_port)
        );
    }
}
