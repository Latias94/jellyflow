use jellyflow::{
    core::{GraphOp, GraphTransaction, Node, NodeId},
    runtime::{
        runtime::measurement::{NodeInternalsInvalidation, NodeInternalsInvalidationReason},
        schema::{
            NodeControlBinding, NodeControlBindingSource, NodeControlDescriptor, NodeControlKind,
            NodeSurfaceSlotDescriptor, NodeSurfaceSlotKind,
        },
    },
};
use serde_json::Value;

use crate::json_binding::{
    field_row_slot_data_path, join_data_path, read_bound_value, semantic_json_lookup,
    set_bound_value,
};

/// GPUI-local primitive family selected for a semantic control descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenGpuiControlPrimitive {
    TextInput,
    TextArea,
    NumberInput,
    Select,
    MultiSelect,
    Switch,
    Slider,
    CodeEditor,
    ColorSwatch,
    AssetPickerStub,
    VariablePickerStub,
    PortBindingDisplay,
}

impl OpenGpuiControlPrimitive {
    pub fn is_stub(self) -> bool {
        matches!(
            self,
            Self::AssetPickerStub | Self::VariablePickerStub | Self::PortBindingDisplay
        )
    }
}

/// Capability level for a concrete GPUI control mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenGpuiControlSupport {
    Native,
    Fallback,
    Stub,
    Unsupported,
}

/// Adapter-facing render plan for one semantic control.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenGpuiControlPlan {
    pub key: String,
    pub label: String,
    pub kind: NodeControlKind,
    pub primitive: OpenGpuiControlPrimitive,
    pub support: OpenGpuiControlSupport,
    pub value: Option<Value>,
    pub binding: Option<NodeControlBinding>,
    pub options: Vec<OpenGpuiControlOptionPlan>,
    pub placeholder: Option<String>,
    pub help_text: Option<String>,
    pub disabled_reason: Option<String>,
    pub read_only: bool,
    pub secret: bool,
}

impl OpenGpuiControlPlan {
    pub fn is_editable(&self) -> bool {
        self.binding.is_some() && self.disabled_reason.is_none() && !self.read_only
    }

    pub fn is_partial_stub(&self) -> bool {
        self.support == OpenGpuiControlSupport::Stub || self.primitive.is_stub()
    }
}

/// Adapter-facing render plan for one select-like option.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenGpuiControlOptionPlan {
    pub value: Value,
    pub label: String,
    pub disabled: bool,
}

/// Context needed to resolve a semantic control inside a slot or repeatable item.
#[derive(Debug, Clone, Copy, Default)]
pub struct OpenGpuiControlProjectionContext<'a> {
    pub slot: Option<&'a NodeSurfaceSlotDescriptor>,
    pub item_data: Option<&'a Value>,
    pub item_path: Option<&'a str>,
}

/// Mutation plan emitted by a GPUI control edit.
#[derive(Debug, Clone)]
pub struct OpenGpuiControlEditPlan {
    pub transaction: GraphTransaction,
    pub invalidation: NodeInternalsInvalidation,
}

/// Build render plans for controls declared on one slot.
pub fn project_slot_controls(
    node_data: &Value,
    slot: &NodeSurfaceSlotDescriptor,
) -> Vec<OpenGpuiControlPlan> {
    let mut controls = slot.controls.iter().collect::<Vec<_>>();
    controls.sort_by_key(|control| (control.order_key(), control.key.clone()));

    controls
        .into_iter()
        .map(|control| {
            project_control(
                node_data,
                control,
                OpenGpuiControlProjectionContext {
                    slot: Some(slot),
                    item_data: None,
                    item_path: None,
                },
            )
        })
        .collect()
}

/// Build a render plan for one semantic control descriptor.
pub fn project_control(
    node_data: &Value,
    control: &NodeControlDescriptor,
    context: OpenGpuiControlProjectionContext<'_>,
) -> OpenGpuiControlPlan {
    let binding = control_write_binding(control, context);
    let value = control_current_value(node_data, control, context).cloned();
    let primitive = primitive_for_kind(control.kind);
    let support = support_for_primitive(primitive);

    OpenGpuiControlPlan {
        key: control.key.clone(),
        label: control
            .display_label()
            .unwrap_or(control.key.as_str())
            .to_owned(),
        kind: control.kind,
        primitive,
        support,
        value,
        binding,
        options: control
            .options
            .iter()
            .map(|option| OpenGpuiControlOptionPlan {
                value: option.value.clone(),
                label: option.label.clone(),
                disabled: option.disabled,
            })
            .collect(),
        placeholder: control.presentation.placeholder.clone(),
        help_text: control.presentation.help_text.clone(),
        disabled_reason: control.editability.disabled_reason.clone(),
        read_only: control.editability.read_only,
        secret: control.editability.secret,
    }
}

