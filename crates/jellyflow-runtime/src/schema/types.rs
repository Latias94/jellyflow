use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::kit::{NodeKitContentDensity, NodeKitLayoutHints};
use jellyflow_core::core::{
    CanvasPoint, CanvasSize, Node, NodeId, NodeKindKey, Port, PortCapacity, PortDirection, PortId,
    PortKey, PortKind,
};
use jellyflow_core::ops::{GraphOp, GraphTransaction};
use jellyflow_core::types::TypeDesc;

fn port_view_descriptor_is_default(value: &PortViewDescriptor) -> bool {
    value.is_default()
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn node_surface_layout_budget_is_empty(value: &NodeSurfaceLayoutBudget) -> bool {
    value.is_empty()
}

/// Declares a port for a node kind.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PortDecl {
    /// Stable schema key for this port.
    pub key: PortKey,
    /// Direction.
    pub dir: PortDirection,
    /// Kind.
    pub kind: PortKind,
    /// Capacity.
    pub capacity: PortCapacity,
    /// Optional type descriptor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ty: Option<TypeDesc>,
    /// UI-facing label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Adapter-facing handle and label presentation metadata.
    #[serde(default, skip_serializing_if = "port_view_descriptor_is_default")]
    pub view: PortViewDescriptor,
}

/// Adapter-facing side hint for a node port handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortViewSide {
    Top,
    Right,
    Bottom,
    Left,
}

impl PortViewSide {
    pub fn fallback_for_direction(dir: PortDirection) -> Self {
        match dir {
            PortDirection::In => Self::Left,
            PortDirection::Out => Self::Right,
        }
    }
}

/// Adapter-facing visibility hint for a node port handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortHandleVisibility {
    Visible,
    Hidden,
    Collapsed,
}

/// Renderer-neutral metadata that helps adapters place and present handles.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PortViewDescriptor {
    /// Preferred side for the handle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side: Option<PortViewSide>,
    /// Deterministic order within side/group.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<i32>,
    /// Optional grouping key for adapters that cluster ports.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Optional adapter anchor id, such as a table field or form row id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
    /// Optional lane key inside a node renderer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lane: Option<String>,
    /// Optional slot key inside a lane or anchor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot: Option<String>,
    /// Optional label override for adapter handle labels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Optional adapter icon key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_key: Option<String>,
    /// Optional handle visibility hint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visibility: Option<PortHandleVisibility>,
}

impl PortViewDescriptor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn side(side: PortViewSide) -> Self {
        Self {
            side: Some(side),
            ..Self::default()
        }
    }

    pub fn top() -> Self {
        Self::side(PortViewSide::Top)
    }

    pub fn right() -> Self {
        Self::side(PortViewSide::Right)
    }

    pub fn bottom() -> Self {
        Self::side(PortViewSide::Bottom)
    }

    pub fn left() -> Self {
        Self::side(PortViewSide::Left)
    }

    pub fn with_order(mut self, order: i32) -> Self {
        self.order = Some(order);
        self
    }

    pub fn with_group(mut self, group: impl Into<String>) -> Self {
        self.group = Some(group.into());
        self
    }

    pub fn with_anchor(mut self, anchor: impl Into<String>) -> Self {
        self.anchor = Some(anchor.into());
        self
    }

    pub fn with_lane(mut self, lane: impl Into<String>) -> Self {
        self.lane = Some(lane.into());
        self
    }

    pub fn with_slot(mut self, slot: impl Into<String>) -> Self {
        self.slot = Some(slot.into());
        self
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn with_icon_key(mut self, icon_key: impl Into<String>) -> Self {
        self.icon_key = Some(icon_key.into());
        self
    }

    pub fn with_visibility(mut self, visibility: PortHandleVisibility) -> Self {
        self.visibility = Some(visibility);
        self
    }

    pub fn hidden(self) -> Self {
        self.with_visibility(PortHandleVisibility::Hidden)
    }

    pub fn collapsed(self) -> Self {
        self.with_visibility(PortHandleVisibility::Collapsed)
    }

    pub fn is_visible(&self) -> bool {
        matches!(self.visibility, None | Some(PortHandleVisibility::Visible))
    }

    pub fn is_hidden(&self) -> bool {
        matches!(self.visibility, Some(PortHandleVisibility::Hidden))
    }

    pub fn is_collapsed(&self) -> bool {
        matches!(self.visibility, Some(PortHandleVisibility::Collapsed))
    }

    pub fn is_hidden_or_collapsed(&self) -> bool {
        !self.is_visible()
    }

    pub fn resolved_side(&self, dir: PortDirection) -> PortViewSide {
        self.side
            .unwrap_or_else(|| PortViewSide::fallback_for_direction(dir))
    }

    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
}

/// Renderer-neutral semantic role for a node-local surface slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeSurfaceSlotKind {
    Header,
    Body,
    Footer,
    Badge,
    Icon,
    FieldRow,
    ActionRow,
    Preview,
    NestedRegion,
    StatusBanner,
    PortRail,
    ConfigGroup,
    MetricBadge,
}

/// Renderer-neutral semantic role for an editable node-local control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeControlKind {
    TextInput,
    NumberInput,
    Select,
    MultiSelect,
    Toggle,
    Code,
    Color,
    Asset,
    VariablePicker,
    Expression,
    TextArea,
    Slider,
    PortBinding,
}

/// Data-oriented binding target for a node-local control.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeControlBinding {
    pub source: NodeControlBindingSource,
    pub path: String,
}

/// Semantic source namespace for a control binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeControlBindingSource {
    DataPath,
    Slot,
    JsonPointer,
    GraphSymbol,
    PortAnchor,
}

impl NodeControlBinding {
    pub fn new(source: NodeControlBindingSource, path: impl Into<String>) -> Self {
        Self {
            source,
            path: path.into(),
        }
    }

    pub fn data_path(path: impl Into<String>) -> Self {
        Self::new(NodeControlBindingSource::DataPath, path)
    }

    pub fn slot(path: impl Into<String>) -> Self {
        Self::new(NodeControlBindingSource::Slot, path)
    }

    pub fn json_pointer(path: impl Into<String>) -> Self {
        Self::new(NodeControlBindingSource::JsonPointer, path)
    }

    pub fn graph_symbol(path: impl Into<String>) -> Self {
        Self::new(NodeControlBindingSource::GraphSymbol, path)
    }

    pub fn port_anchor(path: impl Into<String>) -> Self {
        Self::new(NodeControlBindingSource::PortAnchor, path)
    }
}

/// Inline selectable value for select-like controls.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeControlOption {
    pub value: Value,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_key: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub disabled: bool,
}

impl NodeControlOption {
    pub fn new(value: impl Into<Value>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            icon_key: None,
            disabled: false,
        }
    }

    pub fn with_icon_key(mut self, icon_key: impl Into<String>) -> Self {
        self.icon_key = Some(icon_key.into());
        self
    }

    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }
}

/// Renderer-neutral option source for controls whose choices are adapter or graph supplied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "key")]
pub enum NodeControlOptionSource {
    Inline,
    Variables,
    Assets,
    Ports,
    Custom(String),
}

impl Default for NodeControlOptionSource {
    fn default() -> Self {
        Self::Inline
    }
}

fn node_control_option_source_is_default(value: &NodeControlOptionSource) -> bool {
    value == &NodeControlOptionSource::Inline
}

/// Validation rules adapters can present without owning validation execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum NodeControlValidationRule {
    Required,
    EnumValues { values: Vec<Value> },
    Regex { pattern: String },
    Range { min: Option<f64>, max: Option<f64> },
    ExpressionShape { language: Option<String> },
}

/// Validation metadata for a node-local control.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct NodeControlValidation {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<NodeControlValidationRule>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub messages: Vec<String>,
}

impl NodeControlValidation {
    pub fn with_rule(mut self, rule: NodeControlValidationRule) -> Self {
        self.rules.push(rule);
        self
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.messages.push(message.into());
        self
    }

    pub fn required(mut self) -> Self {
        self.rules.push(NodeControlValidationRule::Required);
        self
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty() && self.messages.is_empty()
    }
}

fn node_control_validation_is_empty(value: &NodeControlValidation) -> bool {
    value.is_empty()
}

/// Presentation hints for adapter-local widgets.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct NodeControlPresentation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub compact_label: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub multiline: bool,
}

impl NodeControlPresentation {
    pub fn with_placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    pub fn with_help_text(mut self, help_text: impl Into<String>) -> Self {
        self.help_text = Some(help_text.into());
        self
    }

    pub fn with_icon_key(mut self, icon_key: impl Into<String>) -> Self {
        self.icon_key = Some(icon_key.into());
        self
    }

