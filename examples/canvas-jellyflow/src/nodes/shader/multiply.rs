use jellyflow_core::types::TypeDesc;
use jellyflow_core::{
    CanvasSize, Node, NodeId, NodeKindKey, PortCapacity, PortDirection, PortId, PortKind,
};
use jellyflow_open_gpui::authoring::{
    OpenGpuiNodeAuthoring, OpenGpuiNodeBinding, OpenGpuiNodeObservation, OpenGpuiPortSide,
};
use jellyflow_open_gpui::registry::{
    OpenGpuiNodeAdapterPresentation, OpenGpuiNodeRegistryBuilder, OpenGpuiNodeRendererDescriptor,
};
#[cfg(test)]
use jellyflow_open_gpui::retained_field::RetainedFieldAttempt;
use jellyflow_open_gpui::retained_field::{
    RetainedField, RetainedFieldAttemptState, RetainedFieldConflictChoice, RetainedFieldError,
    RetainedFieldKey, RetainedFieldResolution, RetainedFieldStore, RetainedFieldSubmitMode,
    RetainedFieldUpdate,
};
use jellyflow_runtime::commands::GraphCommand;
use jellyflow_runtime::geometry::HandlePosition;
use jellyflow_runtime::schema::{NodeKindSpec, NodePortSpec};
use jellyflow_runtime::view::RendererStamp;
use open_gpui::{
    App, AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement, Render,
    Styled, Window, div, px,
};
use open_gpui_ui_components::prelude::Sizable;
use open_gpui_ui_components::{Button, ButtonVariant, NumberInput};

use super::{ShaderNodeError, ShaderPort, ShaderPortElement, port_element};
use crate::product_catalog::{NativeProductCatalog, NativeProductNodePresentation};
use crate::product_theme::NativeProductTheme;

pub const SHADER_MULTIPLY_KIND: &str = "shader.multiply";
pub const SHADER_MULTIPLY_RENDERER: &str = "jellyflow.open-gpui.shader.multiply";
pub const SHADER_MULTIPLY_DEFAULT_SIZE: CanvasSize = CanvasSize {
    width: 260.0,
    height: 160.0,
};
pub const SHADER_NODE_SELECTOR: &str = "jellyflow-native-shader-multiply";
pub const SHADER_PREVIEW_CONTROL_SELECTOR: &str = "jellyflow-native-shader-preview-control";
pub const SHADER_PREVIEW_VALUE_SELECTOR: &str = "jellyflow-native-shader-preview-value";
pub const SHADER_FACTOR_COMMAND_PENDING_SELECTOR: &str =
    "jellyflow-native-shader-factor-command-pending";
pub const SHADER_FACTOR_COMMAND_DIAGNOSTIC_SELECTOR: &str =
    "jellyflow-native-shader-factor-command-diagnostic";
pub const SHADER_FACTOR_NOTICE_SELECTOR: &str = "jellyflow-native-shader-factor-notice";
pub const SHADER_FACTOR_FIELD: &str = "factor";

pub type ShaderFieldStore = RetainedFieldStore<f32>;

/// One retained, framework-native shader node mounted by [`OpenGpuiSceneHost`].
///
/// [`OpenGpuiSceneHost`]: jellyflow_open_gpui::host::OpenGpuiSceneHost
pub struct ShaderMultiplyNode {
    binding: Entity<OpenGpuiNodeBinding>,
    factor: RetainedField<f32>,
    product_theme: NativeProductTheme,
    product_presentation: NativeProductNodePresentation,
    preview_expanded: bool,
    diagnostic: Option<String>,
}

impl ShaderMultiplyNode {
    fn new(
        binding: Entity<OpenGpuiNodeBinding>,
        factor: RetainedField<f32>,
        product_theme: NativeProductTheme,
        product_presentation: NativeProductNodePresentation,
    ) -> Self {
        Self {
            binding,
            factor,
            product_theme,
            product_presentation,
            preview_expanded: false,
            diagnostic: None,
        }
    }

    fn edit_factor(&mut self, value: f32, cx: &mut Context<Self>) {
        if !value.is_finite() {
            self.diagnostic = Some("shader factor must be finite".to_owned());
            cx.notify();
            return;
        }
        if let Err(error) = self.factor.edit(value) {
            self.diagnostic = Some(error.to_string());
            cx.notify();
            return;
        }
        if self.factor.can_submit()
            && let Err(error) = self.submit_dirty_factor(cx)
        {
            self.diagnostic = Some(error.to_string());
        }
        cx.notify();
    }

