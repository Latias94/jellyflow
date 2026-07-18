use std::collections::{BTreeMap, BTreeSet};

use jellyflow::core::{
    CanvasSize, Node, NodeId, Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
};
use jellyflow::runtime::schema::{NodeKindSpec, NodePortSpec};
use jellyflow::runtime::{
    GraphCommand, GraphCommandOutcome, GraphRevision, GraphSnapshot, IncidentEdgePolicy,
    TopologyEdit, TopologyPort,
};
use jellyflow_open_gpui::{
    OpenGpuiActivityKind, OpenGpuiActivityLease, OpenGpuiCommandTicket,
    OpenGpuiCommandTicketResult, OpenGpuiNodeBinding, OpenGpuiNodeInteractionGuard,
    OpenGpuiNodeRegistryBuilder, OpenGpuiPortHandle,
};
use open_gpui::{
    App, AppContext, Context, Entity, InteractiveElement, ParentElement, Render,
    StatefulInteractiveElement, Styled, Task, Window, div, px, rgb,
};
use open_gpui_ui_components::{Button, ButtonVariant};

use super::WORKFLOW_PORT_DIAMETER;
use super::{WorkflowNodeError, WorkflowNodeStatus, WorkflowPort, workflow_alert, workflow_header};

pub const WORKFLOW_CONDITION_KIND: &str = "workflow.condition";
pub const WORKFLOW_CONDITION_RENDERER: &str = "jellyflow.open-gpui.workflow.condition";
pub const WORKFLOW_CONDITION_DEFAULT_SIZE: CanvasSize = CanvasSize {
    width: 360.0,
    height: 290.0,
};

const WORKFLOW_CONDITION_NODE_ID: u128 = 0xd1fa_0002;
const CONDITION_BODY_HORIZONTAL_PADDING: f32 = 14.0;
const CONDITION_BRANCH_HORIZONTAL_PADDING: f32 = 9.0;
const CONDITION_BRANCH_ROW_HEIGHT: f32 = 37.0;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkflowBranch {
    pub(super) id: String,
    pub(super) label: String,
    pub(super) port_id: PortId,
}