    pub fn with_unit(mut self, unit: impl Into<String>) -> Self {
        self.unit = Some(unit.into());
        self
    }

    pub fn with_language(mut self, language: impl Into<String>) -> Self {
        self.language = Some(language.into());
        self
    }

    pub fn compact_label(mut self) -> Self {
        self.compact_label = true;
        self
    }

    pub fn multiline(mut self) -> Self {
        self.multiline = true;
        self
    }

    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

fn node_control_presentation_is_empty(value: &NodeControlPresentation) -> bool {
    value.is_empty()
}

/// Editability metadata for adapter-local controls.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct NodeControlEditability {
    #[serde(default, skip_serializing_if = "is_false")]
    pub read_only: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_reason: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub secret: bool,
}

impl NodeControlEditability {
    pub fn read_only(mut self) -> Self {
        self.read_only = true;
        self
    }

    pub fn disabled(mut self, reason: impl Into<String>) -> Self {
        self.disabled_reason = Some(reason.into());
        self
    }

    pub fn secret(mut self) -> Self {
        self.secret = true;
        self
    }

    pub fn is_disabled(&self) -> bool {
        self.disabled_reason.is_some()
    }

    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

fn node_control_editability_is_empty(value: &NodeControlEditability) -> bool {
    value.is_empty()
}

/// Renderer-neutral metadata for one adapter-local editable control.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeControlDescriptor {
    pub key: String,
    pub kind: NodeControlKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding: Option<NodeControlBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<NodeControlOption>,
    #[serde(default, skip_serializing_if = "node_control_option_source_is_default")]
    pub option_source: NodeControlOptionSource,
    #[serde(default, skip_serializing_if = "node_control_validation_is_empty")]
    pub validation: NodeControlValidation,
    #[serde(default, skip_serializing_if = "node_control_presentation_is_empty")]
    pub presentation: NodeControlPresentation,
    #[serde(default, skip_serializing_if = "node_control_editability_is_empty")]
    pub editability: NodeControlEditability,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot: Option<String>,
}

impl NodeControlDescriptor {
    pub fn new(key: impl Into<String>, kind: NodeControlKind) -> Self {
        Self {
            key: key.into(),
            kind,
            label: None,
            binding: None,
            options: Vec::new(),
            option_source: NodeControlOptionSource::Inline,
            validation: NodeControlValidation::default(),
            presentation: NodeControlPresentation::default(),
            editability: NodeControlEditability::default(),
            order: None,
            anchor: None,
            slot: None,
        }
    }

    pub fn text_input(key: impl Into<String>) -> Self {
        Self::new(key, NodeControlKind::TextInput)
    }

    pub fn number_input(key: impl Into<String>) -> Self {
        Self::new(key, NodeControlKind::NumberInput)
    }

    pub fn select(key: impl Into<String>) -> Self {
        Self::new(key, NodeControlKind::Select)
    }

    pub fn multi_select(key: impl Into<String>) -> Self {
        Self::new(key, NodeControlKind::MultiSelect)
    }

    pub fn toggle(key: impl Into<String>) -> Self {
        Self::new(key, NodeControlKind::Toggle)
    }

    pub fn code(key: impl Into<String>) -> Self {
        Self::new(key, NodeControlKind::Code)
    }

    pub fn color(key: impl Into<String>) -> Self {
        Self::new(key, NodeControlKind::Color)
    }

    pub fn asset(key: impl Into<String>) -> Self {
        Self::new(key, NodeControlKind::Asset)
    }

    pub fn variable_picker(key: impl Into<String>) -> Self {
        Self::new(key, NodeControlKind::VariablePicker)
    }

    pub fn expression(key: impl Into<String>) -> Self {
        Self::new(key, NodeControlKind::Expression)
    }

    pub fn text_area(key: impl Into<String>) -> Self {
        Self::new(key, NodeControlKind::TextArea)
            .with_presentation(NodeControlPresentation::default().multiline())
    }

    pub fn slider(key: impl Into<String>) -> Self {
        Self::new(key, NodeControlKind::Slider)
    }

    pub fn port_binding(key: impl Into<String>) -> Self {
        Self::new(key, NodeControlKind::PortBinding)
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn with_binding(mut self, binding: NodeControlBinding) -> Self {
        self.binding = Some(binding);
        self
    }

    pub fn with_option(mut self, option: NodeControlOption) -> Self {
        self.options.push(option);
        self
    }

    pub fn with_options(mut self, options: impl IntoIterator<Item = NodeControlOption>) -> Self {
        self.options.extend(options);
        self
    }

    pub fn with_option_source(mut self, option_source: NodeControlOptionSource) -> Self {
        self.option_source = option_source;
        self
    }

    pub fn with_validation(mut self, validation: NodeControlValidation) -> Self {
        self.validation = validation;
        self
    }

    pub fn with_validation_rule(mut self, rule: NodeControlValidationRule) -> Self {
        self.validation.rules.push(rule);
        self
    }

    pub fn required(self) -> Self {
        self.with_validation_rule(NodeControlValidationRule::Required)
    }

    pub fn with_presentation(mut self, presentation: NodeControlPresentation) -> Self {
        self.presentation = presentation;
        self
    }

    pub fn with_placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.presentation.placeholder = Some(placeholder.into());
        self
    }

    pub fn with_help_text(mut self, help_text: impl Into<String>) -> Self {
        self.presentation.help_text = Some(help_text.into());
        self
    }

    pub fn with_language(mut self, language: impl Into<String>) -> Self {
        self.presentation.language = Some(language.into());
        self
    }

    pub fn with_editability(mut self, editability: NodeControlEditability) -> Self {
        self.editability = editability;
        self
    }

    pub fn read_only(mut self) -> Self {
        self.editability.read_only = true;
        self
    }

    pub fn disabled(mut self, reason: impl Into<String>) -> Self {
        self.editability.disabled_reason = Some(reason.into());
        self
    }

    pub fn secret(mut self) -> Self {
        self.editability.secret = true;
        self
    }

    pub fn with_order(mut self, order: i32) -> Self {
        self.order = Some(order);
        self
    }

    pub fn with_anchor(mut self, anchor: impl Into<String>) -> Self {
        self.anchor = Some(anchor.into());
        self
    }

    pub fn with_slot(mut self, slot: impl Into<String>) -> Self {
        self.slot = Some(slot.into());
        self
    }

    pub fn data_key(&self) -> Option<&str> {
        self.binding
            .as_ref()
            .and_then(|binding| match binding.source {
                NodeControlBindingSource::DataPath | NodeControlBindingSource::Slot => {
                    Some(binding.path.as_str())
                }
                NodeControlBindingSource::JsonPointer
                | NodeControlBindingSource::GraphSymbol
                | NodeControlBindingSource::PortAnchor => None,
            })
            .or(self.slot.as_deref())
    }

    pub fn display_label(&self) -> Option<&str> {
        self.label.as_deref().or_else(|| {
            self.key
                .split_once('.')
                .map(|(_, tail)| tail)
                .or(Some(self.key.as_str()))
        })
    }

    pub fn order_key(&self) -> i32 {
        self.order.unwrap_or(i32::MAX)
    }
}

/// Renderer-neutral overflow affordance adapters should expose when content is capped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeSurfaceOverflowIndicator {
    Summary,
    Count,
    Scroll,
    Fade,
}

/// Semantic layout budget for readable node-internal product surfaces.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct NodeSurfaceLayoutBudget {
    /// Minimum logical node size needed before rendering full-density internals.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_readable_size: Option<CanvasSize>,
    /// Preferred logical node size for first render or resize suggestions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferred_size: Option<CanvasSize>,
    /// Maximum logical text lines adapters should budget for ordinary slots.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot_line_budget: Option<usize>,
    /// Maximum logical text lines adapters should budget for controls.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control_line_budget: Option<usize>,
    /// Suggested visible repeatable item count before showing overflow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repeatable_visible_items: Option<usize>,
    /// Semantic overflow affordance expected when content is capped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overflow_indicator: Option<NodeSurfaceOverflowIndicator>,
    /// Density tiers adapters should prefer when the full surface does not fit.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub density_priority: Vec<NodeKitContentDensity>,
}

impl NodeSurfaceLayoutBudget {
    pub fn with_min_readable_size(mut self, size: CanvasSize) -> Self {
        self.min_readable_size = Some(size);
        self
    }

    pub fn with_preferred_size(mut self, size: CanvasSize) -> Self {
        self.preferred_size = Some(size);
        self
    }

    pub fn with_slot_line_budget(mut self, lines: usize) -> Self {
        self.slot_line_budget = Some(lines);
        self
    }

