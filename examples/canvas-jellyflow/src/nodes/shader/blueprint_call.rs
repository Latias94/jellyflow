use std::collections::{BTreeMap, BTreeSet};

use jellyflow_core::types::TypeDesc;
use jellyflow_core::{
    CanvasSize, Node, NodeId, NodeKindKey, Port, PortCapacity, PortDirection, PortId, PortKey,
    PortKind,
};
use jellyflow_open_gpui::authoring::{
    OpenGpuiNodeAuthoring, OpenGpuiNodeBinding, OpenGpuiPortHandle,
};
use jellyflow_open_gpui::registry::{OpenGpuiNodeRegistrationPolicy, OpenGpuiNodeRegistryBuilder};
use jellyflow_open_gpui::size_policy::{OpenGpuiContentHeightPolicy, OpenGpuiNodeSizePolicy};
use jellyflow_runtime::commands::{GraphCommand, IncidentEdgePolicy, TopologyEdit, TopologyPort};
use jellyflow_runtime::graph::{GraphRevision, GraphSnapshot, GraphVersion};
use jellyflow_runtime::schema::{NodeKindSpec, NodePortSpec};
use jellyflow_runtime::view::NodeInstanceKey;
use open_gpui::{
    AppContext, Context, Entity, InteractiveElement, ParentElement, Render,
    StatefulInteractiveElement, Styled, Window, div, px,
};
use open_gpui_ui_components::{Button, ButtonVariant};

use super::{ShaderNodeError, ShaderPort, port_element};
use crate::nodes::topology_attempt::{
    TopologyAttempt, TopologyAttemptPhase, TopologyDispatchRequest, TopologyTransitionEffect,
};
use crate::product_theme::NativeProductTheme;

pub const BLUEPRINT_CALL_KIND: &str = "blueprint.call";
pub const BLUEPRINT_CALL_RENDERER: &str = "jellyflow.open-gpui.blueprint.call";
pub const BLUEPRINT_CALL_DEFAULT_SIZE: CanvasSize = CanvasSize {
    width: 320.0,
    height: 240.0,
};
pub const BLUEPRINT_NODE_SELECTOR: &str = "jellyflow-native-blueprint-call";
pub const BLUEPRINT_ADD_INPUT_SELECTOR: &str = "jellyflow-native-blueprint-add-input";
pub const BLUEPRINT_COMMAND_PENDING_SELECTOR: &str = "jellyflow-native-blueprint-command-pending";
pub const BLUEPRINT_COMMAND_DIAGNOSTIC_SELECTOR: &str =
    "jellyflow-native-blueprint-command-diagnostic";
pub const BLUEPRINT_RETRY_SELECTOR: &str = "jellyflow-native-blueprint-retry";

const BLUEPRINT_PINS_FIELD: &str = "pins";
const BLUEPRINT_DYNAMIC_PORT_PREFIX: &str = "arg.";
const BLUEPRINT_BODY_HORIZONTAL_PADDING: f32 = 14.0;
const BLUEPRINT_PIN_HORIZONTAL_PADDING: f32 = 9.0;
const BLUEPRINT_STATIC_PORT_ROW_HEIGHT: f32 = 32.0;
const BLUEPRINT_PIN_ROW_HEIGHT: f32 = 38.0;
const BLUEPRINT_PIN_LIST_MAX_HEIGHT: f32 = 220.0;
const BLUEPRINT_PORT_DIAMETER: f32 = 12.0;
const BLUEPRINT_MIN_HEIGHT: f32 = 190.0;
const BLUEPRINT_MAX_HEIGHT: f32 = 440.0;

/// A durable Blueprint input identity, independent of its name and display order.
#[derive(Clone, Debug, PartialEq)]
pub struct BlueprintPin {
    id: String,
    port_id: PortId,
    name: String,
    ty: TypeDesc,
}

