mod condition;
mod llm;

use jellyflow_core::types::TypeDesc;
use jellyflow_core::{
    CanvasPoint, Edge, EdgeId, EdgeKind, Graph, GraphBuilder, GraphId, GraphValidationError,
    NodeId, NodeKindKey, PortDirection, PortId,
};
use jellyflow_open_gpui::OpenGpuiNodeRegistryBuilder;
use jellyflow_open_gpui::accessibility::OPEN_GPUI_DIAGNOSTIC_ROLE;
use jellyflow_open_gpui::authoring::OpenGpuiPortHandle;
use jellyflow_open_gpui::chrome::OpenGpuiPortMarker;
use jellyflow_runtime::schema::{NodeCatalog, NodeKindSpec};
use open_gpui::{
    AnyElement, Div, ElementId, Entity, InteractiveElement, IntoElement, ParentElement,
    SharedString, Stateful, StatefulInteractiveElement, Styled, div, px, rgb,
};

use crate::model_options::WorkflowModelOptionsState;

use super::native_graph_theme;

pub use condition::{
    ConditionCaseAction, WORKFLOW_CONDITION_DEFAULT_SIZE, WORKFLOW_CONDITION_KIND,
    WORKFLOW_CONDITION_RENDERER, WorkflowCase, WorkflowConditionNode, WorkflowConditionOutputs,
    WorkflowElseBranch, condition_cases, condition_outputs, plan_condition_case_action,
    workflow_condition_add_selector, workflow_condition_case_selector,
    workflow_condition_else_selector, workflow_condition_node_id,
    workflow_condition_pending_selector, workflow_condition_remove_selector,
    workflow_condition_scroll_selector, workflow_condition_up_selector,
};
pub use llm::{
    WORKFLOW_LLM_DEFAULT_SIZE, WORKFLOW_LLM_KIND, WORKFLOW_LLM_RENDERER, WORKFLOW_MODEL_FIELD,
    WORKFLOW_PROMPT_FIELD, WorkflowFieldStore, WorkflowLlmNode, workflow_llm_node_id,
    workflow_model_accept_remote_selector, workflow_model_conflict_selector,
    workflow_model_option_selector, workflow_model_options_empty_selector,
    workflow_model_options_failed_selector, workflow_model_options_loading_selector,
    workflow_model_options_partial_selector, workflow_model_options_retry_selector,
    workflow_model_reapply_selector, workflow_model_selector,
    workflow_prompt_accept_remote_selector, workflow_prompt_apply_selector,
    workflow_prompt_conflict_selector, workflow_prompt_reapply_selector, workflow_prompt_selector,
};

const WORKFLOW_GRAPH_ID: u128 = 0xd1fa_0000;
const WORKFLOW_LLM_INPUT_ID: u128 = 0xd1fa_0101;
const WORKFLOW_LLM_OUTPUT_ID: u128 = 0xd1fa_0102;
const WORKFLOW_CONDITION_INPUT_ID: u128 = 0xd1fa_0201;
const WORKFLOW_MATCH_PORT_ID: u128 = 0xd1fa_0202;
const WORKFLOW_ELSE_PORT_ID: u128 = 0xd1fa_0203;
const WORKFLOW_REVIEW_PORT_ID: u128 = 0xd1fa_0204;
const WORKFLOW_FORWARD_EDGE_ID: u128 = 0xd1fa_0301;
const WORKFLOW_RETRY_EDGE_ID: u128 = 0xd1fa_0302;
const MATCH_CASE_ID: &str = "case-match";
const REVIEW_CASE_ID: &str = "case-review";
const WORKFLOW_HEADER_HEIGHT: f32 = 44.0;
pub(super) const WORKFLOW_PORT_DIAMETER: f32 = 13.0;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum WorkflowNodeError {
    MissingNode {
        node: NodeId,
    },
    WrongNodeKind {
        node: NodeId,
        expected: &'static str,
        actual: String,
    },
    InvalidData {
        node: NodeId,
        field: &'static str,
    },
    EmptyCaseId,
    ReservedCaseId {
        case_id: String,
    },
    EmptyCaseLabel {
        case_id: String,
    },
    DuplicateCaseId {
        case_id: String,
    },
    DuplicateCasePort {
        port: PortId,
    },
    CaseNotFound {
        case_id: String,
    },
    CaseReorderIndexOutOfBounds {
        index: usize,
        case_count: usize,
    },
    MissingCasePort {
        node: NodeId,
        port: PortId,
    },
    InvalidCasePort {
        node: NodeId,
        port: PortId,
    },
    MissingElsePort {
        node: NodeId,
    },
    InvalidElsePort {
        node: NodeId,
        port: PortId,
    },
    InvalidConditionPortOrder {
        node: NodeId,
    },
    BindingUnavailable {
        message: String,
    },
    DraftConflict {
        field: &'static str,
    },
    CommandRejected {
        message: String,
    },
}