    fn accept_remote_factor(&mut self, cx: &mut Context<Self>) {
        let result = read_shader_binding(&self.binding, cx).and_then(|(authoring, observation)| {
            self.factor.observe(observation, cx).map_err(field_error)?;
            match self
                .factor
                .resolve(&authoring, RetainedFieldConflictChoice::AcceptRemote, cx)
            {
                Ok(RetainedFieldResolution::AcceptedRemote { .. }) => Ok(()),
                Ok(_) => Err(ShaderNodeError::DraftConflict),
                Err(error) => Err(field_error(error)),
            }
        });
        self.diagnostic = result.err().map(|error| error.to_string());
        cx.notify();
    }

    fn reapply_local_factor(&mut self, cx: &mut Context<Self>) {
        let result = self.reapply_factor(cx);
        self.diagnostic = result.err().map(|error| error.to_string());
        cx.notify();
    }

    fn submit_dirty_factor(&mut self, cx: &mut Context<Self>) -> Result<(), ShaderNodeError> {
        let (authoring, observation) = read_shader_binding(&self.binding, cx)?;
        self.factor.observe(observation, cx).map_err(field_error)?;
        let plan = match self
            .factor
            .plan(authoring, RetainedFieldSubmitMode::Commit, cx)
        {
            Ok(plan) => plan,
            Err(RetainedFieldError::Clean) => return Ok(()),
            Err(error) => return Err(field_error(error)),
        };
        Self::dispatch_factor_plan(plan, cx)
    }

    fn reapply_factor(&mut self, cx: &mut Context<Self>) -> Result<(), ShaderNodeError> {
        let (authoring, observation) = read_shader_binding(&self.binding, cx)?;
        self.factor.observe(observation, cx).map_err(field_error)?;
        let plan = self
            .factor
            .plan(authoring, RetainedFieldSubmitMode::Reapply, cx)
            .map_err(field_error)?;
        Self::dispatch_factor_plan(plan, cx)
    }

    fn dispatch_factor_plan(
        plan: jellyflow_open_gpui::retained_field::RetainedFieldDispatchPlan<'_, f32>,
        cx: &mut Context<Self>,
    ) -> Result<(), ShaderNodeError> {
        let node_id = plan.authoring().node_id();
        let version = plan.authoring().version();
        let next_data = shader_data_with_factor(node_id, plan.authoring().node(), *plan.value())?;
        let command = GraphCommand::set_node_data(version, node_id, next_data)
            .with_label("edit shader factor");
        plan.dispatch(command, cx).map_err(field_error)
    }

    fn observe_factor(
        &mut self,
        observation: OpenGpuiNodeObservation<f32>,
        cx: &mut Context<Self>,
    ) {
        let update = match self.factor.observe(observation, cx) {
            Ok(update) => update,
            Err(error) => {
                self.diagnostic = Some(error.to_string());
                return;
            }
        };
        if matches!(
            update,
            RetainedFieldUpdate::Acknowledged {
                local_changed_after_submission: true,
                ..
            } | RetainedFieldUpdate::UncertainResolved { .. }
        ) && self.factor.can_submit()
        {
            self.defer_dirty_submission(cx);
        }
    }

    fn defer_dirty_submission(&self, cx: &mut Context<Self>) {
        let node = cx.weak_entity();
        cx.defer(move |cx| {
            let _ = node.update(cx, |node, cx| {
                if node.factor.can_submit()
                    && let Err(error) = node.submit_dirty_factor(cx)
                {
                    node.diagnostic = Some(error.to_string());
                }
                cx.notify();
            });
        });
    }
}

fn field_error(error: RetainedFieldError) -> ShaderNodeError {
    match error {
        RetainedFieldError::ConflictRequiresResolution | RetainedFieldError::NoConflict => {
            ShaderNodeError::DraftConflict
        }
        error => ShaderNodeError::CommandRejected {
            message: error.to_string(),
        },
    }
}

