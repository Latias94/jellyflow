mod condition;
mod llm;

use jellyflow_core::types::TypeDesc;
use jellyflow_core::{
    CanvasPoint, CanvasRect, Edge, EdgeId, EdgeKind, EdgeRouteSpec, EdgeViewDescriptor, Graph,
    GraphBuilder, GraphId, GraphValidationError, NodeId, NodeKindKey, PortDirection, PortId,
    PortKind,
};
use jellyflow_open_gpui::accessibility::OPEN_GPUI_DIAGNOSTIC_ROLE;
use jellyflow_open_gpui::authoring::{OpenGpuiPortHandle, OpenGpuiPortSide};
use jellyflow_open_gpui::chrome::{OpenGpuiPortMarker, OpenGpuiPortTone};
use jellyflow_open_gpui::registry::OpenGpuiNodeRegistryBuilder;
use jellyflow_runtime::geometry::HandlePosition;
use jellyflow_runtime::schema::{NodeCatalog, NodeKindSpec};
use open_gpui::{
    AnyElement, Div, ElementId, Entity, InteractiveElement, IntoElement, ParentElement,
    SharedString, Stateful, StatefulInteractiveElement, Styled, div, px,
};

use crate::model_options::WorkflowModelOptionsState;
use crate::product_catalog::NativeProductCatalog;
use crate::product_domain::condition::{
    ConditionCase, ConditionPredicate, condition_case_route_port, condition_case_value_port,
    condition_data,
};
use crate::product_theme::{NativeProductTheme, NativeToneTokens};

pub use condition::{
    WORKFLOW_CONDITION_DEFAULT_SIZE, WORKFLOW_CONDITION_KIND, WORKFLOW_CONDITION_RENDERER,
    WorkflowConditionNode, workflow_condition_add_selector, workflow_condition_body_selector,
    workflow_condition_case_actions_selector, workflow_condition_case_label_selector,
    workflow_condition_case_selector, workflow_condition_case_value_selector,
    workflow_condition_diagnostic_selector, workflow_condition_else_label_selector,
    workflow_condition_else_selector, workflow_condition_field_selector,
    workflow_condition_node_id, workflow_condition_node_selector,
    workflow_condition_pending_selector, workflow_condition_provider_failed_selector,
    workflow_condition_remove_selector, workflow_condition_scroll_selector,
    workflow_condition_up_selector, workflow_condition_value_selector,
};
pub use llm::{
    WORKFLOW_LLM_DEFAULT_SIZE, WORKFLOW_LLM_KIND, WORKFLOW_LLM_RENDERER, WORKFLOW_MODEL_FIELD,
    WORKFLOW_PROMPT_FIELD, WorkflowFieldStore, WorkflowLlmNode, workflow_llm_body_selector,
    workflow_llm_node_id, workflow_llm_node_selector, workflow_model_accept_remote_selector,
    workflow_model_conflict_selector, workflow_model_option_selector,
    workflow_model_options_empty_selector, workflow_model_options_failed_selector,
    workflow_model_options_loading_selector, workflow_model_options_partial_selector,
    workflow_model_options_retry_selector, workflow_model_reapply_selector,
    workflow_model_selector, workflow_prompt_accept_remote_selector,
    workflow_prompt_apply_selector, workflow_prompt_conflict_selector,
    workflow_prompt_reapply_selector, workflow_prompt_selector,
};
const WORKFLOW_GRAPH_ID: u128 = 0xd1fa_0000;
const WORKFLOW_LLM_INPUT_ID: u128 = 0xd1fa_0101;
const WORKFLOW_LLM_OUTPUT_ID: u128 = 0xd1fa_0102;
const WORKFLOW_CONDITION_INPUT_ID: u128 = 0xd1fa_0201;
const WORKFLOW_MATCH_PORT_ID: u128 = 0xd1fa_0202;
const WORKFLOW_ELSE_PORT_ID: u128 = 0xd1fa_0203;
const WORKFLOW_REVIEW_PORT_ID: u128 = 0xd1fa_0204;
const WORKFLOW_MATCH_VALUE_PORT_ID: u128 = 0xd1fa_0205;
const WORKFLOW_REVIEW_VALUE_PORT_ID: u128 = 0xd1fa_0206;
const WORKFLOW_RESPONSE_LLM_NODE_ID: u128 = 0xd1fa_0003;
const WORKFLOW_RESPONSE_LLM_INPUT_ID: u128 = 0xd1fa_0401;
const WORKFLOW_RESPONSE_LLM_OUTPUT_ID: u128 = 0xd1fa_0402;
const WORKFLOW_FORWARD_EDGE_ID: u128 = 0xd1fa_0301;
const WORKFLOW_SUCCESS_EDGE_ID: u128 = 0xd1fa_0302;
const WORKFLOW_RETRY_EDGE_ID: u128 = 0xd1fa_0303;
const MATCH_CASE_ID: &str = "case-match";
const REVIEW_CASE_ID: &str = "case-review";
const WORKFLOW_HEADER_HEIGHT: f32 = 44.0;

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
    pub(super) kind: PortKind,
    pub(super) handle: OpenGpuiPortHandle,
}