impl std::fmt::Display for WorkflowNodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingNode { node } => write!(f, "workflow node is missing: {node:?}"),
            Self::WrongNodeKind {
                node,
                expected,
                actual,
            } => write!(
                f,
                "workflow node {node:?} has kind {actual:?}, expected {expected:?}"
            ),
            Self::InvalidData { node, field } => {
                write!(f, "workflow node {node:?} has invalid {field} data")
            }
            Self::EmptyCaseId => f.write_str("workflow condition case id must not be empty"),
            Self::ReservedCaseId { case_id } => write!(
                f,
                "workflow condition case id {case_id:?} is reserved for the immutable Else branch"
            ),
            Self::EmptyCaseLabel { case_id } => {
                write!(f, "workflow condition case {case_id:?} must have a label")
            }
            Self::DuplicateCaseId { case_id } => {
                write!(f, "workflow condition case id is duplicated: {case_id:?}")
            }
            Self::DuplicateCasePort { port } => {
                write!(f, "workflow condition case port is duplicated: {port:?}")
            }
            Self::CaseNotFound { case_id } => {
                write!(f, "workflow condition case does not exist: {case_id:?}")
            }
            Self::CaseReorderIndexOutOfBounds { index, case_count } => write!(
                f,
                "workflow condition case reorder index {index} is outside {case_count} cases"
            ),
            Self::MissingCasePort { node, port } => {
                write!(
                    f,
                    "workflow node {node:?} is missing condition case port {port:?}"
                )
            }
            Self::InvalidCasePort { node, port } => write!(
                f,
                "workflow node {node:?} has an invalid condition case port {port:?}"
            ),
            Self::MissingElsePort { node } => {
                write!(
                    f,
                    "workflow condition node {node:?} is missing its Else port"
                )
            }
            Self::InvalidElsePort { node, port } => write!(
                f,
                "workflow condition node {node:?} has invalid Else port {port:?}"
            ),
            Self::InvalidConditionPortOrder { node } => write!(
                f,
                "workflow condition node {node:?} must order case ports before Else"
            ),
            Self::BindingUnavailable { message } => {
                write!(f, "workflow binding is unavailable: {message}")
            }
            Self::DraftConflict { field } => write!(
                f,
                "workflow {field} draft requires explicit conflict resolution"
            ),
            Self::CommandRejected { message } => {
                write!(f, "workflow command was rejected: {message}")
            }
        }
    }
}

impl std::error::Error for WorkflowNodeError {}

pub(super) struct WorkflowPort {
    pub(super) id: PortId,
    pub(super) direction: PortDirection,
    pub(super) handle: OpenGpuiPortHandle,
}

#[derive(Clone, Copy)]
pub(super) enum WorkflowNodeStatus {
    Ready,
    Draft,
    Conflict,
    Error,
}

impl WorkflowNodeStatus {
    const fn key(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Draft => "draft",
            Self::Conflict => "conflict",
            Self::Error => "error",
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Ready => "READY",
            Self::Draft => "DRAFT",
            Self::Conflict => "CONFLICT",
            Self::Error => "ERROR",
        }
    }

    const fn colors(self) -> (u32, u32) {
        match self {
            Self::Ready => (0x9ee6b8, 0x234433),
            Self::Draft => (0xf0d28a, 0x4a3d25),
            Self::Conflict => (0xf0b68f, 0x4d3028),
            Self::Error => (0xf19a93, 0x4d292d),
        }
    }
}