    pub fn with_control_line_budget(mut self, lines: usize) -> Self {
        self.control_line_budget = Some(lines);
        self
    }

    pub fn with_repeatable_visible_items(mut self, items: usize) -> Self {
        self.repeatable_visible_items = Some(items);
        self
    }

    pub fn with_overflow_indicator(mut self, indicator: NodeSurfaceOverflowIndicator) -> Self {
        self.overflow_indicator = Some(indicator);
        self
    }

    pub fn with_density_priority(
        mut self,
        density_priority: impl IntoIterator<Item = NodeKitContentDensity>,
    ) -> Self {
        self.density_priority = density_priority.into_iter().collect();
        self
    }

    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

/// Descriptor for node data arrays or maps rendered as stable dynamic rows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeRepeatableCollectionDescriptor {
    pub key: String,
    pub item_source: String,
    pub item_id_path: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub item_template_slots: Vec<NodeSurfaceSlotDescriptor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub empty_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_items: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_items: Option<usize>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub reorderable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub add_action: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remove_action: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reorder_action: Option<String>,
    #[serde(default)]
    pub anchor_rule: NodeRepeatableAnchorRule,
}

/// Stable key derivation for slots and anchors generated from repeatable items.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeRepeatableAnchorRule {
    pub slot_prefix: String,
    pub anchor_prefix: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port_key_path: Option<String>,
}

impl Default for NodeRepeatableAnchorRule {
    fn default() -> Self {
        Self {
            slot_prefix: "item".to_owned(),
            anchor_prefix: "item".to_owned(),
            port_key_path: None,
        }
    }
}

/// Projected instance of one repeatable item for adapter rendering and measurement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeRepeatableItemProjection {
    pub collection_key: String,
    pub item_id: String,
    pub item_index: usize,
    pub slot_key: String,
    pub anchor: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port_key: Option<String>,
    #[serde(default)]
    pub item_data: Value,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub slots: Vec<NodeSurfaceSlotDescriptor>,
}

impl NodeRepeatableCollectionDescriptor {
    pub fn new(
        key: impl Into<String>,
        item_source: impl Into<String>,
        item_id_path: impl Into<String>,
    ) -> Self {
        let key = key.into();
        Self {
            anchor_rule: NodeRepeatableAnchorRule {
                slot_prefix: key.clone(),
                anchor_prefix: key.clone(),
                port_key_path: None,
            },
            key,
            item_source: item_source.into(),
            item_id_path: item_id_path.into(),
            item_template_slots: Vec::new(),
            label: None,
            empty_label: None,
            min_items: None,
            max_items: None,
            reorderable: false,
            add_action: None,
            remove_action: None,
            reorder_action: None,
        }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn with_empty_label(mut self, empty_label: impl Into<String>) -> Self {
        self.empty_label = Some(empty_label.into());
        self
    }

    pub fn with_item_template_slot(mut self, slot: NodeSurfaceSlotDescriptor) -> Self {
        self.item_template_slots.push(slot);
        self
    }

    pub fn with_item_template_slots(
        mut self,
        slots: impl IntoIterator<Item = NodeSurfaceSlotDescriptor>,
    ) -> Self {
        self.item_template_slots.extend(slots);
        self
    }

    pub fn with_min_items(mut self, min_items: usize) -> Self {
        self.min_items = Some(min_items);
        self
    }

    pub fn with_max_items(mut self, max_items: usize) -> Self {
        self.max_items = Some(max_items);
        self
    }

    pub fn reorderable(mut self) -> Self {
        self.reorderable = true;
        self
    }

    pub fn with_add_action(mut self, action: impl Into<String>) -> Self {
        self.add_action = Some(action.into());
        self
    }

    pub fn with_remove_action(mut self, action: impl Into<String>) -> Self {
        self.remove_action = Some(action.into());
        self
    }

    pub fn with_reorder_action(mut self, action: impl Into<String>) -> Self {
        self.reorder_action = Some(action.into());
        self
    }

    pub fn with_anchor_rule(mut self, anchor_rule: NodeRepeatableAnchorRule) -> Self {
        self.anchor_rule = anchor_rule;
        self
    }

    pub fn item_projections(&self, node_data: &Value) -> Vec<NodeRepeatableItemProjection> {
        let Some(items) = semantic_json_lookup(node_data, &self.item_source) else {
            return Vec::new();
        };

        match items {
            Value::Array(items) => items
                .iter()
                .enumerate()
                .filter_map(|(index, item)| self.item_projection(index, item))
                .collect(),
            Value::Object(items) => items
                .iter()
                .enumerate()
                .filter_map(|(index, (key, item))| {
                    self.item_projection_with_fallback_id(index, item, key.as_str())
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    pub fn is_empty_for(&self, node_data: &Value) -> bool {
        self.item_projections(node_data).is_empty()
    }

    pub fn add_disabled_reason(&self, node_data: &Value) -> Option<String> {
        let max_items = self.max_items?;
        let len = self.item_projections(node_data).len();
        (len >= max_items).then(|| format!("Maximum of {max_items} items reached"))
    }

    pub fn remove_disabled_reason(&self, node_data: &Value) -> Option<String> {
        let min_items = self.min_items?;
        let len = self.item_projections(node_data).len();
        (len <= min_items).then(|| format!("Minimum of {min_items} items required"))
    }

    fn item_projection(
        &self,
        item_index: usize,
        item_data: &Value,
    ) -> Option<NodeRepeatableItemProjection> {
        let item_id = semantic_json_lookup(item_data, &self.item_id_path)
            .and_then(json_scalar_to_stable_string)?;
        self.item_projection_with_id(item_index, item_data, &item_id)
    }

    fn item_projection_with_fallback_id(
        &self,
        item_index: usize,
        item_data: &Value,
        fallback_id: &str,
    ) -> Option<NodeRepeatableItemProjection> {
        let item_id = semantic_json_lookup(item_data, &self.item_id_path)
            .and_then(json_scalar_to_stable_string)
            .unwrap_or_else(|| fallback_id.to_owned());
        self.item_projection_with_id(item_index, item_data, &item_id)
    }

    fn item_projection_with_id(
        &self,
        item_index: usize,
        item_data: &Value,
        item_id: &str,
    ) -> Option<NodeRepeatableItemProjection> {
        let item_id = sanitize_repeatable_key(item_id)?;
        let slot_key = format!("{}.{}", self.anchor_rule.slot_prefix, item_id);
        let anchor = format!("{}.{}", self.anchor_rule.anchor_prefix, item_id);
        let port_key = self
            .anchor_rule
            .port_key_path
            .as_deref()
            .and_then(|path| semantic_json_lookup(item_data, path))
            .and_then(json_scalar_to_stable_string);
        let slots = self
            .item_template_slots
            .iter()
            .map(|slot| repeatable_item_slot(slot, &slot_key, &anchor, item_id))
            .collect();

        Some(NodeRepeatableItemProjection {
            collection_key: self.key.clone(),
            item_id: item_id.to_owned(),
            item_index,
            slot_key,
            anchor,
            port_key,
            item_data: item_data.clone(),
            slots,
        })
    }
}

impl NodeRepeatableAnchorRule {
    pub fn new(slot_prefix: impl Into<String>, anchor_prefix: impl Into<String>) -> Self {
        Self {
            slot_prefix: slot_prefix.into(),
            anchor_prefix: anchor_prefix.into(),
            port_key_path: None,
        }
    }

    pub fn with_port_key_path(mut self, port_key_path: impl Into<String>) -> Self {
        self.port_key_path = Some(port_key_path.into());
        self
    }
}

fn repeatable_item_slot(
    template: &NodeSurfaceSlotDescriptor,
    slot_key: &str,
    anchor: &str,
    item_id: &str,
) -> NodeSurfaceSlotDescriptor {
    let suffix = template.key_tail().unwrap_or(template.key.as_str());
    let mut slot = template.clone();
    slot.key = format!("{slot_key}.{suffix}");
    slot.anchor = Some(format!("{anchor}.{suffix}"));
    if let Some(template_slot) = template.slot.as_deref() {
        slot.slot = Some(format!("{template_slot}.{item_id}"));
    }
    slot
}

fn json_scalar_to_stable_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn sanitize_repeatable_key(value: &str) -> Option<&str> {
    (!value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')))
    .then_some(value)
}

/// Renderer-neutral action target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ActionTarget {
    Graph,
    Node {
        node_kind: String,
    },
    Edge,
    Port {
        port_key: String,
    },
    Slot {
        slot_key: String,
    },
    Control {
        control_key: String,
    },
    RepeatableItem {
        collection_key: String,
        item_id: String,
    },
    DroppedWire {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source_port_key: Option<String>,
    },
    Inspector {
        inspector_key: String,
    },
    Blackboard {
        blackboard_key: String,
    },
}

/// Renderer-neutral action intent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ActionIntent {
    InsertNode {
        node_kind: String,
    },
    OpenMenu {
        menu_key: String,
    },
    OpenInspector {
        inspector_key: String,
    },
    AddRepeatableItem {
        collection_key: String,
    },
    RemoveRepeatableItem {
        collection_key: String,
        item_id: String,
    },
    ReorderRepeatableItem {
        collection_key: String,
        item_id: String,
    },
    SetControlValue {
        control_key: String,
    },
    RunNode,
    OpenBlackboard {
        blackboard_key: String,
    },
    Custom {
        key: String,
    },
}

/// Applicability and disabled-state metadata for actions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionAvailability {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_reason: Option<String>,
}

fn default_true() -> bool {
    true
}

impl Default for ActionAvailability {
    fn default() -> Self {
        Self {
            enabled: true,
            disabled_reason: None,
        }
    }
}

impl ActionAvailability {
    pub fn enabled() -> Self {
        Self::default()
    }

    pub fn disabled(reason: impl Into<String>) -> Self {
        Self {
            enabled: false,
            disabled_reason: Some(reason.into()),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

fn action_availability_is_default(value: &ActionAvailability) -> bool {
    value == &ActionAvailability::default()
}

/// Shortcut metadata adapters can map to toolkit-specific accelerators.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionShortcut {
    pub key: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub ctrl: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub shift: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub alt: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub meta: bool,
}

impl ActionShortcut {
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            ctrl: false,
            shift: false,
            alt: false,
            meta: false,
        }
    }

    pub fn ctrl(mut self) -> Self {
        self.ctrl = true;
        self
    }

    pub fn shift(mut self) -> Self {
        self.shift = true;
        self
    }

    pub fn alt(mut self) -> Self {
        self.alt = true;
        self
    }

    pub fn meta(mut self) -> Self {
        self.meta = true;
        self
    }
}

/// Renderer-neutral command/action descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeActionDescriptor {
    pub key: String,
    pub label: String,
    pub target: ActionTarget,
    pub intent: ActionIntent,
    #[serde(default, skip_serializing_if = "action_availability_is_default")]
    pub availability: ActionAvailability,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<i32>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub danger: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shortcut: Option<ActionShortcut>,
}

impl NodeActionDescriptor {
    pub fn new(
        key: impl Into<String>,
        label: impl Into<String>,
        target: ActionTarget,
        intent: ActionIntent,
    ) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            target,
            intent,
            availability: ActionAvailability::default(),
            group: None,
            order: None,
            danger: false,
            icon_key: None,
            shortcut: None,
        }
    }

