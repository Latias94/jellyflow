mod blueprint_call;
mod multiply;

use jellyflow_core::{NodeId, PortDirection, PortId, PortKey};
use jellyflow_open_gpui::OpenGpuiNodeRegistryBuilder;
use jellyflow_open_gpui::authoring::OpenGpuiPortHandle;
use jellyflow_open_gpui::chrome::OpenGpuiPortMarker;
use jellyflow_runtime::schema::NodeKindSpec;
use open_gpui::{
    AnyElement, InteractiveElement, IntoElement, ParentElement, SharedString, Styled, div, px,
};

use super::native_graph_theme;

pub use blueprint_call::{
    BLUEPRINT_ADD_INPUT_SELECTOR, BLUEPRINT_CALL_DEFAULT_SIZE, BLUEPRINT_CALL_KIND,
    BLUEPRINT_CALL_RENDERER, BLUEPRINT_NODE_SELECTOR, BlueprintCallNode, BlueprintPin,
    BlueprintPinAction, blueprint_pin_down_selector, blueprint_pin_remove_selector,
    blueprint_pin_row_selector, blueprint_pin_scroll_selector, blueprint_pin_up_selector,
    blueprint_pins, blueprint_port_selector, plan_blueprint_pin_action,
};
pub use multiply::{
    SHADER_FACTOR_FIELD, SHADER_MULTIPLY_DEFAULT_SIZE, SHADER_MULTIPLY_KIND,
    SHADER_MULTIPLY_RENDERER, SHADER_NODE_SELECTOR, SHADER_PREVIEW_CONTROL_SELECTOR,
    SHADER_PREVIEW_VALUE_SELECTOR, ShaderFieldStore, ShaderMultiplyNode,
    shader_connected_input_selector, shader_factor_accept_remote_selector,
    shader_factor_conflict_selector, shader_factor_reapply_selector, shader_inline_editor_selector,
    shader_inline_value_selector, shader_port_selector,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShaderNodeError {
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
    MissingPort {
        node: NodeId,
        port: PortId,
    },
    DuplicateBlueprintPin {
        port: PortId,
    },
    DuplicateBlueprintPinId {
        pin_id: String,
    },
    DuplicateBlueprintPinName {
        name: String,
    },
    EmptyBlueprintPinId,
    EmptyBlueprintPinName,
    BlueprintPinNotFound {
        pin_id: String,
    },
    InvalidBlueprintPinIndex {
        pin_id: String,
        index: usize,
        len: usize,
    },
    InvalidBlueprintPortOrder {
        node: NodeId,
    },
    BindingUnavailable {
        message: String,
    },
    DraftConflict,
    CommandRejected {
        message: String,
    },
}

impl std::fmt::Display for ShaderNodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingNode { node } => write!(f, "shader node is missing: {node:?}"),
            Self::WrongNodeKind {
                node,
                expected,
                actual,
            } => write!(
                f,
                "shader node {node:?} has kind {actual:?}, expected {expected:?}"
            ),
            Self::InvalidData { node, field } => {
                write!(f, "shader node {node:?} has invalid {field} data")
            }
            Self::MissingPort { node, port } => {
                write!(f, "shader node {node:?} is missing port {port:?}")
            }
            Self::DuplicateBlueprintPin { port } => {
                write!(f, "blueprint pin id is duplicated: {port:?}")
            }
            Self::DuplicateBlueprintPinId { pin_id } => {
                write!(f, "blueprint logical pin id is duplicated: {pin_id:?}")
            }
            Self::DuplicateBlueprintPinName { name } => {
                write!(f, "blueprint pin name is duplicated: {name:?}")
            }
            Self::EmptyBlueprintPinId => f.write_str("blueprint logical pin id must not be empty"),
            Self::EmptyBlueprintPinName => f.write_str("blueprint pin name must not be empty"),
            Self::BlueprintPinNotFound { pin_id } => {
                write!(f, "blueprint pin does not exist: {pin_id:?}")
            }
            Self::InvalidBlueprintPinIndex { pin_id, index, len } => {
                write!(
                    f,
                    "blueprint pin {pin_id:?} cannot move to index {index}; pin count is {len}"
                )
            }
            Self::InvalidBlueprintPortOrder { node } => {
                write!(
                    f,
                    "blueprint node {node:?} has invalid static/dynamic port order"
                )
            }
            Self::BindingUnavailable { message } => {
                write!(f, "shader binding is unavailable: {message}")
            }
            Self::DraftConflict => {
                f.write_str("shader factor draft requires explicit conflict resolution")
            }
            Self::CommandRejected { message } => {
                write!(f, "shader command was rejected: {message}")
            }
        }
    }
}

impl std::error::Error for ShaderNodeError {}

struct ShaderPort {
    id: PortId,
    key: PortKey,
    direction: PortDirection,
    label: SharedString,
    handle: OpenGpuiPortHandle,
}

struct ShaderPortElement {
    key: PortKey,
    label: SharedString,
    marker: AnyElement,
}

pub(super) fn catalog_specs() -> [NodeKindSpec; 2] {
    [multiply::catalog_spec(), blueprint_call::catalog_spec()]
}

pub(super) fn register_with_field_store(
    registry: &mut OpenGpuiNodeRegistryBuilder,
    field_store: ShaderFieldStore,
) {
    multiply::register(registry, field_store);
    blueprint_call::register(registry);
}

fn port_element(port: ShaderPort, selector_for: fn(PortId) -> String) -> ShaderPortElement {
    let selector = selector_for(port.id);
    let chrome = OpenGpuiPortMarker::new(port.handle.presentation(), native_graph_theme());
    let minimum_hit_diameter = chrome.minimum_hit_diameter();
    let marker = port
        .handle
        .with_minimum_hit_diameter(minimum_hit_diameter)
        .interactive(
            div()
                .debug_selector(move || selector.clone())
                .w(px(12.0))
                .h(px(12.0))
                .flex()
                .items_center()
                .justify_center()
                .child(chrome),
        );
    ShaderPortElement {
        key: port.key,
        label: port.label,
        marker: marker.into_any_element(),
    }
}