impl Render for ShaderMultiplyNode {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        let theme = self.product_theme;
        let product_presentation = self.product_presentation.clone();
        let input_a_text = product_presentation
            .port(&jellyflow_core::PortKey::new("a"))
            .expect("validated Multiply product presentation contains port a")
            .label()
            .to_owned();
        let input_b_text = product_presentation
            .port(&jellyflow_core::PortKey::new("b"))
            .expect("validated Multiply product presentation contains port b")
            .label()
            .to_owned();
        let result_text = product_presentation
            .port(&jellyflow_core::PortKey::new("result"))
            .expect("validated Multiply product presentation contains result")
            .label()
            .to_owned();
        let drag_handle = self.binding.read(cx).drag_handle();
        let (node_id, instance_scope, factor_observation, title, preview, input_a_connected, ports) = {
            let binding = self.binding.read(cx);
            let node_id = self.factor.instance().node();
            let instance_scope = format!("{:?}", self.factor.instance());
            let facts = binding.authoring().map(|authoring| {
                let data = &authoring.node().data;
                let title = data
                    .get("title")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or(product_presentation.title())
                    .to_owned();
                let factor = finite_f32(data.get(SHADER_FACTOR_FIELD), 1.0);
                let factor_observation = authoring
                    .observe_field(|node| finite_f32(node.data.get(SHADER_FACTOR_FIELD), 1.0));
                let preview = finite_f32(data.get("preview"), factor);
                let input_a_connected = authoring
                    .ports()
                    .find(|port| port.facts().dir == PortDirection::In && port.facts().key.0 == "a")
                    .is_some_and(|port| port.connection().is_connected());
                let ports = authoring
                    .ports()
                    .map(|port| {
                        let key = port.facts().key.clone();
                        let product_port = product_presentation.port(&key);
                        let label = product_port
                            .map(|presentation| presentation.label().into())
                            .unwrap_or_else(|| key.0.clone().into());
                        let handle = product_port
                            .map(|presentation| {
                                port.handle()
                                    .clone()
                                    .with_side(open_gpui_side(presentation.side()))
                            })
                            .unwrap_or_else(|| port.handle().clone());
                        ShaderPort {
                            id: port.id(),
                            key,
                            direction: port.facts().dir,
                            kind: port.facts().kind,
                            label,
                            handle,
                        }
                    })
                    .collect();
                (factor_observation, title, preview, input_a_connected, ports)
            });
            match facts {
                Ok((observation, title, preview, input_a_connected, ports)) => (
                    node_id,
                    instance_scope,
                    Some(observation),
                    title,
                    preview,
                    input_a_connected,
                    ports,
                ),
                Err(_) => (
                    node_id,
                    instance_scope,
                    None,
                    "Retired shader node".to_owned(),
                    1.0,
                    false,
                    Vec::new(),
                ),
            }
        };
        if let Some(observation) = factor_observation {
            self.observe_factor(observation, cx);
        }
        let inline_factor = *self.factor.local();
        let factor_input_disabled = self.factor.conflict().is_some();
        let conflict_resolution_disabled = self.factor.attempt_state().blocks_submission();

        let mut input_a = None;
        let mut input_b = None;
        let mut output = None;
        for port in ports {
            let ShaderPortElement { key, marker, .. } =
                port_element(port, shader_port_selector, theme);
            match key.0.as_str() {
                "a" => input_a = Some(marker),
                "b" => input_b = Some(marker),
                "result" => output = Some(marker),
                _ => {}
            }
        }

        let node = cx.weak_entity();
        let preview_button = Button::new(
            format!("jellyflow-shader-preview-{instance_scope}"),
            "Preview",
        )
        .variant(ButtonVariant::Ghost)
        .xsmall()
        .selected(self.preview_expanded)
        .on_activate(move |_, _, cx| {
            let _ = node.update(cx, |node, cx| {
                node.preview_expanded = !node.preview_expanded;
                cx.notify();
            });
        });
        let preview_control = div()
            .debug_selector(|| SHADER_PREVIEW_CONTROL_SELECTOR.to_owned())
            .child(preview_button);

        let input_a_label_selector = shader_port_label_selector(node_id, "a");
        let input_a_label = if input_a_connected {
            let selector = shader_connected_input_selector(node_id, "a");
            div().debug_selector(move || selector).child(
                div()
                    .debug_selector(move || input_a_label_selector.clone())
                    .text_sm()
                    .text_color(theme.text.secondary)
                    .child(input_a_text.clone()),
            )
        } else {
            div()
                .debug_selector(move || input_a_label_selector.clone())
                .text_sm()
                .text_color(theme.text.secondary)
                .child(input_a_text)
        };
        let factor_editor = (!input_a_connected).then(|| {
            let editor_selector = shader_inline_editor_selector(node_id, "a");
            let value_selector = shader_inline_value_selector(node_id, "a", inline_factor);
            let inline_node = cx.weak_entity();
            div()
                .debug_selector(move || editor_selector)
                .w(px(92.0))
                .child(
                    div().debug_selector(move || value_selector).child(
                        NumberInput::new(
                            format!("jellyflow-shader-factor-{instance_scope}"),
                            "Factor",
                        )
                        .value(inline_factor)
                        .min(-16.0)
                        .max(16.0)
                        .step(0.05)
                        .disabled(factor_input_disabled)
                        .xsmall()
                        .on_change(move |change, _, cx| {
                            let _ = inline_node.update(cx, |node, cx| {
                                node.edit_factor(change.value(), cx);
                            });
                        }),
                    ),
                )
        });

