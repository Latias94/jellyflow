use jellyflow::{
    core::{CanvasPoint, PortKey},
    runtime::{
        runtime::connection::ConnectionHandleRef,
        schema::{
            ActionIntent, ActionTarget, MenuDescriptor, MenuSurface, NodeActionDescriptor,
            NodeKindViewDescriptor, NodeRegistry,
        },
    },
};

/// Adapter-local UI surface being resolved for a menu/action plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenGpuiActionSurface {
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
        source_port_key: Option<String>,
    },
    Toolbar {
        node_kind: Option<String>,
    },
    Blackboard {
        blackboard_key: String,
    },
    Inspector {
        inspector_key: String,
    },
}

impl OpenGpuiActionSurface {
    pub fn menu_surface(&self) -> MenuSurface {
        match self {
            Self::Graph => MenuSurface::Graph,
            Self::Node { .. } => MenuSurface::Node,
            Self::Edge => MenuSurface::Edge,
            Self::Port { .. } => MenuSurface::Port,
            Self::Slot { .. } => MenuSurface::Slot,
            Self::Control { .. } => MenuSurface::Control,
            Self::RepeatableItem { .. } => MenuSurface::Node,
            Self::DroppedWire { .. } => MenuSurface::DroppedWire,
            Self::Toolbar { .. } => MenuSurface::Toolbar,
            Self::Blackboard { .. } => MenuSurface::Blackboard,
            Self::Inspector { .. } => MenuSurface::Inspector,
        }
    }
}

/// Render plan for one semantic action in GPUI local widgets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenGpuiActionPlan {
    pub key: String,
    pub label: String,
    pub target: ActionTarget,
    pub intent: ActionIntent,
    pub enabled: bool,
    pub disabled_reason: Option<String>,
    pub group: Option<String>,
    pub order: Option<i32>,
    pub danger: bool,
    pub icon_key: Option<String>,
    pub shortcut: Option<String>,
}

impl OpenGpuiActionPlan {
    pub fn dispatchable(&self) -> bool {
        self.enabled && self.disabled_reason.is_none()
    }
}

/// Render plan for a GPUI local menu or action group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenGpuiMenuPlan {
    pub key: String,
    pub label: String,
    pub surface: MenuSurface,
    pub actions: Vec<OpenGpuiActionPlan>,
}

impl OpenGpuiMenuPlan {
    pub fn enabled_actions(&self) -> impl Iterator<Item = &OpenGpuiActionPlan> {
        self.actions.iter().filter(|action| action.dispatchable())
    }
}

/// Action dispatch plan emitted by local GPUI widgets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenGpuiActionDispatchPlan {
    pub action_key: String,
    pub intent: ActionIntent,
    pub target: ActionTarget,
}

/// Result of choosing an insert action from a dropped-wire menu.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenGpuiDroppedWireInsertPlan {
    pub action_key: String,
    pub node_kind: String,
    pub source: ConnectionHandleRef,
    pub pointer: CanvasPoint,
}

/// Build a menu plan from one descriptor and the actions declared on the same node kind.
pub fn project_menu(
    descriptor: &NodeKindViewDescriptor,
    menu: &MenuDescriptor,
    surface: &OpenGpuiActionSurface,
) -> OpenGpuiMenuPlan {
    OpenGpuiMenuPlan {
        key: menu.key.clone(),
        label: menu.label.clone().unwrap_or_else(|| menu.key.clone()),
        surface: menu.surface.clone(),
        actions: menu
            .action_keys
            .iter()
            .filter_map(|key| descriptor.action(key))
            .filter(|action| action_matches_surface(action, surface))
            .map(project_action)
            .collect(),
    }
}