    pub fn disabled(mut self, reason: impl Into<String>) -> Self {
        self.availability = ActionAvailability::disabled(reason);
        self
    }

    pub fn with_group(mut self, group: impl Into<String>) -> Self {
        self.group = Some(group.into());
        self
    }

    pub fn with_order(mut self, order: i32) -> Self {
        self.order = Some(order);
        self
    }

    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }

    pub fn with_icon_key(mut self, icon_key: impl Into<String>) -> Self {
        self.icon_key = Some(icon_key.into());
        self
    }

    pub fn with_shortcut(mut self, shortcut: ActionShortcut) -> Self {
        self.shortcut = Some(shortcut);
        self
    }

    pub fn is_enabled(&self) -> bool {
        self.availability.is_enabled()
    }
}

/// Surface on which a renderer-local menu appears.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum MenuSurface {
    Graph,
    Node,
    Edge,
    Port,
    Slot,
    Control,
    DroppedWire,
    Toolbar,
    Blackboard,
    Inspector,
}

/// Renderer-neutral menu descriptor. Adapters own popup state and widgets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MenuDescriptor {
    pub key: String,
    pub surface: MenuSurface,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub action_keys: Vec<String>,
}

impl MenuDescriptor {
    pub fn new(key: impl Into<String>, surface: MenuSurface) -> Self {
        Self {
            key: key.into(),
            surface,
            label: None,
            action_keys: Vec::new(),
        }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn with_action_key(mut self, action_key: impl Into<String>) -> Self {
        self.action_keys.push(action_key.into());
        self
    }

    pub fn with_action_keys(
        mut self,
        action_keys: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.action_keys
            .extend(action_keys.into_iter().map(Into::into));
        self
    }
}

/// Target for an inspector surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum InspectorTarget {
    Graph,
    Node {
        node_kind: String,
    },
    Edge,
    Port {
        port_key: String,
    },
    Slot {
        slot_key: String,
    },
    Control {
        control_key: String,
    },
    RepeatableItem {
        collection_key: String,
        item_id: String,
    },
    Diagnostic {
        diagnostic_key: String,
    },
}

/// Renderer-neutral inspector descriptor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InspectorDescriptor {
    pub key: String,
    pub target: InspectorTarget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub controls: Vec<NodeControlDescriptor>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub action_keys: Vec<String>,
}

impl InspectorDescriptor {
    pub fn new(key: impl Into<String>, target: InspectorTarget) -> Self {
        Self {
            key: key.into(),
            target,
            label: None,
            controls: Vec::new(),
            action_keys: Vec::new(),
        }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn with_control(mut self, control: NodeControlDescriptor) -> Self {
        self.controls.push(control);
        self
    }

    pub fn with_action_key(mut self, action_key: impl Into<String>) -> Self {
        self.action_keys.push(action_key.into());
        self
    }
}

/// Graph-level property collection exposed to adapters as a local blackboard UI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlackboardDescriptor {
    pub key: String,
    pub label: String,
    pub collection: NodeRepeatableCollectionDescriptor,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub action_keys: Vec<String>,
}

impl BlackboardDescriptor {
    pub fn new(
        key: impl Into<String>,
        label: impl Into<String>,
        collection: NodeRepeatableCollectionDescriptor,
    ) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            collection,
            action_keys: Vec::new(),
        }
    }

    pub fn with_action_key(mut self, action_key: impl Into<String>) -> Self {
        self.action_keys.push(action_key.into());
        self
    }
}

/// Adapter-facing visibility hint for a node-local surface slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeSurfaceSlotVisibility {
    Visible,
    Hidden,
    Collapsed,
}

/// Shared semantic role for adapter-owned chrome around a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeChromeKind {
    Resizer,
    Toolbar,
    StatusStrip,
    ValidationBanner,
    RunActionStrip,
    InspectorAnchor,
}

/// Preferred placement for adapter-owned chrome around a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeChromePlacement {
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
    TopLeft,
    InsideHeader,
    InsideFooter,
}

/// Adapter-facing visibility hint for node chrome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeChromeVisibility {
    Always,
    Selected,
    Hovered,
    Focused,
    Hidden,
}

/// Renderer-neutral metadata for adapter-owned node chrome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeChromeDescriptor {
    pub key: String,
    pub kind: NodeChromeKind,
    pub placement: NodeChromePlacement,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub renderer_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visibility: Option<NodeChromeVisibility>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<i32>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub interactive: bool,
}

impl NodeChromeDescriptor {
    pub fn new(
        key: impl Into<String>,
        kind: NodeChromeKind,
        placement: NodeChromePlacement,
    ) -> Self {
        Self {
            key: key.into(),
            kind,
            placement,
            label: None,
            renderer_key: None,
            icon_key: None,
            visibility: None,
            order: None,
            interactive: false,
        }
    }

    pub fn resizer(key: impl Into<String>) -> Self {
        Self::new(
            key,
            NodeChromeKind::Resizer,
            NodeChromePlacement::BottomRight,
        )
        .interactive()
        .with_visibility(NodeChromeVisibility::Selected)
    }

    pub fn toolbar(key: impl Into<String>, placement: NodeChromePlacement) -> Self {
        Self::new(key, NodeChromeKind::Toolbar, placement)
            .interactive()
            .with_visibility(NodeChromeVisibility::Selected)
    }

    pub fn status_strip(key: impl Into<String>, placement: NodeChromePlacement) -> Self {
        Self::new(key, NodeChromeKind::StatusStrip, placement)
            .with_visibility(NodeChromeVisibility::Always)
    }

    pub fn validation_banner(key: impl Into<String>, placement: NodeChromePlacement) -> Self {
        Self::new(key, NodeChromeKind::ValidationBanner, placement)
            .with_visibility(NodeChromeVisibility::Always)
    }

