use std::collections::{BTreeMap, BTreeSet};

use jellyflow_core::{
    CanvasSize, NodeId, NodeKindKey, Port, PortCapacity, PortDirection, PortId, PortKind, TypeDesc,
};
use jellyflow_open_gpui::authoring::{
    MutationLayoutEffect, MutationRequest, MutationSettlement, OpenGpuiManagedMutation,
    OpenGpuiNodeAuthoring, OpenGpuiNodeBinding, OpenGpuiPortHandle, OpenGpuiPortSide,
};
use jellyflow_open_gpui::registry::{
    OpenGpuiNodeAdapterPresentation, OpenGpuiNodeRegistrationPolicy, OpenGpuiNodeRegistryBuilder,
    OpenGpuiNodeRendererDescriptor,
};
use jellyflow_open_gpui::size_policy::{OpenGpuiContentHeightPolicy, OpenGpuiNodeSizePolicy};
use jellyflow_runtime::commands::IncidentEdgePolicy;
use jellyflow_runtime::geometry::HandlePosition;
use jellyflow_runtime::schema::{NodeKindSpec, NodePortSpec};
use jellyflow_runtime::view::NodeInstanceKey;
use jellyflow_runtime::view::RendererStamp;
use open_gpui::{
    AppContext, Context, Entity, InteractiveElement, ParentElement, Render,
    StatefulInteractiveElement, Styled, Window, div, px,
};
use open_gpui_ui_components::{Button, ButtonVariant};

use super::{ShaderPort, ShaderPortElement, port_element};
use crate::product_catalog::{NativeProductCatalog, NativeProductNodePresentation};
use crate::product_domain::blueprint::{
    BlueprintError, BlueprintIntent, BlueprintObservation, BlueprintPin, blueprint_default_data,
    dynamic_pin_key,
};
use crate::product_theme::NativeProductTheme;

pub use crate::product_domain::blueprint::BLUEPRINT_CALL_KIND;

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
pub const BLUEPRINT_DISCARD_SELECTOR: &str = "jellyflow-native-blueprint-discard";

const BLUEPRINT_BODY_HORIZONTAL_PADDING: f32 = 14.0;
const BLUEPRINT_PIN_HORIZONTAL_PADDING: f32 = 9.0;
const BLUEPRINT_STATIC_PORT_ROW_HEIGHT: f32 = 32.0;
const BLUEPRINT_PIN_ROW_HEIGHT: f32 = 38.0;
const BLUEPRINT_PIN_LIST_MAX_HEIGHT: f32 = 220.0;
const BLUEPRINT_MIN_HEIGHT: f32 = 190.0;
const BLUEPRINT_MAX_HEIGHT: f32 = 440.0;

pub struct BlueprintCallNode {
    binding: Entity<OpenGpuiNodeBinding>,
    node_id: NodeId,
    mutation: OpenGpuiManagedMutation<BlueprintIntent>,
    product_theme: NativeProductTheme,
    product_presentation: NativeProductNodePresentation,
    mutation_diagnostic: Option<String>,
}

struct BlueprintRenderData {
    observation: Option<BlueprintObservation>,
    title: String,
    static_inputs: Vec<ShaderPort>,
    outputs: Vec<ShaderPort>,
    pins: Vec<(BlueprintPin, OpenGpuiPortHandle)>,
}

impl BlueprintCallNode {
    #[must_use]
    fn new(
        binding: Entity<OpenGpuiNodeBinding>,
        instance: NodeInstanceKey,
        mutation: OpenGpuiManagedMutation<BlueprintIntent>,
        product_theme: NativeProductTheme,
        product_presentation: NativeProductNodePresentation,
    ) -> Self {
        Self {
            binding,
            node_id: instance.node(),
            mutation,
            product_theme,
            product_presentation,
            mutation_diagnostic: None,
        }
    }