impl BlueprintPin {
    pub fn new(
        id: impl Into<String>,
        port_id: PortId,
        name: impl Into<String>,
        ty: TypeDesc,
    ) -> Result<Self, ShaderNodeError> {
        let id = id.into();
        if id.trim().is_empty() {
            return Err(ShaderNodeError::EmptyBlueprintPinId);
        }
        let name = name.into();
        if name.trim().is_empty() {
            return Err(ShaderNodeError::EmptyBlueprintPinName);
        }
        Ok(Self {
            id: id.trim().to_owned(),
            port_id,
            name: name.trim().to_owned(),
            ty,
        })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub const fn port_id(&self) -> PortId {
        self.port_id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub const fn ty(&self) -> &TypeDesc {
        &self.ty
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum BlueprintPinAction {
    Add(BlueprintPin),
    Reorder { pin_id: String, index: usize },
    Remove { pin_id: String },
}

#[derive(Clone, Debug, PartialEq)]
struct BlueprintPinIntent {
    action: BlueprintPinAction,
    incident_edges: IncidentEdgePolicy,
}

pub struct BlueprintCallNode {
    binding: Entity<OpenGpuiNodeBinding>,
    node_id: NodeId,
    topology: TopologyAttempt<BlueprintPinIntent>,
    product_theme: NativeProductTheme,
    diagnostic: Option<String>,
}

struct BlueprintRenderData {
    instance: NodeInstanceKey,
    revision: GraphRevision,
    title: String,
    static_inputs: Vec<ShaderPort>,
    outputs: Vec<ShaderPort>,
    pins: Vec<(BlueprintPin, OpenGpuiPortHandle)>,
    ports: BTreeMap<PortId, Port>,
    node: Node,
}

impl BlueprintCallNode {
    #[must_use]
    pub fn new(
        binding: Entity<OpenGpuiNodeBinding>,
        instance: NodeInstanceKey,
        product_theme: NativeProductTheme,
    ) -> Self {
        Self {
            binding,
            node_id: instance.node(),
            topology: TopologyAttempt::new(instance),
            product_theme,
            diagnostic: None,
        }
    }

    fn add_input(&mut self, cx: &mut Context<Self>) {
        if self.topology.phase() != TopologyAttemptPhase::Idle {
            return;
        }
        let authoring = match self.binding.read(cx).authoring() {
            Ok(authoring) => authoring,
            Err(error) => {
                self.topology.observe_authoring_error(&error);
                self.diagnostic = Some(error.to_string());
                return;
            }
        };
        let pins = match parse_blueprint_pins(self.node_id, &authoring.node().data) {
            Ok(pins) => pins,
            Err(error) => {
                self.diagnostic = Some(error.to_string());
                return;
            }
        };
        let next_index = next_blueprint_input_index(
            pins.iter()
                .filter_map(|pin| blueprint_input_index(pin.name())),
        );
        let port_id = PortId::new();
        let pin = BlueprintPin::new(
            format!("pin-{}", port_id.0),
            port_id,
            format!("input_{next_index}"),
            TypeDesc::Float,
        );
        match pin {
            Ok(pin) => {
                self.apply_action(BlueprintPinAction::Add(pin), IncidentEdgePolicy::Reject, cx)
            }
            Err(error) => self.diagnostic = Some(error.to_string()),
        }
    }

    fn apply_action(
        &mut self,
        action: BlueprintPinAction,
        incident_edges: IncidentEdgePolicy,
        cx: &mut Context<Self>,
    ) {
        if self.topology.phase() != TopologyAttemptPhase::Idle {
            return;
        }
        self.enqueue_request(
            TopologyDispatchRequest::new(BlueprintPinIntent {
                action,
                incident_edges,
            }),
            cx,
        );
        cx.notify();
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
        request: TopologyDispatchRequest<BlueprintPinIntent>,
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
        let ports = collect_bound_ports(&authoring);
        if blueprint_action_satisfied(
            self.node_id,
            authoring.node(),
            &ports,
            &request.intent().action,
        ) {
            let effect = self.topology.observe(
                authoring.instance(),
                authoring.version().revision(),
                |intent| {
                    blueprint_action_satisfied(
                        self.node_id,
                        authoring.node(),
                        &ports,
                        &intent.action,
                    )
                },
            );
            self.apply_topology_effect(effect, cx);
            return;
        }
        let command = build_blueprint_pin_command(
            authoring.version(),
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
        match dispatch {
            Ok(()) => {}
            Err(error) => {
                self.diagnostic = Some(error.to_string());
                return;
            }
        }
        self.diagnostic = None;
    }

    fn reconcile_command(&mut self, data: &BlueprintRenderData, cx: &mut Context<Self>) {
        let effect = self
            .topology
            .observe(&data.instance, data.revision, |intent| {
                blueprint_action_satisfied(self.node_id, &data.node, &data.ports, &intent.action)
            });
        self.apply_topology_effect(effect, cx);
    }

    fn apply_topology_effect(&mut self, effect: TopologyTransitionEffect, cx: &mut Context<Self>) {
        if effect.topology_became_visible()
            && let Err(error) = self
                .binding
                .update(cx, |binding, cx| binding.invalidate_layout(cx))
        {
            self.diagnostic = Some(format!(
                "blueprint topology became visible but native layout invalidation failed: {error}"
            ));
            return;
        }
        self.diagnostic = self.topology.diagnostic().map(str::to_owned);
    }
}

impl Render for BlueprintCallNode {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        let theme = self.product_theme;
        let node_id = self.node_id;
        let drag_handle = self.binding.read(cx).drag_handle();
        let authoring = self.binding.read(cx).authoring();
        let (binding_valid, mut data) = match authoring {
            Ok(authoring) => match read_blueprint_authoring(&authoring) {
                Ok(data) => {
                    self.reconcile_command(&data, cx);
                    (true, data)
                }
                Err(error) => {
                    self.diagnostic = Some(error.to_string());
                    (false, retired_render_data(drag_handle.instance().clone()))
                }
            },
            Err(error) => {
                self.topology.observe_authoring_error(&error);
                self.diagnostic = Some(error.to_string());
                (false, retired_render_data(drag_handle.instance().clone()))
            }
        };
        let phase = self.topology.phase();
        let commands_locked = phase != TopologyAttemptPhase::Idle || !binding_valid;
        let authoritative_pins = data
            .pins
            .iter()
            .map(|(pin, _)| pin.clone())
            .collect::<Vec<_>>();
        let mut input_ports = data.static_inputs.drain(..);
        let mut output_ports = data.outputs.drain(..);
        let static_row_count = input_ports.len().max(output_ports.len());
        let mut static_rows = div().flex().flex_col().gap(px(theme.spacing.xs));
        for _ in 0..static_row_count {
            let input = input_ports
                .next()
                .map(|port| port_element(port, blueprint_port_selector, theme));
            let output = output_ports
                .next()
                .map(|port| port_element(port, blueprint_port_selector, theme));
            let mut row = div()
                .relative()
                .h(px(BLUEPRINT_STATIC_PORT_ROW_HEIGHT))
                .px(px(BLUEPRINT_PIN_HORIZONTAL_PADDING))
                .flex()
                .items_center()
                .justify_between()
                .rounded(px(theme.radius.control))
                .bg(theme.surface.inset);
            if let Some(input) = input {
                let input_label_selector =
                    blueprint_port_label_selector(node_id, input.key.0.as_str());
                row = row.child(
                    div()
                        .flex()
                        .items_center()
                        .child(
                            div()
                                .absolute()
                                .left(px(-(BLUEPRINT_BODY_HORIZONTAL_PADDING
                                    + BLUEPRINT_PORT_DIAMETER / 2.0)))
                                .top(px((BLUEPRINT_STATIC_PORT_ROW_HEIGHT
                                    - BLUEPRINT_PORT_DIAMETER)
                                    / 2.0))
                                .child(input.marker),
                        )
                        .child(
                            div()
                                .debug_selector(move || input_label_selector.clone())
                                .text_xs()
                                .child(input.label),
                        ),
                );
            }
            if let Some(output) = output {
                let output_label_selector =
                    blueprint_port_label_selector(node_id, output.key.0.as_str());
                row = row.child(
                    div()
                        .flex()
                        .items_center()
                        .child(
                            div()
                                .debug_selector(move || output_label_selector.clone())
                                .text_xs()
                                .child(output.label),
                        )
                        .child(
                            div()
                                .absolute()
                                .right(px(-(BLUEPRINT_BODY_HORIZONTAL_PADDING
                                    + BLUEPRINT_PORT_DIAMETER / 2.0)))
                                .top(px((BLUEPRINT_STATIC_PORT_ROW_HEIGHT
                                    - BLUEPRINT_PORT_DIAMETER)
                                    / 2.0))
                                .child(output.marker),
                        ),
                );
            }
            static_rows = static_rows.child(row);
        }

        let add_node = cx.weak_entity();
        let add_button = div()
            .debug_selector(|| BLUEPRINT_ADD_INPUT_SELECTOR.to_owned())
            .child(
                Button::new(
                    format!("jellyflow-blueprint-add-input-{}", node_id.0),
                    "Add input",
                )
                .variant(ButtonVariant::Secondary)
                .disabled(commands_locked)
                .on_click(move |_, window, cx| {
                    let _ = add_node.update(cx, |node, cx| node.add_input(cx));
                    window.refresh();
                }),
            );

        let mut rows = div().flex().flex_col().gap(px(theme.spacing.sm));
        let pin_count = data.pins.len();
        for (index, (pin, handle)) in data.pins.drain(..).enumerate() {
            let port_id = pin.port_id();
            let marker = port_element(
                ShaderPort {
                    id: port_id,
                    key: dynamic_blueprint_key(pin.id()),
                    direction: PortDirection::In,
                    kind: PortKind::Data,
                    label: pin.name().to_owned().into(),
                    handle,
                },
                blueprint_port_selector,
                theme,
            )
            .marker;
            let row_selector = blueprint_pin_row_selector(node_id, port_id);
            let label_selector = blueprint_pin_label_selector(node_id, port_id);
            let mut actions = div().flex_none().flex().gap(px(4.0));
            if index > 0 {
                let move_node = cx.weak_entity();
                let pin_id = pin.id().to_owned();
                let selector = blueprint_pin_up_selector(node_id, port_id);
                actions = actions.child(
                    div().debug_selector(move || selector.clone()).child(
                        Button::new(format!("jellyflow-blueprint-up-{}", port_id.0), "Up")
                            .variant(ButtonVariant::Secondary)
                            .disabled(commands_locked)
                            .on_click(move |_, window, cx| {
                                let action = BlueprintPinAction::Reorder {
                                    pin_id: pin_id.clone(),
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
            if index + 1 < pin_count {
                let move_node = cx.weak_entity();
                let pin_id = pin.id().to_owned();
                let selector = blueprint_pin_down_selector(node_id, port_id);
                actions = actions.child(
                    div().debug_selector(move || selector.clone()).child(
                        Button::new(format!("jellyflow-blueprint-down-{}", port_id.0), "Down")
                            .variant(ButtonVariant::Secondary)
                            .disabled(commands_locked)
                            .on_click(move |_, window, cx| {
                                let action = BlueprintPinAction::Reorder {
                                    pin_id: pin_id.clone(),
                                    index: index + 1,
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
            let pin_id = pin.id().to_owned();
            let remove_selector = blueprint_pin_remove_selector(node_id, port_id);
            actions = actions.child(
                div().debug_selector(move || remove_selector.clone()).child(
                    Button::new(
                        format!("jellyflow-blueprint-remove-{}", port_id.0),
                        "Remove",
                    )
                    .variant(ButtonVariant::Secondary)
                    .disabled(commands_locked)
                    .on_click(move |_, window, cx| {
                        let action = BlueprintPinAction::Remove {
                            pin_id: pin_id.clone(),
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
                    .debug_selector(move || row_selector.clone())
                    .relative()
                    .h(px(BLUEPRINT_PIN_ROW_HEIGHT))
                    .pl(px(BLUEPRINT_PIN_HORIZONTAL_PADDING))
                    .pr(px(BLUEPRINT_PIN_HORIZONTAL_PADDING))
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
                                    .absolute()
                                    .left(px(-(BLUEPRINT_BODY_HORIZONTAL_PADDING
                                        + BLUEPRINT_PORT_DIAMETER / 2.0)))
                                    .top(px(
                                        (BLUEPRINT_PIN_ROW_HEIGHT - BLUEPRINT_PORT_DIAMETER) / 2.0
                                    ))
                                    .child(marker),
                            )
                            .child(
                                div()
                                    .debug_selector(move || label_selector.clone())
                                    .min_w(px(0.0))
                                    .flex_1()
                                    .truncate()
                                    .text_sm()
                                    .child(pin.name().to_owned()),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .text_xs()
                                    .text_color(theme.text.muted)
                                    .child(type_label(pin.ty())),
                            ),
                    )
                    .child(actions),
            );
        }

        if matches!(
            phase,
            TopologyAttemptPhase::Pending | TopologyAttemptPhase::AwaitingSnapshot
        ) && let Some(intent) = self.topology.active_intent()
            && !blueprint_action_satisfied_from_pins(&authoritative_pins, &intent.action)
        {
            rows = rows.child(
                div()
                    .debug_selector(|| BLUEPRINT_COMMAND_PENDING_SELECTOR.to_owned())
                    .h(px(BLUEPRINT_PIN_ROW_HEIGHT))
                    .px(px(BLUEPRINT_PIN_HORIZONTAL_PADDING))
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
                            .child(pending_action_label(&intent.action)),
                    )
                    .child(div().flex_none().text_xs().child("Pending")),
            );
        }

        let retry_allowed = binding_valid
            && (phase == TopologyAttemptPhase::Recoverable
                || (phase == TopologyAttemptPhase::Uncertain
                    && self.topology.can_confirm_uncertain_retry()));
        let retry = retry_allowed.then(|| {
            let retry_node = cx.weak_entity();
            div()
                .debug_selector(|| BLUEPRINT_RETRY_SELECTOR.to_owned())
                .child(
                    Button::new(
                        format!("jellyflow-blueprint-retry-{}", node_id.0),
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
                )
        });

        let diagnostic = self.diagnostic.clone().map(|message| {
            div()
                .debug_selector(|| BLUEPRINT_COMMAND_DIAGNOSTIC_SELECTOR.to_owned())
                .px(px(8.0))
                .py(px(5.0))
                .rounded(px(theme.radius.control))
                .bg(theme.status.danger.surface)
                .text_xs()
                .text_color(theme.status.danger.text)
                .child(message)
        });
        let pin_scroll_selector = blueprint_pin_scroll_selector(node_id);
        let body_selector = blueprint_body_selector(node_id);
        let controls = div()
            .debug_selector(move || body_selector.clone())
            .px(px(BLUEPRINT_BODY_HORIZONTAL_PADDING))
            .py(px(12.0))
            .flex()
            .flex_col()
            .gap(px(9.0))
            .child(static_rows)
            .child(
                div()
                    .id(format!("jellyflow-blueprint-pin-scroll-{}", node_id.0))
                    .debug_selector(move || pin_scroll_selector.clone())
                    .max_h(px(BLUEPRINT_PIN_LIST_MAX_HEIGHT))
                    .overflow_y_scroll()
                    .child(rows),
            )
            .child(add_button)
            .children(retry)
            .children(diagnostic);

        let header_title_selector = blueprint_header_title_selector(node_id);
        let header_metadata_selector = blueprint_header_metadata_selector(node_id);
        let content = div()
            .debug_selector(|| BLUEPRINT_NODE_SELECTOR.to_owned())
            .relative()
            .w_full()
            .rounded(px(theme.radius.node))
            .border_1()
            .border_color(theme.node.alternate_border)
            .bg(theme.node.alternate_surface)
            .text_color(theme.text.primary)
            .child(
                drag_handle.wrap(
                    div()
                        .h(px(42.0))
                        .px(px(14.0))
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(8.0))
                        .border_b_1()
                        .border_color(theme.header.border)
                        .bg(theme.header.alternate_surface)
                        .child(
                            div()
                                .debug_selector(move || header_title_selector.clone())
                                .min_w(px(0.0))
                                .flex_1()
                                .truncate()
                                .child(data.title),
                        )
                        .child(
                            div()
                                .debug_selector(move || header_metadata_selector.clone())
                                .flex_none()
                                .text_xs()
                                .text_color(theme.status.success.text)
                                .child("BLUEPRINT"),
                        ),
                ),
            )
            .child(controls);
        self.binding.read(cx).measure_content(content)
    }
}

pub(super) fn catalog_spec() -> NodeKindSpec {
    NodeKindSpec::builder(
        BLUEPRINT_CALL_KIND,
        "Function Call",
        BLUEPRINT_CALL_RENDERER,
    )
    .category(["Blueprint", "Functions"])
    .keywords(["call", "function", "dynamic input"])
    .default_size(BLUEPRINT_CALL_DEFAULT_SIZE)
    .ports([
        NodePortSpec::new(
            "exec_in",
            PortDirection::In,
            PortKind::Exec,
            PortCapacity::Single,
        )
        .with_label("In"),
        NodePortSpec::new(
            "exec_out",
            PortDirection::Out,
            PortKind::Exec,
            PortCapacity::Multi,
        )
        .with_label("Out"),
        NodePortSpec::new(
            "value",
            PortDirection::In,
            PortKind::Data,
            PortCapacity::Single,
        )
        .with_type(TypeDesc::Float)
        .with_label("Value"),
    ])
    .default_data(serde_json::json!({
        "title": "Function Call",
        "pins": [],
    }))
    .build()
}

pub(super) fn register(
    registry: &mut OpenGpuiNodeRegistryBuilder,
    product_theme: NativeProductTheme,
) {
    let policy = OpenGpuiNodeRegistrationPolicy::default().with_size_policy(
        OpenGpuiNodeSizePolicy::ContentHeight(
            OpenGpuiContentHeightPolicy::new(BLUEPRINT_MIN_HEIGHT, BLUEPRINT_MAX_HEIGHT)
                .expect("blueprint content-height bounds must be valid"),
        ),
    );
    registry.register_with_policy(
        BLUEPRINT_CALL_RENDERER,
        policy,
        move |binding, _window, cx| {
            let instance = binding
                .read(cx)
                .authoring()
                .expect("a Blueprint node factory must receive a live authoring binding")
                .instance()
                .clone();
            cx.new(|_| BlueprintCallNode::new(binding, instance, product_theme))
        },
    );
}

#[must_use]
pub fn blueprint_port_selector(port: PortId) -> String {
    format!("jellyflow-native-blueprint-port-{port:?}")
}

#[must_use]
pub fn blueprint_header_title_selector(node: NodeId) -> String {
    format!("jellyflow-native-blueprint-header-title-{}", node.0)
}

#[must_use]
pub fn blueprint_header_metadata_selector(node: NodeId) -> String {
    format!("jellyflow-native-blueprint-header-metadata-{}", node.0)
}

#[must_use]
pub fn blueprint_body_selector(node: NodeId) -> String {
    format!("jellyflow-native-blueprint-body-{}", node.0)
}

#[must_use]
pub fn blueprint_port_label_selector(node: NodeId, port_key: &str) -> String {
    format!(
        "jellyflow-native-blueprint-port-label-{}-{port_key}",
        node.0
    )
}

#[must_use]
pub fn blueprint_pin_row_selector(node: NodeId, port: PortId) -> String {
    format!("jellyflow-native-blueprint-pin-row-{node:?}-{port:?}")
}

#[must_use]
pub fn blueprint_pin_label_selector(node: NodeId, port: PortId) -> String {
    format!("jellyflow-native-blueprint-pin-label-{node:?}-{port:?}")
}

#[must_use]
pub fn blueprint_pin_up_selector(node: NodeId, port: PortId) -> String {
    format!("jellyflow-native-blueprint-pin-up-{node:?}-{port:?}")
}

#[must_use]
pub fn blueprint_pin_down_selector(node: NodeId, port: PortId) -> String {
    format!("jellyflow-native-blueprint-pin-down-{node:?}-{port:?}")
}

#[must_use]
pub fn blueprint_pin_remove_selector(node: NodeId, port: PortId) -> String {
    format!("jellyflow-native-blueprint-pin-remove-{node:?}-{port:?}")
}

#[must_use]
pub fn blueprint_pin_scroll_selector(node: NodeId) -> String {
    format!("jellyflow-native-blueprint-pin-scroll-{node:?}")
}

/// Reads and validates the durable dynamic pin schema against real graph ports.
pub fn blueprint_pins(
    snapshot: &GraphSnapshot,
    node_id: NodeId,
) -> Result<Vec<BlueprintPin>, ShaderNodeError> {
    let node = snapshot
        .graph()
        .node(&node_id)
        .ok_or(ShaderNodeError::MissingNode { node: node_id })?;
    ensure_blueprint_kind(node_id, node)?;
    let ports = collect_snapshot_ports(snapshot, node_id, node)?;
    let pins = parse_blueprint_pins(node_id, &node.data)?;
    validate_blueprint_topology(node_id, node, &ports, &pins)?;
    Ok(pins)
}

/// Plans one atomic Blueprint pin topology edit from an owned graph snapshot.
pub fn plan_blueprint_pin_action(
    snapshot: &GraphSnapshot,
    node_id: NodeId,
    action: BlueprintPinAction,
    incident_edges: IncidentEdgePolicy,
) -> Result<GraphCommand, ShaderNodeError> {
    let node = snapshot
        .graph()
        .node(&node_id)
        .ok_or(ShaderNodeError::MissingNode { node: node_id })?;
    let ports = collect_snapshot_ports(snapshot, node_id, node)?;
    build_blueprint_pin_command(
        snapshot.version(),
        node_id,
        node,
        &ports,
        action,
        incident_edges,
    )
}

fn collect_snapshot_ports(
    snapshot: &GraphSnapshot,
    node_id: NodeId,
    node: &Node,
) -> Result<BTreeMap<PortId, Port>, ShaderNodeError> {
    node.ports
        .iter()
        .map(|id| {
            snapshot
                .graph()
                .port(id)
                .cloned()
                .map(|port| (*id, port))
                .ok_or(ShaderNodeError::MissingPort {
                    node: node_id,
                    port: *id,
                })
        })
        .collect()
}

fn collect_bound_ports(authoring: &OpenGpuiNodeAuthoring) -> BTreeMap<PortId, Port> {
    authoring
        .ports()
        .map(|port| (port.id(), port.facts().clone()))
        .collect()
}

fn read_blueprint_authoring(
    authoring: &OpenGpuiNodeAuthoring,
) -> Result<BlueprintRenderData, ShaderNodeError> {
    let node_id = authoring.node_id();
    let node = authoring.node();
    ensure_blueprint_kind(node_id, node)?;
    let ports = collect_bound_ports(authoring);
    let pins = parse_blueprint_pins(node_id, &node.data)?;
    validate_blueprint_topology(node_id, node, &ports, &pins)?;
    let mut handles = authoring
        .ports()
        .map(|port| (port.id(), port.handle().clone()))
        .collect::<BTreeMap<_, _>>();
    let pin_rows = pins
        .iter()
        .map(|pin| {
            handles
                .remove(&pin.port_id())
                .map(|handle| (pin.clone(), handle))
                .ok_or(ShaderNodeError::MissingPort {
                    node: node_id,
                    port: pin.port_id(),
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let static_ports = authoring
        .ports()
        .filter_map(|port| {
            handles.remove(&port.id()).map(|handle| ShaderPort {
                id: port.id(),
                key: port.facts().key.clone(),
                direction: port.facts().dir,
                kind: port.facts().kind,
                label: blueprint_port_label(port.facts()).into(),
                handle,
            })
        })
        .collect::<Vec<_>>();
    let (static_inputs, outputs) = static_ports
        .into_iter()
        .partition(|port| port.direction == PortDirection::In);
    let title = node
        .data
        .get("title")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("Function Call")
        .to_owned();
    Ok(BlueprintRenderData {
        instance: authoring.instance().clone(),
        revision: authoring.version().revision(),
        title,
        static_inputs,
        outputs,
        pins: pin_rows,
        ports,
        node: node.clone(),
    })
}

fn retired_render_data(instance: NodeInstanceKey) -> BlueprintRenderData {
    BlueprintRenderData {
        instance,
        revision: 0,
        title: "Retired Blueprint node".to_owned(),
        static_inputs: Vec::new(),
        outputs: Vec::new(),
        pins: Vec::new(),
        ports: BTreeMap::new(),
        node: Node {
            kind: NodeKindKey::new(BLUEPRINT_CALL_KIND),
            kind_version: 1,
            pos: jellyflow_core::CanvasPoint { x: 0.0, y: 0.0 },
            origin: None,
            selectable: None,
            draggable: None,
            connectable: None,
            deletable: None,
            parent: None,
            extent: None,
            expand_parent: None,
            size: None,
            hidden: false,
            collapsed: false,
            ports: Vec::new(),
            data: serde_json::json!({ "title": "Retired Blueprint node", "pins": [] }),
        },
    }
}

fn blueprint_action_satisfied(
    node_id: NodeId,
    node: &Node,
    ports: &BTreeMap<PortId, Port>,
    action: &BlueprintPinAction,
) -> bool {
    let Ok(pins) = parse_blueprint_pins(node_id, &node.data) else {
        return false;
    };
    validate_blueprint_topology(node_id, node, ports, &pins).is_ok()
        && blueprint_action_satisfied_from_pins(&pins, action)
}

fn blueprint_action_satisfied_from_pins(
    pins: &[BlueprintPin],
    action: &BlueprintPinAction,
) -> bool {
    match action {
        BlueprintPinAction::Add(pin) => pins.iter().any(|current| current == pin),
        BlueprintPinAction::Reorder { pin_id, index } => {
            pins.get(*index).is_some_and(|pin| pin.id() == pin_id)
        }
        BlueprintPinAction::Remove { pin_id } => pins.iter().all(|pin| pin.id() != pin_id),
    }
}

fn build_blueprint_pin_command(
    version: GraphVersion,
    node_id: NodeId,
    node: &Node,
    ports: &BTreeMap<PortId, Port>,
    action: BlueprintPinAction,
    incident_edges: IncidentEdgePolicy,
) -> Result<GraphCommand, ShaderNodeError> {
    ensure_blueprint_kind(node_id, node)?;
    let original_pins = parse_blueprint_pins(node_id, &node.data)?;
    validate_blueprint_topology(node_id, node, ports, &original_pins)?;
    let mut pins = original_pins;
    match action {
        BlueprintPinAction::Add(pin) => {
            if pins.iter().any(|current| current.id == pin.id) {
                return Err(ShaderNodeError::DuplicateBlueprintPinId { pin_id: pin.id });
            }
            if pins.iter().any(|current| current.port_id == pin.port_id)
                || ports.contains_key(&pin.port_id)
            {
                return Err(ShaderNodeError::DuplicateBlueprintPin { port: pin.port_id });
            }
            if pins.iter().any(|current| current.name == pin.name) {
                return Err(ShaderNodeError::DuplicateBlueprintPinName { name: pin.name });
            }
            pins.push(pin);
        }
        BlueprintPinAction::Reorder { pin_id, index } => {
            let current = pins
                .iter()
                .position(|pin| pin.id == pin_id)
                .ok_or_else(|| ShaderNodeError::BlueprintPinNotFound {
                    pin_id: pin_id.clone(),
                })?;
            if index >= pins.len() {
                return Err(ShaderNodeError::InvalidBlueprintPinIndex {
                    pin_id,
                    index,
                    len: pins.len(),
                });
            }
            let pin = pins.remove(current);
            pins.insert(index, pin);
        }
        BlueprintPinAction::Remove { pin_id } => {
            let index = pins
                .iter()
                .position(|pin| pin.id == pin_id)
                .ok_or(ShaderNodeError::BlueprintPinNotFound { pin_id })?;
            pins.remove(index);
        }
    }
    validate_unique_pins(&pins)?;
    let data = replace_blueprint_pins(node_id, &node.data, &pins)?;
    let static_ports = static_blueprint_ports(node_id, node, ports)?;
    let ordered = static_ports
        .into_iter()
        .chain(
            pins.iter()
                .map(|pin| (pin.port_id(), dynamic_blueprint_port(node_id, pin))),
        )
        .map(|(id, port)| TopologyPort::new(id, port));
    Ok(GraphCommand::topology(
        version,
        TopologyEdit::new()
            .set_node_data(node_id, data)
            .replace_node_ports(node_id, ordered, incident_edges),
    )
    .with_label("edit blueprint pins"))
}

fn parse_blueprint_pins(
    node_id: NodeId,
    data: &serde_json::Value,
) -> Result<Vec<BlueprintPin>, ShaderNodeError> {
    let raw_pins = data
        .get(BLUEPRINT_PINS_FIELD)
        .ok_or(ShaderNodeError::InvalidData {
            node: node_id,
            field: BLUEPRINT_PINS_FIELD,
        })?;
    let raw_pins = raw_pins.as_array().ok_or(ShaderNodeError::InvalidData {
        node: node_id,
        field: BLUEPRINT_PINS_FIELD,
    })?;
    let pins = raw_pins
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let id = value.get("id").and_then(serde_json::Value::as_str).ok_or(
                ShaderNodeError::InvalidData {
                    node: node_id,
                    field: "pins[].id",
                },
            )?;
            let port_id = value
                .get("port_id")
                .cloned()
                .ok_or(ShaderNodeError::InvalidData {
                    node: node_id,
                    field: "pins[].port_id",
                })
                .and_then(|value| {
                    serde_json::from_value(value).map_err(|_| ShaderNodeError::InvalidData {
                        node: node_id,
                        field: "pins[].port_id",
                    })
                })?;
            let name = value
                .get("name")
                .and_then(serde_json::Value::as_str)
                .ok_or(ShaderNodeError::InvalidData {
                    node: node_id,
                    field: "pins[].name",
                })?;
            let ty = value
                .get("type")
                .cloned()
                .ok_or(ShaderNodeError::InvalidData {
                    node: node_id,
                    field: "pins[].type",
                })
                .and_then(|value| {
                    serde_json::from_value(value).map_err(|_| ShaderNodeError::InvalidData {
                        node: node_id,
                        field: "pins[].type",
                    })
                })?;
            let order = value
                .get("order")
                .and_then(serde_json::Value::as_u64)
                .and_then(|order| usize::try_from(order).ok())
                .ok_or(ShaderNodeError::InvalidData {
                    node: node_id,
                    field: "pins[].order",
                })?;
            if order != index {
                return Err(ShaderNodeError::InvalidData {
                    node: node_id,
                    field: "pins[].order",
                });
            }
            BlueprintPin::new(id, port_id, name, ty)
        })
        .collect::<Result<Vec<_>, _>>()?;
    validate_unique_pins(&pins)?;
    Ok(pins)
}

fn replace_blueprint_pins(
    node_id: NodeId,
    data: &serde_json::Value,
    pins: &[BlueprintPin],
) -> Result<serde_json::Value, ShaderNodeError> {
    let mut data = data.clone();
    let object = data.as_object_mut().ok_or(ShaderNodeError::InvalidData {
        node: node_id,
        field: "object",
    })?;
    object.insert(
        BLUEPRINT_PINS_FIELD.to_owned(),
        serde_json::Value::Array(
            pins.iter()
                .enumerate()
                .map(|(order, pin)| {
                    serde_json::json!({
                        "id": pin.id,
                        "port_id": pin.port_id,
                        "name": pin.name,
                        "type": pin.ty,
                        "order": order,
                    })
                })
                .collect(),
        ),
    );
    Ok(data)
}

fn validate_unique_pins(pins: &[BlueprintPin]) -> Result<(), ShaderNodeError> {
    let mut ids = BTreeSet::new();
    let mut ports = BTreeSet::new();
    let mut names = BTreeSet::new();
    for pin in pins {
        if !ids.insert(pin.id.as_str()) {
            return Err(ShaderNodeError::DuplicateBlueprintPinId {
                pin_id: pin.id.clone(),
            });
        }
        if !ports.insert(pin.port_id) {
            return Err(ShaderNodeError::DuplicateBlueprintPin { port: pin.port_id });
        }
        if !names.insert(pin.name.as_str()) {
            return Err(ShaderNodeError::DuplicateBlueprintPinName {
                name: pin.name.clone(),
            });
        }
    }
    Ok(())
}

fn validate_blueprint_topology(
    node_id: NodeId,
    node: &Node,
    ports: &BTreeMap<PortId, Port>,
    pins: &[BlueprintPin],
) -> Result<(), ShaderNodeError> {
    let static_ports = static_blueprint_ports(node_id, node, ports)?;
    for pin in pins {
        let port = ports
            .get(&pin.port_id())
            .ok_or(ShaderNodeError::MissingPort {
                node: node_id,
                port: pin.port_id(),
            })?;
        if !blueprint_pin_matches(node_id, port, pin) {
            return Err(ShaderNodeError::InvalidData {
                node: node_id,
                field: "pins[].port_id",
            });
        }
    }
    let expected = static_ports
        .iter()
        .map(|(id, _)| *id)
        .chain(pins.iter().map(BlueprintPin::port_id))
        .collect::<Vec<_>>();
    if node.ports != expected || ports.len() != expected.len() {
        return Err(ShaderNodeError::InvalidBlueprintPortOrder { node: node_id });
    }
    Ok(())
}

fn static_blueprint_ports(
    node_id: NodeId,
    node: &Node,
    ports: &BTreeMap<PortId, Port>,
) -> Result<Vec<(PortId, Port)>, ShaderNodeError> {
    let mut result = Vec::with_capacity(3);
    for (key, direction, kind, capacity, ty) in [
        (
            "exec_in",
            PortDirection::In,
            PortKind::Exec,
            PortCapacity::Single,
            None,
        ),
        (
            "exec_out",
            PortDirection::Out,
            PortKind::Exec,
            PortCapacity::Multi,
            None,
        ),
        (
            "value",
            PortDirection::In,
            PortKind::Data,
            PortCapacity::Single,
            Some(TypeDesc::Float),
        ),
    ] {
        let mut matches = node.ports.iter().filter_map(|id| {
            ports
                .get(id)
                .filter(|port| port.node == node_id && port.key.0 == key)
                .map(|port| (*id, port.clone()))
        });
        let Some(port) = matches.next() else {
            return Err(ShaderNodeError::InvalidData {
                node: node_id,
                field: "static ports",
            });
        };
        if matches.next().is_some() {
            return Err(ShaderNodeError::InvalidData {
                node: node_id,
                field: "static ports",
            });
        }
        if port.1.dir != direction
            || port.1.kind != kind
            || port.1.capacity != capacity
            || port.1.ty != ty
        {
            return Err(ShaderNodeError::InvalidData {
                node: node_id,
                field: "static ports",
            });
        }
        result.push(port);
    }
    Ok(result)
}

fn blueprint_pin_matches(node_id: NodeId, port: &Port, pin: &BlueprintPin) -> bool {
    port.node == node_id
        && port.key == dynamic_blueprint_key(pin.id())
        && port.dir == PortDirection::In
        && port.kind == PortKind::Data
        && port.capacity == PortCapacity::Single
        && port.ty.as_ref() == Some(pin.ty())
        && port.data.get("pin_id").and_then(serde_json::Value::as_str) == Some(pin.id())
        && port.data.get("label").and_then(serde_json::Value::as_str) == Some(pin.name())
}

fn ensure_blueprint_kind(node_id: NodeId, node: &Node) -> Result<(), ShaderNodeError> {
    if node.kind == NodeKindKey::new(BLUEPRINT_CALL_KIND) {
        Ok(())
    } else {
        Err(ShaderNodeError::WrongNodeKind {
            node: node_id,
            expected: BLUEPRINT_CALL_KIND,
            actual: node.kind.0.clone(),
        })
    }
}

fn dynamic_blueprint_port(node: NodeId, pin: &BlueprintPin) -> Port {
    Port {
        node,
        key: dynamic_blueprint_key(pin.id()),
        dir: PortDirection::In,
        kind: PortKind::Data,
        capacity: PortCapacity::Single,
        connectable: None,
        connectable_start: None,
        connectable_end: None,
        ty: Some(pin.ty.clone()),
        data: serde_json::json!({
            "pin_id": pin.id,
            "label": pin.name,
        }),
    }
}

fn dynamic_blueprint_key(pin_id: &str) -> PortKey {
    PortKey::new(format!("{BLUEPRINT_DYNAMIC_PORT_PREFIX}{pin_id}"))
}

fn blueprint_input_index(name: &str) -> Option<usize> {
    name.strip_prefix("input_")?
        .parse::<usize>()
        .ok()
        .filter(|index| *index > 0)
}

fn next_blueprint_input_index(indices: impl IntoIterator<Item = usize>) -> usize {
    let occupied = indices.into_iter().collect::<BTreeSet<_>>();
    occupied
        .iter()
        .enumerate()
        .find_map(|(offset, index)| {
            let expected = offset.saturating_add(1);
            (*index != expected).then_some(expected)
        })
        .unwrap_or_else(|| occupied.len().saturating_add(1))
}

pub(super) fn blueprint_port_label(port: &Port) -> String {
    port.data
        .get("label")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(&port.key.0)
        .to_owned()
}

fn pending_action_label(action: &BlueprintPinAction) -> String {
    match action {
        BlueprintPinAction::Add(pin) => format!("Add {}", pin.name()),
        BlueprintPinAction::Reorder { pin_id, index } => {
            format!("Move {pin_id} to position {}", index + 1)
        }
        BlueprintPinAction::Remove { pin_id } => format!("Remove {pin_id}"),
    }
}

fn type_label(ty: &TypeDesc) -> &'static str {
    match ty {
        TypeDesc::Bool => "BOOL",
        TypeDesc::Int => "INT",
        TypeDesc::Float => "FLOAT",
        TypeDesc::String => "STRING",
        TypeDesc::Bytes => "BYTES",
        TypeDesc::Any => "ANY",
        TypeDesc::Unknown => "UNKNOWN",
        TypeDesc::Never => "NEVER",
        TypeDesc::Null => "NULL",
        TypeDesc::List { .. } => "LIST",
        TypeDesc::Map { .. } => "MAP",
        TypeDesc::Object { .. } => "OBJECT",
        TypeDesc::Union { .. } => "UNION",
        TypeDesc::Option { .. } => "OPTION",
        TypeDesc::Var { .. } => "VAR",
        TypeDesc::Opaque { .. } => "OPAQUE",
    }
}

#[cfg(test)]
mod tests {
    use jellyflow_runtime::commands::GraphCommandOutcome;
    use jellyflow_runtime::graph::GraphRuntime;

    use super::*;

    #[test]
    fn data_without_pins_is_rejected_without_a_compatibility_path() {
        let legacy = serde_json::json!({ "title": "Legacy static call" });
        assert!(matches!(
            parse_blueprint_pins(test_node_id(), &legacy),
            Err(ShaderNodeError::InvalidData { field: "pins", .. })
        ));
    }

    #[test]
    fn add_reorder_remove_persist_complete_identity_and_real_port_order() {
        let runtime = blueprint_runtime();
        let first = test_pin("roughness", test_dynamic_port_id());
        let second = BlueprintPin::new(
            "metallic",
            PortId::from_u128(0xb106),
            "metallic",
            TypeDesc::Float,
        )
        .unwrap();
        apply(
            &runtime,
            BlueprintPinAction::Add(first.clone()),
            IncidentEdgePolicy::Reject,
        );
        apply(
            &runtime,
            BlueprintPinAction::Add(second.clone()),
            IncidentEdgePolicy::Reject,
        );
        apply(
            &runtime,
            BlueprintPinAction::Reorder {
                pin_id: second.id().to_owned(),
                index: 0,
            },
            IncidentEdgePolicy::Reject,
        );
        let reordered = runtime.snapshot().unwrap();
        assert_eq!(
            blueprint_pins(&reordered, test_node_id()).unwrap(),
            vec![second.clone(), first.clone()]
        );
        assert_eq!(
            &reordered.graph().node(&test_node_id()).unwrap().ports[3..],
            &[second.port_id(), first.port_id()]
        );
        assert_eq!(
            reordered.graph().port(&first.port_id()).unwrap().key,
            dynamic_blueprint_key(first.id())
        );

        apply(
            &runtime,
            BlueprintPinAction::Remove {
                pin_id: second.id().to_owned(),
            },
            IncidentEdgePolicy::Cascade,
        );
        let removed = runtime.snapshot().unwrap();
        assert_eq!(
            blueprint_pins(&removed, test_node_id()).unwrap(),
            vec![first.clone()]
        );
        assert!(removed.graph().port(&second.port_id()).is_none());
        assert!(removed.graph().port(&first.port_id()).is_some());
    }

    #[test]
    fn satisfied_action_requires_complete_ordered_authoritative_ports() {
        let runtime = blueprint_runtime();
        let pin = test_pin("roughness", test_dynamic_port_id());
        apply(
            &runtime,
            BlueprintPinAction::Add(pin.clone()),
            IncidentEdgePolicy::Reject,
        );
        let snapshot = runtime.snapshot().unwrap();
        let node_id = test_node_id();
        let node = snapshot.graph().node(&node_id).unwrap().clone();
        let ports = collect_snapshot_ports(&snapshot, node_id, &node).unwrap();
        let action = BlueprintPinAction::Add(pin.clone());
        assert!(blueprint_action_satisfied(node_id, &node, &ports, &action));

        let mut missing = ports.clone();
        missing.remove(&pin.port_id());
        assert!(!blueprint_action_satisfied(
            node_id, &node, &missing, &action
        ));

        let mut wrong_owner = ports.clone();
        wrong_owner.get_mut(&pin.port_id()).unwrap().node = NodeId::from_u128(0xdead);
        assert!(!blueprint_action_satisfied(
            node_id,
            &node,
            &wrong_owner,
            &action
        ));

        let mut wrong_order = node.clone();
        wrong_order.ports.swap(0, 3);
        assert!(!blueprint_action_satisfied(
            node_id,
            &wrong_order,
            &ports,
            &action
        ));
    }

    #[test]
    fn topology_edit_undo_and_redo_restore_data_and_ports_together() {
        let runtime = blueprint_runtime();
        let pin = test_pin("roughness", test_dynamic_port_id());
        apply(
            &runtime,
            BlueprintPinAction::Add(pin.clone()),
            IncidentEdgePolicy::Reject,
        );
        let added = runtime.snapshot().unwrap();
        assert_eq!(
            blueprint_pins(&added, test_node_id()).unwrap(),
            vec![pin.clone()]
        );
        assert_eq!(
            added.graph().node(&test_node_id()).unwrap().data["pins"][0]["order"],
            0,
        );

        assert!(matches!(
            runtime
                .execute(GraphCommand::undo(added.version()))
                .unwrap(),
            GraphCommandOutcome::Applied(_)
        ));
        let undone = runtime.snapshot().unwrap();
        assert!(blueprint_pins(&undone, test_node_id()).unwrap().is_empty());
        assert!(undone.graph().port(&pin.port_id()).is_none());

        assert!(matches!(
            runtime
                .execute(GraphCommand::redo(undone.version()))
                .unwrap(),
            GraphCommandOutcome::Applied(_)
        ));
        let redone = runtime.snapshot().unwrap();
        assert_eq!(
            blueprint_pins(&redone, test_node_id()).unwrap(),
            vec![pin.clone()]
        );
        assert!(redone.graph().port(&pin.port_id()).is_some());
    }

    #[test]
    fn persisted_pin_order_is_explicit_and_must_match_array_order() {
        let data = serde_json::json!({
            "title": "Broken order",
            "pins": [{
                "id": "first",
                "port_id": test_dynamic_port_id(),
                "name": "First",
                "type": TypeDesc::Float,
                "order": 1,
            }],
        });
        assert!(matches!(
            parse_blueprint_pins(test_node_id(), &data),
            Err(ShaderNodeError::InvalidData {
                field: "pins[].order",
                ..
            })
        ));
    }

    fn blueprint_runtime() -> GraphRuntime {
        let catalog = crate::nodes::native_node_catalog().unwrap();
        let creation = catalog
            .instantiate_node_with_ids(
                &NodeKindKey::new(BLUEPRINT_CALL_KIND),
                test_node_id(),
                jellyflow_core::CanvasPoint { x: 0.0, y: 0.0 },
                [
                    PortId::from_u128(0xb101),
                    PortId::from_u128(0xb102),
                    PortId::from_u128(0xb103),
                ],
            )
            .unwrap();
        let (node_id, node, ports) = creation.into_parts();
        let mut graph =
            jellyflow_core::GraphBuilder::new(jellyflow_core::GraphId::from_u128(0xb100));
        graph.insert_node(node_id, node);
        for (port_id, port) in ports {
            graph.insert_port(port_id, port);
        }
        GraphRuntime::builder(catalog)
            .build(graph.build().unwrap())
            .unwrap()
    }

    fn apply(runtime: &GraphRuntime, action: BlueprintPinAction, policy: IncidentEdgePolicy) {
        let snapshot = runtime.snapshot().unwrap();
        let command = plan_blueprint_pin_action(&snapshot, test_node_id(), action, policy).unwrap();
        assert!(matches!(
            runtime.execute(command).unwrap(),
            GraphCommandOutcome::Applied(_)
        ));
    }

    fn test_node_id() -> NodeId {
        NodeId::from_u128(0xb100)
    }

    fn test_dynamic_port_id() -> PortId {
        PortId::from_u128(0xb105)
    }

    fn test_pin(id: &str, port_id: PortId) -> BlueprintPin {
        BlueprintPin::new(id, port_id, id, TypeDesc::Float).unwrap()
    }
}