    pub fn run_action_strip(key: impl Into<String>, placement: NodeChromePlacement) -> Self {
        Self::new(key, NodeChromeKind::RunActionStrip, placement)
            .interactive()
            .with_visibility(NodeChromeVisibility::Selected)
    }

    pub fn inspector_anchor(key: impl Into<String>, placement: NodeChromePlacement) -> Self {
        Self::new(key, NodeChromeKind::InspectorAnchor, placement)
            .with_visibility(NodeChromeVisibility::Selected)
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn with_renderer_key(mut self, renderer_key: impl Into<String>) -> Self {
        self.renderer_key = Some(renderer_key.into());
        self
    }

    pub fn with_icon_key(mut self, icon_key: impl Into<String>) -> Self {
        self.icon_key = Some(icon_key.into());
        self
    }

    pub fn with_visibility(mut self, visibility: NodeChromeVisibility) -> Self {
        self.visibility = Some(visibility);
        self
    }

    pub fn with_order(mut self, order: i32) -> Self {
        self.order = Some(order);
        self
    }

    pub fn interactive(mut self) -> Self {
        self.interactive = true;
        self
    }

    pub fn hidden(self) -> Self {
        self.with_visibility(NodeChromeVisibility::Hidden)
    }

    pub fn effective_visibility(&self) -> NodeChromeVisibility {
        self.visibility.unwrap_or(NodeChromeVisibility::Always)
    }

    pub fn is_visible_for_state(&self, selected: bool, hovered: bool, focused: bool) -> bool {
        match self.effective_visibility() {
            NodeChromeVisibility::Always => true,
            NodeChromeVisibility::Selected => selected,
            NodeChromeVisibility::Hovered => hovered,
            NodeChromeVisibility::Focused => focused,
            NodeChromeVisibility::Hidden => false,
        }
    }
}

/// Adapter-facing, toolkit-neutral semantic surface projection for one node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeSurfaceProjection {
    /// Current density tier used by the adapter.
    pub density: NodeKitContentDensity,
    /// Maximum slots the adapter should show at this density.
    pub slot_limit: usize,
    /// Whether the adapter should prioritize compact value previews.
    pub compact_values: bool,
    /// Whether all visible slots should be shown without truncation.
    pub expand_all_slots: bool,
}

impl NodeSurfaceProjection {
    pub fn from_layout_hints(layout_hints: &NodeKitLayoutHints, zoom: f32) -> Self {
        let density = layout_hints.content_density_for_zoom(zoom);
        let (slot_limit, compact_values, expand_all_slots) = match density {
            NodeKitContentDensity::Compact => (2, true, false),
            NodeKitContentDensity::Regular => (3, true, false),
            NodeKitContentDensity::Full => (usize::MAX, false, true),
        };

        Self {
            density,
            slot_limit,
            compact_values,
            expand_all_slots,
        }
    }
}

/// Renderer-neutral node-local slot metadata for rich adapter surfaces.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeSurfaceSlotDescriptor {
    /// Stable slot key within the node kind, such as `header.main` or `field.primary_key`.
    pub key: String,
    /// Semantic role that adapters map to toolkit-specific widgets.
    pub kind: NodeSurfaceSlotKind,
    /// Optional UI-facing label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Deterministic order within the slot kind/lane.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<i32>,
    /// Optional adapter anchor id used by ports or nested regions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
    /// Optional lane key for adapters that group slots.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lane: Option<String>,
    /// Optional sub-slot key inside a lane or anchor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot: Option<String>,
    /// Optional adapter renderer key for this slot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub renderer_key: Option<String>,
    /// Optional adapter icon key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_key: Option<String>,
    /// Optional visibility hint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visibility: Option<NodeSurfaceSlotVisibility>,
    /// Renderer-neutral controls adapters can map to local toolkit widgets.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub controls: Vec<NodeControlDescriptor>,
}

impl NodeSurfaceSlotDescriptor {
    pub fn new(key: impl Into<String>, kind: NodeSurfaceSlotKind) -> Self {
        Self {
            key: key.into(),
            kind,
            label: None,
            order: None,
            anchor: None,
            lane: None,
            slot: None,
            renderer_key: None,
            icon_key: None,
            visibility: None,
            controls: Vec::new(),
        }
    }

    pub fn header(key: impl Into<String>) -> Self {
        Self::new(key, NodeSurfaceSlotKind::Header)
    }

    pub fn body(key: impl Into<String>) -> Self {
        Self::new(key, NodeSurfaceSlotKind::Body)
    }

    pub fn footer(key: impl Into<String>) -> Self {
        Self::new(key, NodeSurfaceSlotKind::Footer)
    }

    pub fn badge(key: impl Into<String>) -> Self {
        Self::new(key, NodeSurfaceSlotKind::Badge)
    }

    pub fn icon(key: impl Into<String>) -> Self {
        Self::new(key, NodeSurfaceSlotKind::Icon)
    }

    pub fn field_row(key: impl Into<String>) -> Self {
        Self::new(key, NodeSurfaceSlotKind::FieldRow)
    }

    pub fn action_row(key: impl Into<String>) -> Self {
        Self::new(key, NodeSurfaceSlotKind::ActionRow)
    }

    pub fn preview(key: impl Into<String>) -> Self {
        Self::new(key, NodeSurfaceSlotKind::Preview)
    }

    pub fn nested_region(key: impl Into<String>) -> Self {
        Self::new(key, NodeSurfaceSlotKind::NestedRegion)
    }

    pub fn status_banner(key: impl Into<String>) -> Self {
        Self::new(key, NodeSurfaceSlotKind::StatusBanner)
    }

    pub fn port_rail(key: impl Into<String>) -> Self {
        Self::new(key, NodeSurfaceSlotKind::PortRail)
    }

    pub fn config_group(key: impl Into<String>) -> Self {
        Self::new(key, NodeSurfaceSlotKind::ConfigGroup)
    }

    pub fn metric_badge(key: impl Into<String>) -> Self {
        Self::new(key, NodeSurfaceSlotKind::MetricBadge)
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn with_order(mut self, order: i32) -> Self {
        self.order = Some(order);
        self
    }

    pub fn with_anchor(mut self, anchor: impl Into<String>) -> Self {
        self.anchor = Some(anchor.into());
        self
    }

    pub fn with_lane(mut self, lane: impl Into<String>) -> Self {
        self.lane = Some(lane.into());
        self
    }

    pub fn with_slot(mut self, slot: impl Into<String>) -> Self {
        self.slot = Some(slot.into());
        self
    }

    pub fn with_renderer_key(mut self, renderer_key: impl Into<String>) -> Self {
        self.renderer_key = Some(renderer_key.into());
        self
    }

    pub fn with_icon_key(mut self, icon_key: impl Into<String>) -> Self {
        self.icon_key = Some(icon_key.into());
        self
    }

    pub fn with_visibility(mut self, visibility: NodeSurfaceSlotVisibility) -> Self {
        self.visibility = Some(visibility);
        self
    }

    pub fn with_control(mut self, control: NodeControlDescriptor) -> Self {
        self.controls.push(control);
        self
    }

    pub fn with_controls(
        mut self,
        controls: impl IntoIterator<Item = NodeControlDescriptor>,
    ) -> Self {
        self.controls.extend(controls);
        self
    }

    pub fn hidden(self) -> Self {
        self.with_visibility(NodeSurfaceSlotVisibility::Hidden)
    }

    pub fn collapsed(self) -> Self {
        self.with_visibility(NodeSurfaceSlotVisibility::Collapsed)
    }

    pub fn key_tail(&self) -> Option<&str> {
        self.key.split_once('.').map(|(_, tail)| tail)
    }

    pub fn data_key(&self) -> Option<&str> {
        self.slot.as_deref().or_else(|| {
            (self.kind == NodeSurfaceSlotKind::FieldRow)
                .then(|| self.key_tail())
                .flatten()
        })
    }

    pub fn display_label(&self) -> Option<&str> {
        self.label.as_deref().or_else(|| self.key_tail())
    }

    pub fn order_key(&self) -> i32 {
        self.order.unwrap_or(i32::MAX)
    }

    pub fn is_visible(&self) -> bool {
        matches!(
            self.visibility,
            None | Some(NodeSurfaceSlotVisibility::Visible)
        )
    }

    pub fn is_hidden(&self) -> bool {
        matches!(self.visibility, Some(NodeSurfaceSlotVisibility::Hidden))
    }

    pub fn is_collapsed(&self) -> bool {
        matches!(self.visibility, Some(NodeSurfaceSlotVisibility::Collapsed))
    }

    pub fn is_hidden_or_collapsed(&self) -> bool {
        !self.is_visible()
    }
}

/// Minimal adapter-facing slot projection derived from a semantic slot descriptor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeSurfaceSlotProjection {
    pub key: String,
    pub kind: NodeSurfaceSlotKind,
    pub label: String,
    pub value: String,
    pub visible: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub controls: Vec<NodeControlDescriptor>,
}

