use jellyflow::{
    NodeGraphStore,
    core::{Node, NodeId},
    runtime::schema::{ActionIntent, NodeRegistry},
};
use serde_json::{Number, Value};

use crate::{
    OpenGpuiActionDispatchPlan, OpenGpuiControlEditPlan, OpenGpuiControlOptionPlan,
    OpenGpuiControlPlan, OpenGpuiControlPrimitive, OpenGpuiControlSupport, OpenGpuiInspectorPlan,
    OpenGpuiMenuPlan, OpenGpuiRepeatableActionPlan, OpenGpuiRepeatableEditError,
    OpenGpuiRepeatableEditPlan, plan_action_dispatch, plan_control_edit,
    plan_inspector_control_edit, plan_repeatable_action,
};

/// Stateless adapter-local entry point for turning GPUI component events into semantic outcomes.
#[derive(Debug, Clone, Copy, Default)]
pub struct OpenGpuiAuthoringController;

/// Typed value emitted by a local Open GPUI component.
#[derive(Debug, Clone, PartialEq)]
pub enum OpenGpuiControlEventValue {
    Text(String),
    Number(f64),
    Bool(bool),
    Json(Value),
    SelectOptionKey(String),
}

/// Host hook context for creating product-specific default repeatable items.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenGpuiRepeatableAddItemContext {
    pub action_key: String,
    pub collection_key: String,
    pub item_count: usize,
}

/// Adapter authoring outcome. `Skipped` is explicit so GPUI callers do not infer intent from
/// `None`.
#[derive(Debug, Clone, PartialEq)]
pub enum OpenGpuiAuthoringOutcome<T> {
    Planned(T),
    Skipped(OpenGpuiAuthoringSkipReason),
}

impl<T> OpenGpuiAuthoringOutcome<T> {
    pub fn is_planned(&self) -> bool {
        matches!(self, Self::Planned(_))
    }

    pub fn skip_reason(&self) -> Option<&OpenGpuiAuthoringSkipReason> {
        match self {
            Self::Planned(_) => None,
            Self::Skipped(reason) => Some(reason),
        }
    }

    pub fn into_plan(self) -> Option<T> {
        match self {
            Self::Planned(plan) => Some(plan),
            Self::Skipped(_) => None,
        }
    }
}

/// Reason a GPUI component event was intentionally ignored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenGpuiAuthoringSkipReason {
    DisabledControl {
        control_key: String,
        reason: String,
    },
    ReadOnlyControl {
        control_key: String,
    },
    StubControl {
        control_key: String,
        primitive: OpenGpuiControlPrimitive,
    },
    UnsupportedControl {
        control_key: String,
        support: OpenGpuiControlSupport,
    },
    NoWritableBinding {
        control_key: String,
    },
    UnchangedControl {
        control_key: String,
    },
    MissingSelectOption {
        control_key: String,
        option_key: String,
    },
    DisabledSelectOption {
        control_key: String,
        option_key: String,
    },
    MissingInspectorControl {
        inspector_key: String,
        control_key: String,
    },
    MissingAction {
        menu_key: String,
        action_key: String,
    },
    DisabledAction {
        action_key: String,
        reason: Option<String>,
    },
    MissingNode {
        node_id: String,
    },
    MissingNodeDescriptor {
        node_kind: String,
    },
    MissingActionNodeTarget {
        action_key: String,
    },
    MissingRepeatableCollection {
        action_key: String,
        collection_key: String,
    },
    MissingRepeatableReorderTarget {
        action_key: String,
        collection_key: String,
        item_id: String,
    },
    UnsupportedActionIntent {
        action_key: String,
        intent: ActionIntent,
    },
}

impl OpenGpuiAuthoringController {
    pub fn plan_control_event(
        &self,
        node_id: NodeId,
        node: &Node,
        control: &OpenGpuiControlPlan,
        event: OpenGpuiControlEventValue,
    ) -> Result<OpenGpuiAuthoringOutcome<OpenGpuiControlEditPlan>, String> {
        if let Some(reason) = skip_control_before_value(control) {
            return Ok(OpenGpuiAuthoringOutcome::Skipped(reason));
        }
        let value = match event {
            OpenGpuiControlEventValue::Text(value) => Value::String(value),
            OpenGpuiControlEventValue::Number(value) => json_number(value, &control.key)?,
            OpenGpuiControlEventValue::Bool(value) => Value::Bool(value),
            OpenGpuiControlEventValue::Json(value) => value,
            OpenGpuiControlEventValue::SelectOptionKey(option_key) => {
                return match resolve_select_option(control, &option_key) {
                    OpenGpuiAuthoringOutcome::Planned(value) => {
                        self.plan_control_value_edit(node_id, node, control, value)
                    }
                    OpenGpuiAuthoringOutcome::Skipped(reason) => {
                        Ok(OpenGpuiAuthoringOutcome::Skipped(reason))
                    }
                };
            }
        };
        self.plan_control_value_edit(node_id, node, control, value)
    }

    pub fn plan_store_control_event(
        &self,
        store: &NodeGraphStore,
        registry: &NodeRegistry,
        node_id: NodeId,
        control: &OpenGpuiControlPlan,
        event: OpenGpuiControlEventValue,
    ) -> Result<OpenGpuiAuthoringOutcome<OpenGpuiControlEditPlan>, String> {
        let Some(node) = store.graph().nodes().get(&node_id) else {
            return Ok(OpenGpuiAuthoringOutcome::Skipped(
                OpenGpuiAuthoringSkipReason::MissingNode {
                    node_id: format!("{node_id:?}"),
                },
            ));
        };
        if registry.view_descriptor(&node.kind).is_none() {
            return Ok(OpenGpuiAuthoringOutcome::Skipped(
                OpenGpuiAuthoringSkipReason::MissingNodeDescriptor {
                    node_kind: node.kind.0.clone(),
                },
            ));
        }
        self.plan_control_event(node_id, node, control, event)
    }

