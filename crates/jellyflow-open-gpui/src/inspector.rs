use jellyflow::{
    core::{CanvasRect, Node, NodeId},
    runtime::runtime::measurement::NodeMeasurement,
    runtime::schema::{
        InspectorDescriptor, InspectorTarget, NodeKindViewDescriptor,
        NodeRepeatableCollectionDescriptor,
    },
};
use serde_json::Value;

use crate::{
    OpenGpuiControlEditPlan, OpenGpuiControlPlan, OpenGpuiControlProjectionContext,
    OpenGpuiMenuPlan,
    json_binding::{repeatable_item_id, semantic_json_lookup},
    plan_control_edit, project_action, project_control,
};

/// Adapter-local inspector surface being resolved by GPUI view state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenGpuiInspectorSurface {
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

/// GPUI-local inspector render plan built from a semantic descriptor.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenGpuiInspectorPlan {
    pub key: String,
    pub label: String,
    pub target: InspectorTarget,
    pub target_region_key: Option<String>,
    pub controls: Vec<OpenGpuiControlPlan>,
    pub action_menu: OpenGpuiMenuPlan,
    pub editable: bool,
    pub read_only_reason: Option<String>,
}

/// Source used to position an inspector target highlight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenGpuiInspectorTargetSource {
    Measured,
    Fallback,
    Missing,
}

/// Node-local bounds for the semantic region targeted by an inspector.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpenGpuiInspectorTargetBounds {
    pub rect: Option<CanvasRect>,
    pub source: OpenGpuiInspectorTargetSource,
}

impl OpenGpuiInspectorPlan {
    pub fn editable_controls(&self) -> impl Iterator<Item = &OpenGpuiControlPlan> {
        self.controls.iter().filter(|control| control.is_editable())
    }
}

/// Resolve an inspector target against fresh runtime measurement facts.
pub fn resolve_inspector_target_bounds(
    inspector: &OpenGpuiInspectorPlan,
    measurement: Option<&NodeMeasurement>,
    fallback: Option<CanvasRect>,
) -> OpenGpuiInspectorTargetBounds {
    let measured = measurement
        .and_then(|measurement| measured_rect_for_inspector_target(inspector, measurement));
    if let Some(rect) = measured {
        return OpenGpuiInspectorTargetBounds {
            rect: Some(rect),
            source: OpenGpuiInspectorTargetSource::Measured,
        };
    }
    if let Some(rect) = fallback {
        return OpenGpuiInspectorTargetBounds {
            rect: Some(rect),
            source: OpenGpuiInspectorTargetSource::Fallback,
        };
    }
    OpenGpuiInspectorTargetBounds {
        rect: None,
        source: OpenGpuiInspectorTargetSource::Missing,
    }
}

fn measured_rect_for_inspector_target(
    inspector: &OpenGpuiInspectorPlan,
    measurement: &NodeMeasurement,
) -> Option<CanvasRect> {
    match &inspector.target {
        InspectorTarget::Slot { .. }
        | InspectorTarget::Control { .. }
        | InspectorTarget::RepeatableItem { .. } => {
            inspector
                .target_region_key
                .as_ref()
                .and_then(|target_region_key| {
                    measurement
                        .slots
                        .iter()
                        .find(|slot| slot.is_visible() && slot.key == *target_region_key)
                        .map(|slot| slot.rect)
                })
        }
        InspectorTarget::Port { port_key } => measurement
            .anchors
            .iter()
            .find(|anchor| {
                anchor.is_visible()
                    && anchor
                        .port_key
                        .as_ref()
                        .is_some_and(|measured| measured.0 == *port_key)
            })
            .map(|anchor| anchor.rect),
        InspectorTarget::Graph
        | InspectorTarget::Node { .. }
        | InspectorTarget::Edge
        | InspectorTarget::Diagnostic { .. } => None,
    }
}

/// Project all inspector descriptors that match one GPUI-local surface.
pub fn project_inspectors_for_surface(
    descriptor: &NodeKindViewDescriptor,
    node_data: &Value,
    surface: &OpenGpuiInspectorSurface,
) -> Vec<OpenGpuiInspectorPlan> {
    descriptor
        .inspectors
        .iter()
        .filter(|inspector| inspector_matches_surface(inspector, surface))
        .map(|inspector| project_inspector(descriptor, node_data, inspector))
        .collect()
}