/// Build a synthetic menu plan for all actions on one descriptor that match a surface.
pub fn project_actions_for_surface(
    descriptor: &NodeKindViewDescriptor,
    surface: &OpenGpuiActionSurface,
) -> OpenGpuiMenuPlan {
    let surface_kind = surface.menu_surface();
    OpenGpuiMenuPlan {
        key: format!("synthetic.{surface_kind:?}"),
        label: synthetic_menu_label(surface),
        surface: surface_kind,
        actions: sorted_actions_for_surface(&descriptor.actions, surface),
    }
}

/// Build dropped-wire insert menu candidates across the whole node registry.
pub fn project_dropped_wire_menu(
    registry: &NodeRegistry,
    source: ConnectionHandleRef,
    source_port_key: Option<&PortKey>,
    pointer: CanvasPoint,
) -> OpenGpuiMenuPlan {
    let surface = OpenGpuiActionSurface::DroppedWire {
        source_port_key: source_port_key.map(|key| key.0.clone()),
    };
    let mut actions = registry
        .view_descriptors()
        .into_iter()
        .flat_map(|descriptor| sorted_actions_for_surface(&descriptor.actions, &surface))
        .collect::<Vec<_>>();
    sort_action_plans(&mut actions);

    OpenGpuiMenuPlan {
        key: format!(
            "dropped-wire:{}:{}:{}",
            source.node.0, source.port.0, pointer.x
        ),
        label: "Insert compatible node".to_owned(),
        surface: MenuSurface::DroppedWire,
        actions,
    }
}

/// Convert one action descriptor into a GPUI-local render plan.
pub fn project_action(action: &NodeActionDescriptor) -> OpenGpuiActionPlan {
    OpenGpuiActionPlan {
        key: action.key.clone(),
        label: action.label.clone(),
        target: action.target.clone(),
        intent: action.intent.clone(),
        enabled: action.availability.enabled,
        disabled_reason: action.availability.disabled_reason.clone(),
        group: action.group.clone(),
        order: action.order,
        danger: action.danger,
        icon_key: action.icon_key.clone(),
        shortcut: action.shortcut.as_ref().map(shortcut_label),
    }
}

/// Resolve a local UI selection into a semantic action dispatch plan.
pub fn plan_action_dispatch(
    menu: &OpenGpuiMenuPlan,
    action_key: &str,
) -> Option<OpenGpuiActionDispatchPlan> {
    let action = menu
        .actions
        .iter()
        .find(|action| action.key == action_key && action.dispatchable())?;
    Some(OpenGpuiActionDispatchPlan {
        action_key: action.key.clone(),
        intent: action.intent.clone(),
        target: action.target.clone(),
    })
}

/// Resolve an enabled dropped-wire insert action into the concrete insert plan needed by GPUI.
pub fn plan_dropped_wire_insert(
    menu: &OpenGpuiMenuPlan,
    action_key: &str,
    source: ConnectionHandleRef,
    pointer: CanvasPoint,
) -> Option<OpenGpuiDroppedWireInsertPlan> {
    let dispatch = plan_action_dispatch(menu, action_key)?;
    let ActionIntent::InsertNode { node_kind } = dispatch.intent else {
        return None;
    };
    Some(OpenGpuiDroppedWireInsertPlan {
        action_key: dispatch.action_key,
        node_kind,
        source,
        pointer,
    })
}

fn sorted_actions_for_surface(
    actions: &[NodeActionDescriptor],
    surface: &OpenGpuiActionSurface,
) -> Vec<OpenGpuiActionPlan> {
    let mut plans = actions
        .iter()
        .filter(|action| action_matches_surface(action, surface))
        .map(project_action)
        .collect::<Vec<_>>();
    sort_action_plans(&mut plans);
    plans
}

fn sort_action_plans(actions: &mut [OpenGpuiActionPlan]) {
    actions.sort_by(|a, b| {
        a.group
            .cmp(&b.group)
            .then_with(|| {
                a.order
                    .unwrap_or(i32::MAX)
                    .cmp(&b.order.unwrap_or(i32::MAX))
            })
            .then_with(|| a.label.cmp(&b.label))
            .then_with(|| a.key.cmp(&b.key))
    });
}