/// Build a node-data transaction and internals invalidation for an editable control value.
pub fn plan_control_edit(
    node_id: NodeId,
    node: &Node,
    control: &OpenGpuiControlPlan,
    value: Value,
) -> Result<Option<OpenGpuiControlEditPlan>, String> {
    if control.disabled_reason.is_some() {
        return Ok(None);
    }
    if control.read_only {
        return Ok(None);
    }
    let binding = control
        .binding
        .as_ref()
        .ok_or_else(|| format!("control `{}` has no writable binding", control.key))?;
    let from = node.data.clone();
    let mut to = from.clone();
    set_bound_node_value(&mut to, binding, value)?;
    if from == to {
        return Ok(None);
    }

    Ok(Some(OpenGpuiControlEditPlan {
        transaction: GraphTransaction::from_ops([GraphOp::SetNodeData {
            id: node_id,
            from,
            to,
        }])
        .with_label("Set GPUI node control value"),
        invalidation: NodeInternalsInvalidation::one(
            node_id,
            NodeInternalsInvalidationReason::DataChanged,
        ),
    }))
}

pub fn primitive_for_kind(kind: NodeControlKind) -> OpenGpuiControlPrimitive {
    match kind {
        NodeControlKind::TextInput => OpenGpuiControlPrimitive::TextInput,
        NodeControlKind::TextArea => OpenGpuiControlPrimitive::TextArea,
        NodeControlKind::NumberInput => OpenGpuiControlPrimitive::NumberInput,
        NodeControlKind::Select => OpenGpuiControlPrimitive::Select,
        NodeControlKind::MultiSelect => OpenGpuiControlPrimitive::MultiSelect,
        NodeControlKind::Toggle => OpenGpuiControlPrimitive::Switch,
        NodeControlKind::Code | NodeControlKind::Expression => OpenGpuiControlPrimitive::CodeEditor,
        NodeControlKind::Color => OpenGpuiControlPrimitive::ColorSwatch,
        NodeControlKind::Asset => OpenGpuiControlPrimitive::AssetPickerStub,
        NodeControlKind::VariablePicker => OpenGpuiControlPrimitive::VariablePickerStub,
        NodeControlKind::Slider => OpenGpuiControlPrimitive::Slider,
        NodeControlKind::PortBinding => OpenGpuiControlPrimitive::PortBindingDisplay,
    }
}

pub fn support_for_primitive(primitive: OpenGpuiControlPrimitive) -> OpenGpuiControlSupport {
    match primitive {
        OpenGpuiControlPrimitive::TextInput
        | OpenGpuiControlPrimitive::TextArea
        | OpenGpuiControlPrimitive::NumberInput
        | OpenGpuiControlPrimitive::Select
        | OpenGpuiControlPrimitive::MultiSelect
        | OpenGpuiControlPrimitive::Switch
        | OpenGpuiControlPrimitive::Slider => OpenGpuiControlSupport::Native,
        OpenGpuiControlPrimitive::CodeEditor | OpenGpuiControlPrimitive::ColorSwatch => {
            OpenGpuiControlSupport::Fallback
        }
        OpenGpuiControlPrimitive::AssetPickerStub
        | OpenGpuiControlPrimitive::VariablePickerStub
        | OpenGpuiControlPrimitive::PortBindingDisplay => OpenGpuiControlSupport::Stub,
    }
}

fn control_current_value<'a>(
    node_data: &'a Value,
    control: &NodeControlDescriptor,
    context: OpenGpuiControlProjectionContext<'a>,
) -> Option<&'a Value> {
    if let Some(item_data) = context.item_data {
        return control
            .binding
            .as_ref()
            .and_then(|binding| read_bound_value(item_data, binding, false))
            .or_else(|| {
                control
                    .slot
                    .as_deref()
                    .and_then(|path| semantic_json_lookup(item_data, path))
            });
    }

    if let Some(binding) = &control.binding {
        match binding.source {
            NodeControlBindingSource::DataPath => {
                return read_bound_value(node_data, binding, false);
            }
            NodeControlBindingSource::Slot => {
                return read_bound_value(
                    node_data,
                    binding,
                    context
                        .slot
                        .is_some_and(|slot| slot.kind == NodeSurfaceSlotKind::FieldRow),
                );
            }
            NodeControlBindingSource::JsonPointer => {
                return read_bound_value(node_data, binding, false);
            }
            NodeControlBindingSource::GraphSymbol | NodeControlBindingSource::PortAnchor => {}
        }
    }

    control
        .slot
        .as_deref()
        .and_then(|path| semantic_json_lookup(node_data, path))
        .or_else(|| {
            context
                .slot
                .and_then(NodeSurfaceSlotDescriptor::data_key)
                .and_then(|path| semantic_json_lookup(node_data, path))
        })
}