        let mut input_a_row = div()
            .relative()
            .h(px(40.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .children(
                input_a.map(|marker| div().absolute().left(px(-6.0)).top(px(14.0)).child(marker)),
            )
            .child(input_a_label)
            .child(div().flex_1())
            .children(factor_editor);
        if let Some(marker) = output {
            let output_label_selector = shader_port_label_selector(node_id, "result");
            input_a_row = input_a_row
                .child(
                    div()
                        .debug_selector(move || output_label_selector.clone())
                        .text_sm()
                        .text_color(theme.text.secondary)
                        .child(result_text),
                )
                .child(div().absolute().right(px(-6.0)).top(px(14.0)).child(marker));
        }
        let input_b_label_selector = shader_port_label_selector(node_id, "b");
        let input_b_row = div()
            .relative()
            .h(px(32.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .children(
                input_b.map(|marker| div().absolute().left(px(-6.0)).top(px(10.0)).child(marker)),
            )
            .child(
                div()
                    .debug_selector(move || input_b_label_selector.clone())
                    .text_sm()
                    .text_color(theme.text.secondary)
                    .child(input_b_text),
            );

        let status_content = if self.factor.conflict().is_some() {
            let accept_node = cx.weak_entity();
            let reapply_node = cx.weak_entity();
            let accept_selector = shader_factor_accept_remote_selector(node_id);
            let reapply_selector = shader_factor_reapply_selector(node_id);
            Some(
                div()
                    .debug_selector(move || shader_factor_conflict_selector(node_id))
                    .h_full()
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .rounded(px(theme.radius.control))
                    .border_1()
                    .border_color(theme.status.conflict.border)
                    .bg(theme.status.conflict.surface)
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .truncate()
                            .text_xs()
                            .text_color(theme.status.conflict.text)
                            .child("Factor conflict"),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(2.0))
                            .child(
                                div().debug_selector(move || accept_selector.clone()).child(
                                    Button::new(
                                        format!("jellyflow-shader-factor-accept-{instance_scope}"),
                                        "Use remote",
                                    )
                                    .variant(ButtonVariant::Ghost)
                                    .xsmall()
                                    .disabled(conflict_resolution_disabled)
                                    .on_activate(
                                        move |_, window, cx| {
                                            let _ = accept_node.update(cx, |node, cx| {
                                                node.accept_remote_factor(cx)
                                            });
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
                                            format!(
                                                "jellyflow-shader-factor-reapply-{instance_scope}"
                                            ),
                                            "Reapply",
                                        )
                                        .variant(ButtonVariant::Ghost)
                                        .xsmall()
                                        .disabled(conflict_resolution_disabled)
                                        .on_activate(
                                            move |_, window, cx| {
                                                let _ = reapply_node.update(cx, |node, cx| {
                                                    node.reapply_local_factor(cx)
                                                });
                                                window.refresh();
                                            },
                                        ),
                                    ),
                            ),
                    )
                    .into_any_element(),
            )
        } else if let Some(message) = self.factor.notice().map(str::to_owned) {
            Some(
                div()
                    .debug_selector(|| SHADER_FACTOR_NOTICE_SELECTOR.to_owned())
                    .h_full()
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .rounded(px(theme.radius.control))
                    .bg(theme.status.danger.surface)
                    .text_color(theme.status.danger.text)
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .truncate()
                            .text_xs()
                            .child(message),
                    )
                    .into_any_element(),
            )
        } else if let Some(message) = self.diagnostic.clone() {
            Some(
                div()
                    .debug_selector(|| SHADER_FACTOR_COMMAND_DIAGNOSTIC_SELECTOR.to_owned())
                    .h_full()
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .rounded(px(theme.radius.control))
                    .bg(theme.status.danger.surface)
                    .text_color(theme.status.danger.text)
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .truncate()
                            .text_xs()
                            .child(message),
                    )
                    .into_any_element(),
            )
        } else if matches!(
            self.factor.attempt_state(),
            RetainedFieldAttemptState::Pending { .. }
                | RetainedFieldAttemptState::SnapshotFirst { .. }
                | RetainedFieldAttemptState::ReceiptFirst { .. }
        ) {
            Some(
                div()
                    .debug_selector(|| SHADER_FACTOR_COMMAND_PENDING_SELECTOR.to_owned())
                    .h_full()
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .rounded(px(theme.radius.control))
                    .bg(theme.status.info.surface)
                    .text_xs()
                    .text_color(theme.status.info.text)
                    .child("Saving factor...")
                    .into_any_element(),
            )
        } else if self.preview_expanded {
            Some(
                div()
                    .debug_selector(|| SHADER_PREVIEW_VALUE_SELECTOR.to_owned())
                    .h_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_t_1()
                    .border_color(theme.border.subtle)
                    .text_xs()
                    .text_color(theme.text.muted)
                    .child("Output")
                    .child(
                        div()
                            .text_color(theme.status.success.text)
                            .child(format!("{preview:.2}")),
                    )
                    .into_any_element(),
            )
        } else {
            None
        };

        let status = div().h(px(40.0)).px(px(12.0)).children(status_content);
        let body_selector = shader_body_selector(node_id);
        let body = div()
            .debug_selector(move || body_selector.clone())
            .py(px(3.0))
            .flex()
            .flex_col()
            .child(input_a_row)
            .child(input_b_row)
            .child(status);

        let header_title_selector = shader_header_title_selector(node_id);
        let header_metadata_selector = shader_header_metadata_selector(node_id);
        div()
            .debug_selector(|| SHADER_NODE_SELECTOR.to_owned())
            .relative()
            .size_full()
            .rounded(px(theme.radius.node))
            .border_1()
            .border_color(theme.node.border)
            .bg(theme.node.surface)
            .text_color(theme.text.primary)
            .child(
                drag_handle.wrap(
                    div()
                        .h(px(40.0))
                        .px(px(12.0))
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(8.0))
                        .border_b_1()
                        .border_color(theme.header.border)
                        .bg(theme.header.surface)
                        .child(
                            div()
                                .debug_selector(move || header_title_selector.clone())
                                .min_w(px(0.0))
                                .flex_1()
                                .truncate()
                                .text_sm()
                                .child(title),
                        )
                        .child(
                            div()
                                .debug_selector(move || header_metadata_selector.clone())
                                .flex_none()
                                .flex()
                                .items_center()
                                .gap(px(4.0))
                                .child(
                                    div()
                                        .px(px(6.0))
                                        .py(px(3.0))
                                        .rounded(px(theme.radius.control))
                                        .bg(theme.header.badge_surface)
                                        .text_xs()
                                        .text_color(theme.header.badge_text)
                                        .child("FLOAT"),
                                )
                                .child(preview_control),
                        ),
                ),
            )
            .child(body)
    }
}

