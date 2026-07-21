use jellyflow_core::types::TypeDesc;
use jellyflow_core::{
    CanvasSize, Node, NodeId, NodeKindKey, PortCapacity, PortDirection, PortId, PortKind,
};
use jellyflow_open_gpui::authoring::{OpenGpuiNodeAuthoring, OpenGpuiNodeBinding};
use jellyflow_open_gpui::{
    OpenGpuiCommandTicketResult, OpenGpuiNodeRegistryBuilder, RetainedField, RetainedFieldAttempt,
    RetainedFieldAttemptState, RetainedFieldConflictChoice, RetainedFieldError,
    RetainedFieldIntent, RetainedFieldKey, RetainedFieldResolution, RetainedFieldSettlement,
    RetainedFieldStore, RetainedFieldUpdate,
};
use jellyflow_runtime::schema::{NodeKindSpec, NodePortSpec};
use jellyflow_runtime::{GraphCommand, GraphRevision};
use open_gpui::{
    App, AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement, Render,
    Styled, Window, div, px, rgb,
};
use open_gpui_ui_components::prelude::Sizable;
use open_gpui_ui_components::{Button, ButtonVariant, NumberInput};

use super::{ShaderNodeError, ShaderPort, ShaderPortElement, port_element};

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
/// [`OpenGpuiSceneHost`]: jellyflow_open_gpui::OpenGpuiSceneHost
pub struct ShaderMultiplyNode {
    binding: Entity<OpenGpuiNodeBinding>,
    factor: RetainedField<f32>,
    preview_expanded: bool,
    diagnostic: Option<String>,
}