impl NodeSurfaceSlotProjection {
    pub fn from_descriptor(
        node_data: &Value,
        slot: &NodeSurfaceSlotDescriptor,
        compact_values: bool,
    ) -> Self {
        let label = slot.display_label().unwrap_or(slot.key.as_str()).to_owned();
        let value = slot_value_preview(node_data, slot, compact_values).unwrap_or_default();
        Self {
            key: slot.key.clone(),
            kind: slot.kind,
            label,
            value,
            visible: slot.is_visible(),
            controls: slot.controls.clone(),
        }
    }
}

/// Schema for a node kind.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeSchema {
    /// Canonical kind key.
    pub kind: NodeKindKey,
    /// Latest schema version for this kind.
    pub latest_kind_version: u32,
    /// Kind aliases (renames).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kind_aliases: Vec<NodeKindKey>,

    /// UI-facing title.
    pub title: String,
    /// Category path (for create-node search/palette).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub category: Vec<String>,
    /// Search keywords.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
    /// Adapter-facing renderer key.
    ///
    /// Runtime keeps this as data instead of a component reference so React, Svelte, native, and
    /// future adapters can map the key to their own renderer registry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub renderer_key: Option<String>,
    /// Default logical node size for adapters that need an initial rect before measurement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_size: Option<CanvasSize>,
    /// Semantic readable-surface budget for adapter-local node internals.
    #[serde(default, skip_serializing_if = "node_surface_layout_budget_is_empty")]
    pub layout_budget: NodeSurfaceLayoutBudget,

    /// Declared ports.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ports: Vec<PortDecl>,

    /// Renderer-neutral semantic slots for rich adapter node surfaces.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub surface_slots: Vec<NodeSurfaceSlotDescriptor>,
    /// Renderer-neutral dynamic row/list descriptors for node-local authoring surfaces.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repeatable_collections: Vec<NodeRepeatableCollectionDescriptor>,
    /// Renderer-neutral action descriptors for node-local authoring surfaces.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<NodeActionDescriptor>,
    /// Renderer-neutral menu descriptors. Adapters own popup widgets and state.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub menus: Vec<MenuDescriptor>,
    /// Renderer-neutral inspector descriptors. Adapters own panel widgets and focus.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inspectors: Vec<InspectorDescriptor>,
    /// Graph-level property lists exposed as adapter-local blackboard panels.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blackboards: Vec<BlackboardDescriptor>,
    /// Renderer-neutral semantic chrome around rich adapter node surfaces.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chrome: Vec<NodeChromeDescriptor>,

    /// Default node payload.
    #[serde(default)]
    pub default_data: Value,
}

/// Builder for adapter-facing node schemas.
#[derive(Debug, Clone)]
pub struct NodeSchemaBuilder {
    schema: NodeSchema,
}

/// Error returned when a node cannot be instantiated from schema.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NodeInstantiationError {
    /// The requested node kind is not registered.
    #[error("node kind schema not found: {0:?}")]
    MissingSchema(NodeKindKey),
    /// The caller supplied a different number of port ids than the schema declares.
    #[error("port id count mismatch: expected {expected}, got {actual}")]
    PortIdCountMismatch { expected: usize, actual: usize },
}

/// Concrete graph records produced from a node schema.
#[derive(Debug, Clone)]
pub struct NodeInstantiation {
    /// Allocated node id.
    pub node_id: NodeId,
    /// Node record to add to the graph.
    pub node: Node,
    /// Port records to add to the graph, in schema/UI order.
    pub ports: Vec<(PortId, Port)>,
}

impl NodeInstantiation {
    /// Consumes this instantiation into graph records.
    pub fn into_parts(self) -> (NodeId, Node, Vec<(PortId, Port)>) {
        (self.node_id, self.node, self.ports)
    }

    /// Consumes this instantiation into add-node/add-port operations.
    pub fn into_ops(self) -> Vec<GraphOp> {
        let port_order = self.node.ports.clone();
        let mut node = self.node;
        node.ports = Vec::new();

        let mut ops =
            Vec::with_capacity(self.ports.len() + usize::from(!port_order.is_empty()) + 1);
        ops.push(GraphOp::AddNode {
            id: self.node_id,
            node,
        });
        ops.extend(
            self.ports
                .into_iter()
                .map(|(id, port)| GraphOp::AddPort { id, port }),
        );
        if !port_order.is_empty() {
            ops.push(GraphOp::SetNodePorts {
                id: self.node_id,
                from: Vec::new(),
                to: port_order,
            });
        }
        ops
    }

    /// Consumes this instantiation into an unlabeled graph transaction.
    pub fn into_transaction(self) -> GraphTransaction {
        GraphTransaction::from_ops(self.into_ops())
    }

    /// Consumes this instantiation into a labeled graph transaction.
    pub fn into_labeled_transaction(self, label: impl Into<String>) -> GraphTransaction {
        self.into_transaction().with_label(label)
    }
}

impl NodeSchema {
    /// Starts a node schema builder with renderer-neutral defaults.
    pub fn builder(kind: impl Into<NodeKindKey>, title: impl Into<String>) -> NodeSchemaBuilder {
        NodeSchemaBuilder {
            schema: NodeSchema {
                kind: kind.into(),
                latest_kind_version: 1,
                kind_aliases: Vec::new(),
                title: title.into(),
                category: Vec::new(),
                keywords: Vec::new(),
                renderer_key: None,
                default_size: None,
                layout_budget: NodeSurfaceLayoutBudget::default(),
                ports: Vec::new(),
                surface_slots: Vec::new(),
                repeatable_collections: Vec::new(),
                actions: Vec::new(),
                menus: Vec::new(),
                inspectors: Vec::new(),
                blackboards: Vec::new(),
                chrome: Vec::new(),
                default_data: Value::Null,
            },
        }
    }

    /// Instantiates a node and its declared ports with freshly allocated ids.
    pub fn instantiate(&self, pos: CanvasPoint) -> NodeInstantiation {
        let node_id = NodeId::new();
        let port_ids = std::iter::repeat_with(PortId::new)
            .take(self.ports.len())
            .collect();
        self.instantiate_from_port_ids(node_id, pos, port_ids)
    }

    /// Instantiates a node and its declared ports with caller-provided ids.
    pub fn instantiate_with_ids(
        &self,
        node_id: NodeId,
        pos: CanvasPoint,
        port_ids: impl IntoIterator<Item = PortId>,
    ) -> Result<NodeInstantiation, NodeInstantiationError> {
        let port_ids: Vec<PortId> = port_ids.into_iter().collect();
        if port_ids.len() != self.ports.len() {
            return Err(NodeInstantiationError::PortIdCountMismatch {
                expected: self.ports.len(),
                actual: port_ids.len(),
            });
        }

        Ok(self.instantiate_from_port_ids(node_id, pos, port_ids))
    }

    fn instantiate_from_port_ids(
        &self,
        node_id: NodeId,
        pos: CanvasPoint,
        port_ids: Vec<PortId>,
    ) -> NodeInstantiation {
        let ports = self
            .ports
            .iter()
            .zip(port_ids.iter().copied())
            .map(|(decl, port_id)| (port_id, decl.instantiate(node_id)))
            .collect();

        NodeInstantiation {
            node_id,
            node: Node {
                kind: self.kind.clone(),
                kind_version: self.latest_kind_version,
                pos,
                origin: None,
                selectable: None,
                focusable: None,
                draggable: None,
                connectable: None,
                deletable: None,
                parent: None,
                extent: None,
                expand_parent: None,
                size: self.default_size,
                hidden: false,
                collapsed: false,
                ports: port_ids,
                data: self.default_data.clone(),
            },
            ports,
        }
    }
}

impl NodeSchemaBuilder {
    /// Sets the latest schema version for this node kind.
    pub fn latest_kind_version(mut self, version: u32) -> Self {
        self.schema.latest_kind_version = version;
        self
    }

    /// Adds one alias for this node kind.
    pub fn alias(mut self, alias: impl Into<NodeKindKey>) -> Self {
        self.schema.kind_aliases.push(alias.into());
        self
    }

    /// Adds aliases for this node kind.
    pub fn aliases(mut self, aliases: impl IntoIterator<Item = impl Into<NodeKindKey>>) -> Self {
        self.schema
            .kind_aliases
            .extend(aliases.into_iter().map(Into::into));
        self
    }

    /// Sets the create-node category path.
    pub fn category(mut self, category: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.schema.category = category.into_iter().map(Into::into).collect();
        self
    }