/// Build one inspector plan from a semantic descriptor.
pub fn project_inspector(
    descriptor: &NodeKindViewDescriptor,
    node_data: &Value,
    inspector: &InspectorDescriptor,
) -> OpenGpuiInspectorPlan {
    let repeatable = repeatable_context_for_inspector(descriptor, node_data, &inspector.target);
    let target_region_key = inspector_target_region_key(descriptor, node_data, &inspector.target);
    let controls = inspector
        .controls
        .iter()
        .map(|control| {
            project_control(
                node_data,
                control,
                OpenGpuiControlProjectionContext {
                    slot: None,
                    item_data: repeatable.as_ref().map(|context| context.item_data),
                    item_path: repeatable
                        .as_ref()
                        .map(|context| context.item_path.as_str()),
                },
            )
        })
        .collect::<Vec<_>>();
    let action_menu = inspector_action_menu(descriptor, inspector, repeatable.as_ref());
    let read_only_reason = inspector_read_only_reason(inspector, &controls);

    OpenGpuiInspectorPlan {
        key: inspector.key.clone(),
        label: inspector
            .label
            .clone()
            .unwrap_or_else(|| inspector.key.clone()),
        target: inspector.target.clone(),
        target_region_key,
        editable: read_only_reason.is_none(),
        read_only_reason,
        controls,
        action_menu,
    }
}

/// Resolve an inspector-local control edit through the same write path used by in-node controls.
pub fn plan_inspector_control_edit(
    node_id: NodeId,
    node: &Node,
    inspector: &OpenGpuiInspectorPlan,
    control_key: &str,
    value: Value,
) -> Result<Option<OpenGpuiControlEditPlan>, String> {
    let control = inspector
        .controls
        .iter()
        .find(|control| control.key == control_key)
        .ok_or_else(|| {
            format!(
                "inspector `{}` has no control `{control_key}`",
                inspector.key
            )
        })?;
    plan_control_edit(node_id, node, control, value)
}

fn inspector_action_menu(
    descriptor: &NodeKindViewDescriptor,
    inspector: &InspectorDescriptor,
    repeatable: Option<&RepeatableInspectorContext<'_>>,
) -> OpenGpuiMenuPlan {
    let mut action_keys = inspector.action_keys.clone();
    if let Some(repeatable) = repeatable {
        action_keys.extend(repeatable.collection.add_action.clone());
        action_keys.extend(repeatable.collection.remove_action.clone());
        action_keys.extend(repeatable.collection.reorder_action.clone());
    }

    let actions = action_keys
        .into_iter()
        .filter_map(|key| descriptor.action(key))
        .map(|action| {
            let mut action = project_action(action);
            if let Some(repeatable) = repeatable {
                let (target, intent) =
                    repeatable_item_action_parts(action.target, action.intent, repeatable);
                action.target = target;
                action.intent = intent;
            }
            action
        })
        .collect::<Vec<_>>();

    OpenGpuiMenuPlan {
        key: format!("inspector.menu.{}", inspector.key),
        label: inspector
            .label
            .clone()
            .unwrap_or_else(|| inspector.key.clone()),
        surface: jellyflow::runtime::schema::MenuSurface::Inspector,
        actions,
    }
}

fn repeatable_item_action_parts(
    target: jellyflow::runtime::schema::ActionTarget,
    intent: jellyflow::runtime::schema::ActionIntent,
    repeatable: &RepeatableInspectorContext<'_>,
) -> (
    jellyflow::runtime::schema::ActionTarget,
    jellyflow::runtime::schema::ActionIntent,
) {
    use jellyflow::runtime::schema::{ActionIntent, ActionTarget};

    match intent {
        ActionIntent::RemoveRepeatableItem { collection_key, .. } => (
            ActionTarget::RepeatableItem {
                collection_key: repeatable.collection.key.clone(),
                item_id: repeatable.item_id.clone(),
            },
            ActionIntent::RemoveRepeatableItem {
                collection_key,
                item_id: repeatable.item_id.clone(),
            },
        ),
        ActionIntent::ReorderRepeatableItem { collection_key, .. } => (
            ActionTarget::RepeatableItem {
                collection_key: repeatable.collection.key.clone(),
                item_id: repeatable.item_id.clone(),
            },
            ActionIntent::ReorderRepeatableItem {
                collection_key,
                item_id: repeatable.item_id.clone(),
            },
        ),
        other => (target, other),
    }
}