pub(super) fn catalog_spec() -> NodeKindSpec {
    NodeKindSpec::builder(SHADER_MULTIPLY_KIND)
        .ports([
            NodePortSpec::new("a", PortDirection::In, PortKind::Data, PortCapacity::Single)
                .with_type(TypeDesc::Float),
            NodePortSpec::new("b", PortDirection::In, PortKind::Data, PortCapacity::Single)
                .with_type(TypeDesc::Float),
            NodePortSpec::new(
                "result",
                PortDirection::Out,
                PortKind::Data,
                PortCapacity::Multi,
            )
            .with_type(TypeDesc::Float),
        ])
        .default_data(serde_json::json!({
            "title": "Multiply",
            "factor": 1.0,
            "preview": 1.0,
        }))
        .build()
}

pub(super) fn register(
    registry: &mut OpenGpuiNodeRegistryBuilder,
    field_store: ShaderFieldStore,
    product_theme: NativeProductTheme,
    product_catalog: &NativeProductCatalog,
) {
    let product_presentation = product_catalog
        .get(&NodeKindKey::new(SHADER_MULTIPLY_KIND), 1)
        .expect("Multiply renderer requires its product presentation")
        .clone();
    let accessible_name = product_presentation.title().to_owned();
    registry
        .register(OpenGpuiNodeRendererDescriptor::new(
            SHADER_MULTIPLY_KIND,
            1,
            RendererStamp::new(SHADER_MULTIPLY_RENDERER, 1),
            OpenGpuiNodeAdapterPresentation::new(accessible_name),
            move |binding, _window, cx| {
                Ok(mount_shader_multiply(
                    binding,
                    field_store.clone(),
                    product_theme,
                    product_presentation.clone(),
                    cx,
                ))
            },
        ))
        .expect("unique shader multiply renderer descriptor");
}

fn mount_shader_multiply(
    binding: Entity<OpenGpuiNodeBinding>,
    field_store: ShaderFieldStore,
    product_theme: NativeProductTheme,
    product_presentation: NativeProductNodePresentation,
    cx: &mut App,
) -> Entity<ShaderMultiplyNode> {
    let initial = binding
        .read(cx)
        .authoring()
        .expect("a shader node factory must receive a live authoring binding");
    let observation = initial
        .try_observe_field(|node| shader_factor_from_node(initial.node_id(), node))
        .unwrap_or_else(|_| initial.observe_field(|_| 1.0));
    let key = RetainedFieldKey::new(SHADER_FACTOR_FIELD)
        .expect("shader factor field key is a non-empty constant");
    let factor = RetainedField::mount(observation, key, field_store)
        .expect("shader node retained field must mount while its binding is live");
    cx.new(|_| ShaderMultiplyNode::new(binding, factor, product_theme, product_presentation))
}