fn workflow_header_port_elements(ports: Vec<WorkflowPort>) -> Vec<AnyElement> {
    ports
        .into_iter()
        .map(|port| {
            let WorkflowPort {
                id,
                direction,
                handle,
            } = port;
            let chrome = OpenGpuiPortMarker::new(handle.presentation(), native_graph_theme());
            let minimum_hit_diameter = chrome.minimum_hit_diameter();
            let marker = handle
                .with_minimum_hit_diameter(minimum_hit_diameter)
                .interactive(
                    div()
                        .debug_selector(move || workflow_port_selector(id))
                        .w(px(WORKFLOW_PORT_DIAMETER))
                        .h(px(WORKFLOW_PORT_DIAMETER))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(chrome),
                );
            let top = px((WORKFLOW_HEADER_HEIGHT - WORKFLOW_PORT_DIAMETER) / 2.0);
            let edge_offset = px(-WORKFLOW_PORT_DIAMETER / 2.0);
            let port = div().absolute().top(top).flex().items_center();
            match direction {
                PortDirection::In => port.left(edge_offset).child(marker).into_any_element(),
                PortDirection::Out => port.right(edge_offset).child(marker).into_any_element(),
            }
        })
        .collect()
}

pub(super) fn workflow_header(
    node_id: NodeId,
    title: String,
    badge: &'static str,
    status: WorkflowNodeStatus,
    ports: Vec<WorkflowPort>,
) -> impl open_gpui::IntoElement {
    let status_selector = workflow_node_status_selector(node_id, status.key());
    let (status_text, status_background) = status.colors();
    div()
        .relative()
        .h(px(WORKFLOW_HEADER_HEIGHT))
        .px(px(14.0))
        .flex()
        .items_center()
        .justify_between()
        .border_b_1()
        .border_color(rgb(0x414750))
        .child(title)
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(7.0))
                .child(div().text_xs().text_color(rgb(0xaab6c6)).child(badge))
                .child(
                    div()
                        .debug_selector(move || status_selector.clone())
                        .px(px(6.0))
                        .py(px(2.0))
                        .rounded(px(3.0))
                        .bg(rgb(status_background))
                        .text_xs()
                        .text_color(rgb(status_text))
                        .child(status.label()),
                ),
        )
        .children(workflow_header_port_elements(ports))
}

pub(super) fn workflow_alert(
    id: impl Into<ElementId>,
    name: impl Into<SharedString>,
) -> Stateful<Div> {
    div()
        .id(id)
        .role(OPEN_GPUI_DIAGNOSTIC_ROLE)
        .aria_label(name)
}

#[must_use]
pub fn workflow_node_status_selector(node: NodeId, status: &str) -> String {
    format!("jellyflow-workflow-status-{}-{status}", node.0)
}

#[must_use]
pub fn workflow_port_selector(port: PortId) -> String {
    format!("jellyflow-workflow-port-{}", port.0)
}

pub(super) fn catalog_specs() -> [NodeKindSpec; 2] {
    [llm::catalog_spec(), condition::catalog_spec()]
}

pub(super) fn register(
    registry: &mut OpenGpuiNodeRegistryBuilder,
    field_store: WorkflowFieldStore,
    model_options: Entity<WorkflowModelOptionsState>,
) {
    llm::register(registry, field_store, model_options);
    condition::register(registry);
}