fn inspector_target_region_key(
    descriptor: &NodeKindViewDescriptor,
    node_data: &Value,
    target: &InspectorTarget,
) -> Option<String> {
    match target {
        InspectorTarget::Slot { slot_key } => Some(slot_key.clone()),
        InspectorTarget::Control { control_key } => Some(control_key.clone()),
        InspectorTarget::RepeatableItem {
            collection_key,
            item_id,
        } => descriptor
            .repeatable_collections
            .iter()
            .find(|collection| collection.key == *collection_key)
            .and_then(|collection| {
                collection
                    .item_projections(node_data)
                    .into_iter()
                    .find(|item| item.item_id == *item_id)
                    .map(|item| item.slot_key)
            }),
        InspectorTarget::Graph
        | InspectorTarget::Node { .. }
        | InspectorTarget::Edge
        | InspectorTarget::Port { .. }
        | InspectorTarget::Diagnostic { .. } => None,
    }
}

fn inspector_read_only_reason(
    inspector: &InspectorDescriptor,
    controls: &[OpenGpuiControlPlan],
) -> Option<String> {
    if matches!(inspector.target, InspectorTarget::Diagnostic { .. }) {
        return Some(
            "diagnostic inspector is read-only until a writable descriptor is provided".to_owned(),
        );
    }
    if controls.is_empty() {
        return None;
    }
    controls
        .iter()
        .all(|control| !control.is_editable())
        .then(|| "no editable inspector controls".to_owned())
}

fn inspector_matches_surface(
    inspector: &InspectorDescriptor,
    surface: &OpenGpuiInspectorSurface,
) -> bool {
    match (&inspector.target, surface) {
        (InspectorTarget::Graph, OpenGpuiInspectorSurface::Graph) => true,
        (
            InspectorTarget::Node { node_kind },
            OpenGpuiInspectorSurface::Node { node_kind: current },
        ) => node_kind == current,
        (InspectorTarget::Edge, OpenGpuiInspectorSurface::Edge) => true,
        (
            InspectorTarget::Port { port_key },
            OpenGpuiInspectorSurface::Port { port_key: current },
        ) => port_key == current,
        (
            InspectorTarget::Slot { slot_key },
            OpenGpuiInspectorSurface::Slot { slot_key: current },
        ) => slot_key == current,
        (
            InspectorTarget::Control { control_key },
            OpenGpuiInspectorSurface::Control {
                control_key: current,
            },
        ) => control_key == current,
        (
            InspectorTarget::RepeatableItem {
                collection_key,
                item_id,
            },
            OpenGpuiInspectorSurface::RepeatableItem {
                collection_key: current_collection,
                item_id: current_item,
            },
        ) => collection_key == current_collection && item_id == current_item,
        (
            InspectorTarget::Diagnostic { diagnostic_key },
            OpenGpuiInspectorSurface::Diagnostic {
                diagnostic_key: current,
            },
        ) => diagnostic_key == current,
        _ => false,
    }
}

struct RepeatableInspectorContext<'a> {
    collection: &'a NodeRepeatableCollectionDescriptor,
    item_id: String,
    item_path: String,
    item_data: &'a Value,
}

fn repeatable_context_for_inspector<'a>(
    descriptor: &'a NodeKindViewDescriptor,
    node_data: &'a Value,
    target: &InspectorTarget,
) -> Option<RepeatableInspectorContext<'a>> {
    let InspectorTarget::RepeatableItem {
        collection_key,
        item_id,
    } = target
    else {
        return None;
    };
    let collection = descriptor.repeatable_collection(collection_key)?;
    repeatable_item_path(collection, node_data, item_id).map(|(item_path, item_data)| {
        RepeatableInspectorContext {
            collection,
            item_id: item_id.clone(),
            item_path,
            item_data,
        }
    })
}