const fn open_gpui_side(side: HandlePosition) -> OpenGpuiPortSide {
    match side {
        HandlePosition::Top => OpenGpuiPortSide::Top,
        HandlePosition::Right => OpenGpuiPortSide::Right,
        HandlePosition::Bottom => OpenGpuiPortSide::Bottom,
        HandlePosition::Left => OpenGpuiPortSide::Left,
    }
}

#[must_use]
pub fn shader_port_selector(port: PortId) -> String {
    format!("jellyflow-native-shader-port-{port:?}")
}

#[must_use]
pub fn shader_header_title_selector(node: NodeId) -> String {
    format!("jellyflow-native-shader-header-title-{}", node.0)
}

#[must_use]
pub fn shader_header_metadata_selector(node: NodeId) -> String {
    format!("jellyflow-native-shader-header-metadata-{}", node.0)
}

#[must_use]
pub fn shader_body_selector(node: NodeId) -> String {
    format!("jellyflow-native-shader-body-{}", node.0)
}

#[must_use]
pub fn shader_port_label_selector(node: NodeId, port_key: &str) -> String {
    format!("jellyflow-native-shader-port-label-{}-{port_key}", node.0)
}

#[must_use]
pub fn shader_inline_editor_selector(node: NodeId, input_key: &str) -> String {
    format!("jellyflow-native-shader-inline-{node:?}-{input_key}")
}

#[must_use]
pub fn shader_inline_value_selector(node: NodeId, input_key: &str, value: f32) -> String {
    format!("jellyflow-native-shader-inline-value-{node:?}-{input_key}-{value:.2}")
}

#[must_use]
pub fn shader_connected_input_selector(node: NodeId, input_key: &str) -> String {
    format!("jellyflow-native-shader-connected-{node:?}-{input_key}")
}

#[must_use]
pub fn shader_factor_conflict_selector(node: NodeId) -> String {
    format!("jellyflow-native-shader-factor-conflict-{node:?}")
}

#[must_use]
pub fn shader_factor_accept_remote_selector(node: NodeId) -> String {
    format!("jellyflow-native-shader-factor-accept-{node:?}")
}

#[must_use]
pub fn shader_factor_reapply_selector(node: NodeId) -> String {
    format!("jellyflow-native-shader-factor-reapply-{node:?}")
}

fn read_shader_binding(
    binding: &Entity<OpenGpuiNodeBinding>,
    cx: &mut App,
) -> Result<(OpenGpuiNodeAuthoring, OpenGpuiNodeObservation<f32>), ShaderNodeError> {
    let authoring =
        binding
            .read(cx)
            .authoring()
            .map_err(|error| ShaderNodeError::BindingUnavailable {
                message: error.to_string(),
            })?;
    let observation =
        authoring.try_observe_field(|node| shader_factor_from_node(authoring.node_id(), node))?;
    Ok((authoring, observation))
}

fn shader_factor_from_node(node_id: NodeId, node: &Node) -> Result<f32, ShaderNodeError> {
    ensure_shader_kind(node_id, node)?;
    Ok(finite_f32(node.data.get(SHADER_FACTOR_FIELD), 1.0))
}

fn shader_data_with_factor(
    node_id: NodeId,
    node: &Node,
    factor: f32,
) -> Result<serde_json::Value, ShaderNodeError> {
    ensure_shader_kind(node_id, node)?;
    let mut data = node.data.clone();
    let object = data.as_object_mut().ok_or(ShaderNodeError::InvalidData {
        node: node_id,
        field: "object",
    })?;
    object.insert(SHADER_FACTOR_FIELD.to_owned(), serde_json::json!(factor));
    Ok(data)
}

fn ensure_shader_kind(node_id: NodeId, node: &Node) -> Result<(), ShaderNodeError> {
    if node.kind != NodeKindKey::new(SHADER_MULTIPLY_KIND) {
        return Err(ShaderNodeError::WrongNodeKind {
            node: node_id,
            expected: SHADER_MULTIPLY_KIND,
            actual: node.kind.0.clone(),
        });
    }
    Ok(())
}

