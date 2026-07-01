use jellyflow::{
    NodeGraphEditorConfig, NodeGraphViewState,
    core::NodeGraphConnectionMode,
    runtime::{
        DispatchError,
        rules::{
            ConnectPlan, Diagnostic, plan_connect_typed_with_mode_and_policy,
            plan_connect_with_mode_and_policy,
        },
        runtime::create_node::CREATE_NODE_TRANSACTION_LABEL,
    },
};
use jellyflow::{
    NodeGraphStore,
    core::{
        CanvasPoint, Graph, GraphTransaction, NodeId, NodeKindKey, PortDirection, PortId, PortKey,
    },
    runtime::{
        runtime::connection::{CONNECT_EDGE_TRANSACTION_LABEL, ConnectionHandleRef},
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

/// Atomic transaction plan for applying a dropped-wire insert selection.
#[derive(Debug, Clone)]
pub struct OpenGpuiDroppedWireInsertTransactionPlan {
    pub insert: OpenGpuiDroppedWireInsertPlan,
    pub node_id: NodeId,
    pub target_port: PortId,
    pub transaction: GraphTransaction,
}

/// Result of committing a dropped-wire insert through the store.
#[derive(Debug, Clone)]
pub struct OpenGpuiDroppedWireInsertOutcome {
    pub plan: OpenGpuiDroppedWireInsertTransactionPlan,
    pub dispatch: jellyflow::DispatchOutcome,
}

/// Failure path for dropped-wire insert planning or dispatch.
#[derive(Debug, thiserror::Error)]
pub enum OpenGpuiDroppedWireInsertError {
    #[error("dropped-wire action is not dispatchable: {action_key}")]
    MissingOrDisabledAction { action_key: String },
    #[error("node kind schema not found: {node_kind}")]
    MissingSchema { node_kind: String },
    #[error("source handle is missing from graph: {port:?}")]
    MissingSourcePort { port: PortId },
    #[error("no compatible target port found for node kind {node_kind}")]
    NoCompatibleTargetPort {
        node_kind: String,
        diagnostics: Vec<Diagnostic>,
    },
    #[error("insert transaction did not apply to scratch graph: {0}")]
    ScratchApply(String),
    #[error(transparent)]
    Dispatch(#[from] DispatchError),
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

/// Plans the atomic graph mutation behind a dropped-wire insert selection.
///
/// The adapter creates a schema node and then asks Jellyflow's existing connection rules to choose
/// a valid source-to-new-node connection. The returned transaction contains both the create-node
/// ops and the accepted connection ops, so a rejected connection cannot leave an orphan node.
pub fn plan_dropped_wire_insert_transaction(
    graph: &Graph,
    registry: &NodeRegistry,
    menu: &OpenGpuiMenuPlan,
    action_key: &str,
    source: ConnectionHandleRef,
    pointer: CanvasPoint,
    mode: NodeGraphConnectionMode,
) -> Result<OpenGpuiDroppedWireInsertTransactionPlan, OpenGpuiDroppedWireInsertError> {
    let insert = plan_dropped_wire_insert(menu, action_key, source, pointer).ok_or_else(|| {
        OpenGpuiDroppedWireInsertError::MissingOrDisabledAction {
            action_key: action_key.to_owned(),
        }
    })?;
    let node_kind = NodeKindKey::new(insert.node_kind.clone());
    let instantiation = registry
        .instantiate_node(&node_kind, pointer)
        .map_err(|_| OpenGpuiDroppedWireInsertError::MissingSchema {
            node_kind: insert.node_kind.clone(),
        })?;
    let candidate_ports =
        dropped_wire_target_port_candidates(graph, &insert.source, &instantiation.ports)?;
    let create_ops = instantiation.clone().into_ops();
    let mut scratch = graph.clone();
    GraphTransaction::from_ops(create_ops.clone())
        .with_label(CREATE_NODE_TRANSACTION_LABEL)
        .apply_to(&mut scratch)
        .map_err(|error| OpenGpuiDroppedWireInsertError::ScratchApply(error.to_string()))?;

    let interaction_store = NodeGraphStore::new(
        graph.clone(),
        NodeGraphViewState::default(),
        NodeGraphEditorConfig::default(),
    );
    let interaction = interaction_store.resolved_interaction_state();
    let mut diagnostics = Vec::new();

    for candidate in candidate_ports {
        let connect_plan = plan_dropped_wire_connection(
            &scratch,
            insert.source.port,
            candidate,
            mode,
            &interaction,
        );
        if connect_plan.is_reject() {
            diagnostics.extend(connect_plan.diagnostics().iter().cloned());
            continue;
        }
        if connect_plan.ops().is_empty() {
            continue;
        }

        let mut ops = create_ops;
        ops.extend(connect_plan.into_ops());
        return Ok(OpenGpuiDroppedWireInsertTransactionPlan {
            insert,
            node_id: instantiation.node_id,
            target_port: candidate,
            transaction: GraphTransaction::from_ops(ops).with_label(format!(
                "{CREATE_NODE_TRANSACTION_LABEL} + {CONNECT_EDGE_TRANSACTION_LABEL}"
            )),
        });
    }

    Err(OpenGpuiDroppedWireInsertError::NoCompatibleTargetPort {
        node_kind: insert.node_kind,
        diagnostics,
    })
}

/// Commits a dropped-wire insert selection through the normal Jellyflow store dispatch path.
pub fn apply_dropped_wire_insert(
    store: &mut NodeGraphStore,
    registry: &NodeRegistry,
    menu: &OpenGpuiMenuPlan,
    action_key: &str,
    source: ConnectionHandleRef,
    pointer: CanvasPoint,
    mode: NodeGraphConnectionMode,
) -> Result<OpenGpuiDroppedWireInsertOutcome, OpenGpuiDroppedWireInsertError> {
    let plan = plan_dropped_wire_insert_transaction(
        store.graph(),
        registry,
        menu,
        action_key,
        source,
        pointer,
        mode,
    )?;
    let dispatch = store.dispatch_transaction(&plan.transaction)?;
    Ok(OpenGpuiDroppedWireInsertOutcome { plan, dispatch })
}

fn dropped_wire_target_port_candidates(
    graph: &Graph,
    source: &ConnectionHandleRef,
    ports: &[(PortId, jellyflow::core::Port)],
) -> Result<Vec<PortId>, OpenGpuiDroppedWireInsertError> {
    if !graph.ports().contains_key(&source.port) {
        return Err(OpenGpuiDroppedWireInsertError::MissingSourcePort { port: source.port });
    }
    let opposite_direction = match source.direction {
        PortDirection::In => PortDirection::Out,
        PortDirection::Out => PortDirection::In,
    };
    Ok(ports
        .iter()
        .filter_map(|(id, port)| (port.dir == opposite_direction).then_some(*id))
        .collect())
}

fn plan_dropped_wire_connection(
    graph: &Graph,
    source: PortId,
    target: PortId,
    mode: NodeGraphConnectionMode,
    interaction: &jellyflow::runtime::io::NodeGraphInteractionState,
) -> ConnectPlan {
    let has_typed_endpoint = |port_id: PortId| {
        graph
            .ports()
            .get(&port_id)
            .is_some_and(|port| port.ty.is_some())
    };
    if has_typed_endpoint(source) || has_typed_endpoint(target) {
        let mut compat = jellyflow::core::DefaultTypeCompatibility;
        return plan_connect_typed_with_mode_and_policy(
            graph,
            source,
            target,
            mode,
            interaction,
            |graph, port| graph.ports().get(&port).and_then(|port| port.ty.clone()),
            &mut compat,
        );
    }

    plan_connect_with_mode_and_policy(graph, source, target, mode, interaction)
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
        NodeGraphStore,
        core::{CanvasPoint, NodeId, PortDirection, PortId},
        runtime::io::{NodeGraphEditorConfig, NodeGraphViewState},
        runtime::schema::{
            ActionAvailability, MenuDescriptor, MenuSurface, NodeKitRegistry, NodeRegistry,
            NodeSchema, PortDecl,
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
    fn dropped_wire_insert_transaction_creates_node_and_connects_source_atomically() {
        let mut registry = NodeRegistry::new();
        registry.register(
            NodeSchema::builder("demo.source_out", "Source")
                .port(PortDecl::data_output("out"))
                .action(NodeActionDescriptor::new(
                    "action.insert.target",
                    "Insert target",
                    ActionTarget::DroppedWire {
                        source_port_key: Some("out".to_owned()),
                    },
                    ActionIntent::InsertNode {
                        node_kind: "demo.target_in".to_owned(),
                    },
                ))
                .build(),
        );
        registry.register(
            NodeSchema::builder("demo.target_in", "Target")
                .port(PortDecl::data_input("in"))
                .build(),
        );
        let mut store = NodeGraphStore::new(
            jellyflow::core::Graph::new(jellyflow::core::GraphId::from_u128(1)),
            NodeGraphViewState::default(),
            NodeGraphEditorConfig::default(),
        );
        let source = store
            .apply_create_node_from_schema(
                &registry,
                jellyflow::runtime::runtime::create_node::CreateNodeRequest::new(
                    jellyflow::core::NodeKindKey::new("demo.source_out"),
                    CanvasPoint { x: 0.0, y: 0.0 },
                ),
            )
            .expect("source node creates");
        let source_node = source.node_id();
        let source_port = source.port_ids().next().expect("source output exists");
        let handle = ConnectionHandleRef::new(source_node, source_port, PortDirection::Out);
        let menu = project_dropped_wire_menu(
            &registry,
            handle,
            Some(&PortKey::new("out")),
            CanvasPoint { x: 240.0, y: 96.0 },
        );

        let outcome = apply_dropped_wire_insert(
            &mut store,
            &registry,
            &menu,
            "action.insert.target",
            handle,
            CanvasPoint { x: 240.0, y: 96.0 },
            jellyflow::core::NodeGraphConnectionMode::Strict,
        )
        .expect("dropped-wire insert should commit");

        assert_eq!(outcome.plan.insert.node_kind, "demo.target_in");
        assert!(store.graph().nodes().contains_key(&outcome.plan.node_id));
        assert_eq!(store.graph().edges().len(), 1);
        assert_eq!(
            outcome.dispatch.committed().label(),
            Some("create node + connect edge")
        );
        assert!(matches!(
            outcome.dispatch.committed().ops(),
            [
                jellyflow::core::GraphOp::AddNode { .. },
                jellyflow::core::GraphOp::AddPort { .. },
                jellyflow::core::GraphOp::SetNodePorts { .. },
                jellyflow::core::GraphOp::AddEdge { .. },
            ]
        ));
    }

    #[test]
    fn dropped_wire_insert_rejection_leaves_graph_unchanged() {
        let mut registry = NodeRegistry::new();
        registry.register(
            NodeSchema::builder("demo.source_out", "Source")
                .port(PortDecl::data_output("out"))
                .action(NodeActionDescriptor::new(
                    "action.insert.bad",
                    "Insert bad",
                    ActionTarget::DroppedWire {
                        source_port_key: Some("out".to_owned()),
                    },
                    ActionIntent::InsertNode {
                        node_kind: "demo.inputless".to_owned(),
                    },
                ))
                .build(),
        );
        registry.register(
            NodeSchema::builder("demo.inputless", "Inputless")
                .port(PortDecl::data_output("out"))
                .build(),
        );
        let mut store = NodeGraphStore::new(
            jellyflow::core::Graph::new(jellyflow::core::GraphId::from_u128(2)),
            NodeGraphViewState::default(),
            NodeGraphEditorConfig::default(),
        );
        let source = store
            .apply_create_node_from_schema(
                &registry,
                jellyflow::runtime::runtime::create_node::CreateNodeRequest::new(
                    jellyflow::core::NodeKindKey::new("demo.source_out"),
                    CanvasPoint::default(),
                ),
            )
            .expect("source creates");
        let source_port = source.port_ids().next().expect("source port exists");
        let before_nodes = store.graph().nodes().len();
        let before_ports = store.graph().ports().len();
        let handle = ConnectionHandleRef::new(source.node_id(), source_port, PortDirection::Out);
        let menu = project_dropped_wire_menu(
            &registry,
            handle,
            Some(&PortKey::new("out")),
            CanvasPoint { x: 64.0, y: 96.0 },
        );

        let error = apply_dropped_wire_insert(
            &mut store,
            &registry,
            &menu,
            "action.insert.bad",
            handle,
            CanvasPoint { x: 64.0, y: 96.0 },
            jellyflow::core::NodeGraphConnectionMode::Strict,
        )
        .expect_err("inputless insert should be rejected before dispatch");

        assert!(matches!(
            error,
            OpenGpuiDroppedWireInsertError::NoCompatibleTargetPort { .. }
        ));
        assert_eq!(store.graph().nodes().len(), before_nodes);
        assert_eq!(store.graph().ports().len(), before_ports);
        assert!(store.graph().edges().is_empty());
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
