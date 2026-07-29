mod blueprint_call;
mod multiply;

#[cfg(test)]
use jellyflow_core::TypeDesc;
use jellyflow_core::{NodeId, PortDirection, PortId, PortKey, PortKind};
use jellyflow_open_gpui::authoring::OpenGpuiPortHandle;
use jellyflow_open_gpui::chrome::{OpenGpuiPortMarker, OpenGpuiPortTone};
use jellyflow_open_gpui::registry::OpenGpuiNodeRegistryBuilder;
use jellyflow_open_gpui::theme::OpenGpuiGraphStyleSlot;
use jellyflow_runtime::schema::NodeKindSpec;
use open_gpui::{
    AnyElement, InteractiveElement, IntoElement, ParentElement, SharedString, Styled, div, px,
};

use crate::product_theme::NativeProductTheme;

pub use crate::product_theme::NATIVE_FLOAT_STYLE_SLOT as SHADER_FLOAT_STYLE_SLOT;

pub use blueprint_call::{
    BLUEPRINT_ADD_INPUT_SELECTOR, BLUEPRINT_CALL_DEFAULT_SIZE, BLUEPRINT_CALL_KIND,
    BLUEPRINT_CALL_RENDERER, BLUEPRINT_DISCARD_SELECTOR, BLUEPRINT_NODE_SELECTOR,
    BlueprintCallNode, blueprint_body_selector, blueprint_header_metadata_selector,
    blueprint_header_title_selector, blueprint_pin_down_selector, blueprint_pin_label_selector,
    blueprint_pin_remove_selector, blueprint_pin_row_selector, blueprint_pin_scroll_selector,
    blueprint_pin_up_selector, blueprint_port_label_selector, blueprint_port_selector,
};
pub use multiply::{
    SHADER_FACTOR_FIELD, SHADER_MULTIPLY_DEFAULT_SIZE, SHADER_MULTIPLY_KIND,
    SHADER_MULTIPLY_RENDERER, SHADER_NODE_SELECTOR, SHADER_PREVIEW_CONTROL_SELECTOR,
    SHADER_PREVIEW_VALUE_SELECTOR, ShaderFieldStore, ShaderMultiplyNode, shader_body_selector,
    shader_connected_input_selector, shader_factor_accept_remote_selector,
    shader_factor_conflict_selector, shader_factor_reapply_selector,
    shader_header_metadata_selector, shader_header_title_selector, shader_inline_editor_selector,
    shader_inline_value_selector, shader_port_label_selector, shader_port_selector,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShaderNodeError {
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
    DraftConflict,
    CommandRejected {
        message: String,
    },
}

impl std::fmt::Display for ShaderNodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
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
    kind: PortKind,
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
    product_theme: NativeProductTheme,
) {
    multiply::register(registry, field_store, product_theme);
    blueprint_call::register(registry, product_theme);
}

fn port_style_slot(kind: PortKind) -> Option<OpenGpuiGraphStyleSlot> {
    (kind == PortKind::Data).then_some(SHADER_FLOAT_STYLE_SLOT)
}

fn port_element(
    port: ShaderPort,
    selector_for: fn(PortId) -> String,
    product_theme: NativeProductTheme,
) -> ShaderPortElement {
    let selector = selector_for(port.id);
    let chrome = OpenGpuiPortMarker::new(
        port.handle.presentation(),
        OpenGpuiPortTone::from(port.kind),
        product_theme.graph_theme(),
    );
    let chrome = match port_style_slot(port.kind) {
        Some(slot) => chrome.with_style_slot(slot),
        None => chrome,
    };
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shader_float_type_and_data_markers_share_one_bounded_slot() {
        let product_theme = NativeProductTheme::shader_blueprint();
        let painters = product_theme.edge_painter_registry();
        let theme = product_theme.graph_theme();

        assert_eq!(
            painters.style_slot_for_type(&TypeDesc::Float),
            Some(SHADER_FLOAT_STYLE_SLOT)
        );
        assert_eq!(
            port_style_slot(PortKind::Data),
            Some(SHADER_FLOAT_STYLE_SLOT)
        );
        assert_eq!(port_style_slot(PortKind::Exec), None);
        assert!(theme.style_slot(SHADER_FLOAT_STYLE_SLOT).is_some());
    }
}