fn finite_f32(value: Option<&serde_json::Value>, fallback: f32) -> f32 {
    value
        .and_then(serde_json::Value::as_f64)
        .map(|value| value as f32)
        .filter(|value| value.is_finite())
        .unwrap_or(fallback)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use jellyflow_core::{CanvasPoint, CanvasRect, GraphBuilder, GraphId};
    use jellyflow_open_gpui::host::OpenGpuiSceneHost;
    use jellyflow_open_gpui::registry::OpenGpuiNodeRegistry;
    use jellyflow_open_gpui::retained_field::RetainedFieldPhase;
    use jellyflow_runtime::graph::GraphRuntime;
    use jellyflow_runtime::schema::NodeCatalog;
    use jellyflow_runtime::view::ViewSessionOptions;

    use super::*;

    #[open_gpui::test]
    fn real_host_commits_the_newest_factor_after_a_second_pending_edit(
        cx: &mut open_gpui::TestAppContext,
    ) {
        let mut catalog = NodeCatalog::new();
        catalog
            .register(catalog_spec())
            .expect("register shader multiply kind");
        let graph = pending_factor_graph(&catalog);
        let runtime = GraphRuntime::builder(catalog.clone())
            .build(graph)
            .expect("valid shader factor graph");
        let view = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("attach shader factor view");
        let captured = Rc::new(RefCell::new(None::<Entity<ShaderMultiplyNode>>));
        let captured_for_factory = Rc::clone(&captured);
        let draft_store = ShaderFieldStore::new();
        let product_presentation = crate::product_catalog::native_product_catalog()
            .expect("native product catalog")
            .get(&NodeKindKey::new(SHADER_MULTIPLY_KIND), 1)
            .expect("Multiply product presentation")
            .clone();
        let mut registry = OpenGpuiNodeRegistry::builder();
        registry
            .register(OpenGpuiNodeRendererDescriptor::new(
                SHADER_MULTIPLY_KIND,
                1,
                RendererStamp::new(SHADER_MULTIPLY_RENDERER, 1),
                OpenGpuiNodeAdapterPresentation::new("Multiply"),
                move |binding, _window, cx| {
                    let node = mount_shader_multiply(
                        binding,
                        draft_store.clone(),
                        NativeProductTheme::shader_blueprint(),
                        product_presentation.clone(),
                        cx,
                    );
                    captured_for_factory.borrow_mut().replace(node.clone());
                    Ok(node)
                },
            ))
            .expect("unique shader factor renderer descriptor");
        let (_host, cx) = cx.add_window_view(move |_, _| {
            OpenGpuiSceneHost::new(
                view,
                registry
                    .seal(&catalog)
                    .expect("valid shader factor registry"),
                pending_factor_viewport(),
            )
            .expect("valid Open GPUI scene host")
        });
        draw_pending_factor_host(cx);
        let node = captured
            .borrow()
            .clone()
            .expect("mounted shader multiply node");

        cx.update_entity(&node, |node, cx| {
            node.edit_factor(2.0, cx);
            assert!(matches!(
                node.factor.attempt_state(),
                RetainedFieldAttemptState::Pending { .. }
            ));
            node.edit_factor(3.0, cx);
            assert_eq!(node.factor.local(), &3.0);
            assert_eq!(
                node.factor
                    .attempt_state()
                    .attempt()
                    .map(RetainedFieldAttempt::get),
                Some(1)
            );
        });
        for _ in 0..4 {
            draw_pending_factor_host(cx);
        }

        let snapshot = runtime.snapshot().expect("latest shader factor snapshot");
        assert_eq!(snapshot.revision(), 2);
        assert_eq!(pending_factor_value(snapshot.graph()), 3.0);
        assert_eq!(runtime.history_state().unwrap().undo_len(), 2);
        cx.read_entity(&node, |node, _| {
            assert_eq!(node.factor.local(), &3.0);
            assert_eq!(node.factor.phase(), RetainedFieldPhase::Clean);
            assert_eq!(node.factor.attempt_state(), RetainedFieldAttemptState::Idle);
            assert!(node.factor.notice().is_none());
            assert!(node.diagnostic.is_none());
        });

        runtime
            .execute(GraphCommand::undo(snapshot.version()))
            .applied_revision()
            .expect("latest factor undo applies");
        draw_pending_factor_host(cx);
        assert_eq!(
            pending_factor_value(runtime.snapshot().unwrap().graph()),
            2.0
        );
        cx.read_entity(&node, |node, _| {
            assert_eq!(node.factor.local(), &2.0);
            assert_eq!(node.factor.phase(), RetainedFieldPhase::Clean);
        });
    }

    #[open_gpui::test]
    fn conflict_status_replaces_preview_and_stays_inside_fixed_node_bounds(
        cx: &mut open_gpui::TestAppContext,
    ) {
        let mut catalog = NodeCatalog::new();
        catalog
            .register(catalog_spec())
            .expect("register shader multiply kind");
        let graph = pending_factor_graph(&catalog);
        let runtime = GraphRuntime::builder(catalog.clone())
            .build(graph)
            .expect("valid shader conflict graph");
        let view = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("attach shader conflict view");
        let captured = Rc::new(RefCell::new(None::<Entity<ShaderMultiplyNode>>));
        let captured_for_factory = Rc::clone(&captured);
        let draft_store = ShaderFieldStore::new();
        let product_presentation = crate::product_catalog::native_product_catalog()
            .expect("native product catalog")
            .get(&NodeKindKey::new(SHADER_MULTIPLY_KIND), 1)
            .expect("Multiply product presentation")
            .clone();
        let mut registry = OpenGpuiNodeRegistry::builder();
        registry
            .register(OpenGpuiNodeRendererDescriptor::new(
                SHADER_MULTIPLY_KIND,
                1,
                RendererStamp::new(SHADER_MULTIPLY_RENDERER, 1),
                OpenGpuiNodeAdapterPresentation::new("Multiply"),
                move |binding, _window, cx| {
                    let node = mount_shader_multiply(
                        binding,
                        draft_store.clone(),
                        NativeProductTheme::shader_blueprint(),
                        product_presentation.clone(),
                        cx,
                    );
                    captured_for_factory.borrow_mut().replace(node.clone());
                    Ok(node)
                },
            ))
            .expect("unique shader conflict renderer descriptor");
        let (_host, cx) = cx.add_window_view(move |_, _| {
            OpenGpuiSceneHost::new(
                view,
                registry
                    .seal(&catalog)
                    .expect("valid shader conflict registry"),
                pending_factor_viewport(),
            )
            .expect("valid Open GPUI scene host")
        });
        draw_pending_factor_host(cx);
        let node = captured
            .borrow()
            .clone()
            .expect("mounted shader conflict node");

        cx.update_entity(&node, |node, cx| {
            node.factor.edit(2.0).expect("retain local factor edit");
            node.preview_expanded = true;
            cx.notify();
        });
        let snapshot = runtime.snapshot().expect("shader conflict snapshot");
        let remote = shader_data_with_factor(
            pending_factor_node_id(),
            snapshot
                .graph()
                .node(&pending_factor_node_id())
                .expect("shader conflict node"),
            3.0,
        )
        .expect("valid remote shader factor");
        runtime
            .execute(GraphCommand::set_node_data(
                snapshot.version(),
                pending_factor_node_id(),
                remote,
            ))
            .applied_revision()
            .expect("apply remote shader factor");
        draw_pending_factor_host(cx);

        let node_bounds = cx
            .debug_bounds(SHADER_NODE_SELECTOR)
            .expect("fixed shader node bounds");
        let conflict_bounds = cx
            .debug_bounds(&shader_factor_conflict_selector(pending_factor_node_id()))
            .expect("shader conflict status");
        let accept_bounds = cx
            .debug_bounds(&shader_factor_accept_remote_selector(
                pending_factor_node_id(),
            ))
            .expect("shader accept-remote action");
        let reapply_bounds = cx
            .debug_bounds(&shader_factor_reapply_selector(pending_factor_node_id()))
            .expect("shader reapply action");
        assert!(conflict_bounds.is_contained_within(&node_bounds));
        assert!(accept_bounds.is_contained_within(&node_bounds));
        assert!(reapply_bounds.is_contained_within(&node_bounds));
        assert!(cx.debug_bounds(SHADER_PREVIEW_VALUE_SELECTOR).is_none());
    }

    fn pending_factor_graph(catalog: &NodeCatalog) -> jellyflow_core::Graph {
        let mut graph = GraphBuilder::new(GraphId::from_u128(0x5ade_0000));
        let creation = catalog
            .instantiate_node_with_ids(
                &NodeKindKey::new(SHADER_MULTIPLY_KIND),
                pending_factor_node_id(),
                CanvasRect {
                    origin: CanvasPoint::default(),
                    size: SHADER_MULTIPLY_DEFAULT_SIZE,
                },
                [
                    PortId::from_u128(0x5ade_1101),
                    PortId::from_u128(0x5ade_1102),
                    PortId::from_u128(0x5ade_1103),
                ],
            )
            .expect("instantiate shader multiply node");
        let (node_id, node, ports) = creation.into_parts();
        graph.insert_node(node_id, node);
        for (port_id, port) in ports {
            graph.insert_port(port_id, port);
        }
        graph.build().expect("shader factor graph")
    }

    fn pending_factor_node_id() -> NodeId {
        NodeId::from_u128(0x5ade_1000)
    }

    fn pending_factor_value(graph: &jellyflow_core::Graph) -> f32 {
        graph.node(&pending_factor_node_id()).unwrap().data[SHADER_FACTOR_FIELD]
            .as_f64()
            .expect("numeric shader factor") as f32
    }

    fn pending_factor_viewport() -> CanvasRect {
        CanvasRect {
            origin: CanvasPoint { x: -10.0, y: -10.0 },
            size: CanvasSize {
                width: 320.0,
                height: 240.0,
            },
        }
    }

    fn draw_pending_factor_host(cx: &mut open_gpui::VisualTestContext) {
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        cx.run_until_parked();
    }
}