    fn add_input(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.mutation.settlement(), MutationSettlement::Ready) {
            return;
        }
        let authoring = match self.binding.read(cx).authoring() {
            Ok(authoring) => authoring,
            Err(error) => {
                let settlement = self.mutation.observe_authoring_error(&error);
                self.apply_mutation_settlement(&settlement);
                return;
            }
        };
        let observation = match blueprint_observation(&authoring) {
            Ok(observation) => observation,
            Err(error) => {
                self.mutation_diagnostic = Some(error.to_string());
                return;
            }
        };
        let next_index = next_blueprint_input_index(
            observation
                .pins()
                .iter()
                .filter_map(|pin| blueprint_input_index(pin.name())),
        );
        let port_id = PortId::new();
        let pin = BlueprintPin::new(
            format!("pin-{}", port_id.0),
            port_id,
            format!("input_{next_index}"),
            TypeDesc::Float,
        )
        .expect("generated Blueprint pins are valid");
        self.apply_intent(BlueprintIntent::add(pin), cx);
    }

    fn apply_intent(&mut self, intent: BlueprintIntent, cx: &mut Context<Self>) {
        if !matches!(self.mutation.settlement(), MutationSettlement::Ready) {
            return;
        }
        self.enqueue_request(MutationRequest::new(intent), cx);
        cx.notify();
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
        request: MutationRequest<BlueprintIntent>,
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
        let observation = match blueprint_observation(&authoring) {
            Ok(observation) => observation,
            Err(error) => {
                self.reject_request(request, error.to_string());
                return;
            }
        };
        if observation.satisfies(request.intent()) {
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

    fn reject_request(&mut self, request: MutationRequest<BlueprintIntent>, diagnostic: String) {
        if !self.mutation.reject_request(request) {
            self.mutation_diagnostic = Some(diagnostic);
            return;
        }
        self.mutation_diagnostic = Some(diagnostic);
    }

    fn reconcile_command(
        &mut self,
        authoring: &OpenGpuiNodeAuthoring,
        observation: &BlueprintObservation,
        cx: &mut Context<Self>,
    ) -> MutationSettlement {
        self.mutation
            .observe(authoring, |intent| observation.satisfies(intent), cx)
    }

    fn apply_mutation_settlement(&mut self, settlement: &MutationSettlement) {
        self.mutation_diagnostic = settlement.diagnostic().map(ToString::to_string);
    }

    fn current_diagnostic(&self) -> Option<&str> {
        self.mutation_diagnostic.as_deref().or_else(|| {
            self.mutation
                .layout_failure()
                .map(jellyflow_open_gpui::authoring::MutationDiagnostic::message)
        })
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
        let settlement;
        let data = match authoring {
            Ok(authoring) => {
                match read_blueprint_authoring(&authoring, &self.product_presentation) {
                    Ok(data) => {
                        settlement = self.reconcile_command(
                            &authoring,
                            data.observation
                                .as_ref()
                                .expect("live Blueprint render data has an observation"),
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
                }
            }
            Err(error) => {
                settlement = self.mutation.observe_authoring_error(&error);
                self.apply_mutation_settlement(&settlement);
                Err(error.to_string())
            }
        };
        let binding_valid = data.is_ok();
        let mut data = match data {
            Ok(data) => data,
            Err(error) => {
                self.mutation_diagnostic = Some(error);
                retired_render_data()
            }
        };
        let commands_locked = !binding_valid
            || !matches!(
                settlement,
                MutationSettlement::Ready | MutationSettlement::Satisfied { .. }
            );
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
                let marker_diameter = input.layout_diameter;
                let input_label_selector =
                    blueprint_port_label_selector(node_id, input.key.0.as_str());
                row = row.child(
                    div()
                        .flex()
                        .items_center()
                        .child(
                            div()
                                .absolute()
                                .left(px(
                                    -(BLUEPRINT_BODY_HORIZONTAL_PADDING + marker_diameter / 2.0)
                                ))
                                .top(px(
                                    (BLUEPRINT_STATIC_PORT_ROW_HEIGHT - marker_diameter) / 2.0
                                ))
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
                let marker_diameter = output.layout_diameter;
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
                                .right(px(
                                    -(BLUEPRINT_BODY_HORIZONTAL_PADDING + marker_diameter / 2.0)
                                ))
                                .top(px(
                                    (BLUEPRINT_STATIC_PORT_ROW_HEIGHT - marker_diameter) / 2.0
                                ))
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
            let ShaderPortElement {
                marker,
                layout_diameter,
                ..
            } = port_element(
                ShaderPort {
                    id: port_id,
                    key: dynamic_pin_key(pin.id()),
                    direction: PortDirection::In,
                    kind: PortKind::Data,
                    label: pin.name().to_owned().into(),
                    handle,
                },
                blueprint_port_selector,
                theme,
            );
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
                                let intent = BlueprintIntent::reorder(pin_id.clone(), index - 1);
                                let _ =
                                    move_node.update(cx, |node, cx| node.apply_intent(intent, cx));
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
                                let intent = BlueprintIntent::reorder(pin_id.clone(), index + 1);
                                let _ =
                                    move_node.update(cx, |node, cx| node.apply_intent(intent, cx));
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
                        let intent =
                            BlueprintIntent::remove(pin_id.clone(), IncidentEdgePolicy::Cascade);
                        let _ = remove_node.update(cx, |node, cx| node.apply_intent(intent, cx));
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
                                        + layout_diameter / 2.0)))
                                    .top(px((BLUEPRINT_PIN_ROW_HEIGHT - layout_diameter) / 2.0))
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

        if matches!(settlement, MutationSettlement::Pending { .. })
            && let Some(intent) = self.mutation.active_intent()
            && !data
                .observation
                .as_ref()
                .is_some_and(|observation| observation.satisfies(intent))
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
                            .child(pending_intent_label(intent)),
                    )
                    .child(div().flex_none().text_xs().child("Pending")),
            );
        }

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
            div()
                .debug_selector(|| BLUEPRINT_RETRY_SELECTOR.to_owned())
                .child(
                    Button::new(
                        format!("jellyflow-blueprint-retry-{}", node_id.0),
                        if matches!(settlement, MutationSettlement::Uncertain { .. }) {
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

        let discard =
            (!binding_valid && self.mutation.unobservable_abandonment().is_some()).then(|| {
                let discard_node = cx.weak_entity();
                div()
                    .debug_selector(|| BLUEPRINT_DISCARD_SELECTOR.to_owned())
                    .child(
                        Button::new(
                            format!("jellyflow-blueprint-discard-{}", node_id.0),
                            "Discard change",
                        )
                        .variant(ButtonVariant::Secondary)
                        .on_click(move |_, window, cx| {
                            let _ =
                                discard_node.update(cx, |node, cx| node.discard_unobservable(cx));
                            window.refresh();
                        }),
                    )
            });

        let diagnostic = self
            .current_diagnostic()
            .map(ToOwned::to_owned)
            .map(|message| {
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
            .children(discard)
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
    NodeKindSpec::builder(BLUEPRINT_CALL_KIND)
        .ports([
            NodePortSpec::new(
                "exec_in",
                PortDirection::In,
                PortKind::Exec,
                PortCapacity::Single,
            ),
            NodePortSpec::new(
                "exec_out",
                PortDirection::Out,
                PortKind::Exec,
                PortCapacity::Multi,
            ),
            NodePortSpec::new(
                "value",
                PortDirection::In,
                PortKind::Data,
                PortCapacity::Single,
            )
            .with_type(TypeDesc::Float),
        ])
        .default_data(blueprint_default_data())
        .build()
}

pub(super) fn register(
    registry: &mut OpenGpuiNodeRegistryBuilder,
    product_theme: NativeProductTheme,
    product_catalog: &NativeProductCatalog,
) {
    let product_presentation = product_catalog
        .get(&NodeKindKey::new(BLUEPRINT_CALL_KIND), 1)
        .expect("Blueprint renderer requires its product presentation")
        .clone();
    let accessible_name = product_presentation.title().to_owned();
    let policy = OpenGpuiNodeRegistrationPolicy::default().with_size_policy(
        OpenGpuiNodeSizePolicy::ContentHeight(
            OpenGpuiContentHeightPolicy::new(BLUEPRINT_MIN_HEIGHT, BLUEPRINT_MAX_HEIGHT)
                .expect("blueprint content-height bounds must be valid"),
        ),
    );
    registry
        .register(
            OpenGpuiNodeRendererDescriptor::new(
                BLUEPRINT_CALL_KIND,
                1,
                RendererStamp::new(BLUEPRINT_CALL_RENDERER, 1),
                OpenGpuiNodeAdapterPresentation::new(accessible_name),
                move |binding, _window, cx| {
                    let (instance, mutation) = {
                        let binding = binding.read(cx);
                        let instance = binding
                            .authoring()
                            .expect(
                                "a Blueprint node factory must receive a live authoring binding",
                            )
                            .instance()
                            .clone();
                        (instance, binding.managed_mutation())
                    };
                    Ok(cx.new(|_| {
                        BlueprintCallNode::new(
                            binding,
                            instance,
                            mutation,
                            product_theme,
                            product_presentation.clone(),
                        )
                    }))
                },
            )
            .with_policy(policy),
        )
        .expect("unique Blueprint renderer descriptor");
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

fn read_blueprint_authoring(
    authoring: &OpenGpuiNodeAuthoring,
    product_presentation: &NativeProductNodePresentation,
) -> Result<BlueprintRenderData, BlueprintError> {
    let observation = blueprint_observation(authoring)?;
    let mut handles = authoring
        .ports()
        .map(|port| (port.id(), port.handle().clone()))
        .collect::<BTreeMap<_, _>>();
    let pins = observation
        .pins()
        .iter()
        .map(|pin| {
            handles
                .remove(&pin.port_id())
                .map(|handle| (pin.clone(), handle))
                .ok_or(BlueprintError::MissingPort {
                    node: authoring.node_id(),
                    port: pin.port_id(),
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let static_ports = observation
        .static_ports()
        .iter()
        .map(|port_id| {
            let facts = observation
                .port(*port_id)
                .expect("validated Blueprint observations retain every static port");
            let handle = handles.remove(port_id).ok_or(BlueprintError::MissingPort {
                node: authoring.node_id(),
                port: *port_id,
            })?;
            let product_port = product_presentation.port(&facts.key);
            let handle = product_port
                .map(|presentation| {
                    handle
                        .clone()
                        .with_side(open_gpui_side(presentation.side()))
                })
                .unwrap_or(handle);
            Ok(ShaderPort {
                id: *port_id,
                key: facts.key.clone(),
                direction: facts.dir,
                kind: facts.kind,
                label: product_port
                    .map(|presentation| presentation.label().into())
                    .unwrap_or_else(|| blueprint_port_label(facts).into()),
                handle,
            })
        })
        .collect::<Result<Vec<_>, BlueprintError>>()?;
    let (static_inputs, outputs) = static_ports
        .into_iter()
        .partition(|port| port.direction == PortDirection::In);
    let title = observation.title().to_owned();
    Ok(BlueprintRenderData {
        observation: Some(observation),
        title,
        static_inputs,
        outputs,
        pins,
    })
}

const fn open_gpui_side(side: HandlePosition) -> OpenGpuiPortSide {
    match side {
        HandlePosition::Top => OpenGpuiPortSide::Top,
        HandlePosition::Right => OpenGpuiPortSide::Right,
        HandlePosition::Bottom => OpenGpuiPortSide::Bottom,
        HandlePosition::Left => OpenGpuiPortSide::Left,
    }
}

fn blueprint_observation(
    authoring: &OpenGpuiNodeAuthoring,
) -> Result<BlueprintObservation, BlueprintError> {
    let ports = authoring
        .ports()
        .map(|port| (port.id(), port.facts().clone()))
        .collect();
    BlueprintObservation::from_parts(
        authoring.version(),
        authoring.node_id(),
        authoring.node(),
        ports,
    )
}

fn retired_render_data() -> BlueprintRenderData {
    BlueprintRenderData {
        observation: None,
        title: "Retired Blueprint node".to_owned(),
        static_inputs: Vec::new(),
        outputs: Vec::new(),
        pins: Vec::new(),
    }
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

fn pending_intent_label(intent: &BlueprintIntent) -> String {
    match intent {
        BlueprintIntent::Add(pin) => format!("Add {}", pin.name()),
        BlueprintIntent::Reorder { pin_id, index } => {
            format!("Move {pin_id} to position {}", index + 1)
        }
        BlueprintIntent::Remove { pin_id, .. } => format!("Remove {pin_id}"),
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
