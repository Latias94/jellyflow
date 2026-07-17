use std::collections::{BTreeMap, BTreeSet};

use jellyflow::core::types::TypeDesc;
use jellyflow::core::{
    CanvasSize, Node, NodeId, NodeKindKey, Port, PortCapacity, PortDirection, PortId, PortKey,
    PortKind,
};
use jellyflow::runtime::schema::{NodeKindSpec, NodePortSpec};
use jellyflow::runtime::{
    GraphCommand, GraphCommandOutcome, GraphRevision, GraphSnapshot, IncidentEdgePolicy,
    TopologyEdit, TopologyPort,
};
use jellyflow_open_gpui::{
    OpenGpuiCommandTicketResult, OpenGpuiNodeBinding, OpenGpuiNodeBindingRead,
    OpenGpuiNodeInteractionGuard, OpenGpuiNodeRegistryBuilder,
};
use open_gpui::{
    App, AppContext, Context, Entity, InteractiveElement, ParentElement, Render, Styled, Task,
    Window, div, px, rgb,
};
use open_gpui_ui_components::{Button, ButtonVariant};

use super::{ShaderNodeError, ShaderPort, port_elements};

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
const BLUEPRINT_AUTOMATIC_INPUT_PREFIX: &str = "arg.input_";

/// One Blueprint-style dynamic data input identified by a stable graph [`PortId`].
#[derive(Clone, Debug, PartialEq)]
pub struct BlueprintPin {
    id: PortId,
    name: String,
    ty: TypeDesc,
}