    pub fn plan_control_value_edit(
        &self,
        node_id: NodeId,
        node: &Node,
        control: &OpenGpuiControlPlan,
        value: Value,
    ) -> Result<OpenGpuiAuthoringOutcome<OpenGpuiControlEditPlan>, String> {
        if let Some(reason) = skip_control_before_value(control) {
            return Ok(OpenGpuiAuthoringOutcome::Skipped(reason));
        }
        match plan_control_edit(node_id, node, control, value)? {
            Some(plan) => Ok(OpenGpuiAuthoringOutcome::Planned(plan)),
            None => Ok(OpenGpuiAuthoringOutcome::Skipped(
                OpenGpuiAuthoringSkipReason::UnchangedControl {
                    control_key: control.key.clone(),
                },
            )),
        }
    }

    pub fn plan_control_text_edit(
        &self,
        node_id: NodeId,
        node: &Node,
        control: &OpenGpuiControlPlan,
        value: impl Into<String>,
    ) -> Result<OpenGpuiAuthoringOutcome<OpenGpuiControlEditPlan>, String> {
        self.plan_control_event(
            node_id,
            node,
            control,
            OpenGpuiControlEventValue::Text(value.into()),
        )
    }

    pub fn plan_control_number_edit(
        &self,
        node_id: NodeId,
        node: &Node,
        control: &OpenGpuiControlPlan,
        value: f64,
    ) -> Result<OpenGpuiAuthoringOutcome<OpenGpuiControlEditPlan>, String> {
        self.plan_control_event(
            node_id,
            node,
            control,
            OpenGpuiControlEventValue::Number(value),
        )
    }

    pub fn plan_control_bool_edit(
        &self,
        node_id: NodeId,
        node: &Node,
        control: &OpenGpuiControlPlan,
        value: bool,
    ) -> Result<OpenGpuiAuthoringOutcome<OpenGpuiControlEditPlan>, String> {
        self.plan_control_event(
            node_id,
            node,
            control,
            OpenGpuiControlEventValue::Bool(value),
        )
    }

    pub fn plan_control_select_edit(
        &self,
        node_id: NodeId,
        node: &Node,
        control: &OpenGpuiControlPlan,
        option_key: impl Into<String>,
    ) -> Result<OpenGpuiAuthoringOutcome<OpenGpuiControlEditPlan>, String> {
        self.plan_control_event(
            node_id,
            node,
            control,
            OpenGpuiControlEventValue::SelectOptionKey(option_key.into()),
        )
    }

    pub fn plan_inspector_control_event(
        &self,
        node_id: NodeId,
        node: &Node,
        inspector: &OpenGpuiInspectorPlan,
        control_key: &str,
        event: OpenGpuiControlEventValue,
    ) -> Result<OpenGpuiAuthoringOutcome<OpenGpuiControlEditPlan>, String> {
        let Some(control) = inspector
            .controls
            .iter()
            .find(|control| control.key == control_key)
        else {
            return Ok(OpenGpuiAuthoringOutcome::Skipped(
                OpenGpuiAuthoringSkipReason::MissingInspectorControl {
                    inspector_key: inspector.key.clone(),
                    control_key: control_key.to_owned(),
                },
            ));
        };
        if let Some(reason) = skip_control_before_value(control) {
            return Ok(OpenGpuiAuthoringOutcome::Skipped(reason));
        }
        let value = match event {
            OpenGpuiControlEventValue::Text(value) => Value::String(value),
            OpenGpuiControlEventValue::Number(value) => json_number(value, &control.key)?,
            OpenGpuiControlEventValue::Bool(value) => Value::Bool(value),
            OpenGpuiControlEventValue::Json(value) => value,
            OpenGpuiControlEventValue::SelectOptionKey(option_key) => {
                return match resolve_select_option(control, &option_key) {
                    OpenGpuiAuthoringOutcome::Planned(value) => self
                        .plan_inspector_control_value_edit(
                            node_id,
                            node,
                            inspector,
                            control_key,
                            value,
                        ),
                    OpenGpuiAuthoringOutcome::Skipped(reason) => {
                        Ok(OpenGpuiAuthoringOutcome::Skipped(reason))
                    }
                };
            }
        };
        self.plan_inspector_control_value_edit(node_id, node, inspector, control_key, value)
    }

    pub fn plan_inspector_control_value_edit(
        &self,
        node_id: NodeId,
        node: &Node,
        inspector: &OpenGpuiInspectorPlan,
        control_key: &str,
        value: Value,
    ) -> Result<OpenGpuiAuthoringOutcome<OpenGpuiControlEditPlan>, String> {
        let Some(control) = inspector
            .controls
            .iter()
            .find(|control| control.key == control_key)
        else {
            return Ok(OpenGpuiAuthoringOutcome::Skipped(
                OpenGpuiAuthoringSkipReason::MissingInspectorControl {
                    inspector_key: inspector.key.clone(),
                    control_key: control_key.to_owned(),
                },
            ));
        };
        if let Some(reason) = skip_control_before_value(control) {
            return Ok(OpenGpuiAuthoringOutcome::Skipped(reason));
        }
        match plan_inspector_control_edit(node_id, node, inspector, control_key, value)? {
            Some(plan) => Ok(OpenGpuiAuthoringOutcome::Planned(plan)),
            None => Ok(OpenGpuiAuthoringOutcome::Skipped(
                OpenGpuiAuthoringSkipReason::UnchangedControl {
                    control_key: control.key.clone(),
                },
            )),
        }
    }

    pub fn plan_menu_action_dispatch(
        &self,
        menu: &OpenGpuiMenuPlan,
        action_key: &str,
    ) -> OpenGpuiAuthoringOutcome<OpenGpuiActionDispatchPlan> {
        let Some(action) = menu.actions.iter().find(|action| action.key == action_key) else {
            return OpenGpuiAuthoringOutcome::Skipped(OpenGpuiAuthoringSkipReason::MissingAction {
                menu_key: menu.key.clone(),
                action_key: action_key.to_owned(),
            });
        };
        if !action.dispatchable() {
            return OpenGpuiAuthoringOutcome::Skipped(
                OpenGpuiAuthoringSkipReason::DisabledAction {
                    action_key: action.key.clone(),
                    reason: action.disabled_reason.clone(),
                },
            );
        }
        match plan_action_dispatch(menu, action_key) {
            Some(plan) => OpenGpuiAuthoringOutcome::Planned(plan),
            None => OpenGpuiAuthoringOutcome::Skipped(OpenGpuiAuthoringSkipReason::MissingAction {
                menu_key: menu.key.clone(),
                action_key: action_key.to_owned(),
            }),
        }
    }