fn action_matches_surface(action: &NodeActionDescriptor, surface: &OpenGpuiActionSurface) -> bool {
    match (&action.target, surface) {
        (ActionTarget::Graph, OpenGpuiActionSurface::Graph) => true,
        (ActionTarget::Graph, OpenGpuiActionSurface::Toolbar { .. }) => true,
        (
            ActionTarget::Node { node_kind },
            OpenGpuiActionSurface::Node { node_kind: current }
            | OpenGpuiActionSurface::Toolbar {
                node_kind: Some(current),
            },
        ) => node_kind == current,
        (ActionTarget::Edge, OpenGpuiActionSurface::Edge) => true,
        (ActionTarget::Port { port_key }, OpenGpuiActionSurface::Port { port_key: current }) => {
            port_key == current
        }
        (ActionTarget::Slot { slot_key }, OpenGpuiActionSurface::Slot { slot_key: current }) => {
            slot_key == current
        }
        (
            ActionTarget::Control { control_key },
            OpenGpuiActionSurface::Control {
                control_key: current,
            },
        ) => control_key == current,
        (
            ActionTarget::RepeatableItem {
                collection_key,
                item_id,
            },
            OpenGpuiActionSurface::RepeatableItem {
                collection_key: current_collection,
                item_id: current_item,
            },
        ) => collection_key == current_collection && item_id == current_item,
        (
            ActionTarget::DroppedWire { source_port_key },
            OpenGpuiActionSurface::DroppedWire {
                source_port_key: current,
            },
        ) => source_port_key
            .as_ref()
            .is_none_or(|expected| current.as_ref() == Some(expected)),
        (
            ActionTarget::Inspector { inspector_key },
            OpenGpuiActionSurface::Inspector {
                inspector_key: current,
            },
        ) => inspector_key == current,
        (
            ActionTarget::Blackboard { blackboard_key },
            OpenGpuiActionSurface::Blackboard {
                blackboard_key: current,
            },
        ) => blackboard_key == current,
        _ => false,
    }
}

fn synthetic_menu_label(surface: &OpenGpuiActionSurface) -> String {
    match surface {
        OpenGpuiActionSurface::Graph => "Graph actions",
        OpenGpuiActionSurface::Node { .. } => "Node actions",
        OpenGpuiActionSurface::Edge => "Edge actions",
        OpenGpuiActionSurface::Port { .. } => "Port actions",
        OpenGpuiActionSurface::Slot { .. } => "Slot actions",
        OpenGpuiActionSurface::Control { .. } => "Control actions",
        OpenGpuiActionSurface::RepeatableItem { .. } => "Item actions",
        OpenGpuiActionSurface::DroppedWire { .. } => "Insert compatible node",
        OpenGpuiActionSurface::Toolbar { .. } => "Toolbar actions",
        OpenGpuiActionSurface::Blackboard { .. } => "Blackboard actions",
        OpenGpuiActionSurface::Inspector { .. } => "Inspector actions",
    }
    .to_owned()
}

fn shortcut_label(shortcut: &jellyflow::runtime::schema::ActionShortcut) -> String {
    let mut parts = Vec::new();
    if shortcut.ctrl {
        parts.push("Ctrl");
    }
    if shortcut.shift {
        parts.push("Shift");
    }
    if shortcut.alt {
        parts.push("Alt");
    }
    if shortcut.meta {
        parts.push("Meta");
    }
    parts.push(shortcut.key.as_str());
    parts.join("+")
}

#[cfg(test)]
mod tests {
    use super::*;
    use jellyflow::{
        core::{CanvasPoint, NodeId, PortDirection, PortId},
        runtime::schema::{
            ActionAvailability, MenuDescriptor, MenuSurface, NodeKitRegistry, NodeRegistry,
            NodeSchema,
        },
    };