impl BlueprintPin {
    pub fn new(id: PortId, name: impl Into<String>, ty: TypeDesc) -> Result<Self, ShaderNodeError> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(ShaderNodeError::EmptyBlueprintPinName);
        }
        Ok(Self {
            id,
            name: name.trim().to_owned(),
            ty,
        })
    }

    pub fn id(&self) -> PortId {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn ty(&self) -> &TypeDesc {
        &self.ty
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum BlueprintPinAction {
    Add(BlueprintPin),
    Remove { port: PortId },
}

#[derive(Clone, Debug, PartialEq)]
struct BlueprintPinIntent {
    action: BlueprintPinAction,
    incident_edges: IncidentEdgePolicy,
}

impl BlueprintPinIntent {
    fn add(pin: BlueprintPin) -> Self {
        Self {
            action: BlueprintPinAction::Add(pin),
            incident_edges: IncidentEdgePolicy::Reject,
        }
    }
}

#[derive(Debug, Default)]
enum BlueprintCommandState {
    #[default]
    Idle,
    InFlight {
        intent: BlueprintPinIntent,
        expected_revision: GraphRevision,
    },
    NeedsRefresh {
        intent: BlueprintPinIntent,
        minimum_revision: GraphRevision,
    },
    ReadyToRetry(BlueprintPinIntent),
}

impl BlueprintCommandState {
    fn is_idle(&self) -> bool {
        matches!(self, Self::Idle)
    }

    fn is_waiting(&self) -> bool {
        matches!(self, Self::InFlight { .. } | Self::NeedsRefresh { .. })
    }

    fn retry_intent(&self) -> Option<&BlueprintPinIntent> {
        match self {
            Self::ReadyToRetry(intent) => Some(intent),
            _ => None,
        }
    }
}

/// Retained Blueprint-style node whose dynamic pins are real graph ports.
pub struct BlueprintCallNode {
    binding: Entity<OpenGpuiNodeBinding>,
    command: BlueprintCommandState,
    command_task: Option<Task<()>>,
    diagnostic: Option<String>,
}

impl BlueprintCallNode {
    #[must_use]
    pub fn new(binding: Entity<OpenGpuiNodeBinding>) -> Self {
        Self {
            binding,
            command: BlueprintCommandState::Idle,
            command_task: None,
            diagnostic: None,
        }
    }

    fn add_input(&mut self, cx: &mut Context<Self>) {
        if !self.command.is_idle() {
            return;
        }
        let result = read_blueprint_binding(&self.binding, cx).and_then(|(node_id, facts)| {
            let next_index = next_blueprint_input_index(
                facts
                    .ports()
                    .filter_map(|port| blueprint_input_index(port.port())),
            );
            let pin = BlueprintPin::new(
                PortId::new(),
                format!("input_{next_index}"),
                TypeDesc::Float,
            )?;
            self.enqueue_pin_intent(facts, node_id, BlueprintPinIntent::add(pin), cx)
        });
        if let Err(error) = result {
            self.diagnostic = Some(error.to_string());
        }
    }

    fn retry_pin_action(&mut self, cx: &mut Context<Self>) {
        let Some(intent) = self.command.retry_intent().cloned() else {
            return;
        };
        let result = read_blueprint_binding(&self.binding, cx).and_then(|(node_id, facts)| {
            if blueprint_intent_is_satisfied(
                node_id,
                facts.node(),
                &collect_bound_ports(&facts),
                &intent,
            ) {
                self.command = BlueprintCommandState::Idle;
                self.diagnostic = None;
                return Ok(());
            }
            self.enqueue_pin_intent(facts, node_id, intent.clone(), cx)
        });
        if let Err(error) = result {
            self.command = BlueprintCommandState::ReadyToRetry(intent);
            self.diagnostic = Some(error.to_string());
        }
    }

    fn enqueue_pin_intent(
        &mut self,
        facts: OpenGpuiNodeBindingRead,
        node_id: NodeId,
        intent: BlueprintPinIntent,
        cx: &mut Context<Self>,
    ) -> Result<(), ShaderNodeError> {
        let expected_revision = facts.graph_revision();
        let ports = collect_bound_ports(&facts);
        let command = match build_blueprint_pin_command(
            expected_revision,
            node_id,
            facts.node(),
            &ports,
            intent.action.clone(),
            intent.incident_edges,
        ) {
            Ok(command) => command,
            Err(error) => {
                self.command = BlueprintCommandState::ReadyToRetry(intent);
                return Err(error);
            }
        };
        let ticket = match facts.enqueue_command(cx, |_| command) {
            Ok(ticket) => ticket,
            Err(error) => {
                self.command = BlueprintCommandState::ReadyToRetry(intent);
                return Err(ShaderNodeError::BindingUnavailable {
                    message: error.to_string(),
                });
            }
        };
        self.command = BlueprintCommandState::InFlight {
            intent: intent.clone(),
            expected_revision,
        };
        self.diagnostic = None;
        self.command_task = Some(cx.spawn(async move |node, cx| {
            let result = ticket.await;
            let _ = node.update(cx, |node, cx| {
                node.finish_pin_ticket(intent, expected_revision, result, cx)
            });
        }));
        Ok(())
    }

    fn finish_pin_ticket(
        &mut self,
        intent: BlueprintPinIntent,
        expected_revision: GraphRevision,
        result: OpenGpuiCommandTicketResult,
        cx: &mut Context<Self>,
    ) {
        self.command_task = None;
        if !matches!(
            &self.command,
            BlueprintCommandState::InFlight {
                intent: active,
                expected_revision: active_revision,
            } if active == &intent && *active_revision == expected_revision
        ) {
            return;
        }

        let (minimum_revision, diagnostic) = blueprint_ticket_refresh(expected_revision, result);
        self.command = BlueprintCommandState::NeedsRefresh {
            intent,
            minimum_revision,
        };
        self.diagnostic = Some(diagnostic);
        cx.notify();
    }

    fn reconcile_pin_snapshot(
        &mut self,
        revision: GraphRevision,
        node_id: NodeId,
        node: &Node,
        ports: &BTreeMap<PortId, Port>,
    ) {
        let state = std::mem::take(&mut self.command);
        let (state, satisfied) = reconcile_blueprint_command(state, revision, node_id, node, ports);
        self.command = state;
        if satisfied {
            self.diagnostic = None;
        }
    }
}

fn reconcile_blueprint_command(
    state: BlueprintCommandState,
    revision: GraphRevision,
    node_id: NodeId,
    node: &Node,
    ports: &BTreeMap<PortId, Port>,
) -> (BlueprintCommandState, bool) {
    match state {
        BlueprintCommandState::NeedsRefresh {
            intent,
            minimum_revision,
        } if revision >= minimum_revision => {
            if blueprint_intent_is_satisfied(node_id, node, ports, &intent) {
                (BlueprintCommandState::Idle, true)
            } else {
                (BlueprintCommandState::ReadyToRetry(intent), false)
            }
        }
        state => (state, false),
    }
}

fn blueprint_ticket_refresh(
    expected_revision: GraphRevision,
    result: OpenGpuiCommandTicketResult,
) -> (GraphRevision, String) {
    match result {
        Ok(GraphCommandOutcome::Applied(receipt)) => (
            receipt.revision(),
            "blueprint pin command applied; waiting for authoritative snapshot".to_owned(),
        ),
        Ok(GraphCommandOutcome::Noop(noop)) => (
            noop.revision(),
            "blueprint pin command made no graph change; resynchronizing".to_owned(),
        ),
        Ok(GraphCommandOutcome::Conflict(conflict)) => (
            conflict.current().revision(),
            format!(
                "blueprint pin revision conflict (expected {}, current {}); refresh required before retry",
                conflict.expected_revision(),
                conflict.current().revision(),
            ),
        ),
        Ok(GraphCommandOutcome::Rejected(rejection)) => (
            expected_revision,
            format!(
                "blueprint pin command was rejected: {}",
                rejection.message()
            ),
        ),
        Err(error) => (
            expected_revision,
            format!(
                "blueprint pin command result is uncertain; resynchronizing before retry: {error}"
            ),
        ),
    }
}

impl Render for BlueprintCallNode {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        let (instance_scope, title, ports, observation) = {
            let binding = self.binding.read(cx);
            let instance_scope = format!("{:?}", binding.snapshot().instance());
            let node_id = binding.snapshot().node_id();
            let facts = binding.snapshot().read().and_then(|facts| {
                let title = facts
                    .node()
                    .data
                    .get("title")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("Function Call")
                    .to_owned();
                let observed_ports = collect_bound_ports(&facts);
                let ports = facts
                    .ports()
                    .zip(binding.port_handles(&facts)?)
                    .map(|(port, handle)| ShaderPort {
                        id: port.id(),
                        direction: port.port().dir,
                        label: blueprint_port_label(port.port()),
                        handle,
                    })
                    .collect();
                Ok((
                    title,
                    ports,
                    facts.graph_revision(),
                    node_id,
                    facts.node().clone(),
                    observed_ports,
                ))
            });
            match facts {
                Ok((title, ports, revision, node_id, node, observed_ports)) => (
                    instance_scope,
                    title,
                    ports,
                    Some((revision, node_id, node, observed_ports)),
                ),
                Err(_) => (
                    instance_scope,
                    "Retired Blueprint node".to_owned(),
                    Vec::new(),
                    None,
                ),
            }
        };
        if let Some((revision, node_id, node, observed_ports)) = observation {
            self.reconcile_pin_snapshot(revision, node_id, &node, &observed_ports);
        }
        let (inputs, outputs): (Vec<_>, Vec<_>) = ports
            .into_iter()
            .partition(|port| port.direction == PortDirection::In);

        let node = cx.weak_entity();
        let add_input = Button::new(
            format!("jellyflow-blueprint-add-input-{instance_scope}"),
            "Add input",
        )
        .variant(ButtonVariant::Secondary)
        .disabled(!self.command.is_idle())
        .on_click(move |_, _, cx| {
            let _ = node.update(cx, |node, cx| {
                node.add_input(cx);
                cx.notify();
            });
        });
        let mut body = div()
            .px(px(18.0))
            .py(px(12.0))
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0xabc4dc))
                    .child("Executes a native function with typed dynamic inputs"),
            )
            .child(OpenGpuiNodeInteractionGuard::control(
                div()
                    .debug_selector(|| BLUEPRINT_ADD_INPUT_SELECTOR.to_owned())
                    .child(add_input),
            ));
        if self.command.is_waiting() {
            body = body.child(
                div()
                    .debug_selector(|| BLUEPRINT_COMMAND_PENDING_SELECTOR.to_owned())
                    .px(px(8.0))
                    .py(px(5.0))
                    .rounded(px(4.0))
                    .bg(rgb(0x27364b))
                    .text_color(rgb(0x9fc4ff))
                    .child("Updating pins..."),
            );
        }
        if self.command.retry_intent().is_some() {
            let retry_node = cx.weak_entity();
            body = body.child(
                div()
                    .debug_selector(|| BLUEPRINT_RETRY_SELECTOR.to_owned())
                    .child(
                        Button::new(
                            format!("jellyflow-blueprint-retry-{instance_scope}"),
                            "Retry pin edit",
                        )
                        .variant(ButtonVariant::Secondary)
                        .on_click(move |_, window, cx| {
                            let _ = retry_node.update(cx, |node, cx| node.retry_pin_action(cx));
                            window.refresh();
                        }),
                    ),
            );
        }
        if let Some(diagnostic) = &self.diagnostic {
            body = body.child(
                div()
                    .debug_selector(|| BLUEPRINT_COMMAND_DIAGNOSTIC_SELECTOR.to_owned())
                    .px(px(8.0))
                    .py(px(5.0))
                    .rounded(px(4.0))
                    .bg(rgb(0x3a2026))
                    .text_color(rgb(0xffa5ad))
                    .child(diagnostic.clone()),
            );
        }

        div()
            .debug_selector(|| BLUEPRINT_NODE_SELECTOR.to_owned())
            .relative()
            .size_full()
            .rounded(px(5.0))
            .border_1()
            .border_color(rgb(0x476b87))
            .bg(rgb(0x182633))
            .text_color(rgb(0xe8f1f7))
            .child(
                div()
                    .h(px(42.0))
                    .px(px(14.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(rgb(0x35536b))
                    .child(title)
                    .child(div().text_xs().text_color(rgb(0x7ed2bd)).child("BLUEPRINT")),
            )
            .child(body)
            .children(port_elements(
                inputs,
                PortDirection::In,
                blueprint_port_selector,
            ))
            .children(port_elements(
                outputs,
                PortDirection::Out,
                blueprint_port_selector,
            ))
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
    .default_data(serde_json::json!({ "title": "Function Call" }))
    .build()
}

pub(super) fn register(registry: &mut OpenGpuiNodeRegistryBuilder) {
    registry.register(BLUEPRINT_CALL_RENDERER, |binding, _window, cx| {
        cx.new(|_| BlueprintCallNode::new(binding))
    });
}

#[must_use]
pub fn blueprint_port_selector(port: PortId) -> String {
    format!("jellyflow-native-blueprint-port-{port:?}")
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
    ensure_blueprint_kind(node_id, node)?;
    let ports = node
        .ports
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
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    build_blueprint_pin_command(
        snapshot.revision(),
        node_id,
        node,
        &ports,
        action,
        incident_edges,
    )
}

fn read_blueprint_binding(
    binding: &Entity<OpenGpuiNodeBinding>,
    cx: &mut App,
) -> Result<(NodeId, OpenGpuiNodeBindingRead), ShaderNodeError> {
    let node_id = binding.read(cx).snapshot().node_id();
    let facts = binding.read(cx).snapshot().read().map_err(|error| {
        ShaderNodeError::BindingUnavailable {
            message: error.to_string(),
        }
    })?;
    ensure_blueprint_kind(node_id, facts.node())?;
    Ok((node_id, facts))
}

fn collect_bound_ports(facts: &OpenGpuiNodeBindingRead) -> BTreeMap<PortId, Port> {
    facts
        .ports()
        .map(|bound| (bound.id(), bound.port().clone()))
        .collect()
}

fn blueprint_intent_is_satisfied(
    node_id: NodeId,
    node: &Node,
    ports: &BTreeMap<PortId, Port>,
    intent: &BlueprintPinIntent,
) -> bool {
    match &intent.action {
        BlueprintPinAction::Add(pin) => {
            node.ports.contains(&pin.id())
                && ports
                    .get(&pin.id())
                    .is_some_and(|port| blueprint_pin_matches(node_id, port, pin))
        }
        BlueprintPinAction::Remove { port } => {
            !node.ports.contains(port) && !ports.contains_key(port)
        }
    }
}

fn blueprint_pin_matches(node_id: NodeId, port: &Port, pin: &BlueprintPin) -> bool {
    port.node == node_id
        && port.key == dynamic_blueprint_key(pin.name())
        && port.dir == PortDirection::In
        && port.kind == PortKind::Data
        && port.capacity == PortCapacity::Single
        && port.ty.as_ref() == Some(pin.ty())
        && port.data.get("label").and_then(serde_json::Value::as_str) == Some(pin.name())
}

fn build_blueprint_pin_command(
    revision: GraphRevision,
    node_id: NodeId,
    node: &Node,
    ports: &BTreeMap<PortId, Port>,
    action: BlueprintPinAction,
    incident_edges: IncidentEdgePolicy,
) -> Result<GraphCommand, ShaderNodeError> {
    ensure_blueprint_kind(node_id, node)?;
    let mut next_ports = ports.clone();
    match action {
        BlueprintPinAction::Add(pin) => {
            if next_ports.contains_key(&pin.id) {
                return Err(ShaderNodeError::DuplicateBlueprintPin { port: pin.id });
            }
            let key = dynamic_blueprint_key(pin.name());
            if next_ports.values().any(|port| port.key == key) {
                return Err(ShaderNodeError::DuplicateBlueprintPinName { name: pin.name });
            }
            next_ports.insert(pin.id, dynamic_blueprint_port(node_id, &pin));
        }
        BlueprintPinAction::Remove { port } => {
            let existing = next_ports
                .get(&port)
                .ok_or(ShaderNodeError::BlueprintPinNotFound { port })?;
            if !is_dynamic_blueprint_port(existing) {
                return Err(ShaderNodeError::StaticBlueprintPin { port });
            }
            next_ports.remove(&port);
        }
    }
    validate_blueprint_ports(node_id, &next_ports)?;

    let mut ordered = Vec::with_capacity(next_ports.len());
    for id in &node.ports {
        if let Some(port) = next_ports.remove(id) {
            ordered.push(TopologyPort::new(*id, port));
        }
    }
    ordered.extend(
        next_ports
            .into_iter()
            .map(|(id, port)| TopologyPort::new(id, port)),
    );
    Ok(GraphCommand::topology(
        revision,
        TopologyEdit::new().replace_node_ports(node_id, ordered, incident_edges),
    )
    .with_label("edit blueprint pins"))
}

fn validate_blueprint_ports(
    node_id: NodeId,
    ports: &BTreeMap<PortId, Port>,
) -> Result<(), ShaderNodeError> {
    let mut keys = BTreeSet::new();
    for (id, port) in ports {
        if port.node != node_id {
            return Err(ShaderNodeError::MissingPort {
                node: node_id,
                port: *id,
            });
        }
        if !keys.insert(port.key.clone()) {
            return Err(ShaderNodeError::DuplicateBlueprintPinName {
                name: port.key.0.clone(),
            });
        }
    }
    Ok(())
}

fn ensure_blueprint_kind(node_id: NodeId, node: &Node) -> Result<(), ShaderNodeError> {
    if node.kind != NodeKindKey::new(BLUEPRINT_CALL_KIND) {
        return Err(ShaderNodeError::WrongNodeKind {
            node: node_id,
            expected: BLUEPRINT_CALL_KIND,
            actual: node.kind.0.clone(),
        });
    }
    Ok(())
}

fn dynamic_blueprint_port(node: NodeId, pin: &BlueprintPin) -> Port {
    Port {
        node,
        key: dynamic_blueprint_key(pin.name()),
        dir: PortDirection::In,
        kind: PortKind::Data,
        capacity: PortCapacity::Single,
        connectable: None,
        connectable_start: None,
        connectable_end: None,
        ty: Some(pin.ty.clone()),
        data: serde_json::json!({ "label": pin.name() }),
    }
}

fn dynamic_blueprint_key(name: &str) -> PortKey {
    PortKey::new(format!("arg.{name}"))
}

fn blueprint_input_index(port: &Port) -> Option<usize> {
    port.key
        .0
        .strip_prefix(BLUEPRINT_AUTOMATIC_INPUT_PREFIX)?
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

fn is_dynamic_blueprint_port(port: &Port) -> bool {
    port.dir == PortDirection::In && port.kind == PortKind::Data && port.key.0.starts_with("arg.")
}

fn blueprint_port_label(port: &Port) -> String {
    port.data
        .get("label")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(&port.key.0)
        .to_owned()
}

#[cfg(test)]
mod tests {
    use jellyflow::runtime::ViewCloseReason;
    use jellyflow_open_gpui::OpenGpuiCommandTicketError;

    use super::*;

    #[test]
    fn retained_add_retry_keeps_the_complete_pin_identity() {
        let port = PortId::from_u128(0xabc);
        let intent = BlueprintPinIntent::add(
            BlueprintPin::new(port, "roughness", TypeDesc::Float).expect("valid test pin"),
        );
        let state = BlueprintCommandState::ReadyToRetry(intent);
        let retry = state.retry_intent().expect("retained retry intent");

        let BlueprintPinAction::Add(pin) = &retry.action else {
            panic!("retry must retain the add action");
        };
        assert_eq!(pin.id(), port);
        assert_eq!(pin.name(), "roughness");
        assert_eq!(pin.ty(), &TypeDesc::Float);
    }

    #[test]
    fn conflict_refresh_waits_for_the_required_snapshot_and_never_submits_from_reconcile() {
        let node_id = NodeId::from_u128(0xdef);
        let port = PortId::from_u128(0xabc);
        let intent = BlueprintPinIntent::add(
            BlueprintPin::new(port, "roughness", TypeDesc::Float).expect("valid test pin"),
        );
        let in_flight = BlueprintCommandState::InFlight {
            intent: intent.clone(),
            expected_revision: 9,
        };
        assert!(in_flight.is_waiting());
        let state = BlueprintCommandState::NeedsRefresh {
            intent,
            minimum_revision: 12,
        };
        let node = test_blueprint_node();
        let ports = BTreeMap::new();

        let (state, satisfied) = reconcile_blueprint_command(state, 11, node_id, &node, &ports);
        assert!(!satisfied);
        assert!(matches!(
            state,
            BlueprintCommandState::NeedsRefresh {
                minimum_revision: 12,
                ..
            }
        ));

        let (state, satisfied) = reconcile_blueprint_command(state, 12, node_id, &node, &ports);
        assert!(!satisfied);
        let BlueprintCommandState::ReadyToRetry(retry) = state else {
            panic!("fresh unsatisfied snapshot must require explicit retry");
        };
        let BlueprintPinAction::Add(pin) = retry.action else {
            panic!("retry must retain the add action");
        };
        assert_eq!(pin.id(), port);
    }

    #[test]
    fn ticket_transport_failures_always_require_snapshot_refresh() {
        let failures = [
            OpenGpuiCommandTicketError::HostDropped,
            OpenGpuiCommandTicketError::HostClosed {
                reason: ViewCloseReason::HostClosed,
            },
            OpenGpuiCommandTicketError::MissingReceipt { operation_index: 4 },
            OpenGpuiCommandTicketError::WrongReceiptKind { operation_index: 5 },
        ];

        for failure in failures {
            let (minimum_revision, diagnostic) = blueprint_ticket_refresh(17, Err(failure));
            assert_eq!(minimum_revision, 17);
            assert!(diagnostic.contains("result is uncertain"));
            assert!(diagnostic.contains("resynchronizing before retry"));
        }
    }

    fn test_blueprint_node() -> Node {
        Node {
            kind: NodeKindKey::new(BLUEPRINT_CALL_KIND),
            kind_version: 1,
            pos: jellyflow::core::CanvasPoint { x: 0.0, y: 0.0 },
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
            data: serde_json::json!({ "title": "Function Call" }),
        }
    }
}