    pub fn plan_repeatable_action_dispatch<F>(
        &self,
        store: &NodeGraphStore,
        registry: &NodeRegistry,
        node_id: Option<NodeId>,
        dispatch: &OpenGpuiActionDispatchPlan,
        mut add_item: F,
    ) -> Result<OpenGpuiAuthoringOutcome<OpenGpuiRepeatableActionPlan>, String>
    where
        F: FnMut(&OpenGpuiRepeatableAddItemContext) -> Option<Value>,
    {
        match &dispatch.intent {
            ActionIntent::AddRepeatableItem { collection_key } => {
                let Some(node_id) = node_id else {
                    return Ok(OpenGpuiAuthoringOutcome::Skipped(
                        OpenGpuiAuthoringSkipReason::MissingActionNodeTarget {
                            action_key: dispatch.action_key.clone(),
                        },
                    ));
                };
                let Some((node, descriptor)) = current_node_descriptor(store, registry, node_id)?
                else {
                    return Ok(missing_node_or_descriptor_outcome(store, registry, node_id));
                };
                let Some(collection) = descriptor.repeatable_collection(collection_key) else {
                    return Ok(OpenGpuiAuthoringOutcome::Skipped(
                        OpenGpuiAuthoringSkipReason::MissingRepeatableCollection {
                            action_key: dispatch.action_key.clone(),
                            collection_key: collection_key.clone(),
                        },
                    ));
                };
                let context = OpenGpuiRepeatableAddItemContext {
                    action_key: dispatch.action_key.clone(),
                    collection_key: collection_key.clone(),
                    item_count: collection.item_projections(&node.data).len(),
                };
                let item = add_item(&context).unwrap_or_else(|| Value::Object(Default::default()));
                Ok(OpenGpuiAuthoringOutcome::Planned(
                    OpenGpuiRepeatableActionPlan::Add {
                        collection_key: collection_key.clone(),
                        item,
                    },
                ))
            }
            ActionIntent::RemoveRepeatableItem {
                collection_key,
                item_id,
            } => {
                let Some(node_id) = node_id else {
                    return Ok(OpenGpuiAuthoringOutcome::Skipped(
                        OpenGpuiAuthoringSkipReason::MissingActionNodeTarget {
                            action_key: dispatch.action_key.clone(),
                        },
                    ));
                };
                let Some((_node, descriptor)) = current_node_descriptor(store, registry, node_id)?
                else {
                    return Ok(missing_node_or_descriptor_outcome(store, registry, node_id));
                };
                if descriptor.repeatable_collection(collection_key).is_none() {
                    return Ok(OpenGpuiAuthoringOutcome::Skipped(
                        OpenGpuiAuthoringSkipReason::MissingRepeatableCollection {
                            action_key: dispatch.action_key.clone(),
                            collection_key: collection_key.clone(),
                        },
                    ));
                }
                Ok(OpenGpuiAuthoringOutcome::Planned(
                    OpenGpuiRepeatableActionPlan::Remove {
                        collection_key: collection_key.clone(),
                        item_id: item_id.clone(),
                    },
                ))
            }
            ActionIntent::ReorderRepeatableItem {
                collection_key,
                item_id,
            } => {
                if node_id.is_none() {
                    return Ok(OpenGpuiAuthoringOutcome::Skipped(
                        OpenGpuiAuthoringSkipReason::MissingActionNodeTarget {
                            action_key: dispatch.action_key.clone(),
                        },
                    ));
                }
                Ok(OpenGpuiAuthoringOutcome::Skipped(
                    OpenGpuiAuthoringSkipReason::MissingRepeatableReorderTarget {
                        action_key: dispatch.action_key.clone(),
                        collection_key: collection_key.clone(),
                        item_id: item_id.clone(),
                    },
                ))
            }
            intent => Ok(OpenGpuiAuthoringOutcome::Skipped(
                OpenGpuiAuthoringSkipReason::UnsupportedActionIntent {
                    action_key: dispatch.action_key.clone(),
                    intent: intent.clone(),
                },
            )),
        }
    }

    pub fn apply_repeatable_action_to_store(
        &self,
        store: &mut NodeGraphStore,
        registry: &NodeRegistry,
        node_id: NodeId,
        action: OpenGpuiRepeatableActionPlan,
    ) -> Result<Option<OpenGpuiRepeatableEditPlan>, OpenGpuiRepeatableEditError> {
        let node = store
            .graph()
            .nodes()
            .get(&node_id)
            .cloned()
            .ok_or_else(|| {
                OpenGpuiRepeatableEditError::InvalidEdit(format!("missing node {node_id:?}"))
            })?;
        let descriptor = registry.view_descriptor(&node.kind).ok_or_else(|| {
            OpenGpuiRepeatableEditError::InvalidEdit(format!(
                "missing descriptor for node kind `{}`",
                node.kind.0
            ))
        })?;
        let Some(plan) =
            plan_repeatable_action(&descriptor, store.graph(), node_id, &node, action)?
        else {
            return Ok(None);
        };

        store
            .dispatch_transaction(&plan.transaction)
            .map_err(|error| OpenGpuiRepeatableEditError::InvalidEdit(error.to_string()))?;
        store.invalidate_node_internals(plan.invalidation.clone());
        Ok(Some(plan))
    }
}

pub fn control_option_value_key(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| value.to_string())
}

pub fn control_option_key(option: &OpenGpuiControlOptionPlan) -> String {
    control_option_value_key(&option.value)
}

pub fn control_selected_option_key(control: &OpenGpuiControlPlan) -> Option<String> {
    control.value.as_ref().map(control_option_value_key)
}