fn repeatable_item_path<'a>(
    collection: &NodeRepeatableCollectionDescriptor,
    node_data: &'a Value,
    item_id: &str,
) -> Option<(String, &'a Value)> {
    let items = semantic_json_lookup(node_data, &collection.item_source)?;
    match items {
        Value::Array(items) => items.iter().enumerate().find_map(|(index, item)| {
            repeatable_item_id(collection, item)
                .as_deref()
                .filter(|current| *current == item_id)
                .map(|_| (format!("{}.{}", collection.item_source, index), item))
        }),
        Value::Object(items) => items.iter().find_map(|(key, item)| {
            let current = repeatable_item_id(collection, item).unwrap_or_else(|| key.clone());
            (current == item_id).then(|| (format!("{}.{}", collection.item_source, key), item))
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jellyflow::{
        NodeGraphStore,
        core::{CanvasPoint, CanvasSize, GraphId, NodeKindKey},
        runtime::{
            io::{NodeGraphEditorConfig, NodeGraphViewState},
            runtime::create_node::CreateNodeRequest,
            runtime::measurement::{MeasuredSurfaceSlot, NodeMeasurement},
            schema::{ActionIntent, InspectorTarget, NodeKitRegistry},
        },
    };
    use serde_json::json;

    #[test]
    fn node_inspector_projects_controls_actions_and_dispatchable_edit() {
        let registry = NodeKitRegistry::builtin().node_registry();
        let descriptor = registry
            .view_descriptor(&NodeKindKey::new("demo.llm"))
            .expect("llm descriptor");
        let mut node = test_node("demo.llm", descriptor.default_data.clone());
        let inspectors = project_inspectors_for_surface(
            &descriptor,
            &node.data,
            &OpenGpuiInspectorSurface::Node {
                node_kind: "demo.llm".to_owned(),
            },
        );

        let inspector = inspectors
            .iter()
            .find(|inspector| inspector.key == "inspector.llm")
            .expect("llm inspector");
        assert_eq!(inspector.label, "LLM");
        assert!(inspector.editable);
        assert!(
            inspector
                .controls
                .iter()
                .any(|control| control.key == "inspector.model" && control.is_editable())
        );
        assert!(
            inspector
                .action_menu
                .actions
                .iter()
                .any(|action| action.key == "action.llm.run")
        );

        let plan = plan_inspector_control_edit(
            NodeId::from_u128(1),
            &node,
            inspector,
            "inspector.model",
            json!("gpt-4.1"),
        )
        .expect("edit plan")
        .expect("changed edit");
        let [jellyflow::core::GraphOp::SetNodeData { to, .. }] = plan.transaction.ops() else {
            panic!("expected node data edit");
        };
        assert_eq!(to["meta"]["model"], json!("gpt-4.1"));

        node.data = to.clone();
        assert!(
            plan_inspector_control_edit(
                NodeId::from_u128(1),
                &node,
                inspector,
                "inspector.model",
                json!("gpt-4.1"),
            )
            .expect("same edit")
            .is_none()
        );
    }

    #[test]
    fn repeatable_item_inspector_reuses_item_binding_and_specializes_actions() {
        let (descriptor, node_id, node) = schema_node("demo.table").expect("table schema node");
        let inspectors = project_inspectors_for_surface(
            &descriptor,
            &node.data,
            &OpenGpuiInspectorSurface::RepeatableItem {
                collection_key: "table.columns".to_owned(),
                item_id: "email".to_owned(),
            },
        );

        let inspector = inspectors
            .iter()
            .find(|inspector| inspector.key == "inspector.column.email")
            .expect("column inspector");
        assert_eq!(
            inspector.target_region_key.as_deref(),
            Some("field.column.email")
        );
        let name = inspector
            .controls
            .iter()
            .find(|control| control.key == "inspector.column.name")
            .expect("name control");
        assert_eq!(name.value, Some(json!("email")));
        assert!(inspector.action_menu.actions.iter().any(|action| matches!(
            &action.intent,
            ActionIntent::RemoveRepeatableItem {
                collection_key,
                item_id
            } if collection_key == "table.columns" && item_id == "email"
        )));
        assert!(inspector.action_menu.actions.iter().any(|action| {
            matches!(
                (&action.target, &action.intent),
                (
                    jellyflow::runtime::schema::ActionTarget::Node { node_kind },
                    ActionIntent::AddRepeatableItem { collection_key },
                ) if node_kind == "demo.table" && collection_key == "table.columns"
            )
        }));

        let plan = plan_inspector_control_edit(
            node_id,
            &node,
            inspector,
            "inspector.column.name",
            json!("email_address"),
        )
        .expect("edit plan")
        .expect("changed edit");
        let [jellyflow::core::GraphOp::SetNodeData { to, .. }] = plan.transaction.ops() else {
            panic!("expected node data edit");
        };
        assert_eq!(to["columns"][1]["name"], json!("email_address"));
    }

    #[test]
    fn inspector_target_bounds_prefer_measured_region_then_fallback() {
        let (descriptor, node_id, node) = schema_node("demo.table").expect("table schema node");
        let inspector = project_inspectors_for_surface(
            &descriptor,
            &node.data,
            &OpenGpuiInspectorSurface::RepeatableItem {
                collection_key: "table.columns".to_owned(),
                item_id: "email".to_owned(),
            },
        )
        .into_iter()
        .find(|inspector| inspector.key == "inspector.column.email")
        .expect("column inspector");
        let measured = CanvasRect {
            origin: CanvasPoint { x: 8.0, y: 52.0 },
            size: CanvasSize {
                width: 144.0,
                height: 24.0,
            },
        };
        let fallback = CanvasRect {
            origin: CanvasPoint { x: 0.0, y: 0.0 },
            size: CanvasSize {
                width: 32.0,
                height: 16.0,
            },
        };
        let measurement = NodeMeasurement::new(node_id).with_slots([MeasuredSurfaceSlot::new(
            inspector
                .target_region_key
                .clone()
                .expect("target region key"),
            measured,
        )]);

        let target =
            resolve_inspector_target_bounds(&inspector, Some(&measurement), Some(fallback));
        assert_eq!(target.rect, Some(measured));
        assert_eq!(target.source, OpenGpuiInspectorTargetSource::Measured);

        let fallback_target = resolve_inspector_target_bounds(&inspector, None, Some(fallback));
        assert_eq!(fallback_target.rect, Some(fallback));
        assert_eq!(
            fallback_target.source,
            OpenGpuiInspectorTargetSource::Fallback
        );

        let missing_target = resolve_inspector_target_bounds(&inspector, None, None);
        assert_eq!(missing_target.rect, None);
        assert_eq!(
            missing_target.source,
            OpenGpuiInspectorTargetSource::Missing
        );
    }

    #[test]
    fn diagnostic_inspector_projects_readonly_plan() {
        let registry = NodeKitRegistry::builtin().node_registry();
        let descriptor = registry
            .view_descriptor(&NodeKindKey::new("demo.llm"))
            .expect("llm descriptor");
        let diagnostic = InspectorDescriptor::new(
            "inspector.diagnostic.missing_port",
            InspectorTarget::Diagnostic {
                diagnostic_key: "missing_port".to_owned(),
            },
        )
        .with_label("Missing port");

        let plan = project_inspector(&descriptor, &descriptor.default_data, &diagnostic);

        assert!(!plan.editable);
        assert_eq!(
            plan.read_only_reason.as_deref(),
            Some("diagnostic inspector is read-only until a writable descriptor is provided")
        );
    }

    fn schema_node(
        kind: &str,
    ) -> Result<(NodeKindViewDescriptor, NodeId, Node), Box<dyn std::error::Error>> {
        let registry = NodeKitRegistry::builtin().node_registry();
        let mut store = NodeGraphStore::new(
            jellyflow::core::Graph::new(GraphId::from_u128(1)),
            NodeGraphViewState::default(),
            NodeGraphEditorConfig::default(),
        );
        let outcome = store.apply_create_node_from_schema(
            &registry,
            CreateNodeRequest::new(NodeKindKey::new(kind), CanvasPoint::default()),
        )?;
        let node_id = outcome.node_id();
        let node = store
            .graph()
            .nodes()
            .get(&node_id)
            .expect("created node")
            .clone();
        let descriptor = registry
            .view_descriptor(&node.kind)
            .expect("created descriptor");
        Ok((descriptor, node_id, node))
    }

    fn test_node(kind: &str, data: Value) -> Node {
        Node {
            kind: NodeKindKey::new(kind),
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
}