pub fn workflow_demo_graph() -> Result<Graph, Vec<GraphValidationError>> {
    let llm = workflow_llm_node_id();
    let condition = workflow_condition_node_id();
    let llm_input = PortId::from_u128(WORKFLOW_LLM_INPUT_ID);
    let llm_output = PortId::from_u128(WORKFLOW_LLM_OUTPUT_ID);
    let condition_input = PortId::from_u128(WORKFLOW_CONDITION_INPUT_ID);
    let match_output = PortId::from_u128(WORKFLOW_MATCH_PORT_ID);
    let else_output = PortId::from_u128(WORKFLOW_ELSE_PORT_ID);
    let review_output = PortId::from_u128(WORKFLOW_REVIEW_PORT_ID);
    let cases = [
        WorkflowCase {
            id: MATCH_CASE_ID.to_owned(),
            label: "Intent matches".to_owned(),
            port_id: match_output,
        },
        WorkflowCase {
            id: REVIEW_CASE_ID.to_owned(),
            label: "Needs review".to_owned(),
            port_id: review_output,
        },
    ];
    let mut catalog = NodeCatalog::new();
    for spec in catalog_specs() {
        catalog
            .register(spec)
            .expect("workflow catalog specs must remain valid");
    }
    let llm_creation = catalog
        .instantiate_node_with_ids(
            &NodeKindKey::new(WORKFLOW_LLM_KIND),
            llm,
            CanvasPoint { x: 90.0, y: 210.0 },
            [llm_input, llm_output],
        )
        .expect("workflow LLM fixture must match its catalog ports");
    let condition_creation = catalog
        .instantiate_node_with_ids(
            &NodeKindKey::new(WORKFLOW_CONDITION_KIND),
            condition,
            CanvasPoint { x: 500.0, y: 170.0 },
            [condition_input, else_output],
        )
        .expect("workflow condition fixture must match its fixed catalog ports");
    let mut graph = GraphBuilder::new(GraphId::from_u128(WORKFLOW_GRAPH_ID));
    graph.insert_symbol(
        jellyflow_core::SymbolId::from_u128(WORKFLOW_GRAPH_ID + 1),
        jellyflow_core::Symbol {
            name: "customer_intent".to_owned(),
            ty: Some(TypeDesc::String),
            default_value: Some(serde_json::json!("unknown")),
            meta: serde_json::json!({ "group": "workflow" }),
        },
    );
    let (llm_id, mut llm_node, llm_ports) = llm_creation.into_parts();
    llm_node.data = serde_json::json!({
        "title": "Classify request",
        "prompt": "",
        "model": llm::DEFAULT_MODEL,
    });
    graph.insert_node(llm_id, llm_node);
    for (port_id, port) in llm_ports {
        graph.insert_port(port_id, port);
    }

    let (condition_id, mut condition_node, condition_ports) = condition_creation.into_parts();
    condition_node.data = condition::condition_data("Route by intent", &cases);
    condition_node.ports.retain(|port| *port != else_output);
    condition_node
        .ports
        .extend(cases.iter().map(WorkflowCase::port_id));
    condition_node.ports.push(else_output);
    graph.insert_node(condition_id, condition_node);
    for (port_id, port) in condition_ports {
        graph.insert_port(port_id, port);
    }
    for case in &cases {
        graph.insert_port(case.port_id(), condition::case_port(condition, case));
    }
    graph.insert_edge(
        EdgeId::from_u128(WORKFLOW_FORWARD_EDGE_ID),
        Edge::new(EdgeKind::Exec, llm_output, condition_input),
    );
    graph.insert_edge(
        EdgeId::from_u128(WORKFLOW_RETRY_EDGE_ID),
        Edge::new(EdgeKind::Exec, else_output, llm_input),
    );
    graph.build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use open_gpui::Element;
    use open_gpui::accesskit::{Action, Node};

    #[test]
    fn workflow_conflict_alert_uses_the_production_accesskit_path() {
        let alert = workflow_alert(
            "workflow-conflict-test",
            "Prompt conflict: Prompt changed in another view",
        );
        assert_eq!(alert.a11y_role(), Some(OPEN_GPUI_DIAGNOSTIC_ROLE));

        let mut node = Node::new(alert.a11y_role().expect("workflow alert role"));
        alert.write_a11y_info(&mut node);
        assert_eq!(node.role(), open_gpui::Role::Alert);
        assert_eq!(
            node.label(),
            Some("Prompt conflict: Prompt changed in another view")
        );
        assert!(!node.supports_action(Action::Click));
        assert!(!node.supports_action(Action::Focus));
    }
}