fn resolve_select_option(
    control: &OpenGpuiControlPlan,
    option_key: &str,
) -> OpenGpuiAuthoringOutcome<Value> {
    let Some(option) = control
        .options
        .iter()
        .find(|option| control_option_key(option) == option_key)
    else {
        return OpenGpuiAuthoringOutcome::Skipped(
            OpenGpuiAuthoringSkipReason::MissingSelectOption {
                control_key: control.key.clone(),
                option_key: option_key.to_owned(),
            },
        );
    };
    if option.disabled {
        return OpenGpuiAuthoringOutcome::Skipped(
            OpenGpuiAuthoringSkipReason::DisabledSelectOption {
                control_key: control.key.clone(),
                option_key: option_key.to_owned(),
            },
        );
    }
    OpenGpuiAuthoringOutcome::Planned(option.value.clone())
}

fn skip_control_before_value(control: &OpenGpuiControlPlan) -> Option<OpenGpuiAuthoringSkipReason> {
    if let Some(reason) = &control.disabled_reason {
        return Some(OpenGpuiAuthoringSkipReason::DisabledControl {
            control_key: control.key.clone(),
            reason: reason.clone(),
        });
    }
    if control.read_only {
        return Some(OpenGpuiAuthoringSkipReason::ReadOnlyControl {
            control_key: control.key.clone(),
        });
    }
    if control.primitive.is_stub() {
        return Some(OpenGpuiAuthoringSkipReason::StubControl {
            control_key: control.key.clone(),
            primitive: control.primitive,
        });
    }
    if matches!(control.support, OpenGpuiControlSupport::Unsupported) {
        return Some(OpenGpuiAuthoringSkipReason::UnsupportedControl {
            control_key: control.key.clone(),
            support: control.support,
        });
    }
    if control.binding.is_none() {
        return Some(OpenGpuiAuthoringSkipReason::NoWritableBinding {
            control_key: control.key.clone(),
        });
    }
    None
}

fn json_number(value: f64, control_key: &str) -> Result<Value, String> {
    Number::from_f64(value)
        .map(Value::Number)
        .ok_or_else(|| format!("control `{control_key}` received a non-finite number"))
}

fn current_node_descriptor<'a>(
    store: &'a NodeGraphStore,
    registry: &NodeRegistry,
    node_id: NodeId,
) -> Result<Option<(&'a Node, jellyflow::runtime::schema::NodeKindViewDescriptor)>, String> {
    let Some(node) = store.graph().nodes().get(&node_id) else {
        return Ok(None);
    };
    let Some(descriptor) = registry.view_descriptor(&node.kind) else {
        return Ok(None);
    };
    Ok(Some((node, descriptor)))
}