fn control_write_binding(
    control: &NodeControlDescriptor,
    context: OpenGpuiControlProjectionContext<'_>,
) -> Option<NodeControlBinding> {
    let binding = control
        .binding
        .clone()
        .or_else(|| {
            control
                .slot
                .as_ref()
                .map(|slot| NodeControlBinding::data_path(slot.clone()))
        })
        .or_else(|| {
            context
                .slot
                .and_then(NodeSurfaceSlotDescriptor::data_key)
                .map(|path| NodeControlBinding::slot(path.to_owned()))
        })?;

    if let Some(item_path) = context.item_path {
        return match binding.source {
            NodeControlBindingSource::DataPath | NodeControlBindingSource::Slot => Some(
                NodeControlBinding::data_path(join_data_path(item_path, &binding.path)),
            ),
            NodeControlBindingSource::JsonPointer
            | NodeControlBindingSource::GraphSymbol
            | NodeControlBindingSource::PortAnchor => None,
        };
    }

    match binding.source {
        NodeControlBindingSource::DataPath | NodeControlBindingSource::JsonPointer => Some(binding),
        NodeControlBindingSource::Slot => {
            let path = if let Some(slot) = context.slot {
                if slot.kind == NodeSurfaceSlotKind::FieldRow {
                    field_row_slot_data_path(&binding.path)
                } else {
                    binding.path
                }
            } else {
                binding.path
            };
            Some(NodeControlBinding::data_path(path))
        }
        NodeControlBindingSource::GraphSymbol | NodeControlBindingSource::PortAnchor => None,
    }
}