    /// Adds one search keyword.
    pub fn keyword(mut self, keyword: impl Into<String>) -> Self {
        self.schema.keywords.push(keyword.into());
        self
    }

    /// Adds search keywords.
    pub fn keywords(mut self, keywords: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.schema
            .keywords
            .extend(keywords.into_iter().map(Into::into));
        self
    }

    /// Sets the adapter-owned renderer lookup key.
    pub fn renderer_key(mut self, renderer_key: impl Into<String>) -> Self {
        self.schema.renderer_key = Some(renderer_key.into());
        self
    }

    /// Sets the fallback logical node size.
    pub fn default_size(mut self, size: CanvasSize) -> Self {
        self.schema.default_size = Some(size);
        self
    }

    /// Sets the semantic readable-surface budget for adapter-local node internals.
    pub fn layout_budget(mut self, budget: NodeSurfaceLayoutBudget) -> Self {
        self.schema.layout_budget = budget;
        self
    }

    /// Adds one declared port.
    pub fn port(mut self, port: PortDecl) -> Self {
        self.schema.ports.push(port);
        self
    }

    /// Adds declared ports.
    pub fn ports(mut self, ports: impl IntoIterator<Item = PortDecl>) -> Self {
        self.schema.ports.extend(ports);
        self
    }

    /// Adds one renderer-neutral node surface slot.
    pub fn surface_slot(mut self, slot: NodeSurfaceSlotDescriptor) -> Self {
        self.schema.surface_slots.push(slot);
        self
    }

    /// Adds renderer-neutral node surface slots.
    pub fn surface_slots(
        mut self,
        slots: impl IntoIterator<Item = NodeSurfaceSlotDescriptor>,
    ) -> Self {
        self.schema.surface_slots.extend(slots);
        self
    }

    /// Adds one renderer-neutral repeatable collection descriptor.
    pub fn repeatable_collection(mut self, collection: NodeRepeatableCollectionDescriptor) -> Self {
        self.schema.repeatable_collections.push(collection);
        self
    }

    /// Adds renderer-neutral repeatable collection descriptors.
    pub fn repeatable_collections(
        mut self,
        collections: impl IntoIterator<Item = NodeRepeatableCollectionDescriptor>,
    ) -> Self {
        self.schema.repeatable_collections.extend(collections);
        self
    }

    /// Adds one renderer-neutral action descriptor.
    pub fn action(mut self, action: NodeActionDescriptor) -> Self {
        self.schema.actions.push(action);
        self
    }

    /// Adds renderer-neutral action descriptors.
    pub fn actions(mut self, actions: impl IntoIterator<Item = NodeActionDescriptor>) -> Self {
        self.schema.actions.extend(actions);
        self
    }

    /// Adds one renderer-neutral menu descriptor.
    pub fn menu(mut self, menu: MenuDescriptor) -> Self {
        self.schema.menus.push(menu);
        self
    }

    /// Adds renderer-neutral menu descriptors.
    pub fn menus(mut self, menus: impl IntoIterator<Item = MenuDescriptor>) -> Self {
        self.schema.menus.extend(menus);
        self
    }

    /// Adds one renderer-neutral inspector descriptor.
    pub fn inspector(mut self, inspector: InspectorDescriptor) -> Self {
        self.schema.inspectors.push(inspector);
        self
    }

    /// Adds renderer-neutral inspector descriptors.
    pub fn inspectors(mut self, inspectors: impl IntoIterator<Item = InspectorDescriptor>) -> Self {
        self.schema.inspectors.extend(inspectors);
        self
    }

    /// Adds one renderer-neutral blackboard descriptor.
    pub fn blackboard(mut self, blackboard: BlackboardDescriptor) -> Self {
        self.schema.blackboards.push(blackboard);
        self
    }

    /// Adds renderer-neutral blackboard descriptors.
    pub fn blackboards(
        mut self,
        blackboards: impl IntoIterator<Item = BlackboardDescriptor>,
    ) -> Self {
        self.schema.blackboards.extend(blackboards);
        self
    }

    /// Adds one renderer-neutral node chrome descriptor.
    pub fn chrome(mut self, chrome: NodeChromeDescriptor) -> Self {
        self.schema.chrome.push(chrome);
        self
    }

    /// Adds renderer-neutral node chrome descriptors.
    pub fn chromes(mut self, chromes: impl IntoIterator<Item = NodeChromeDescriptor>) -> Self {
        self.schema.chrome.extend(chromes);
        self
    }

    /// Sets the default node payload.
    pub fn default_data(mut self, data: Value) -> Self {
        self.schema.default_data = data;
        self
    }

    /// Builds the schema.
    pub fn build(self) -> NodeSchema {
        self.schema
    }
}

impl From<NodeSchemaBuilder> for NodeSchema {
    fn from(value: NodeSchemaBuilder) -> Self {
        value.build()
    }
}

impl PortDecl {
    /// Creates a port declaration.
    pub fn new(
        key: impl Into<PortKey>,
        dir: PortDirection,
        kind: PortKind,
        capacity: PortCapacity,
    ) -> Self {
        Self {
            key: key.into(),
            dir,
            kind,
            capacity,
            ty: None,
            label: None,
            view: PortViewDescriptor::default(),
        }
    }

    /// Creates a single-capacity data input port.
    pub fn data_input(key: impl Into<PortKey>) -> Self {
        Self::new(key, PortDirection::In, PortKind::Data, PortCapacity::Single)
    }

    /// Creates a multi-capacity data output port.
    pub fn data_output(key: impl Into<PortKey>) -> Self {
        Self::new(key, PortDirection::Out, PortKind::Data, PortCapacity::Multi)
    }

    /// Sets the port type descriptor.
    pub fn with_type(mut self, ty: TypeDesc) -> Self {
        self.ty = Some(ty);
        self
    }

    /// Sets the adapter-facing label.
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Sets the adapter-facing port view descriptor.
    pub fn with_view(mut self, view: PortViewDescriptor) -> Self {
        self.view = view;
        self
    }

    /// Places this port on the top side.
    pub fn on_top(self) -> Self {
        self.with_view(PortViewDescriptor::top())
    }

    /// Places this port on the right side.
    pub fn on_right(self) -> Self {
        self.with_view(PortViewDescriptor::right())
    }

    /// Places this port on the bottom side.
    pub fn on_bottom(self) -> Self {
        self.with_view(PortViewDescriptor::bottom())
    }

    /// Places this port on the left side.
    pub fn on_left(self) -> Self {
        self.with_view(PortViewDescriptor::left())
    }

    /// Sets deterministic ordering within the selected side/group.
    pub fn with_view_order(mut self, order: i32) -> Self {
        self.view.order = Some(order);
        self
    }

    /// Groups this port with related handles for adapter presentation.
    pub fn with_view_group(mut self, group: impl Into<String>) -> Self {
        self.view.group = Some(group.into());
        self
    }

    /// Anchors this port to an adapter-owned region such as a field row.
    pub fn with_view_anchor(mut self, anchor: impl Into<String>) -> Self {
        self.view.anchor = Some(anchor.into());
        self
    }

    /// Hides this handle from adapter hit testing without removing the semantic port.
    pub fn hidden_handle(mut self) -> Self {
        self.view.visibility = Some(PortHandleVisibility::Hidden);
        self
    }

    /// Sets the capacity.
    pub fn with_capacity(mut self, capacity: PortCapacity) -> Self {
        self.capacity = capacity;
        self
    }

    fn instantiate(&self, node: NodeId) -> Port {
        Port {
            node,
            key: self.key.clone(),
            dir: self.dir,
            kind: self.kind,
            capacity: self.capacity,
            connectable: None,
            connectable_start: None,
            connectable_end: None,
            ty: self.ty.clone(),
            data: Value::Null,
        }
    }
}