fn missing_node_or_descriptor_outcome<T>(
    store: &NodeGraphStore,
    registry: &NodeRegistry,
    node_id: NodeId,
) -> OpenGpuiAuthoringOutcome<T> {
    let Some(node) = store.graph().nodes().get(&node_id) else {
        return OpenGpuiAuthoringOutcome::Skipped(OpenGpuiAuthoringSkipReason::MissingNode {
            node_id: format!("{node_id:?}"),
        });
    };
    if registry.view_descriptor(&node.kind).is_none() {
        return OpenGpuiAuthoringOutcome::Skipped(
            OpenGpuiAuthoringSkipReason::MissingNodeDescriptor {
                node_kind: node.kind.0.clone(),
            },
        );
    }
    OpenGpuiAuthoringOutcome::Skipped(OpenGpuiAuthoringSkipReason::MissingNode {
        node_id: format!("{node_id:?}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use jellyflow::{
        NodeGraphStore,
        core::{CanvasPoint, CanvasSize, Graph, GraphId, GraphOp, GraphTransaction, NodeKindKey},
        runtime::{
            io::{NodeGraphEditorConfig, NodeGraphViewState},
            runtime::create_node::CreateNodeRequest,
            runtime::measurement::NodeInternalsInvalidationReason,
            schema::{
                ActionIntent, ActionTarget, InspectorTarget, MenuSurface, NodeControlBinding,
                NodeControlDescriptor, NodeControlOption, NodeKitRegistry,
            },
        },
    };
    use serde_json::json;

    use crate::{
        OpenGpuiActionPlan, OpenGpuiControlProjectionContext, OpenGpuiMenuPlan, project_control,
    };

    #[test]
    fn text_input_and_textarea_events_plan_node_data_edits() {
        let controller = OpenGpuiAuthoringController;
        let node_id = NodeId::from_u128(1);
        let node = test_node(json!({
            "fields": {
                "title": "Old",
                "prompt": "Summarize"
            }
        }));
        let title = project_control(
            &node.data,
            &NodeControlDescriptor::text_input("control.title")
                .with_binding(NodeControlBinding::data_path("fields.title")),
            OpenGpuiControlProjectionContext::default(),
        );
        let prompt = project_control(
            &node.data,
            &NodeControlDescriptor::text_area("control.prompt")
                .with_binding(NodeControlBinding::data_path("fields.prompt")),
            OpenGpuiControlProjectionContext::default(),
        );

        let title_plan = controller
            .plan_control_text_edit(node_id, &node, &title, "New")
            .expect("title edit")
            .into_plan()
            .expect("title plan");
        assert_node_data_value(&title_plan, ["fields", "title"], json!("New"));

        let prompt_plan = controller
            .plan_control_text_edit(node_id, &node, &prompt, "Explain {{ input }}")
            .expect("prompt edit")
            .into_plan()
            .expect("prompt plan");
        assert_node_data_value(
            &prompt_plan,
            ["fields", "prompt"],
            json!("Explain {{ input }}"),
        );
        assert_eq!(
            prompt_plan.invalidation.reason,
            NodeInternalsInvalidationReason::DataChanged
        );
    }

    #[test]
    fn number_slider_and_switch_events_preserve_json_types() {
        let controller = OpenGpuiAuthoringController;
        let node_id = NodeId::from_u128(2);
        let node = test_node(json!({
            "temperature": 0.2,
            "threshold": 0.4,
            "stream": false
        }));
        let number = project_control(
            &node.data,
            &NodeControlDescriptor::number_input("control.temperature")
                .with_binding(NodeControlBinding::data_path("temperature")),
            OpenGpuiControlProjectionContext::default(),
        );
        let slider = project_control(
            &node.data,
            &NodeControlDescriptor::slider("control.threshold")
                .with_binding(NodeControlBinding::data_path("threshold")),
            OpenGpuiControlProjectionContext::default(),
        );
        let switch = project_control(
            &node.data,
            &NodeControlDescriptor::toggle("control.stream")
                .with_binding(NodeControlBinding::data_path("stream")),
            OpenGpuiControlProjectionContext::default(),
        );

        let number_plan = controller
            .plan_control_number_edit(node_id, &node, &number, 0.75)
            .expect("number edit")
            .into_plan()
            .expect("number plan");
        assert_node_data_value(&number_plan, ["temperature"], json!(0.75));

        let slider_plan = controller
            .plan_control_number_edit(node_id, &node, &slider, 0.9)
            .expect("slider edit")
            .into_plan()
            .expect("slider plan");
        assert_node_data_value(&slider_plan, ["threshold"], json!(0.9));

        let switch_plan = controller
            .plan_control_bool_edit(node_id, &node, &switch, true)
            .expect("switch edit")
            .into_plan()
            .expect("switch plan");
        assert_node_data_value(&switch_plan, ["stream"], json!(true));
    }

    #[test]
    fn select_event_resolves_original_option_value_not_label() {
        let controller = OpenGpuiAuthoringController;
        let node_id = NodeId::from_u128(3);
        let node = test_node(json!({ "meta": { "model": "old" } }));
        let descriptor = NodeControlDescriptor::select("control.model")
            .with_binding(NodeControlBinding::data_path("meta.model"))
            .with_options([
                NodeControlOption::new(json!("gpt-4.1-mini"), "GPT 4.1 Mini"),
                NodeControlOption::new(
                    json!({ "provider": "openai", "model": "gpt-4.1" }),
                    "GPT 4.1",
                ),
            ]);
        let control = project_control(
            &node.data,
            &descriptor,
            OpenGpuiControlProjectionContext::default(),
        );
        let option_key = control_option_value_key(&json!({
            "provider": "openai",
            "model": "gpt-4.1"
        }));

        let plan = controller
            .plan_control_select_edit(node_id, &node, &control, option_key)
            .expect("select edit")
            .into_plan()
            .expect("select plan");

        assert_node_data_value(
            &plan,
            ["meta", "model"],
            json!({ "provider": "openai", "model": "gpt-4.1" }),
        );
    }

    #[test]
    fn unavailable_controls_report_skip_reasons_without_mutation() {
        let controller = OpenGpuiAuthoringController;
        let node_id = NodeId::from_u128(4);
        let node = test_node(json!({ "value": "old" }));
        let cases = [
            (
                NodeControlDescriptor::text_input("control.disabled")
                    .with_binding(NodeControlBinding::data_path("value"))
                    .disabled("locked"),
                OpenGpuiAuthoringSkipReason::DisabledControl {
                    control_key: "control.disabled".to_owned(),
                    reason: "locked".to_owned(),
                },
            ),
            (
                NodeControlDescriptor::text_input("control.readonly")
                    .with_binding(NodeControlBinding::data_path("value"))
                    .read_only(),
                OpenGpuiAuthoringSkipReason::ReadOnlyControl {
                    control_key: "control.readonly".to_owned(),
                },
            ),
            (
                NodeControlDescriptor::asset("control.asset")
                    .with_binding(NodeControlBinding::data_path("value")),
                OpenGpuiAuthoringSkipReason::StubControl {
                    control_key: "control.asset".to_owned(),
                    primitive: OpenGpuiControlPrimitive::AssetPickerStub,
                },
            ),
            (
                NodeControlDescriptor::text_input("control.unbound"),
                OpenGpuiAuthoringSkipReason::NoWritableBinding {
                    control_key: "control.unbound".to_owned(),
                },
            ),
        ];

        for (descriptor, expected) in cases {
            let control = project_control(
                &node.data,
                &descriptor,
                OpenGpuiControlProjectionContext::default(),
            );
            let outcome = controller
                .plan_control_text_edit(node_id, &node, &control, "new")
                .expect("skipped edit");
            assert_skip_reason(outcome, expected);
        }

        let unchanged = project_control(
            &node.data,
            &NodeControlDescriptor::text_input("control.value")
                .with_binding(NodeControlBinding::data_path("value")),
            OpenGpuiControlProjectionContext::default(),
        );
        assert_skip_reason(
            controller
                .plan_control_text_edit(node_id, &node, &unchanged, "old")
                .expect("unchanged edit"),
            OpenGpuiAuthoringSkipReason::UnchangedControl {
                control_key: "control.value".to_owned(),
            },
        );
    }

    #[test]
    fn select_event_reports_missing_and_disabled_options() {
        let controller = OpenGpuiAuthoringController;
        let node_id = NodeId::from_u128(5);
        let node = test_node(json!({ "mode": "a" }));
        let descriptor = NodeControlDescriptor::select("control.mode")
            .with_binding(NodeControlBinding::data_path("mode"))
            .with_options([
                NodeControlOption::new("a", "A"),
                NodeControlOption::new("b", "B").disabled(),
            ]);
        let control = project_control(
            &node.data,
            &descriptor,
            OpenGpuiControlProjectionContext::default(),
        );
        let disabled_key = control_option_value_key(&json!("b"));
        assert_skip_reason(
            controller
                .plan_control_select_edit(node_id, &node, &control, disabled_key.clone())
                .expect("disabled option"),
            OpenGpuiAuthoringSkipReason::DisabledSelectOption {
                control_key: "control.mode".to_owned(),
                option_key: disabled_key,
            },
        );
        assert_skip_reason(
            controller
                .plan_control_select_edit(node_id, &node, &control, "\"missing\"")
                .expect("missing option"),
            OpenGpuiAuthoringSkipReason::MissingSelectOption {
                control_key: "control.mode".to_owned(),
                option_key: "\"missing\"".to_owned(),
            },
        );
    }

    #[test]
    fn store_control_events_read_current_store_before_planning_each_edit() {
        let controller = OpenGpuiAuthoringController;
        let registry = NodeKitRegistry::builtin().node_registry();
        let mut store = NodeGraphStore::new(
            Graph::new(GraphId::from_u128(10)),
            NodeGraphViewState::default(),
            NodeGraphEditorConfig::default(),
        );
        let outcome = store
            .apply_create_node_from_schema(
                &registry,
                CreateNodeRequest::new(NodeKindKey::new("demo.llm"), CanvasPoint::default()),
            )
            .expect("create llm node");
        let node_id = outcome.node_id();
        let descriptor = registry
            .view_descriptor(&NodeKindKey::new("demo.llm"))
            .expect("llm descriptor");
        let initial_node = store.graph().nodes().get(&node_id).expect("llm node");
        let prompt_slot = descriptor
            .surface_slot("field.prompt")
            .expect("prompt slot");
        let model_slot = descriptor.surface_slot("badge.model").expect("model slot");
        let prompt = crate::project_slot_controls(&initial_node.data, prompt_slot)
            .into_iter()
            .find(|control| control.key == "control.prompt")
            .expect("prompt control");
        let model = crate::project_slot_controls(&initial_node.data, model_slot)
            .into_iter()
            .find(|control| control.key == "control.model")
            .expect("model control");

        let prompt_plan = controller
            .plan_store_control_event(
                &store,
                &registry,
                node_id,
                &prompt,
                OpenGpuiControlEventValue::Text("Keep this prompt".to_owned()),
            )
            .expect("prompt edit")
            .into_plan()
            .expect("prompt plan");
        store
            .dispatch_transaction(&prompt_plan.transaction)
            .expect("prompt transaction");

        let option = model
            .options
            .iter()
            .find(|option| option.label == "GPT 4.1")
            .expect("model option");
        let model_plan = controller
            .plan_store_control_event(
                &store,
                &registry,
                node_id,
                &model,
                OpenGpuiControlEventValue::SelectOptionKey(control_option_key(option)),
            )
            .expect("model edit")
            .into_plan()
            .expect("model plan");
        store
            .dispatch_transaction(&model_plan.transaction)
            .expect("model transaction");

        let data = &store.graph().nodes().get(&node_id).expect("llm node").data;
        assert_eq!(
            data.pointer("/fields/prompt"),
            Some(&json!("Keep this prompt"))
        );
        assert_eq!(data.pointer("/meta/model"), Some(&json!("gpt-4.1")));
    }

    #[test]
    fn store_control_events_report_missing_node_and_descriptor() {
        let controller = OpenGpuiAuthoringController;
        let registry = NodeKitRegistry::builtin().node_registry();
        let empty_store = NodeGraphStore::new(
            Graph::new(GraphId::from_u128(11)),
            NodeGraphViewState::default(),
            NodeGraphEditorConfig::default(),
        );
        let control = project_control(
            &json!({ "value": "old" }),
            &NodeControlDescriptor::text_input("control.value")
                .with_binding(NodeControlBinding::data_path("value")),
            OpenGpuiControlProjectionContext::default(),
        );
        assert_skip_reason(
            controller
                .plan_store_control_event(
                    &empty_store,
                    &registry,
                    NodeId::from_u128(99),
                    &control,
                    OpenGpuiControlEventValue::Text("new".to_owned()),
                )
                .expect("missing node"),
            OpenGpuiAuthoringSkipReason::MissingNode {
                node_id: format!("{:?}", NodeId::from_u128(99)),
            },
        );

        let graph = Graph::new(GraphId::from_u128(12));
        let node_id = NodeId::from_u128(12);
        let mut store = NodeGraphStore::new(
            graph,
            NodeGraphViewState::default(),
            NodeGraphEditorConfig::default(),
        );
        store
            .dispatch_transaction(&GraphTransaction::from_ops([GraphOp::AddNode {
                id: node_id,
                node: Node {
                    kind: NodeKindKey::new("missing.kind"),
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
                    data: json!({ "value": "old" }),
                },
            }]))
            .expect("insert missing descriptor node");
        assert_eq!(
            store
                .graph()
                .nodes()
                .get(&node_id)
                .expect("inserted node")
                .kind,
            NodeKindKey::new("missing.kind")
        );
        assert_skip_reason(
            controller
                .plan_store_control_event(
                    &store,
                    &registry,
                    node_id,
                    &control,
                    OpenGpuiControlEventValue::Text("new".to_owned()),
                )
                .expect("missing descriptor"),
            OpenGpuiAuthoringSkipReason::MissingNodeDescriptor {
                node_kind: "missing.kind".to_owned(),
            },
        );
    }

    #[test]
    fn repeatable_action_dispatch_maps_blackboard_add_and_applies_store_mutation() {
        let controller = OpenGpuiAuthoringController;
        let registry = NodeKitRegistry::builtin().node_registry();
        let mut store = NodeGraphStore::new(
            Graph::new(GraphId::from_u128(13)),
            NodeGraphViewState::default(),
            NodeGraphEditorConfig::default(),
        );
        let outcome = store
            .apply_create_node_from_schema(
                &registry,
                CreateNodeRequest::new(NodeKindKey::new("demo.shader.mix"), CanvasPoint::default()),
            )
            .expect("create shader mix");
        let node_id = outcome.node_id();
        let descriptor = registry
            .view_descriptor(&NodeKindKey::new("demo.shader.mix"))
            .expect("shader descriptor");
        let node = store.graph().nodes().get(&node_id).expect("shader node");
        let blackboard = crate::project_blackboards_for_descriptor(&descriptor, &node.data)
            .into_iter()
            .find(|blackboard| blackboard.key == "blackboard.shader.properties")
            .expect("shader properties blackboard");
        let dispatch = controller
            .plan_menu_action_dispatch(&blackboard.action_menu, "action.shader_property.add")
            .into_plan()
            .expect("blackboard add dispatch");

        let mut fallback_calls = 0;
        let fallback = controller
            .plan_repeatable_action_dispatch(&store, &registry, Some(node_id), &dispatch, |_| {
                fallback_calls += 1;
                None
            })
            .expect("fallback repeatable action")
            .into_plan()
            .expect("fallback add plan");
        assert_eq!(fallback_calls, 1);
        assert_eq!(
            fallback,
            OpenGpuiRepeatableActionPlan::Add {
                collection_key: "shader.properties".to_owned(),
                item: json!({}),
            }
        );

        let mut factory_calls = 0;
        let repeatable = controller
            .plan_repeatable_action_dispatch(
                &store,
                &registry,
                Some(node_id),
                &dispatch,
                |context| {
                    factory_calls += 1;
                    assert_eq!(context.action_key, "action.shader_property.add");
                    assert_eq!(context.collection_key, "shader.properties");
                    assert_eq!(context.item_count, 2);
                    Some(json!({ "name": "roughness" }))
                },
            )
            .expect("repeatable action")
            .into_plan()
            .expect("repeatable add plan");
        assert_eq!(factory_calls, 1);
        let plan = controller
            .apply_repeatable_action_to_store(&mut store, &registry, node_id, repeatable)
            .expect("apply repeatable")
            .expect("changed repeatable");
        assert_eq!(plan.collection_key, "shader.properties");

        let updated = store.graph().nodes().get(&node_id).expect("updated node");
        let properties = updated.data["properties"]
            .as_array()
            .expect("shader properties");
        assert_eq!(properties.len(), 3);
        assert_eq!(properties[2]["id"], json!("propertie_3"));
        assert_eq!(properties[2]["name"], json!("roughness"));
    }

    #[test]
    fn repeatable_action_dispatch_maps_remove_by_item_id() {
        let controller = OpenGpuiAuthoringController;
        let registry = NodeKitRegistry::builtin().node_registry();
        let mut store = NodeGraphStore::new(
            Graph::new(GraphId::from_u128(14)),
            NodeGraphViewState::default(),
            NodeGraphEditorConfig::default(),
        );
        let outcome = store
            .apply_create_node_from_schema(
                &registry,
                CreateNodeRequest::new(NodeKindKey::new("demo.shader.mix"), CanvasPoint::default()),
            )
            .expect("create shader mix");
        let node_id = outcome.node_id();
        let dispatch = OpenGpuiActionDispatchPlan {
            action_key: "action.shader_input.remove".to_owned(),
            target: ActionTarget::RepeatableItem {
                collection_key: "shader.inputs".to_owned(),
                item_id: "factor".to_owned(),
            },
            intent: ActionIntent::RemoveRepeatableItem {
                collection_key: "shader.inputs".to_owned(),
                item_id: "factor".to_owned(),
            },
        };

        let repeatable = controller
            .plan_repeatable_action_dispatch(&store, &registry, Some(node_id), &dispatch, |_| None)
            .expect("repeatable remove")
            .into_plan()
            .expect("remove plan");
        assert_eq!(
            repeatable,
            OpenGpuiRepeatableActionPlan::Remove {
                collection_key: "shader.inputs".to_owned(),
                item_id: "factor".to_owned(),
            }
        );
        controller
            .apply_repeatable_action_to_store(&mut store, &registry, node_id, repeatable)
            .expect("apply remove")
            .expect("changed remove");
        let node = store.graph().nodes().get(&node_id).expect("updated node");
        assert!(
            node.data["dynamic_inputs"]
                .as_array()
                .expect("dynamic inputs")
                .iter()
                .all(|item| item["id"] != "factor")
        );
    }

    #[test]
    fn repeatable_action_dispatch_reports_skipped_outcomes() {
        let controller = OpenGpuiAuthoringController;
        let registry = NodeKitRegistry::builtin().node_registry();
        let mut store = NodeGraphStore::new(
            Graph::new(GraphId::from_u128(15)),
            NodeGraphViewState::default(),
            NodeGraphEditorConfig::default(),
        );
        let outcome = store
            .apply_create_node_from_schema(
                &registry,
                CreateNodeRequest::new(NodeKindKey::new("demo.shader.mix"), CanvasPoint::default()),
            )
            .expect("create shader mix");
        let node_id = outcome.node_id();

        let reorder = OpenGpuiActionDispatchPlan {
            action_key: "action.shader_input.reorder".to_owned(),
            target: ActionTarget::Node {
                node_kind: "demo.shader.mix".to_owned(),
            },
            intent: ActionIntent::ReorderRepeatableItem {
                collection_key: "shader.inputs".to_owned(),
                item_id: "factor".to_owned(),
            },
        };
        assert_skip_reason(
            controller
                .plan_repeatable_action_dispatch(&store, &registry, Some(node_id), &reorder, |_| {
                    None
                })
                .expect("reorder skip"),
            OpenGpuiAuthoringSkipReason::MissingRepeatableReorderTarget {
                action_key: "action.shader_input.reorder".to_owned(),
                collection_key: "shader.inputs".to_owned(),
                item_id: "factor".to_owned(),
            },
        );

        let add = OpenGpuiActionDispatchPlan {
            action_key: "action.shader_input.add".to_owned(),
            target: ActionTarget::Node {
                node_kind: "demo.shader.mix".to_owned(),
            },
            intent: ActionIntent::AddRepeatableItem {
                collection_key: "shader.inputs".to_owned(),
            },
        };
        assert_skip_reason(
            controller
                .plan_repeatable_action_dispatch(&store, &registry, None, &add, |_| None)
                .expect("missing target skip"),
            OpenGpuiAuthoringSkipReason::MissingActionNodeTarget {
                action_key: "action.shader_input.add".to_owned(),
            },
        );

        let unsupported = OpenGpuiActionDispatchPlan {
            action_key: "action.run".to_owned(),
            target: ActionTarget::Node {
                node_kind: "demo.shader.mix".to_owned(),
            },
            intent: ActionIntent::RunNode,
        };
        assert_skip_reason(
            controller
                .plan_repeatable_action_dispatch(
                    &store,
                    &registry,
                    Some(node_id),
                    &unsupported,
                    |_| None,
                )
                .expect("unsupported skip"),
            OpenGpuiAuthoringSkipReason::UnsupportedActionIntent {
                action_key: "action.run".to_owned(),
                intent: ActionIntent::RunNode,
            },
        );
    }

    #[test]
    fn invalid_json_pointer_fails_without_mutating_input_node_data() {
        let controller = OpenGpuiAuthoringController;
        let node_id = NodeId::from_u128(6);
        let node = test_node(json!({ "meta": { "model": "old" } }));
        let control = project_control(
            &node.data,
            &NodeControlDescriptor::text_input("control.model")
                .with_binding(NodeControlBinding::json_pointer("meta/model")),
            OpenGpuiControlProjectionContext::default(),
        );

        let error = controller
            .plan_control_text_edit(node_id, &node, &control, "new")
            .expect_err("invalid pointer");

        assert!(error.contains("must start with `/`"));
        assert_eq!(node.data["meta"]["model"], json!("old"));
    }

    #[test]
    fn inspector_control_events_reuse_control_edit_path() {
        let controller = OpenGpuiAuthoringController;
        let node_id = NodeId::from_u128(7);
        let node = test_node(json!({ "meta": { "model": "old" } }));
        let control = project_control(
            &node.data,
            &NodeControlDescriptor::text_input("inspector.model")
                .with_binding(NodeControlBinding::data_path("meta.model")),
            OpenGpuiControlProjectionContext::default(),
        );
        let inspector = OpenGpuiInspectorPlan {
            key: "inspector.llm".to_owned(),
            label: "LLM".to_owned(),
            target: InspectorTarget::Node {
                node_kind: "demo.llm".to_owned(),
            },
            target_region_key: None,
            controls: vec![control],
            action_menu: empty_menu(),
            editable: true,
            read_only_reason: None,
        };

        let plan = controller
            .plan_inspector_control_event(
                node_id,
                &node,
                &inspector,
                "inspector.model",
                OpenGpuiControlEventValue::Text("gpt-4.1".to_owned()),
            )
            .expect("inspector edit")
            .into_plan()
            .expect("inspector plan");

        assert_node_data_value(&plan, ["meta", "model"], json!("gpt-4.1"));
        assert_skip_reason(
            controller
                .plan_inspector_control_value_edit(
                    node_id,
                    &node,
                    &inspector,
                    "missing",
                    json!("new"),
                )
                .expect("missing control"),
            OpenGpuiAuthoringSkipReason::MissingInspectorControl {
                inspector_key: "inspector.llm".to_owned(),
                control_key: "missing".to_owned(),
            },
        );
    }

    #[test]
    fn menu_action_events_report_dispatch_or_skip_reasons() {
        let controller = OpenGpuiAuthoringController;
        let menu = OpenGpuiMenuPlan {
            key: "menu.node".to_owned(),
            label: "Node".to_owned(),
            surface: MenuSurface::Node,
            actions: vec![
                OpenGpuiActionPlan {
                    key: "action.run".to_owned(),
                    label: "Run".to_owned(),
                    target: ActionTarget::Node {
                        node_kind: "demo.llm".to_owned(),
                    },
                    intent: ActionIntent::RunNode,
                    enabled: true,
                    disabled_reason: None,
                    group: None,
                    order: None,
                    danger: false,
                    icon_key: None,
                    shortcut: None,
                },
                OpenGpuiActionPlan {
                    key: "action.disabled".to_owned(),
                    label: "Disabled".to_owned(),
                    target: ActionTarget::Node {
                        node_kind: "demo.llm".to_owned(),
                    },
                    intent: ActionIntent::RunNode,
                    enabled: false,
                    disabled_reason: Some("locked".to_owned()),
                    group: None,
                    order: None,
                    danger: false,
                    icon_key: None,
                    shortcut: None,
                },
            ],
        };

        let dispatch = controller.plan_menu_action_dispatch(&menu, "action.run");
        assert!(dispatch.is_planned());
        assert_eq!(
            dispatch.into_plan().expect("dispatch").intent,
            ActionIntent::RunNode
        );
        assert_eq!(
            controller.plan_menu_action_dispatch(&menu, "action.disabled"),
            OpenGpuiAuthoringOutcome::Skipped(OpenGpuiAuthoringSkipReason::DisabledAction {
                action_key: "action.disabled".to_owned(),
                reason: Some("locked".to_owned())
            })
        );
        assert_eq!(
            controller.plan_menu_action_dispatch(&menu, "action.missing"),
            OpenGpuiAuthoringOutcome::Skipped(OpenGpuiAuthoringSkipReason::MissingAction {
                menu_key: "menu.node".to_owned(),
                action_key: "action.missing".to_owned()
            })
        );
    }

    fn test_node(data: Value) -> Node {
        Node {
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
            data,
        }
    }

    fn assert_node_data_value<const N: usize>(
        plan: &OpenGpuiControlEditPlan,
        path: [&str; N],
        expected: Value,
    ) {
        assert_eq!(plan.invalidation.nodes.len(), 1);
        let [GraphOp::SetNodeData { to, .. }] = plan.transaction.ops() else {
            panic!("expected one node data edit");
        };
        let mut value = to;
        for segment in path {
            value = &value[segment];
        }
        assert_eq!(*value, expected);
    }

    fn assert_skip_reason<T>(
        outcome: OpenGpuiAuthoringOutcome<T>,
        expected: OpenGpuiAuthoringSkipReason,
    ) {
        assert_eq!(outcome.skip_reason(), Some(&expected));
    }

    fn empty_menu() -> OpenGpuiMenuPlan {
        OpenGpuiMenuPlan {
            key: "menu.empty".to_owned(),
            label: "Empty".to_owned(),
            surface: MenuSurface::Inspector,
            actions: Vec::new(),
        }
    }
}