pub(super) struct WorkflowPortElement {
    pub(super) marker: AnyElement,
    pub(super) layout_diameter: f32,
}

pub(super) fn workflow_port_element(
    handle: OpenGpuiPortHandle,
    tone: OpenGpuiPortTone,
    selector: String,
    product_theme: NativeProductTheme,
) -> WorkflowPortElement {
    let chrome = OpenGpuiPortMarker::new(handle.presentation(), tone, product_theme.graph_theme());
    let style = chrome.style();
    let marker = div()
        .debug_selector(move || selector.clone())
        .child(handle.interactive_marker(chrome));
    WorkflowPortElement {
        marker: marker.into_any_element(),
        layout_diameter: style.layout_diameter(),
    }
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

    const fn tone(self, theme: NativeProductTheme) -> NativeToneTokens {
        match self {
            Self::Ready => theme.status.success,
            Self::Draft => theme.status.warning,
            Self::Conflict => theme.status.conflict,
            Self::Error => theme.status.danger,
        }
    }
}

fn workflow_header_port_elements(
    ports: Vec<WorkflowPort>,
    product_theme: NativeProductTheme,
) -> Vec<AnyElement> {
    ports
        .into_iter()
        .map(|port| {
            let WorkflowPort {
                id,
                direction,
                kind,
                handle,
            } = port;
            let WorkflowPortElement {
                marker,
                layout_diameter,
            } = workflow_port_element(
                handle,
                OpenGpuiPortTone::from(kind),
                workflow_port_selector(id),
                product_theme,
            );
            let top = px((WORKFLOW_HEADER_HEIGHT - layout_diameter) / 2.0);
            let edge_offset = px(-layout_diameter / 2.0);
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
    product_theme: NativeProductTheme,
) -> impl open_gpui::IntoElement {
    let status_selector = workflow_node_status_selector(node_id, status.key());
    let title_selector = workflow_header_title_selector(node_id);
    let metadata_selector = workflow_header_metadata_selector(node_id);
    let status_tone = status.tone(product_theme);
    div()
        .relative()
        .h(px(WORKFLOW_HEADER_HEIGHT))
        .px(px(product_theme.spacing.xl))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(product_theme.spacing.lg))
        .border_b_1()
        .border_color(product_theme.header.border)
        .child(
            div()
                .debug_selector(move || title_selector.clone())
                .min_w(px(0.0))
                .flex_1()
                .truncate()
                .child(title),
        )
        .child(
            div()
                .debug_selector(move || metadata_selector.clone())
                .flex_none()
                .flex()
                .items_center()
                .gap(px(product_theme.spacing.sm))
                .child(
                    div()
                        .text_xs()
                        .text_color(product_theme.header.metadata)
                        .child(badge),
                )
                .child(
                    div()
                        .debug_selector(move || status_selector.clone())
                        .px(px(product_theme.spacing.sm))
                        .py(px(product_theme.spacing.xxs))
                        .rounded(px(product_theme.radius.control))
                        .border_1()
                        .border_color(status_tone.border)
                        .bg(status_tone.surface)
                        .text_xs()
                        .text_color(status_tone.text)
                        .child(status.label()),
                ),
        )
        .children(workflow_header_port_elements(ports, product_theme))
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

#[must_use]
pub fn workflow_header_title_selector(node: NodeId) -> String {
    format!("jellyflow-workflow-header-title-{}", node.0)
}

#[must_use]
pub fn workflow_header_metadata_selector(node: NodeId) -> String {
    format!("jellyflow-workflow-header-metadata-{}", node.0)
}

pub(super) fn catalog_specs() -> [NodeKindSpec; 2] {
    [llm::catalog_spec(), condition::catalog_spec()]
}

pub(super) fn register(
    registry: &mut OpenGpuiNodeRegistryBuilder,
    field_store: WorkflowFieldStore,
    model_options: Entity<WorkflowModelOptionsState>,
    product_theme: NativeProductTheme,
    product_catalog: NativeProductCatalog,
) {
    llm::register(
        registry,
        field_store,
        model_options,
        product_theme,
        &product_catalog,
    );
    condition::register(registry, product_theme, &product_catalog);
}

const fn open_gpui_side(side: HandlePosition) -> OpenGpuiPortSide {
    match side {
        HandlePosition::Top => OpenGpuiPortSide::Top,
        HandlePosition::Right => OpenGpuiPortSide::Right,
        HandlePosition::Bottom => OpenGpuiPortSide::Bottom,
        HandlePosition::Left => OpenGpuiPortSide::Left,
    }
}

pub fn workflow_demo_graph() -> Result<Graph, Vec<GraphValidationError>> {
    let llm = workflow_llm_node_id();
    let condition = workflow_condition_node_id();
    let response_llm = NodeId::from_u128(WORKFLOW_RESPONSE_LLM_NODE_ID);
    let llm_input = PortId::from_u128(WORKFLOW_LLM_INPUT_ID);
    let llm_output = PortId::from_u128(WORKFLOW_LLM_OUTPUT_ID);
    let condition_input = PortId::from_u128(WORKFLOW_CONDITION_INPUT_ID);
    let match_output = PortId::from_u128(WORKFLOW_MATCH_PORT_ID);
    let else_output = PortId::from_u128(WORKFLOW_ELSE_PORT_ID);
    let review_output = PortId::from_u128(WORKFLOW_REVIEW_PORT_ID);
    let response_input = PortId::from_u128(WORKFLOW_RESPONSE_LLM_INPUT_ID);
    let response_output = PortId::from_u128(WORKFLOW_RESPONSE_LLM_OUTPUT_ID);
    let cases = [
        ConditionCase::new(
            MATCH_CASE_ID,
            "Intent matches",
            match_output,
            PortId::from_u128(WORKFLOW_MATCH_VALUE_PORT_ID),
            ConditionPredicate::equals(
                "customer_intent",
                serde_json::json!("match"),
                TypeDesc::String,
            )
            .expect("workflow match predicate is valid"),
        )
        .expect("workflow match case is valid"),
        ConditionCase::new(
            REVIEW_CASE_ID,
            "Needs review",
            review_output,
            PortId::from_u128(WORKFLOW_REVIEW_VALUE_PORT_ID),
            ConditionPredicate::equals(
                "customer_intent",
                serde_json::json!("review"),
                TypeDesc::String,
            )
            .expect("workflow review predicate is valid"),
        )
        .expect("workflow review case is valid"),
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
            CanvasRect {
                origin: CanvasPoint { x: 90.0, y: 210.0 },
                size: WORKFLOW_LLM_DEFAULT_SIZE,
            },
            [llm_input, llm_output],
        )
        .expect("workflow LLM fixture must match its catalog ports");
    let condition_creation = catalog
        .instantiate_node_with_ids(
            &NodeKindKey::new(WORKFLOW_CONDITION_KIND),
            condition,
            CanvasRect {
                origin: CanvasPoint { x: 650.0, y: 170.0 },
                size: WORKFLOW_CONDITION_DEFAULT_SIZE,
            },
            [condition_input, else_output],
        )
        .expect("workflow condition fixture must match its fixed catalog ports");
    let response_creation = catalog
        .instantiate_node_with_ids(
            &NodeKindKey::new(WORKFLOW_LLM_KIND),
            response_llm,
            CanvasRect {
                origin: CanvasPoint { x: 650.0, y: 720.0 },
                size: WORKFLOW_LLM_DEFAULT_SIZE,
            },
            [response_input, response_output],
        )
        .expect("workflow response fixture must match its catalog ports");
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
    condition_node.data = condition_data("Route by intent", &cases);
    condition_node.ports.retain(|port| *port != else_output);
    condition_node.ports.extend(
        cases
            .iter()
            .flat_map(|case| [case.value_port_id(), case.port_id()]),
    );
    condition_node.ports.push(else_output);
    graph.insert_node(condition_id, condition_node);
    for (port_id, port) in condition_ports {
        graph.insert_port(port_id, port);
    }
    for case in &cases {
        graph.insert_port(
            case.value_port_id(),
            condition_case_value_port(condition, case),
        );
        graph.insert_port(case.port_id(), condition_case_route_port(condition, case));
    }
    let (response_id, mut response_node, response_ports) = response_creation.into_parts();
    response_node.data = serde_json::json!({
        "title": "Draft response",
        "prompt": "Write a concise response for the matched request.",
        "model": llm::DEFAULT_MODEL,
    });
    graph.insert_node(response_id, response_node);
    for (port_id, port) in response_ports {
        graph.insert_port(port_id, port);
    }
    let mut forward_edge = Edge::new(EdgeKind::Exec, llm_output, condition_input);
    forward_edge.view = EdgeViewDescriptor::new().with_route(EdgeRouteSpec::default_smooth_step());
    graph.insert_edge(EdgeId::from_u128(WORKFLOW_FORWARD_EDGE_ID), forward_edge);
    let mut success_edge = Edge::new(EdgeKind::Exec, match_output, response_input);
    success_edge.view = EdgeViewDescriptor::new().with_route(
        EdgeRouteSpec::smooth_step(
            EdgeRouteSpec::DEFAULT_ORTHOGONAL_OFFSET,
            EdgeRouteSpec::DEFAULT_STEP_POSITION,
            EdgeRouteSpec::DEFAULT_SMOOTH_STEP_BORDER_RADIUS,
            Some(CanvasPoint {
                x: 1_120.0,
                y: 620.0,
            }),
        )
        .expect("workflow success route is valid"),
    );
    graph.insert_edge(EdgeId::from_u128(WORKFLOW_SUCCESS_EDGE_ID), success_edge);
    let mut retry_edge = Edge::new(EdgeKind::Exec, else_output, llm_input);
    retry_edge.view = EdgeViewDescriptor::new().with_route(
        EdgeRouteSpec::smooth_step(
            EdgeRouteSpec::DEFAULT_ORTHOGONAL_OFFSET,
            EdgeRouteSpec::DEFAULT_STEP_POSITION,
            EdgeRouteSpec::DEFAULT_SMOOTH_STEP_BORDER_RADIUS,
            Some(CanvasPoint { x: 520.0, y: 620.0 }),
        )
        .expect("workflow retry route is valid"),
    );
    graph.insert_edge(EdgeId::from_u128(WORKFLOW_RETRY_EDGE_ID), retry_edge);
    graph.build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use open_gpui::Element;
    use open_gpui::accesskit::{Action, Node};
    use open_gpui::{Context, Render, Window};

    const LONG_HEADER_FIXTURE_SELECTOR: &str = "jellyflow-long-header-fixture";

    struct LongHeaderFixture;

    impl Render for LongHeaderFixture {
        fn render(
            &mut self,
            _window: &mut Window,
            _cx: &mut Context<Self>,
        ) -> impl open_gpui::IntoElement {
            div()
                .debug_selector(|| LONG_HEADER_FIXTURE_SELECTOR.to_owned())
                .w(px(260.0))
                .child(workflow_header(
                    NodeId::from_u128(1),
                    "A_REALLY_LONG_UNBROKEN_WORKFLOW_NODE_TITLE_THAT_MUST_NOT_MOVE_METADATA"
                        .to_owned(),
                    "EXEC",
                    WorkflowNodeStatus::Ready,
                    Vec::new(),
                    NativeProductTheme::workflow_dify(),
                ))
        }
    }

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

    #[open_gpui::test]
    fn long_workflow_title_truncates_before_fixed_header_metadata(
        cx: &mut open_gpui::TestAppContext,
    ) {
        let (_fixture, cx) = cx.add_window_view(|_, _| LongHeaderFixture);
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();

        let header = cx
            .debug_bounds(LONG_HEADER_FIXTURE_SELECTOR)
            .expect("fixed workflow header fixture");
        let title = cx
            .debug_bounds(&workflow_header_title_selector(NodeId::from_u128(1)))
            .expect("truncated workflow title");
        let metadata = cx
            .debug_bounds(&workflow_header_metadata_selector(NodeId::from_u128(1)))
            .expect("fixed workflow metadata");

        assert_eq!(header.size.width, px(260.0));
        assert_eq!(header.size.height, px(WORKFLOW_HEADER_HEIGHT));
        assert!(title.is_contained_within(&header));
        assert!(metadata.is_contained_within(&header));
        assert!(title.origin.x + title.size.width + px(10.0) <= metadata.origin.x);
    }

    #[test]
    fn workflow_demo_reserves_a_readable_overview_wire_gutter() {
        let graph = workflow_demo_graph().expect("valid workflow demo graph");
        let llm = graph
            .node(&workflow_llm_node_id())
            .expect("workflow LLM node");
        let condition = graph
            .node(&workflow_condition_node_id())
            .expect("workflow condition node");
        let response = graph
            .node(&NodeId::from_u128(WORKFLOW_RESPONSE_LLM_NODE_ID))
            .expect("workflow response LLM node");
        let llm_width = llm
            .size
            .expect("workflow LLM has an authoritative size")
            .width;

        assert!(condition.pos.x - llm.pos.x - llm_width >= 192.0);
        assert!((condition.pos.y - llm.pos.y).abs() <= 48.0);
        let condition_height = condition
            .size
            .expect("workflow condition has an authoritative size")
            .height;
        assert!(response.pos.y - condition.pos.y - condition_height >= 192.0);
        assert!((response.pos.x - condition.pos.x).abs() <= 1.0);

        let forward = graph
            .edge(&EdgeId::from_u128(WORKFLOW_FORWARD_EDGE_ID))
            .expect("workflow forward edge");
        let success = graph
            .edge(&EdgeId::from_u128(WORKFLOW_SUCCESS_EDGE_ID))
            .expect("workflow success edge");
        let retry = graph
            .edge(&EdgeId::from_u128(WORKFLOW_RETRY_EDGE_ID))
            .expect("workflow retry edge");
        assert_eq!(
            forward.view.route,
            Some(EdgeRouteSpec::default_smooth_step())
        );
        assert_eq!(
            success.view.route,
            Some(
                EdgeRouteSpec::smooth_step(
                    EdgeRouteSpec::DEFAULT_ORTHOGONAL_OFFSET,
                    EdgeRouteSpec::DEFAULT_STEP_POSITION,
                    EdgeRouteSpec::DEFAULT_SMOOTH_STEP_BORDER_RADIUS,
                    Some(CanvasPoint {
                        x: 1_120.0,
                        y: 620.0,
                    }),
                )
                .expect("valid success route"),
            )
        );
        assert_eq!(
            retry.view.route,
            Some(
                EdgeRouteSpec::smooth_step(
                    EdgeRouteSpec::DEFAULT_ORTHOGONAL_OFFSET,
                    EdgeRouteSpec::DEFAULT_STEP_POSITION,
                    EdgeRouteSpec::DEFAULT_SMOOTH_STEP_BORDER_RADIUS,
                    Some(CanvasPoint { x: 520.0, y: 620.0 }),
                )
                .expect("valid retry route"),
            )
        );

        let forward_source = graph.port(&forward.from).expect("forward source port").node;
        let success_target = graph.port(&success.to).expect("success target port").node;
        assert_ne!(forward_source, success_target);
    }
}