/// Renderer-neutral node-kind descriptor for adapter palettes and renderer lookup.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeKindViewDescriptor {
    /// Canonical kind key.
    pub kind: NodeKindKey,
    /// Adapter-owned renderer lookup key.
    pub renderer_key: String,
    /// UI-facing title.
    pub title: String,
    /// Category path for create-node search/palette grouping.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub category: Vec<String>,
    /// Search keywords.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
    /// Default logical node size for initial adapter layout before measurement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_size: Option<CanvasSize>,
    /// Semantic readable-surface budget for adapter-local node internals.
    #[serde(default, skip_serializing_if = "node_surface_layout_budget_is_empty")]
    pub layout_budget: NodeSurfaceLayoutBudget,
    /// Declared ports.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ports: Vec<PortDecl>,
    /// Renderer-neutral semantic slots for rich adapter node surfaces.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub surface_slots: Vec<NodeSurfaceSlotDescriptor>,
    /// Renderer-neutral dynamic row/list descriptors for node-local authoring surfaces.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repeatable_collections: Vec<NodeRepeatableCollectionDescriptor>,
    /// Renderer-neutral action descriptors for node-local authoring surfaces.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<NodeActionDescriptor>,
    /// Renderer-neutral menu descriptors. Adapters own popup widgets and state.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub menus: Vec<MenuDescriptor>,
    /// Renderer-neutral inspector descriptors. Adapters own panel widgets and focus.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inspectors: Vec<InspectorDescriptor>,
    /// Graph-level property lists exposed as adapter-local blackboard panels.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blackboards: Vec<BlackboardDescriptor>,
    /// Renderer-neutral semantic chrome around rich adapter node surfaces.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chrome: Vec<NodeChromeDescriptor>,
    /// Default node payload.
    #[serde(default)]
    pub default_data: Value,
}

impl NodeKindViewDescriptor {
    pub(crate) fn from_schema(schema: &NodeSchema) -> Self {
        Self {
            kind: schema.kind.clone(),
            renderer_key: schema
                .renderer_key
                .clone()
                .unwrap_or_else(|| schema.kind.0.clone()),
            title: schema.title.clone(),
            category: schema.category.clone(),
            keywords: schema.keywords.clone(),
            default_size: schema.default_size,
            layout_budget: schema.layout_budget.clone(),
            ports: schema.ports.clone(),
            surface_slots: schema.surface_slots.clone(),
            repeatable_collections: schema.repeatable_collections.clone(),
            actions: schema.actions.clone(),
            menus: schema.menus.clone(),
            inspectors: schema.inspectors.clone(),
            blackboards: schema.blackboards.clone(),
            chrome: schema.chrome.clone(),
            default_data: schema.default_data.clone(),
        }
    }

    pub fn port_decl(&self, key: impl AsRef<str>) -> Option<&PortDecl> {
        let key = key.as_ref();
        self.ports.iter().find(|decl| decl.key.0 == key)
    }

    pub fn ports_by_anchor(&self, anchor: impl AsRef<str>) -> Vec<&PortDecl> {
        let anchor = anchor.as_ref();
        let mut ports: Vec<_> = self
            .ports
            .iter()
            .filter(|decl| decl.view.anchor.as_deref() == Some(anchor))
            .collect();
        ports.sort_by(|a, b| {
            a.view
                .order
                .unwrap_or(i32::MAX)
                .cmp(&b.view.order.unwrap_or(i32::MAX))
                .then_with(|| a.key.cmp(&b.key))
        });
        ports
    }

    pub fn port_decl_by_anchor(&self, anchor: impl AsRef<str>) -> Option<&PortDecl> {
        self.ports_by_anchor(anchor).into_iter().next()
    }

    pub fn surface_slot(&self, key: impl AsRef<str>) -> Option<&NodeSurfaceSlotDescriptor> {
        let key = key.as_ref();
        self.surface_slots.iter().find(|slot| slot.key == key)
    }

    pub fn surface_slots_of_kind(
        &self,
        kind: NodeSurfaceSlotKind,
    ) -> Vec<&NodeSurfaceSlotDescriptor> {
        let mut slots: Vec<_> = self
            .surface_slots
            .iter()
            .filter(|slot| slot.kind == kind)
            .collect();
        slots.sort_by(|a, b| {
            a.order_key()
                .cmp(&b.order_key())
                .then_with(|| a.key.cmp(&b.key))
        });
        slots
    }

    pub fn surface_slots_by_anchor(
        &self,
        anchor: impl AsRef<str>,
    ) -> Vec<&NodeSurfaceSlotDescriptor> {
        let anchor = anchor.as_ref();
        let mut slots: Vec<_> = self
            .surface_slots
            .iter()
            .filter(|slot| slot.anchor.as_deref() == Some(anchor))
            .collect();
        slots.sort_by(|a, b| {
            a.order_key()
                .cmp(&b.order_key())
                .then_with(|| a.key.cmp(&b.key))
        });
        slots
    }

    pub fn surface_slot_by_anchor(
        &self,
        anchor: impl AsRef<str>,
    ) -> Option<&NodeSurfaceSlotDescriptor> {
        self.surface_slots_by_anchor(anchor).into_iter().next()
    }

    pub fn repeatable_collection(
        &self,
        key: impl AsRef<str>,
    ) -> Option<&NodeRepeatableCollectionDescriptor> {
        let key = key.as_ref();
        self.repeatable_collections
            .iter()
            .find(|collection| collection.key == key)
    }

    pub fn repeatable_items_projection(
        &self,
        node_data: &Value,
        collection_key: impl AsRef<str>,
    ) -> Vec<NodeRepeatableItemProjection> {
        self.repeatable_collection(collection_key)
            .map(|collection| collection.item_projections(node_data))
            .unwrap_or_default()
    }

    pub fn action(&self, key: impl AsRef<str>) -> Option<&NodeActionDescriptor> {
        let key = key.as_ref();
        self.actions.iter().find(|action| action.key == key)
    }

    pub fn menu(&self, key: impl AsRef<str>) -> Option<&MenuDescriptor> {
        let key = key.as_ref();
        self.menus.iter().find(|menu| menu.key == key)
    }

    pub fn inspector(&self, key: impl AsRef<str>) -> Option<&InspectorDescriptor> {
        let key = key.as_ref();
        self.inspectors
            .iter()
            .find(|inspector| inspector.key == key)
    }

    pub fn blackboard(&self, key: impl AsRef<str>) -> Option<&BlackboardDescriptor> {
        let key = key.as_ref();
        self.blackboards
            .iter()
            .find(|blackboard| blackboard.key == key)
    }

    pub fn surface_slots_projection(
        &self,
        node_data: &Value,
        layout_hints: Option<&NodeKitLayoutHints>,
        zoom: f32,
    ) -> Vec<NodeSurfaceSlotProjection> {
        let projection = layout_hints
            .map(|layout_hints| NodeSurfaceProjection::from_layout_hints(layout_hints, zoom))
            .unwrap_or_else(|| {
                NodeSurfaceProjection::from_layout_hints(&NodeKitLayoutHints::default(), zoom)
            });

        let mut slots = self
            .surface_slots
            .iter()
            .filter(|slot| slot.is_visible())
            .map(|slot| {
                NodeSurfaceSlotProjection::from_descriptor(
                    node_data,
                    slot,
                    projection.compact_values,
                )
            })
            .collect::<Vec<_>>();

        if !projection.expand_all_slots && slots.len() > projection.slot_limit {
            slots.truncate(projection.slot_limit);
        }

        slots
    }
}

fn slot_value_preview(
    node_data: &Value,
    slot: &NodeSurfaceSlotDescriptor,
    compact_values: bool,
) -> Option<String> {
    let value = semantic_slot_value(node_data, slot)?;
    let preview = json_value_preview(value, compact_values);
    (!preview.is_empty()).then_some(preview)
}

fn semantic_slot_value<'a>(
    node_data: &'a Value,
    slot: &NodeSurfaceSlotDescriptor,
) -> Option<&'a Value> {
    let key = slot.data_key()?;
    if slot.kind == NodeSurfaceSlotKind::FieldRow
        && let Some(fields) = node_data.get("fields").and_then(Value::as_object)
        && let Some(value) = fields.get(key)
    {
        return Some(value);
    }
    semantic_json_lookup(node_data, key)
}

fn semantic_json_lookup<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = value;
    for segment in path.split('.') {
        current = current.get(segment)?;
    }
    Some(current)
}

fn json_value_preview(value: &Value, compact_values: bool) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::Array(items) => {
            let preview = items
                .iter()
                .take(if compact_values { 1 } else { 2 })
                .map(|value| json_value_preview(value, compact_values))
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join(" · ");
            if preview.is_empty() {
                format!("{} items", items.len())
            } else if !compact_values && items.len() > 2 {
                format!("{preview} …")
            } else {
                preview
            }
        }
        Value::Object(map) => {
            if let Some(text) = map.get("label").and_then(Value::as_str) {
                return text.to_owned();
            }
            if let Some(text) = map.get("title").and_then(Value::as_str) {
                return text.to_owned();
            }
            let preview = map
                .iter()
                .take(if compact_values { 1 } else { 2 })
                .map(|(key, value)| format!("{key}: {}", json_value_preview(value, compact_values)))
                .collect::<Vec<_>>()
                .join(" · ");
            if preview.is_empty() {
                "{}".to_owned()
            } else {
                preview
            }
        }
        Value::Null => String::new(),
    }
}