impl WorkflowBranch {
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        port_id: PortId,
    ) -> Result<Self, WorkflowNodeError> {
        let id = id.into();
        if id.trim().is_empty() {
            return Err(WorkflowNodeError::EmptyBranchId);
        }
        let label = label.into();
        if label.trim().is_empty() {
            return Err(WorkflowNodeError::EmptyBranchLabel { branch_id: id });
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
pub enum ConditionBranchAction {
    Add(WorkflowBranch),
    Reorder { branch_id: String, index: usize },
    Remove { branch_id: String },
}

pub struct WorkflowConditionNode {
    binding: Entity<OpenGpuiNodeBinding>,
    command_state: ConditionCommandState,
    _command_task: Option<Task<()>>,
    _command_task_lease: Option<OpenGpuiActivityLease>,
    next_attempt: Option<u64>,
    diagnostic: Option<String>,
}

struct ConditionRenderData {
    revision: GraphRevision,
    title: String,
    branches: Vec<(WorkflowBranch, OpenGpuiPortHandle)>,
    inputs: Vec<WorkflowPort>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ConditionBranchIntent {
    action: ConditionBranchAction,
    incident_edges: IncidentEdgePolicy,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ConditionCommandState {
    Idle,
    InFlight {
        attempt: u64,
        intent: ConditionBranchIntent,
        expected_revision: GraphRevision,
    },
    NeedsRefresh {
        intent: ConditionBranchIntent,
        minimum_revision: GraphRevision,
        diagnostic: Option<String>,
    },
    ReadyToRetry {
        intent: ConditionBranchIntent,
        diagnostic: String,
    },
}

impl ConditionCommandState {
    fn after_refresh(self, revision: GraphRevision, branches: &[WorkflowBranch]) -> Self {
        let (intent, minimum_revision, diagnostic) = match self {
            Self::NeedsRefresh {
                intent,
                minimum_revision,
                diagnostic,
            } => (intent, minimum_revision, diagnostic),
            state => return state,
        };
        if revision < minimum_revision {
            return Self::NeedsRefresh {
                intent,
                minimum_revision,
                diagnostic,
            };
        }
        if condition_action_satisfied(branches, &intent.action) {
            return Self::Idle;
        }
        let diagnostic = diagnostic.unwrap_or_else(|| {
            format!(
                "condition command was not reflected by binding revision {revision}; retry explicitly"
            )
        });
        Self::ReadyToRetry { intent, diagnostic }
    }
}

impl WorkflowConditionNode {
    fn new(binding: Entity<OpenGpuiNodeBinding>) -> Self {
        Self {
            binding,
            command_state: ConditionCommandState::Idle,
            _command_task: None,
            _command_task_lease: None,
            next_attempt: Some(1),
            diagnostic: None,
        }
    }

    fn apply_action(
        &mut self,
        action: ConditionBranchAction,
        incident_edges: IncidentEdgePolicy,
        cx: &mut Context<Self>,
    ) {
        if !matches!(&self.command_state, ConditionCommandState::Idle) {
            return;
        }
        self.enqueue_intent(
            ConditionBranchIntent {
                action,
                incident_edges,
            },
            cx,
        );
        cx.notify();
    }

    fn add_branch(&mut self, cx: &mut Context<Self>) {
        if !matches!(&self.command_state, ConditionCommandState::Idle) {
            return;
        }
        let node_id = self.binding.read(cx).snapshot().node_id();
        let port_id = PortId::new();
        let branch_count = self
            .binding
            .read(cx)
            .snapshot()
            .read()
            .ok()
            .and_then(|facts| parse_condition_branches(node_id, &facts.node().data).ok())
            .map_or(0, |branches| branches.len());
        let branch = WorkflowBranch {
            id: format!("branch-{}", port_id.0),
            label: format!("Branch {}", branch_count + 1),
            port_id,
        };
        self.apply_action(
            ConditionBranchAction::Add(branch),
            IncidentEdgePolicy::Reject,
            cx,
        );
    }

    fn retry(&mut self, cx: &mut Context<Self>) {
        let state = std::mem::replace(&mut self.command_state, ConditionCommandState::Idle);
        let ConditionCommandState::ReadyToRetry { intent, .. } = state else {
            self.command_state = state;
            return;
        };
        self.enqueue_intent(intent, cx);
        cx.notify();
    }

    fn enqueue_intent(&mut self, intent: ConditionBranchIntent, cx: &mut Context<Self>) {
        let result = self.plan_and_enqueue(&intent, cx);
        match result {
            Ok(Some((attempt, expected_revision, ticket, task_lease))) => {
                self.command_state = ConditionCommandState::InFlight {
                    attempt,
                    intent: intent.clone(),
                    expected_revision,
                };
                self.diagnostic = None;
                self._command_task_lease = Some(task_lease);
                self._command_task = Some(cx.spawn(async move |node, cx| {
                    let result = ticket.await;
                    let _ = node.update(cx, |node, cx| {
                        node.finish_command(attempt, result, cx);
                    });
                }));
            }
            Ok(None) => {
                self.command_state = ConditionCommandState::Idle;
                self.diagnostic = None;
            }
            Err(error) => {
                let diagnostic = error.to_string();
                self.command_state = ConditionCommandState::ReadyToRetry {
                    intent,
                    diagnostic: diagnostic.clone(),
                };
                self.diagnostic = Some(diagnostic);
            }
        }
    }

    fn plan_and_enqueue(
        &mut self,
        intent: &ConditionBranchIntent,
        cx: &mut Context<Self>,
    ) -> Result<
        Option<(
            u64,
            GraphRevision,
            OpenGpuiCommandTicket,
            OpenGpuiActivityLease,
        )>,
        WorkflowNodeError,
    > {
        let node_id = self.binding.read(cx).snapshot().node_id();
        let facts = self
            .binding
            .read(cx)
            .snapshot()
            .read()
            .map_err(binding_error)?;
        let branches = parse_condition_branches(node_id, &facts.node().data)?;
        if condition_action_satisfied(&branches, &intent.action) {
            return Ok(None);
        }
        let ports = facts
            .ports()
            .map(|bound| (bound.id(), bound.port().clone()))
            .collect();
        let expected_revision = facts.graph_revision();
        let command = build_condition_branch_command(
            expected_revision,
            node_id,
            facts.node(),
            &ports,
            intent.action.clone(),
            intent.incident_edges,
        )?;
        let attempt = self
            .next_attempt
            .ok_or_else(|| WorkflowNodeError::BindingUnavailable {
                message: "condition command attempt identity space is exhausted".to_owned(),
            })?;
        let task_lease = self
            .binding
            .read(cx)
            .activity()
            .acquire(OpenGpuiActivityKind::Task)
            .map_err(binding_error)?;
        let ticket = facts
            .enqueue_command(cx, move |_| command)
            .map_err(binding_error)?;
        self.next_attempt = attempt.checked_add(1);
        Ok(Some((attempt, expected_revision, ticket, task_lease)))
    }

    fn finish_command(
        &mut self,
        attempt: u64,
        result: OpenGpuiCommandTicketResult,
        cx: &mut Context<Self>,
    ) {
        let state = std::mem::replace(&mut self.command_state, ConditionCommandState::Idle);
        let ConditionCommandState::InFlight {
            attempt: active_attempt,
            intent,
            expected_revision,
        } = state
        else {
            self.command_state = state;
            return;
        };
        if active_attempt != attempt {
            self.command_state = ConditionCommandState::InFlight {
                attempt: active_attempt,
                intent,
                expected_revision,
            };
            return;
        }

        let (minimum_revision, diagnostic) = match result {
            Ok(GraphCommandOutcome::Applied(receipt)) => (receipt.revision(), None),
            Ok(GraphCommandOutcome::Noop(noop)) => (noop.revision(), None),
            Ok(GraphCommandOutcome::Conflict(conflict)) => (
                conflict.current().revision(),
                Some(format!(
                    "condition branches changed at revision {}; refresh, then retry explicitly",
                    conflict.current().revision()
                )),
            ),
            Ok(GraphCommandOutcome::Rejected(rejection)) => (
                expected_revision,
                Some(format!(
                    "condition command was rejected: {}",
                    rejection.message()
                )),
            ),
            Err(error) => (
                expected_revision,
                Some(format!(
                    "condition command result is uncertain: {error}; refresh before retry"
                )),
            ),
        };
        self.diagnostic = diagnostic.clone();
        self.command_state = ConditionCommandState::NeedsRefresh {
            intent,
            minimum_revision,
            diagnostic,
        };
        self._command_task = None;
        self._command_task_lease = None;
        cx.notify();
    }

    fn reconcile_command(&mut self, revision: GraphRevision, branches: &[WorkflowBranch]) {
        let state = std::mem::replace(&mut self.command_state, ConditionCommandState::Idle)
            .after_refresh(revision, branches);
        match &state {
            ConditionCommandState::Idle => self.diagnostic = None,
            ConditionCommandState::ReadyToRetry { diagnostic, .. } => {
                self.diagnostic = Some(diagnostic.clone());
            }
            ConditionCommandState::InFlight { .. } | ConditionCommandState::NeedsRefresh { .. } => {
            }
        }
        self.command_state = state;
    }

    fn command_status(&self) -> WorkflowNodeStatus {
        match &self.command_state {
            ConditionCommandState::Idle if self.diagnostic.is_some() => WorkflowNodeStatus::Error,
            ConditionCommandState::Idle => WorkflowNodeStatus::Ready,
            ConditionCommandState::InFlight { .. } | ConditionCommandState::NeedsRefresh { .. } => {
                WorkflowNodeStatus::Draft
            }
            ConditionCommandState::ReadyToRetry { .. } => WorkflowNodeStatus::Conflict,
        }
    }
}

impl Render for WorkflowConditionNode {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        let node_id = self.binding.read(cx).snapshot().node_id();
        let data = match read_condition_binding(&self.binding, cx) {
            Ok(value) => {
                let branches = value
                    .branches
                    .iter()
                    .map(|(branch, _)| branch.clone())
                    .collect::<Vec<_>>();
                self.reconcile_command(value.revision, &branches);
                value
            }
            Err(error) => {
                self.diagnostic = Some(error.to_string());
                ConditionRenderData {
                    revision: 0,
                    title: "Retired condition".to_owned(),
                    branches: Vec::new(),
                    inputs: Vec::new(),
                }
            }
        };
        let commands_locked = !matches!(&self.command_state, ConditionCommandState::Idle);
        let add_node = cx.weak_entity();
        let add_button = div()
            .px(px(CONDITION_BODY_HORIZONTAL_PADDING))
            .debug_selector(move || workflow_condition_add_selector(node_id))
            .child(
                Button::new(workflow_condition_add_id(node_id), "Add branch")
                    .variant(ButtonVariant::Secondary)
                    .disabled(commands_locked)
                    .on_click(move |_, window, cx| {
                        let _ = add_node.update(cx, |node, cx| node.add_branch(cx));
                        window.refresh();
                    }),
            );
        let mut rows = div()
            .px(px(CONDITION_BODY_HORIZONTAL_PADDING))
            .flex()
            .flex_col()
            .gap(px(7.0));
        let branch_count = data.branches.len();
        for (index, (branch, handle)) in data.branches.into_iter().enumerate() {
            let port_id = branch.port_id();
            let row_selector = workflow_condition_branch_selector(node_id, port_id);
            let marker = handle.interactive(
                div()
                    .debug_selector(move || row_selector.clone())
                    .w(px(13.0))
                    .h(px(13.0))
                    .rounded_full()
                    .border_1()
                    .border_color(rgb(0xd0d7e2))
                    .bg(rgb(0xd5a84e)),
                |_, _, _| {},
            );
            let marker = div()
                .absolute()
                .top(px(
                    (CONDITION_BRANCH_ROW_HEIGHT - WORKFLOW_PORT_DIAMETER) / 2.0
                ))
                .right(px(
                    -(CONDITION_BODY_HORIZONTAL_PADDING + CONDITION_BRANCH_HORIZONTAL_PADDING)
                ))
                .child(marker);
            let mut actions = div().flex().gap(px(4.0));
            if index > 0 {
                let move_node = cx.weak_entity();
                let branch_id = branch.id().to_owned();
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
                            let action = ConditionBranchAction::Reorder {
                                branch_id: branch_id.clone(),
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
            if branch_count > 1 {
                let remove_node = cx.weak_entity();
                let branch_id = branch.id().to_owned();
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
                            let action = ConditionBranchAction::Remove {
                                branch_id: branch_id.clone(),
                            };
                            let _ = remove_node.update(cx, |node, cx| {
                                node.apply_action(action, IncidentEdgePolicy::Cascade, cx)
                            });
                            window.refresh();
                        }),
                    ),
                );
            }
            rows = rows.child(
                div()
                    .relative()
                    .h(px(CONDITION_BRANCH_ROW_HEIGHT))
                    .px(px(CONDITION_BRANCH_HORIZONTAL_PADDING))
                    .flex()
                    .items_center()
                    .justify_between()
                    .rounded(px(4.0))
                    .bg(rgb(0x292d35))
                    .child(div().text_sm().child(branch.label().to_owned()))
                    .child(actions)
                    .child(marker),
            );
        }
        let scroll_selector = workflow_condition_scroll_selector(node_id);
        let rows = OpenGpuiNodeInteractionGuard::scroll(
            div()
                .id(format!("jellyflow-workflow-condition-scroll-{}", node_id.0))
                .debug_selector(move || scroll_selector.clone())
                .h(px(148.0))
                .overflow_y_scroll()
                .scrollbar_width(px(7.0))
                .child(rows),
        );
        let retry = matches!(
            &self.command_state,
            ConditionCommandState::ReadyToRetry { .. }
        )
        .then(|| {
            let retry_node = cx.weak_entity();
            let retry_selector = workflow_condition_retry_selector(node_id);
            div().px(px(CONDITION_BODY_HORIZONTAL_PADDING)).child(
                div().debug_selector(move || retry_selector.clone()).child(
                    Button::new(
                        format!("jellyflow-workflow-condition-retry-button-{}", node_id.0),
                        "Retry",
                    )
                    .variant(ButtonVariant::Secondary)
                    .on_click(move |_, window, cx| {
                        let _ = retry_node.update(cx, |node, cx| node.retry(cx));
                        window.refresh();
                    }),
                ),
            )
        });
        let controls = OpenGpuiNodeInteractionGuard::control(
            div()
                .py(px(12.0))
                .flex()
                .flex_col()
                .gap(px(9.0))
                .child(rows)
                .child(add_button)
                .children(retry),
        );
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

        div()
            .relative()
            .size_full()
            .rounded(px(6.0))
            .border_1()
            .border_color(rgb(0x66583c))
            .bg(rgb(0x24262c))
            .text_color(rgb(0xf0ece4))
            .child(workflow_header(
                node_id,
                data.title,
                "CONDITION",
                status,
                data.inputs,
            ))
            .child(controls)
            .children(diagnostic)
    }
}

pub(super) fn catalog_spec() -> NodeKindSpec {
    NodeKindSpec::builder(
        WORKFLOW_CONDITION_KIND,
        "Condition",
        WORKFLOW_CONDITION_RENDERER,
    )
    .category(["Workflow", "Logic"])
    .keywords(["condition", "branch", "route", "if"])
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
    .default_data(serde_json::json!({
        "title": "Condition",
        "branches": [],
    }))
    .build()
}

pub(super) fn register(registry: &mut OpenGpuiNodeRegistryBuilder) {
    registry.register(WORKFLOW_CONDITION_RENDERER, |binding, _window, cx| {
        cx.new(|_| WorkflowConditionNode::new(binding))
    });
}

#[must_use]
pub fn workflow_condition_node_id() -> NodeId {
    NodeId::from_u128(WORKFLOW_CONDITION_NODE_ID)
}

pub fn condition_branches(
    snapshot: &GraphSnapshot,
    node_id: NodeId,
) -> Result<Vec<WorkflowBranch>, WorkflowNodeError> {
    let node = snapshot
        .graph()
        .node(&node_id)
        .ok_or(WorkflowNodeError::MissingNode { node: node_id })?;
    ensure_kind(node_id, node)?;
    let branches = parse_condition_branches(node_id, &node.data)?;
    validate_branch_ports(
        node_id,
        node,
        &branches,
        &node
            .ports
            .iter()
            .filter_map(|port_id| {
                snapshot
                    .graph()
                    .port(port_id)
                    .cloned()
                    .map(|port| (*port_id, port))
            })
            .collect(),
    )?;
    Ok(branches)
}

pub fn plan_condition_branch_action(
    snapshot: &GraphSnapshot,
    node_id: NodeId,
    action: ConditionBranchAction,
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
    build_condition_branch_command(
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
pub fn workflow_condition_branch_selector(node: NodeId, port: PortId) -> String {
    format!("jellyflow-workflow-condition-branch-{}-{}", node.0, port.0)
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

fn workflow_condition_retry_selector(node: NodeId) -> String {
    format!("jellyflow-workflow-condition-retry-{}", node.0)
}

fn workflow_condition_add_id(node: NodeId) -> String {
    format!("jellyflow-workflow-condition-add-button-{}", node.0)
}

fn build_condition_branch_command(
    revision: GraphRevision,
    node_id: NodeId,
    node: &Node,
    ports: &BTreeMap<PortId, Port>,
    action: ConditionBranchAction,
    incident_edges: IncidentEdgePolicy,
) -> Result<GraphCommand, WorkflowNodeError> {
    ensure_kind(node_id, node)?;
    let original_branches = parse_condition_branches(node_id, &node.data)?;
    validate_branch_ports(node_id, node, &original_branches, ports)?;
    let mut branches = original_branches.clone();
    let label = match action {
        ConditionBranchAction::Add(branch) => {
            if branches.iter().any(|existing| existing.id == branch.id) {
                return Err(WorkflowNodeError::DuplicateBranchId {
                    branch_id: branch.id,
                });
            }
            if branches
                .iter()
                .any(|existing| existing.port_id == branch.port_id)
            {
                return Err(WorkflowNodeError::DuplicateBranchPort {
                    port: branch.port_id,
                });
            }
            branches.push(branch);
            "add condition branch"
        }
        ConditionBranchAction::Reorder { branch_id, index } => {
            if index >= branches.len() {
                return Err(WorkflowNodeError::ReorderIndexOutOfBounds {
                    index,
                    branch_count: branches.len(),
                });
            }
            let current = branches
                .iter()
                .position(|branch| branch.id == branch_id)
                .ok_or_else(|| WorkflowNodeError::BranchNotFound {
                    branch_id: branch_id.clone(),
                })?;
            let branch = branches.remove(current);
            branches.insert(index, branch);
            "reorder condition branch"
        }
        ConditionBranchAction::Remove { branch_id } => {
            let current = branches
                .iter()
                .position(|branch| branch.id == branch_id)
                .ok_or_else(|| WorkflowNodeError::BranchNotFound {
                    branch_id: branch_id.clone(),
                })?;
            branches.remove(current);
            "remove condition branch"
        }
    };
    validate_unique_branches(&branches)?;
    let branch_ids: BTreeSet<_> = original_branches
        .iter()
        .map(WorkflowBranch::port_id)
        .collect();
    let mut topology_ports: Vec<_> = node
        .ports
        .iter()
        .filter(|port_id| !branch_ids.contains(port_id))
        .map(|port_id| {
            ports
                .get(port_id)
                .cloned()
                .map(|port| TopologyPort::new(*port_id, port))
                .ok_or(WorkflowNodeError::MissingBranchPort {
                    node: node_id,
                    port: *port_id,
                })
        })
        .collect::<Result<_, _>>()?;
    for branch in &branches {
        let port = ports
            .get(&branch.port_id)
            .cloned()
            .unwrap_or_else(|| branch_port(node_id, branch));
        topology_ports.push(TopologyPort::new(branch.port_id, port));
    }
    let next_data = replace_condition_branches(node_id, &node.data, &branches)?;
    Ok(GraphCommand::topology(
        revision,
        TopologyEdit::new()
            .set_node_data(node_id, next_data)
            .replace_node_ports(node_id, topology_ports, incident_edges),
    )
    .with_label(label))
}

fn read_condition_binding(
    binding: &Entity<OpenGpuiNodeBinding>,
    cx: &App,
) -> Result<ConditionRenderData, WorkflowNodeError> {
    let binding = binding.read(cx);
    let node_id = binding.snapshot().node_id();
    let facts =
        binding
            .snapshot()
            .read()
            .map_err(|error| WorkflowNodeError::BindingUnavailable {
                message: error.to_string(),
            })?;
    ensure_kind(node_id, facts.node())?;
    let branches = parse_condition_branches(node_id, &facts.node().data)?;
    let ports: BTreeMap<_, _> = facts
        .ports()
        .map(|port| (port.id(), port.port().clone()))
        .collect();
    let mut handles = binding
        .port_handles(&facts)
        .map_err(|error| WorkflowNodeError::BindingUnavailable {
            message: error.to_string(),
        })?
        .into_iter()
        .map(|handle| (handle.port(), handle))
        .collect::<BTreeMap<_, _>>();
    validate_branch_ports(node_id, facts.node(), &branches, &ports)?;
    let branch_rows = branches
        .into_iter()
        .map(|branch| {
            handles
                .remove(&branch.port_id())
                .map(|handle| (branch.clone(), handle))
                .ok_or(WorkflowNodeError::MissingBranchPort {
                    node: node_id,
                    port: branch.port_id(),
                })
        })
        .collect::<Result<_, _>>()?;
    let inputs = facts
        .ports()
        .filter(|port| port.port().dir == PortDirection::In)
        .filter_map(|port| {
            handles.remove(&port.id()).map(|handle| WorkflowPort {
                id: port.id(),
                direction: port.port().dir,
                handle,
            })
        })
        .collect();
    let title = facts
        .node()
        .data
        .get("title")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("Condition")
        .to_owned();
    Ok(ConditionRenderData {
        revision: facts.graph_revision(),
        title,
        branches: branch_rows,
        inputs,
    })
}

fn condition_action_satisfied(branches: &[WorkflowBranch], action: &ConditionBranchAction) -> bool {
    match action {
        ConditionBranchAction::Add(branch) => branches.iter().any(|current| current == branch),
        ConditionBranchAction::Reorder { branch_id, index } => branches
            .get(*index)
            .is_some_and(|branch| branch.id() == branch_id),
        ConditionBranchAction::Remove { branch_id } => {
            branches.iter().all(|branch| branch.id() != branch_id)
        }
    }
}

fn binding_error(error: impl std::fmt::Display) -> WorkflowNodeError {
    WorkflowNodeError::BindingUnavailable {
        message: error.to_string(),
    }
}

fn parse_condition_branches(
    node_id: NodeId,
    data: &serde_json::Value,
) -> Result<Vec<WorkflowBranch>, WorkflowNodeError> {
    let branches = data
        .get("branches")
        .and_then(serde_json::Value::as_array)
        .ok_or(WorkflowNodeError::InvalidData {
            node: node_id,
            field: "branches",
        })?;
    let branches = branches
        .iter()
        .map(|branch| {
            let id = branch.get("id").and_then(serde_json::Value::as_str).ok_or(
                WorkflowNodeError::InvalidData {
                    node: node_id,
                    field: "branches[].id",
                },
            )?;
            let label = branch
                .get("label")
                .and_then(serde_json::Value::as_str)
                .ok_or(WorkflowNodeError::InvalidData {
                    node: node_id,
                    field: "branches[].label",
                })?;
            let port_id = branch
                .get("port_id")
                .cloned()
                .ok_or(WorkflowNodeError::InvalidData {
                    node: node_id,
                    field: "branches[].port_id",
                })
                .and_then(|value| {
                    serde_json::from_value(value).map_err(|_| WorkflowNodeError::InvalidData {
                        node: node_id,
                        field: "branches[].port_id",
                    })
                })?;
            WorkflowBranch::new(id, label, port_id)
        })
        .collect::<Result<Vec<_>, _>>()?;
    validate_unique_branches(&branches)?;
    Ok(branches)
}

fn validate_unique_branches(branches: &[WorkflowBranch]) -> Result<(), WorkflowNodeError> {
    let mut ids = BTreeSet::new();
    let mut ports = BTreeSet::new();
    for branch in branches {
        if !ids.insert(branch.id.as_str()) {
            return Err(WorkflowNodeError::DuplicateBranchId {
                branch_id: branch.id.clone(),
            });
        }
        if !ports.insert(branch.port_id) {
            return Err(WorkflowNodeError::DuplicateBranchPort {
                port: branch.port_id,
            });
        }
    }
    Ok(())
}

fn validate_branch_ports(
    node_id: NodeId,
    node: &Node,
    branches: &[WorkflowBranch],
    ports: &BTreeMap<PortId, Port>,
) -> Result<(), WorkflowNodeError> {
    for branch in branches {
        let port = ports
            .get(&branch.port_id)
            .ok_or(WorkflowNodeError::MissingBranchPort {
                node: node_id,
                port: branch.port_id,
            })?;
        if !node.ports.contains(&branch.port_id)
            || port.node != node_id
            || port.dir != PortDirection::Out
            || port.kind != PortKind::Exec
            || port.key.0 != branch_key(branch.id())
        {
            return Err(WorkflowNodeError::InvalidBranchPort {
                node: node_id,
                port: branch.port_id,
            });
        }
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

pub(super) fn condition_data(title: &str, branches: &[WorkflowBranch]) -> serde_json::Value {
    serde_json::json!({
        "title": title,
        "branches": branches
            .iter()
            .map(|branch| serde_json::json!({
                "id": branch.id,
                "label": branch.label,
                "port_id": branch.port_id,
            }))
            .collect::<Vec<_>>(),
    })
}

fn replace_condition_branches(
    node_id: NodeId,
    data: &serde_json::Value,
    branches: &[WorkflowBranch],
) -> Result<serde_json::Value, WorkflowNodeError> {
    let mut value = data.clone();
    let object = value
        .as_object_mut()
        .ok_or(WorkflowNodeError::InvalidData {
            node: node_id,
            field: "object",
        })?;
    object.insert(
        "branches".to_owned(),
        serde_json::Value::Array(
            branches
                .iter()
                .map(|branch| {
                    serde_json::json!({
                        "id": branch.id,
                        "label": branch.label,
                        "port_id": branch.port_id,
                    })
                })
                .collect(),
        ),
    );
    Ok(value)
}

pub(super) fn branch_port(node: NodeId, branch: &WorkflowBranch) -> Port {
    Port {
        node,
        key: PortKey::new(branch_key(branch.id())),
        dir: PortDirection::Out,
        kind: PortKind::Exec,
        capacity: PortCapacity::Multi,
        connectable: None,
        connectable_start: None,
        connectable_end: None,
        ty: None,
        data: serde_json::json!({
            "branch_id": branch.id,
            "label": branch.label,
        }),
    }
}

fn branch_key(branch_id: &str) -> String {
    format!("branch.{branch_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refreshed_add_intent_reuses_its_original_branch_port_on_retry() {
        let branch =
            WorkflowBranch::new("branch-stable", "Stable", PortId::from_u128(0xd1fa_ff01)).unwrap();
        let state = ConditionCommandState::NeedsRefresh {
            intent: ConditionBranchIntent {
                action: ConditionBranchAction::Add(branch.clone()),
                incident_edges: IncidentEdgePolicy::Reject,
            },
            minimum_revision: 3,
            diagnostic: Some("stale command".to_owned()),
        };

        let state = state.after_refresh(3, &[]);
        let ConditionCommandState::ReadyToRetry { intent, .. } = state else {
            panic!("a missing branch should require explicit retry");
        };
        let ConditionBranchAction::Add(retried) = intent.action else {
            panic!("the retry must retain the original add intent");
        };
        assert_eq!(retried, branch);
        assert_eq!(retried.port_id(), PortId::from_u128(0xd1fa_ff01));
    }

    #[test]
    fn uncertain_add_result_is_complete_when_refresh_contains_the_exact_branch() {
        let branch = WorkflowBranch::new(
            "branch-committed",
            "Committed",
            PortId::from_u128(0xd1fa_ff02),
        )
        .unwrap();
        let state = ConditionCommandState::NeedsRefresh {
            intent: ConditionBranchIntent {
                action: ConditionBranchAction::Add(branch.clone()),
                incident_edges: IncidentEdgePolicy::Reject,
            },
            minimum_revision: 4,
            diagnostic: Some("missing receipt".to_owned()),
        };

        assert_eq!(
            state.after_refresh(4, &[branch]),
            ConditionCommandState::Idle,
            "a missing receipt must not make Retry duplicate an already committed port"
        );
    }
}