    #[test]
    fn node_menu_projects_ordered_actions_and_blocks_disabled_dispatch() {
        let mut registry = NodeRegistry::new();
        registry.register(
            NodeSchema::builder("demo.action_node", "Action Node")
                .action(
                    NodeActionDescriptor::new(
                        "action.disabled",
                        "Disabled",
                        ActionTarget::Node {
                            node_kind: "demo.action_node".to_owned(),
                        },
                        ActionIntent::RunNode,
                    )
                    .disabled("Not ready")
                    .with_order(0),
                )
                .action(
                    NodeActionDescriptor::new(
                        "action.run",
                        "Run",
                        ActionTarget::Node {
                            node_kind: "demo.action_node".to_owned(),
                        },
                        ActionIntent::RunNode,
                    )
                    .with_order(1),
                )
                .menu(
                    MenuDescriptor::new("menu.node", MenuSurface::Node)
                        .with_action_keys(["action.run", "action.disabled"]),
                )
                .build(),
        );
        let descriptor = registry
            .view_descriptor(&jellyflow::core::NodeKindKey::new("demo.action_node"))
            .expect("descriptor");
        let menu = project_menu(
            &descriptor,
            descriptor.menu("menu.node").expect("node menu"),
            &OpenGpuiActionSurface::Node {
                node_kind: "demo.action_node".to_owned(),
            },
        );

        assert_eq!(menu.actions[0].key, "action.run");
        assert_eq!(menu.actions[1].key, "action.disabled");
        assert_eq!(
            menu.actions[1].disabled_reason.as_deref(),
            Some("Not ready")
        );
        assert!(plan_action_dispatch(&menu, "action.disabled").is_none());
        assert_eq!(
            plan_action_dispatch(&menu, "action.run")
                .expect("run dispatch")
                .intent,
            ActionIntent::RunNode
        );
    }

    #[test]
    fn dropped_wire_menu_filters_by_source_port_and_plans_insert() {
        let registry = NodeKitRegistry::builtin().node_registry();
        let source = ConnectionHandleRef::new(
            NodeId::from_u128(1),
            PortId::from_u128(2),
            PortDirection::Out,
        );

        let menu = project_dropped_wire_menu(
            &registry,
            source,
            Some(&PortKey::new("completion")),
            CanvasPoint { x: 64.0, y: 96.0 },
        );

        assert!(
            menu.actions
                .iter()
                .any(|action| action.key == "action.insert.llm")
        );
        let insert = plan_dropped_wire_insert(
            &menu,
            "action.insert.llm",
            source,
            CanvasPoint { x: 64.0, y: 96.0 },
        )
        .expect("insert plan");
        assert_eq!(insert.node_kind, "demo.llm");
        assert_eq!(insert.source, source);
        assert_eq!(insert.pointer, CanvasPoint { x: 64.0, y: 96.0 });

        let incompatible = project_dropped_wire_menu(
            &registry,
            source,
            Some(&PortKey::new("other")),
            CanvasPoint::default(),
        );
        assert!(
            incompatible
                .actions
                .iter()
                .all(|action| action.key != "action.insert.llm")
        );
    }

