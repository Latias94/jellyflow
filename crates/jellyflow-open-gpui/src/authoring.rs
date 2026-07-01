use jellyflow::core::{Node, NodeId};
use serde_json::{Number, Value};

use crate::{
    OpenGpuiActionDispatchPlan, OpenGpuiControlEditPlan, OpenGpuiControlOptionPlan,
    OpenGpuiControlPlan, OpenGpuiControlPrimitive, OpenGpuiControlSupport, OpenGpuiInspectorPlan,
    OpenGpuiMenuPlan, plan_action_dispatch, plan_control_edit, plan_inspector_control_edit,
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

#[cfg(test)]
mod tests {
    use super::*;
    use jellyflow::{
        core::{CanvasPoint, CanvasSize, GraphOp, NodeKindKey},
        runtime::{
            runtime::measurement::NodeInternalsInvalidationReason,
            schema::{
                ActionIntent, ActionTarget, InspectorTarget, MenuSurface, NodeControlBinding,
                NodeControlDescriptor, NodeControlOption,
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