fn set_bound_node_value(
    data: &mut Value,
    binding: &NodeControlBinding,
    value: Value,
) -> Result<(), String> {
    set_bound_value(data, binding, value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jellyflow::{
        core::{CanvasPoint, CanvasSize, NodeKindKey},
        runtime::schema::{NodeControlBinding, NodeControlDescriptor, NodeKitRegistry},
    };
    use serde_json::json;

    #[test]
    fn maps_first_gpui_control_set_to_component_primitives() {
        let cases = [
            (
                NodeControlKind::TextInput,
                OpenGpuiControlPrimitive::TextInput,
                OpenGpuiControlSupport::Native,
            ),
            (
                NodeControlKind::TextArea,
                OpenGpuiControlPrimitive::TextArea,
                OpenGpuiControlSupport::Native,
            ),
            (
                NodeControlKind::Select,
                OpenGpuiControlPrimitive::Select,
                OpenGpuiControlSupport::Native,
            ),
            (
                NodeControlKind::NumberInput,
                OpenGpuiControlPrimitive::NumberInput,
                OpenGpuiControlSupport::Native,
            ),
            (
                NodeControlKind::Slider,
                OpenGpuiControlPrimitive::Slider,
                OpenGpuiControlSupport::Native,
            ),
            (
                NodeControlKind::Toggle,
                OpenGpuiControlPrimitive::Switch,
                OpenGpuiControlSupport::Native,
            ),
            (
                NodeControlKind::Code,
                OpenGpuiControlPrimitive::CodeEditor,
                OpenGpuiControlSupport::Fallback,
            ),
            (
                NodeControlKind::Expression,
                OpenGpuiControlPrimitive::CodeEditor,
                OpenGpuiControlSupport::Fallback,
            ),
            (
                NodeControlKind::Color,
                OpenGpuiControlPrimitive::ColorSwatch,
                OpenGpuiControlSupport::Fallback,
            ),
            (
                NodeControlKind::Asset,
                OpenGpuiControlPrimitive::AssetPickerStub,
                OpenGpuiControlSupport::Stub,
            ),
            (
                NodeControlKind::VariablePicker,
                OpenGpuiControlPrimitive::VariablePickerStub,
                OpenGpuiControlSupport::Stub,
            ),
            (
                NodeControlKind::PortBinding,
                OpenGpuiControlPrimitive::PortBindingDisplay,
                OpenGpuiControlSupport::Stub,
            ),
        ];

        for (kind, primitive, support) in cases {
            assert_eq!(primitive_for_kind(kind), primitive);
            assert_eq!(support_for_primitive(primitive), support);
        }
    }

    #[test]
    fn projects_dify_prompt_textarea_and_variable_stub() {
        let registry = NodeKitRegistry::builtin().node_registry();
        let descriptor = registry
            .view_descriptor(&NodeKindKey::new("demo.llm"))
            .expect("builtin llm descriptor");
        let slot = descriptor
            .surface_slot("field.prompt")
            .expect("prompt field slot");
        let node_data = json!({ "fields": { "prompt": "Summarize {{ input }}" } });

        let controls = project_slot_controls(&node_data, slot);

        let prompt = controls
            .iter()
            .find(|control| control.key == "control.prompt")
            .expect("prompt control");
        assert_eq!(prompt.primitive, OpenGpuiControlPrimitive::TextArea);
        assert_eq!(prompt.support, OpenGpuiControlSupport::Native);
        assert_eq!(prompt.value, Some(json!("Summarize {{ input }}")));
        assert!(prompt.is_editable());

        let variable = controls
            .iter()
            .find(|control| control.key == "control.prompt_variable")
            .expect("variable picker control");
        assert_eq!(
            variable.primitive,
            OpenGpuiControlPrimitive::VariablePickerStub
        );
        assert_eq!(variable.support, OpenGpuiControlSupport::Stub);
        assert!(variable.is_partial_stub());
        assert!(!variable.is_editable());
    }

    #[test]
    fn edit_plan_updates_node_data_and_marks_internals_dirty() {
        let node_id = NodeId::from_u128(7);
        let node = Node {
            kind: NodeKindKey::new("demo.llm"),
            kind_version: 1,
            pos: CanvasPoint::default(),
            origin: None,
            selectable: None,
            focusable: None,
            draggable: None,
            connectable: None,
            deletable: None,
            parent: None,
            extent: None,
            expand_parent: None,
            size: Some(CanvasSize {
                width: 228.0,
                height: 196.0,
            }),
            hidden: false,
            collapsed: false,
            ports: Vec::new(),
            data: json!({ "fields": { "prompt": "old" } }),
        };
        let descriptor = NodeControlDescriptor::text_area("control.prompt")
            .with_binding(NodeControlBinding::data_path("fields.prompt"));
        let control = project_control(
            &node.data,
            &descriptor,
            OpenGpuiControlProjectionContext::default(),
        );

        let plan = plan_control_edit(node_id, &node, &control, json!("new"))
            .expect("edit plan")
            .expect("changed edit plan");

        assert_eq!(plan.invalidation.nodes, vec![node_id]);
        assert_eq!(
            plan.invalidation.reason,
            NodeInternalsInvalidationReason::DataChanged
        );
        let [GraphOp::SetNodeData { id, to, .. }] = plan.transaction.ops() else {
            panic!("expected one SetNodeData op");
        };
        assert_eq!(*id, node_id);
        assert_eq!(to["fields"]["prompt"], json!("new"));
    }

    #[test]
    fn readonly_and_disabled_controls_do_not_dispatch_edits() {
        let node_id = NodeId::from_u128(7);
        let mut node = Node {
            kind: NodeKindKey::new("demo.llm"),
            kind_version: 1,
            pos: CanvasPoint::default(),
            origin: None,
            selectable: None,
            focusable: None,
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
            data: json!({ "prompt": "old" }),
        };
        let readonly = NodeControlDescriptor::text_input("control.prompt")
            .with_binding(NodeControlBinding::data_path("prompt"))
            .read_only();
        let readonly_plan = project_control(
            &node.data,
            &readonly,
            OpenGpuiControlProjectionContext::default(),
        );
        assert!(!readonly_plan.is_editable());
        assert!(
            plan_control_edit(node_id, &node, &readonly_plan, json!("new"))
                .expect("readonly edit")
                .is_none()
        );

        node.data = json!({ "prompt": "old" });
        let disabled = NodeControlDescriptor::text_input("control.prompt")
            .with_binding(NodeControlBinding::data_path("prompt"))
            .disabled("locked");
        let disabled_plan = project_control(
            &node.data,
            &disabled,
            OpenGpuiControlProjectionContext::default(),
        );
        assert!(!disabled_plan.is_editable());
        assert!(
            plan_control_edit(node_id, &node, &disabled_plan, json!("new"))
                .expect("disabled edit")
                .is_none()
        );
    }
}