    #[test]
    fn distinct_surfaces_match_distinct_targets() {
        let mut registry = NodeRegistry::new();
        registry.register(
            NodeSchema::builder("demo.surfaces", "Surfaces")
                .actions([
                    NodeActionDescriptor::new(
                        "action.graph",
                        "Graph",
                        ActionTarget::Graph,
                        ActionIntent::Custom {
                            key: "graph".to_owned(),
                        },
                    ),
                    NodeActionDescriptor::new(
                        "action.toolbar",
                        "Toolbar",
                        ActionTarget::Graph,
                        ActionIntent::Custom {
                            key: "toolbar".to_owned(),
                        },
                    )
                    .with_order(-1),
                    NodeActionDescriptor::new(
                        "action.port",
                        "Port",
                        ActionTarget::Port {
                            port_key: "out".to_owned(),
                        },
                        ActionIntent::Custom {
                            key: "port".to_owned(),
                        },
                    ),
                    NodeActionDescriptor::new(
                        "action.slot",
                        "Slot",
                        ActionTarget::Slot {
                            slot_key: "field.title".to_owned(),
                        },
                        ActionIntent::Custom {
                            key: "slot".to_owned(),
                        },
                    ),
                    NodeActionDescriptor::new(
                        "action.item",
                        "Item",
                        ActionTarget::RepeatableItem {
                            collection_key: "items".to_owned(),
                            item_id: "a".to_owned(),
                        },
                        ActionIntent::Custom {
                            key: "item".to_owned(),
                        },
                    ),
                ])
                .build(),
        );
        let descriptor = registry
            .view_descriptor(&jellyflow::core::NodeKindKey::new("demo.surfaces"))
            .expect("descriptor");

        assert_eq!(
            project_actions_for_surface(&descriptor, &OpenGpuiActionSurface::Graph).actions[0].key,
            "action.toolbar"
        );
        assert_eq!(
            project_actions_for_surface(&descriptor, &OpenGpuiActionSurface::Graph).actions[1].key,
            "action.graph"
        );
        assert_eq!(
            project_actions_for_surface(
                &descriptor,
                &OpenGpuiActionSurface::Toolbar { node_kind: None }
            )
            .actions[0]
                .key,
            "action.toolbar"
        );
        assert_eq!(
            project_actions_for_surface(
                &descriptor,
                &OpenGpuiActionSurface::Port {
                    port_key: "out".to_owned()
                }
            )
            .actions[0]
                .key,
            "action.port"
        );
        assert!(
            project_actions_for_surface(
                &descriptor,
                &OpenGpuiActionSurface::Slot {
                    slot_key: "missing".to_owned()
                }
            )
            .actions
            .is_empty()
        );
        assert_eq!(
            project_actions_for_surface(
                &descriptor,
                &OpenGpuiActionSurface::RepeatableItem {
                    collection_key: "items".to_owned(),
                    item_id: "a".to_owned()
                }
            )
            .actions[0]
                .key,
            "action.item"
        );
    }

    #[test]
    fn node_toolbar_surface_can_project_node_actions_without_runtime_toolbar_target() {
        let mut registry = NodeRegistry::new();
        registry.register(
            NodeSchema::builder("demo.toolbar_node", "Toolbar Node")
                .action(NodeActionDescriptor::new(
                    "action.node.run",
                    "Run",
                    ActionTarget::Node {
                        node_kind: "demo.toolbar_node".to_owned(),
                    },
                    ActionIntent::RunNode,
                ))
                .build(),
        );
        let descriptor = registry
            .view_descriptor(&jellyflow::core::NodeKindKey::new("demo.toolbar_node"))
            .expect("descriptor");

        let menu = project_actions_for_surface(
            &descriptor,
            &OpenGpuiActionSurface::Toolbar {
                node_kind: Some("demo.toolbar_node".to_owned()),
            },
        );

        assert_eq!(menu.surface, MenuSurface::Toolbar);
        assert_eq!(menu.actions[0].key, "action.node.run");
        assert_eq!(
            menu.actions[0].target,
            ActionTarget::Node {
                node_kind: "demo.toolbar_node".to_owned(),
            }
        );
    }

    #[test]
    fn disabled_action_plan_preserves_availability_reason() {
        let action = NodeActionDescriptor::new(
            "action.disabled",
            "Disabled",
            ActionTarget::Graph,
            ActionIntent::Custom {
                key: "disabled".to_owned(),
            },
        )
        .disabled("No selection");
        let plan = project_action(&action);

        assert_eq!(
            action.availability,
            ActionAvailability::disabled("No selection")
        );
        assert!(!plan.dispatchable());
        assert_eq!(plan.disabled_reason.as_deref(), Some("No selection"));
    }
}