impl ShaderMultiplyNode {
    fn new(binding: Entity<OpenGpuiNodeBinding>, factor: RetainedField<f32>) -> Self {
        Self {
            binding,
            factor,
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
        self.factor.field_mut().edit(value);
        if self.factor.field().can_submit()
            && let Err(error) = self.submit_dirty_factor(cx)
        {
            self.diagnostic = Some(error.to_string());
        }
        cx.notify();
    }

    fn accept_remote_factor(&mut self, cx: &mut Context<Self>) {
        let result = read_shader_binding(&self.binding, cx).and_then(|(authoring, remote)| {
            let revision = authoring.revision();
            self.factor.field_mut().observe(revision, remote);
            match self
                .factor
                .field_mut()
                .resolve(RetainedFieldConflictChoice::AcceptRemote, revision)
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
        let (authoring, remote) = read_shader_binding(&self.binding, cx)?;
        self.factor
            .field_mut()
            .observe(authoring.revision(), remote);
        let intent = match self.factor.field_mut().plan(authoring.revision()) {
            Ok(intent) => intent,
            Err(RetainedFieldError::Clean) => return Ok(()),
            Err(error) => return Err(field_error(error)),
        };
        self.enqueue_factor_intent(authoring, intent, cx)
    }

    fn reapply_factor(&mut self, cx: &mut Context<Self>) -> Result<(), ShaderNodeError> {
        let (authoring, remote) = read_shader_binding(&self.binding, cx)?;
        let revision = authoring.revision();
        self.factor.field_mut().observe(revision, remote);
        let intent = match self
            .factor
            .field_mut()
            .resolve(RetainedFieldConflictChoice::ReapplyLocal, revision)
        {
            Ok(RetainedFieldResolution::Reapply(intent)) => intent,
            Ok(_) => return Err(ShaderNodeError::DraftConflict),
            Err(error) => return Err(field_error(error)),
        };
        self.enqueue_factor_intent(authoring, intent, cx)
    }

    fn enqueue_factor_intent(
        &mut self,
        authoring: OpenGpuiNodeAuthoring,
        intent: RetainedFieldIntent<f32>,
        cx: &mut Context<Self>,
    ) -> Result<(), ShaderNodeError> {
        let node_id = authoring.node_id();
        let attempt = intent.attempt();
        let next_data = match shader_data_with_factor(node_id, authoring.node(), *intent.value()) {
            Ok(next_data) => next_data,
            Err(error) => {
                let _ = self.factor.not_dispatched(attempt);
                return Err(error);
            }
        };
        let command = GraphCommand::set_node_data(authoring.revision(), node_id, next_data)
            .with_label("edit shader factor");
        if let Err(error) = self.factor.prepare_task(attempt) {
            let _ = self.factor.not_dispatched(attempt);
            return Err(field_error(error));
        }
        let ticket = match authoring.enqueue(command, cx) {
            Ok(ticket) => ticket,
            Err(error) => {
                let _ = self.factor.not_dispatched(attempt);
                return Err(ShaderNodeError::BindingUnavailable {
                    message: error.to_string(),
                });
            }
        };
        self.diagnostic = None;
        let task = cx.spawn(async move |node, cx| {
            let result = ticket.await;
            let _ = node.update(cx, |node, cx| {
                node.finish_factor_ticket(attempt, result, cx)
            });
        });
        if let Err(error) = self.factor.attach_task(attempt, task) {
            let _ = self.factor.field_mut().mark_uncertain(
                attempt,
                format!("shader factor command was dispatched but its task detached: {error}"),
            );
            return Err(field_error(error));
        }
        Ok(())
    }

    fn finish_factor_ticket(
        &mut self,
        attempt: RetainedFieldAttempt,
        result: OpenGpuiCommandTicketResult,
        cx: &mut Context<Self>,
    ) {
        let settlement = match self.factor.settle_ticket(attempt, result) {
            Ok(settlement) => settlement,
            Err(error) => {
                self.diagnostic = Some(error.to_string());
                cx.notify();
                return;
            }
        };
        if matches!(
            settlement,
            RetainedFieldSettlement::Acknowledged {
                local_changed_after_submission: true,
                ..
            }
        ) {
            self.defer_dirty_submission(cx);
        }
        cx.notify();
    }

    fn observe_factor(&mut self, revision: GraphRevision, value: f32, cx: &mut Context<Self>) {
        let update = self.factor.field_mut().observe(revision, value);
        if matches!(
            update,
            RetainedFieldUpdate::Acknowledged {
                local_changed_after_submission: true,
                ..
            } | RetainedFieldUpdate::UncertainResolved { .. }
        ) && self.factor.field().can_submit()
        {
            self.defer_dirty_submission(cx);
        }
    }

    fn defer_dirty_submission(&self, cx: &mut Context<Self>) {
        let node = cx.weak_entity();
        cx.defer(move |cx| {
            let _ = node.update(cx, |node, cx| {
                if node.factor.field().can_submit()
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
        let drag_handle = self.binding.read(cx).drag_handle();
        let (
            node_id,
            instance_scope,
            graph_revision,
            title,
            factor,
            preview,
            input_a_connected,
            ports,
        ) = {
            let binding = self.binding.read(cx);
            let node_id = self.factor.instance().node();
            let instance_scope = format!("{:?}", self.factor.instance());
            let facts = binding.authoring().map(|authoring| {
                let data = &authoring.node().data;
                let title = data
                    .get("title")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("Multiply")
                    .to_owned();
                let factor = finite_f32(data.get(SHADER_FACTOR_FIELD), 1.0);
                let preview = finite_f32(data.get("preview"), factor);
                let input_a_connected = authoring
                    .ports()
                    .find(|port| port.facts().dir == PortDirection::In && port.facts().key.0 == "a")
                    .is_some_and(|port| port.connection().is_connected());
                let ports = authoring
                    .ports()
                    .map(|port| {
                        let key = port.facts().key.clone();
                        let label = match key.0.as_str() {
                            "a" => "A".into(),
                            "b" => "B".into(),
                            "result" => "Result".into(),
                            _ => key.0.clone().into(),
                        };
                        ShaderPort {
                            id: port.id(),
                            key,
                            direction: port.facts().dir,
                            label,
                            handle: port.handle().clone(),
                        }
                    })
                    .collect();
                (
                    authoring.revision(),
                    title,
                    factor,
                    preview,
                    input_a_connected,
                    ports,
                )
            });
            match facts {
                Ok((revision, title, factor, preview, input_a_connected, ports)) => (
                    node_id,
                    instance_scope,
                    Some(revision),
                    title,
                    factor,
                    preview,
                    input_a_connected,
                    ports,
                ),
                Err(_) => (
                    node_id,
                    instance_scope,
                    None,
                    "Retired shader node".to_owned(),
                    *self.factor.field().local(),
                    1.0,
                    false,
                    Vec::new(),
                ),
            }
        };
        if let Some(revision) = graph_revision {
            self.observe_factor(revision, factor, cx);
        }
        if let Err(error) = self.factor.sync_retention() {
            self.diagnostic = Some(format!("shader factor retention failed: {error}"));
        }
        let inline_factor = *self.factor.field().local();
        let factor_input_disabled = self.factor.field().conflict().is_some();
        let conflict_resolution_disabled = self.factor.field().attempt_state().blocks_submission();

        let mut input_a = None;
        let mut input_b = None;
        let mut output = None;
        for port in ports {
            let ShaderPortElement { key, marker, .. } = port_element(port, shader_port_selector);
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
        .on_click(move |_, _, cx| {
            let _ = node.update(cx, |node, cx| {
                node.preview_expanded = !node.preview_expanded;
                cx.notify();
            });
        });
        let preview_control = div()
            .debug_selector(|| SHADER_PREVIEW_CONTROL_SELECTOR.to_owned())
            .child(preview_button);

        let input_a_label = if input_a_connected {
            let selector = shader_connected_input_selector(node_id, "a");
            div()
                .debug_selector(move || selector)
                .text_sm()
                .text_color(rgb(0xdbe3ec))
                .child("A")
        } else {
            div().text_sm().text_color(rgb(0xdbe3ec)).child("A")
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
            input_a_row = input_a_row
                .child(div().text_sm().text_color(rgb(0xdbe3ec)).child("Result"))
                .child(div().absolute().right(px(-6.0)).top(px(14.0)).child(marker));
        }
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
            .child(div().text_sm().text_color(rgb(0xdbe3ec)).child("B"));

        let status_content = if self.factor.field().conflict().is_some() {
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
                    .rounded(px(4.0))
                    .border_1()
                    .border_color(rgb(0x9d654d))
                    .bg(rgb(0x382a28))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .truncate()
                            .text_xs()
                            .text_color(rgb(0xf0c3aa))
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
                                    .on_click(
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
                                        .on_click(
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
        } else if let Some(message) = self.factor.field().notice().map(str::to_owned) {
            Some(
                div()
                    .debug_selector(|| SHADER_FACTOR_NOTICE_SELECTOR.to_owned())
                    .h_full()
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .rounded(px(4.0))
                    .bg(rgb(0x3a2026))
                    .text_color(rgb(0xffa5ad))
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
                    .rounded(px(4.0))
                    .bg(rgb(0x3a2026))
                    .text_color(rgb(0xffa5ad))
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
            self.factor.field().attempt_state(),
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
                    .rounded(px(4.0))
                    .bg(rgb(0x27364b))
                    .text_xs()
                    .text_color(rgb(0x9fc4ff))
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
                    .border_color(rgb(0x34404a))
                    .text_xs()
                    .text_color(rgb(0x9aa7b5))
                    .child("Output")
                    .child(
                        div()
                            .text_color(rgb(0x7fd5ac))
                            .child(format!("{preview:.2}")),
                    )
                    .into_any_element(),
            )
        } else {
            None
        };

        let status = div().h(px(40.0)).px(px(12.0)).children(status_content);
        let body = div()
            .py(px(4.0))
            .flex()
            .flex_col()
            .child(input_a_row)
            .child(input_b_row)
            .child(status);

        div()
            .debug_selector(|| SHADER_NODE_SELECTOR.to_owned())
            .relative()
            .size_full()
            .rounded(px(6.0))
            .border_1()
            .border_color(rgb(0x3b4450))
            .bg(rgb(0x20262d))
            .text_color(rgb(0xf4f7fb))
            .child(
                drag_handle.wrap(
                    div()
                        .h(px(40.0))
                        .px(px(12.0))
                        .flex()
                        .items_center()
                        .justify_between()
                        .border_b_1()
                        .border_color(rgb(0x3b4450))
                        .bg(rgb(0x252b33))
                        .child(div().text_sm().child(title))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(4.0))
                                .child(
                                    div()
                                        .px(px(6.0))
                                        .py(px(3.0))
                                        .rounded(px(3.0))
                                        .bg(rgb(0x303944))
                                        .text_xs()
                                        .text_color(rgb(0xaab4c0))
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
    NodeKindSpec::builder(SHADER_MULTIPLY_KIND, "Multiply", SHADER_MULTIPLY_RENDERER)
        .category(["Shader", "Math"])
        .keywords(["multiply", "float", "vector"])
        .default_size(SHADER_MULTIPLY_DEFAULT_SIZE)
        .ports([
            NodePortSpec::new("a", PortDirection::In, PortKind::Data, PortCapacity::Single)
                .with_type(TypeDesc::Float)
                .with_label("A"),
            NodePortSpec::new("b", PortDirection::In, PortKind::Data, PortCapacity::Single)
                .with_type(TypeDesc::Float)
                .with_label("B"),
            NodePortSpec::new(
                "result",
                PortDirection::Out,
                PortKind::Data,
                PortCapacity::Multi,
            )
            .with_type(TypeDesc::Float)
            .with_label("Result"),
        ])
        .default_data(serde_json::json!({
            "title": "Multiply",
            "factor": 1.0,
            "preview": 1.0,
        }))
        .build()
}

pub(super) fn register(registry: &mut OpenGpuiNodeRegistryBuilder, field_store: ShaderFieldStore) {
    registry.register(SHADER_MULTIPLY_RENDERER, move |binding, _window, cx| {
        mount_shader_multiply(binding, field_store.clone(), cx)
    });
}

fn mount_shader_multiply(
    binding: Entity<OpenGpuiNodeBinding>,
    field_store: ShaderFieldStore,
    cx: &mut App,
) -> Entity<ShaderMultiplyNode> {
    let initial = binding
        .read(cx)
        .authoring()
        .expect("a shader node factory must receive a live authoring binding");
    let authoritative = shader_factor_from_authoring(&initial).unwrap_or(1.0);
    let key = RetainedFieldKey::new(SHADER_FACTOR_FIELD)
        .expect("shader factor field key is a non-empty constant");
    let factor = RetainedField::mount(
        binding.read(cx),
        key,
        field_store,
        initial.revision(),
        authoritative,
    )
    .expect("shader node retained field must mount while its binding is live");
    cx.new(|_| ShaderMultiplyNode::new(binding, factor))
}

#[must_use]
pub fn shader_port_selector(port: PortId) -> String {
    format!("jellyflow-native-shader-port-{port:?}")
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
) -> Result<(OpenGpuiNodeAuthoring, f32), ShaderNodeError> {
    let authoring =
        binding
            .read(cx)
            .authoring()
            .map_err(|error| ShaderNodeError::BindingUnavailable {
                message: error.to_string(),
            })?;
    let factor = shader_factor_from_authoring(&authoring)?;
    Ok((authoring, factor))
}

fn shader_factor_from_authoring(authoring: &OpenGpuiNodeAuthoring) -> Result<f32, ShaderNodeError> {
    ensure_shader_kind(authoring.node_id(), authoring.node())?;
    Ok(finite_f32(
        authoring.node().data.get(SHADER_FACTOR_FIELD),
        1.0,
    ))
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
    use jellyflow_open_gpui::{
        OpenGpuiCommandTicketError, OpenGpuiNodeRegistry, OpenGpuiSceneHost, RetainedFieldPhase,
        RevisionedField,
    };
    use jellyflow_runtime::schema::NodeCatalog;
    use jellyflow_runtime::{
        GraphCommandOutcome, GraphRuntime, ViewCloseReason, ViewSessionOptions,
    };

    use super::*;

    #[test]
    fn ticket_transport_failures_are_uncertain_and_block_duplicate_submission() {
        let failures = [
            OpenGpuiCommandTicketError::HostDropped,
            OpenGpuiCommandTicketError::HostClosed {
                reason: ViewCloseReason::HostClosed,
            },
            OpenGpuiCommandTicketError::MissingReceipt { operation_index: 7 },
            OpenGpuiCommandTicketError::WrongReceiptKind { operation_index: 8 },
        ];

        for failure in failures {
            let mut field = RevisionedField::new(23, 1.0);
            field.edit(2.0);
            let intent = field.plan(23).expect("dirty factor must produce an intent");
            let settlement = field.settle_ticket(intent.attempt(), Err(failure));
            assert!(matches!(
                settlement,
                RetainedFieldSettlement::Uncertain { .. }
            ));
            assert!(matches!(
                field.attempt_state(),
                RetainedFieldAttemptState::Uncertain { .. }
            ));
            assert!(matches!(
                field.plan(23),
                Err(RetainedFieldError::AttemptPending { .. })
            ));
            assert_eq!(field.local(), &2.0);
            assert!(
                field
                    .notice()
                    .is_some_and(|notice| notice.contains("uncertain"))
            );
        }
    }

    #[test]
    fn fresh_authoritative_snapshot_resolves_uncertain_factor_without_retry() {
        let mut field = RevisionedField::new(23, 1.0);
        field.edit(2.0);
        let intent = field.plan(23).expect("dirty factor intent");
        assert!(matches!(
            field.settle_ticket(
                intent.attempt(),
                Err(OpenGpuiCommandTicketError::HostDropped),
            ),
            RetainedFieldSettlement::Uncertain { .. }
        ));

        assert!(matches!(
            field.observe(24, 2.0),
            RetainedFieldUpdate::UncertainResolved {
                submitted_value_is_authoritative: true,
                ..
            }
        ));
        assert_eq!(field.phase(), RetainedFieldPhase::Clean);
        assert_eq!(field.attempt_state(), RetainedFieldAttemptState::Idle);
        assert!(field.notice().is_none());
    }

    #[test]
    fn snapshot_before_receipt_queues_the_newest_factor_after_acknowledgement() {
        let (mut field, intent) = pending_factor_with_newer_local_value();

        assert!(matches!(
            field.observe(1, 2.0),
            RetainedFieldUpdate::SnapshotFirst { .. }
        ));
        assert!(matches!(
            field.settle_ticket(intent.attempt(), Ok(applied_factor_outcome(2.0))),
            RetainedFieldSettlement::Acknowledged {
                local_changed_after_submission: true,
                ..
            }
        ));

        assert_latest_factor_can_be_submitted(field);
    }

    #[test]
    fn receipt_before_snapshot_queues_the_newest_factor_after_acknowledgement() {
        let (mut field, intent) = pending_factor_with_newer_local_value();

        assert!(matches!(
            field.settle_ticket(intent.attempt(), Ok(applied_factor_outcome(2.0))),
            RetainedFieldSettlement::AwaitingSnapshot { .. }
        ));
        assert!(matches!(
            field.observe(1, 2.0),
            RetainedFieldUpdate::Acknowledged {
                local_changed_after_submission: true,
                ..
            }
        ));

        assert_latest_factor_can_be_submitted(field);
    }

    fn pending_factor_with_newer_local_value() -> (RevisionedField<f32>, RetainedFieldIntent<f32>) {
        let mut field = RevisionedField::new(0, 1.0);
        field.edit(2.0);
        let intent = field.plan(0).expect("dirty factor intent");
        field.edit(3.0);
        (field, intent)
    }

    fn assert_latest_factor_can_be_submitted(mut field: RevisionedField<f32>) {
        assert_eq!(field.local(), &3.0);
        assert_eq!(field.phase(), RetainedFieldPhase::Dirty);
        let intent = field
            .plan(1)
            .expect("acknowledged dirty factor must produce the next intent");
        assert_eq!(intent.value(), &3.0);
    }

    fn applied_factor_outcome(value: f32) -> GraphCommandOutcome {
        let mut catalog = NodeCatalog::new();
        catalog
            .register(catalog_spec())
            .expect("register shader multiply kind");
        let graph = pending_factor_graph(&catalog);
        let runtime = GraphRuntime::builder(catalog)
            .build(graph)
            .expect("valid shader receipt graph");
        let snapshot = runtime.snapshot().expect("initial shader snapshot");
        let node = snapshot
            .graph()
            .node(&pending_factor_node_id())
            .expect("shader receipt node");
        let next_data = shader_data_with_factor(pending_factor_node_id(), node, value)
            .expect("valid shader factor data");
        runtime
            .execute(GraphCommand::set_node_data(
                snapshot.revision(),
                pending_factor_node_id(),
                next_data,
            ))
            .expect("apply shader factor command")
    }

    #[open_gpui::test]
    fn real_host_commits_the_newest_factor_after_a_second_pending_edit(
        cx: &mut open_gpui::TestAppContext,
    ) {
        let mut catalog = NodeCatalog::new();
        catalog
            .register(catalog_spec())
            .expect("register shader multiply kind");
        let graph = pending_factor_graph(&catalog);
        let runtime = GraphRuntime::builder(catalog)
            .build(graph)
            .expect("valid shader factor graph");
        let view = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("attach shader factor view");
        let captured = Rc::new(RefCell::new(None::<Entity<ShaderMultiplyNode>>));
        let captured_for_factory = Rc::clone(&captured);
        let draft_store = ShaderFieldStore::new();
        let mut registry = OpenGpuiNodeRegistry::builder();
        registry.register(SHADER_MULTIPLY_RENDERER, move |binding, _window, cx| {
            let node = mount_shader_multiply(binding, draft_store.clone(), cx);
            captured_for_factory.borrow_mut().replace(node.clone());
            node
        });
        let (_host, cx) = cx.add_window_view(move |_, _| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid shader factor registry"),
                pending_factor_viewport(),
            )
        });
        draw_pending_factor_host(cx);
        let node = captured
            .borrow()
            .clone()
            .expect("mounted shader multiply node");

        cx.update_entity(&node, |node, cx| {
            node.edit_factor(2.0, cx);
            assert!(matches!(
                node.factor.field().attempt_state(),
                RetainedFieldAttemptState::Pending { .. }
            ));
            node.edit_factor(3.0, cx);
            assert_eq!(node.factor.field().local(), &3.0);
            assert_eq!(
                node.factor
                    .field()
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
            assert_eq!(node.factor.field().local(), &3.0);
            assert_eq!(node.factor.field().phase(), RetainedFieldPhase::Clean);
            assert_eq!(
                node.factor.field().attempt_state(),
                RetainedFieldAttemptState::Idle
            );
            assert!(node.factor.field().notice().is_none());
            assert!(node.diagnostic.is_none());
        });

        runtime
            .execute(GraphCommand::undo(2))
            .expect("undo latest factor command")
            .applied()
            .expect("latest factor undo applies");
        draw_pending_factor_host(cx);
        assert_eq!(
            pending_factor_value(runtime.snapshot().unwrap().graph()),
            2.0
        );
        cx.read_entity(&node, |node, _| {
            assert_eq!(node.factor.field().local(), &2.0);
            assert_eq!(node.factor.field().phase(), RetainedFieldPhase::Clean);
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
        let runtime = GraphRuntime::builder(catalog)
            .build(graph)
            .expect("valid shader conflict graph");
        let view = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("attach shader conflict view");
        let captured = Rc::new(RefCell::new(None::<Entity<ShaderMultiplyNode>>));
        let captured_for_factory = Rc::clone(&captured);
        let draft_store = ShaderFieldStore::new();
        let mut registry = OpenGpuiNodeRegistry::builder();
        registry.register(SHADER_MULTIPLY_RENDERER, move |binding, _window, cx| {
            let node = mount_shader_multiply(binding, draft_store.clone(), cx);
            captured_for_factory.borrow_mut().replace(node.clone());
            node
        });
        let (_host, cx) = cx.add_window_view(move |_, _| {
            OpenGpuiSceneHost::new(
                view,
                registry.seal().expect("valid shader conflict registry"),
                pending_factor_viewport(),
            )
        });
        draw_pending_factor_host(cx);
        let node = captured
            .borrow()
            .clone()
            .expect("mounted shader conflict node");

        cx.update_entity(&node, |node, cx| {
            node.factor.field_mut().edit(2.0);
            assert!(matches!(
                node.factor.field_mut().observe(1, 3.0),
                RetainedFieldUpdate::Conflict
            ));
            node.preview_expanded = true;
            cx.notify();
        });
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
                CanvasPoint::default(),
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
            window.draw(cx).clear();
        });
        cx.run_until_parked();
    }
}
