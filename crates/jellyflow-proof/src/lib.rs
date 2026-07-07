#![deny(unsafe_code)]

use std::borrow::ToOwned;
use std::fmt::Write as _;

use jellyflow::core::{
    CanvasPoint, CanvasRect, CanvasSize, Graph, GraphBuilder, GraphId, Node, NodeId, NodeKindKey,
};
use jellyflow::prelude::*;
use jellyflow::runtime::schema::{
    ActionIntent, ActionTarget, InspectorDescriptor, InspectorTarget, MenuDescriptor, MenuSurface,
    NodeActionDescriptor, NodeControlBinding, NodeControlDescriptor, NodeControlKind,
    NodeKindViewDescriptor, NodeKitRegistry, NodeRegistry, NodeRepeatableAnchorRule,
    NodeRepeatableCollectionDescriptor, NodeSchema, NodeSurfaceSlotDescriptor, NodeSurfaceSlotKind,
    NodeSurfaceSlotVisibility, PortDecl, PortHandleVisibility, PortViewSide,
};
use jellyflow::{NodeGraphEditorConfig, NodeGraphStore, NodeGraphViewState};
use serde_json::json;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofAdapterTrace {
    pub graph_id: GraphId,
    pub nodes: Vec<ProofNodeTrace>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofNodeTrace {
    pub kind: String,
    pub renderer_key: String,
    pub title: String,
    pub summary: Option<String>,
    pub ports: Vec<ProofPortTrace>,
    pub slots: Vec<ProofSlotTrace>,
    pub actions: Vec<String>,
    pub menus: Vec<String>,
    pub inspectors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofPortTrace {
    pub key: String,
    pub direction: PortDirection,
    pub side: PortViewSide,
    pub anchor: Option<String>,
    pub order: Option<i32>,
    pub visibility: Option<PortHandleVisibility>,
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofSlotTrace {
    pub key: String,
    pub kind: NodeSurfaceSlotKind,
    pub label: Option<String>,
    pub data_key: Option<String>,
    pub value: Option<String>,
    pub anchor: Option<String>,
    pub order: Option<i32>,
    pub visibility: Option<NodeSurfaceSlotVisibility>,
    pub controls: Vec<ProofControlTrace>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofControlTrace {
    pub key: String,
    pub kind: NodeControlKind,
    pub label: Option<String>,
    pub data_key: Option<String>,
    pub disabled_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ComponentTreeProof {
    pub framework_family: &'static str,
    pub graph_id: GraphId,
    pub nodes: Vec<ComponentNodeProof>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ComponentNodeProof {
    pub node_id: NodeId,
    pub kind: String,
    pub renderer_key: String,
    pub component_key: String,
    pub children: Vec<ComponentChildProof>,
    pub measurements: Vec<ComponentMeasurementProof>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentChildProof {
    pub key: String,
    pub kind: NodeSurfaceSlotKind,
    pub label: Option<String>,
    pub value: Option<String>,
    pub anchor: Option<String>,
    pub control_count: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ComponentMeasurementProof {
    pub key: String,
    pub rect: CanvasRect,
    pub anchor: Option<String>,
}

impl ProofAdapterTrace {
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn port_count(&self) -> usize {
        self.nodes.iter().map(|node| node.ports.len()).sum()
    }
}

pub fn proof_node_registry() -> NodeRegistry {
    let mut registry = NodeKitRegistry::builtin().node_registry();
    registry.register(
        NodeSchema::builder("proof.review_card", "Review card")
            .category(["Workflow"])
            .renderer_key("review-card")
            .default_size(CanvasSize {
                width: 240.0,
                height: 144.0,
            })
            .port(
                PortDecl::data_input("source")
                    .with_label("source")
                    .on_left()
                    .with_view_anchor("field.assignee"),
            )
            .port(
                PortDecl::data_output("result")
                    .with_label("result")
                    .on_right()
                    .with_view_anchor("actions.primary"),
            )
            .surface_slot(
                NodeSurfaceSlotDescriptor::header("header.main").with_label("Review card"),
            )
            .surface_slot(
                NodeSurfaceSlotDescriptor::field_row("field.assignee")
                    .with_label("Assignee")
                    .with_slot("assignee")
                    .with_anchor("field.assignee")
                    .with_order(0)
                    .with_control(
                        NodeControlDescriptor::text_input("control.assignee")
                            .with_label("Assignee")
                            .with_binding(NodeControlBinding::slot("assignee")),
                    ),
            )
            .surface_slot(
                NodeSurfaceSlotDescriptor::field_row("field.status")
                    .with_label("Status")
                    .with_slot("status")
                    .with_anchor("field.status")
                    .with_order(1)
                    .with_control(
                        NodeControlDescriptor::select("control.status")
                            .with_label("Status")
                            .with_binding(NodeControlBinding::slot("status")),
                    ),
            )
            .surface_slot(
                NodeSurfaceSlotDescriptor::badge("badge.priority")
                    .with_label("Priority")
                    .with_slot("meta.priority")
                    .with_anchor("meta.priority")
                    .with_order(2),
            )
            .surface_slot(
                NodeSurfaceSlotDescriptor::metric_badge("metric.sla")
                    .with_label("SLA")
                    .with_slot("metrics.sla")
                    .with_anchor("metric.sla")
                    .with_order(3),
            )
            .surface_slot(
                NodeSurfaceSlotDescriptor::config_group("config.routing")
                    .with_label("Routing")
                    .with_slot("config.routing")
                    .with_anchor("config.routing")
                    .with_order(4),
            )
            .surface_slot(
                NodeSurfaceSlotDescriptor::status_banner("status.validation")
                    .with_label("Validation")
                    .with_slot("diagnostics.validation")
                    .with_anchor("status.validation")
                    .with_order(5),
            )
            .surface_slot(
                NodeSurfaceSlotDescriptor::action_row("actions.primary")
                    .with_label("Actions")
                    .with_slot("actions.primary")
                    .with_anchor("actions.primary")
                    .with_order(6),
            )
            .repeatable_collection(
                NodeRepeatableCollectionDescriptor::new("review.checklist", "checklist", "id")
                    .with_label("Checklist")
                    .with_item_template_slot(
                        NodeSurfaceSlotDescriptor::field_row("check")
                            .with_label("Check")
                            .with_slot("checklist")
                            .with_control(
                                NodeControlDescriptor::toggle("control.check.done")
                                    .with_label("Done")
                                    .with_binding(NodeControlBinding::data_path("done")),
                            ),
                    )
                    .with_anchor_rule(NodeRepeatableAnchorRule::new("field.check", "field.check"))
                    .with_min_items(1)
                    .with_max_items(8)
                    .reorderable(),
            )
            .action(NodeActionDescriptor::new(
                "action.review.approve",
                "Approve",
                ActionTarget::Node {
                    node_kind: "proof.review_card".to_owned(),
                },
                ActionIntent::Custom {
                    key: "review.approve".to_owned(),
                },
            ))
            .menu(
                MenuDescriptor::new("menu.review_card", MenuSurface::Node)
                    .with_action_key("action.review.approve"),
            )
            .inspector(
                InspectorDescriptor::new(
                    "inspector.check.policy",
                    InspectorTarget::RepeatableItem {
                        collection_key: "review.checklist".to_owned(),
                        item_id: "policy".to_owned(),
                    },
                )
                .with_control(
                    NodeControlDescriptor::toggle("inspector.check.done")
                        .with_label("Done")
                        .with_binding(NodeControlBinding::data_path("done")),
                ),
            )
            .default_data(json!({
                "title": "Review request",
                "summary": "Proof node for adapter boundaries",
                "assignee": "Maya",
                "status": "Waiting",
                "meta": { "priority": "High" },
                "metrics": { "sla": "2h" },
                "config": {
                    "routing": {
                        "team": "Support",
                        "escalation": "Tier 2"
                    }
                },
                "diagnostics": { "validation": "Needs approval" },
                "actions": { "primary": ["Approve", "Reject"] },
                "checklist": [
                    { "id": "policy", "label": "Policy checked", "done": false },
                    { "id": "owner", "label": "Owner assigned", "done": true }
                ]
            }))
            .build(),
    );
    registry
}

pub fn proof_store() -> NodeGraphStore {
    let graph = proof_graph();
    NodeGraphStore::new(
        graph,
        NodeGraphViewState::default(),
        NodeGraphEditorConfig::default(),
    )
}

pub fn proof_graph() -> Graph {
    let registry = proof_node_registry();
    let instantiation = registry
        .instantiate_node(
            &NodeKindKey::new("proof.review_card"),
            CanvasPoint::default(),
        )
        .expect("proof node instantiation");
    let (node_id, node, ports) = instantiation.into_parts();
    let mut builder = GraphBuilder::new(GraphId::new()).with_node(node_id, node);
    for (port_id, port) in ports {
        builder.insert_port(port_id, port);
    }
    builder.build().expect("proof graph")
}

pub fn proof_adapter_trace() -> ProofAdapterTrace {
    let store = proof_store();
    let registry = proof_node_registry();
    proof_adapter_trace_from_store(&store, &registry)
}

pub fn proof_adapter_trace_from_store(
    store: &NodeGraphStore,
    registry: &NodeRegistry,
) -> ProofAdapterTrace {
    let mut nodes: Vec<_> = store
        .graph()
        .nodes()
        .values()
        .filter_map(|node| {
            let descriptor = registry.view_descriptor(&node.kind)?;
            Some(proof_node_trace(store, node, &descriptor))
        })
        .collect();
    nodes.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then_with(|| a.title.cmp(&b.title))
            .then_with(|| a.renderer_key.cmp(&b.renderer_key))
    });

    ProofAdapterTrace {
        graph_id: store.graph().graph_id(),
        nodes,
    }
}

pub fn dioxus_shaped_component_tree_proof() -> ComponentTreeProof {
    let graph = proof_graph();
    component_tree_proof_from_graph("dioxus-component-tree", &graph, &proof_node_registry())
}

pub fn component_tree_proof_from_graph(
    framework_family: &'static str,
    graph: &Graph,
    registry: &NodeRegistry,
) -> ComponentTreeProof {
    let mut nodes = graph
        .nodes()
        .iter()
        .filter_map(|(node_id, node)| {
            let descriptor = registry.view_descriptor(&node.kind)?;
            Some(component_node_proof(*node_id, node, &descriptor))
        })
        .collect::<Vec<_>>();
    nodes.sort_by_key(|node| node.node_id);

    ComponentTreeProof {
        framework_family,
        graph_id: graph.graph_id(),
        nodes,
    }
}

fn component_node_proof(
    node_id: NodeId,
    node: &Node,
    descriptor: &NodeKindViewDescriptor,
) -> ComponentNodeProof {
    let mut children = descriptor
        .surface_slots
        .iter()
        .filter(|slot| slot.is_visible())
        .map(|slot| ComponentChildProof {
            key: slot.key.clone(),
            kind: slot.kind,
            label: slot.display_label().map(ToOwned::to_owned),
            value: slot_value_preview(&node.data, slot),
            anchor: slot.anchor.clone(),
            control_count: slot.controls.len(),
        })
        .collect::<Vec<_>>();
    children.extend(
        descriptor
            .repeatable_collections
            .iter()
            .flat_map(|collection| collection.item_projections(&node.data))
            .flat_map(|item| {
                item.slots
                    .iter()
                    .filter(|slot| slot.is_visible())
                    .map(|slot| ComponentChildProof {
                        key: slot.key.clone(),
                        kind: slot.kind,
                        label: slot.display_label().map(ToOwned::to_owned),
                        value: slot_value_preview(&item.item_data, slot),
                        anchor: slot.anchor.clone(),
                        control_count: slot.controls.len(),
                    })
                    .collect::<Vec<_>>()
            }),
    );
    let measurements = children
        .iter()
        .enumerate()
        .map(|(index, child)| ComponentMeasurementProof {
            key: child.key.clone(),
            rect: component_child_rect(index),
            anchor: child.anchor.clone(),
        })
        .collect();

    ComponentNodeProof {
        node_id,
        kind: node.kind.0.clone(),
        renderer_key: descriptor.renderer_key.clone(),
        component_key: format!("{}::{}", descriptor.renderer_key, node.kind.0),
        children,
        measurements,
    }
}

pub fn render_proof_trace(trace: &ProofAdapterTrace) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "proof trace: graph={} nodes={} ports={}",
        trace.graph_id,
        trace.node_count(),
        trace.port_count()
    );

    for node in &trace.nodes {
        let _ = writeln!(
            out,
            "- {} [{}] title={} summary={}",
            node.kind,
            node.renderer_key,
            node.title,
            node.summary.as_deref().unwrap_or("-")
        );
        for slot in &node.slots {
            let _ = writeln!(
                out,
                "  slot {} ({:?}) data_key={} value={} anchor={} order={} visibility={}",
                slot.key,
                slot.kind,
                slot.data_key.as_deref().unwrap_or("-"),
                slot.value.as_deref().unwrap_or("-"),
                slot.anchor.as_deref().unwrap_or("-"),
                slot.order
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".to_owned()),
                slot.visibility
                    .map(|visibility| format!("{visibility:?}"))
                    .unwrap_or_else(|| "-".to_owned())
            );
            for control in &slot.controls {
                let _ = writeln!(
                    out,
                    "    control {} ({:?}) data_key={} disabled={}",
                    control.key,
                    control.kind,
                    control.data_key.as_deref().unwrap_or("-"),
                    control.disabled_reason.as_deref().unwrap_or("-")
                );
            }
        }
        for port in &node.ports {
            let _ = writeln!(
                out,
                "  port {} {:?} side={:?} anchor={} order={} visibility={}",
                port.key,
                port.direction,
                port.side,
                port.anchor.as_deref().unwrap_or("-"),
                port.order
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".to_owned()),
                port.visibility
                    .map(|visibility| format!("{visibility:?}"))
                    .unwrap_or_else(|| "-".to_owned())
            );
        }
        for action in &node.actions {
            let _ = writeln!(out, "  action {action}");
        }
        for menu in &node.menus {
            let _ = writeln!(out, "  menu {menu}");
        }
        for inspector in &node.inspectors {
            let _ = writeln!(out, "  inspector {inspector}");
        }
    }

    out
}

fn proof_node_trace(
    store: &NodeGraphStore,
    node: &Node,
    descriptor: &NodeKindViewDescriptor,
) -> ProofNodeTrace {
    let title = node_title(node).unwrap_or_else(|| descriptor.title.clone());
    let summary = node_summary(node);
    let ports = node
        .ports
        .iter()
        .filter_map(|port_id| {
            let port = store.graph().ports().get(port_id)?;
            let decl = descriptor.port_decl(&port.key.0);
            Some(ProofPortTrace {
                key: port.key.0.clone(),
                direction: port.dir,
                side: decl
                    .map(|decl| decl.view.resolved_side(port.dir))
                    .unwrap_or_else(|| PortViewSide::fallback_for_direction(port.dir)),
                anchor: decl.and_then(|decl| decl.view.anchor.clone()),
                order: decl.and_then(|decl| decl.view.order),
                visibility: decl.and_then(|decl| decl.view.visibility),
                label: decl
                    .and_then(|decl| decl.label.clone())
                    .or_else(|| Some(port.key.0.clone())),
            })
        })
        .collect();
    let mut slots = descriptor
        .surface_slots
        .iter()
        .map(|slot| ProofSlotTrace {
            key: slot.key.clone(),
            kind: slot.kind,
            label: slot.label.clone(),
            data_key: slot.data_key().map(ToOwned::to_owned),
            value: slot_value_preview(&node.data, slot),
            anchor: slot.anchor.clone(),
            order: slot.order,
            visibility: slot.visibility,
            controls: slot.controls.iter().map(proof_control_trace).collect(),
        })
        .collect::<Vec<_>>();
    slots.extend(
        descriptor
            .repeatable_collections
            .iter()
            .flat_map(|collection| collection.item_projections(&node.data))
            .flat_map(|item| {
                item.slots
                    .iter()
                    .map(|slot| ProofSlotTrace {
                        key: slot.key.clone(),
                        kind: slot.kind,
                        label: slot.label.clone(),
                        data_key: slot.data_key().map(ToOwned::to_owned),
                        value: slot_value_preview(&item.item_data, slot),
                        anchor: slot.anchor.clone(),
                        order: slot.order,
                        visibility: slot.visibility,
                        controls: slot.controls.iter().map(proof_control_trace).collect(),
                    })
                    .collect::<Vec<_>>()
            }),
    );

    ProofNodeTrace {
        kind: node.kind.0.clone(),
        renderer_key: descriptor.renderer_key.clone(),
        title,
        summary,
        ports,
        slots,
        actions: descriptor
            .actions
            .iter()
            .map(|action| action.key.clone())
            .collect(),
        menus: descriptor
            .menus
            .iter()
            .map(|menu| menu.key.clone())
            .collect(),
        inspectors: descriptor
            .inspectors
            .iter()
            .map(|inspector| inspector.key.clone())
            .collect(),
    }
}

fn proof_control_trace(control: &NodeControlDescriptor) -> ProofControlTrace {
    ProofControlTrace {
        key: control.key.clone(),
        kind: control.kind,
        label: control.label.clone(),
        data_key: control.data_key().map(ToOwned::to_owned),
        disabled_reason: control.editability.disabled_reason.clone(),
    }
}

fn component_child_rect(index: usize) -> CanvasRect {
    CanvasRect {
        origin: CanvasPoint {
            x: 12.0,
            y: 16.0 + index as f32 * 28.0,
        },
        size: CanvasSize {
            width: 216.0,
            height: 22.0,
        },
    }
}

fn node_summary(node: &Node) -> Option<String> {
    let summary = node
        .data
        .get("summary")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    (!summary.is_empty()).then(|| summary.to_owned())
}

fn node_title(node: &Node) -> Option<String> {
    let title = node
        .data
        .get("title")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    (!title.is_empty()).then(|| title.to_owned())
}

fn slot_value_preview(
    node_data: &serde_json::Value,
    slot: &NodeSurfaceSlotDescriptor,
) -> Option<String> {
    let key = slot.data_key()?;
    let value = semantic_json_lookup(node_data, key)?;
    let preview = json_value_preview(value);
    (!preview.is_empty()).then_some(preview)
}

fn semantic_json_lookup<'a>(
    value: &'a serde_json::Value,
    path: &str,
) -> Option<&'a serde_json::Value> {
    let mut current = value;
    for segment in path.split('.') {
        current = current.get(segment)?;
    }
    Some(current)
}

fn json_value_preview(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Bool(value) => value.to_string(),
        serde_json::Value::Number(value) => value.to_string(),
        serde_json::Value::Array(items) => {
            let preview = items
                .iter()
                .take(2)
                .map(json_value_preview)
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join(" · ");
            if preview.is_empty() {
                format!("{} items", items.len())
            } else if items.len() > 2 {
                format!("{preview} …")
            } else {
                preview
            }
        }
        serde_json::Value::Object(map) => {
            if let Some(text) = map.get("label").and_then(serde_json::Value::as_str) {
                return text.to_owned();
            }
            if let Some(text) = map.get("title").and_then(serde_json::Value::as_str) {
                return text.to_owned();
            }
            let preview = map
                .iter()
                .take(2)
                .map(|(key, value)| format!("{key}: {}", json_value_preview(value)))
                .collect::<Vec<_>>()
                .join(" · ");
            if preview.is_empty() {
                "{}".to_owned()
            } else {
                preview
            }
        }
        serde_json::Value::Null => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proof_registry_exposes_a_rich_node_surface() {
        let registry = proof_node_registry();
        let descriptor = registry
            .view_descriptor(&NodeKindKey::new("proof.review_card"))
            .expect("descriptor");

        assert_eq!(descriptor.renderer_key, "review-card");
        assert_eq!(descriptor.surface_slots.len(), 8);
        assert_eq!(descriptor.repeatable_collections.len(), 1);
        assert_eq!(descriptor.actions.len(), 1);
        assert_eq!(descriptor.menus.len(), 1);
        assert_eq!(descriptor.inspectors.len(), 1);
        assert_eq!(descriptor.surface_slots[0].key, "header.main");
        assert_eq!(descriptor.surface_slots[7].key, "actions.primary");
        assert!(
            descriptor
                .surface_slot("field.assignee")
                .expect("assignee slot")
                .controls
                .iter()
                .any(|control| control.kind == NodeControlKind::TextInput)
        );
        assert_eq!(
            descriptor
                .surface_slot_by_anchor("actions.primary")
                .map(|slot| slot.key.as_str()),
            Some("actions.primary")
        );
        assert_eq!(
            descriptor
                .port_decl_by_anchor("field.assignee")
                .map(|decl| decl.key.0.as_str()),
            Some("source")
        );
    }

    #[test]
    fn proof_graph_builds_with_concrete_nodes_and_ports() {
        let graph = proof_graph();
        assert_eq!(graph.nodes().len(), 1);
        assert_eq!(graph.ports().len(), 2);
    }

    #[test]
    fn proof_adapter_trace_is_deterministic_and_rich() {
        let trace = proof_adapter_trace();
        assert_eq!(trace.node_count(), 1);
        assert_eq!(trace.port_count(), 2);

        let node = &trace.nodes[0];
        assert_eq!(node.kind, "proof.review_card");
        assert_eq!(node.renderer_key, "review-card");
        assert_eq!(node.title, "Review request");
        assert_eq!(
            node.summary.as_deref(),
            Some("Proof node for adapter boundaries")
        );
        assert_eq!(node.slots.len(), 10);
        assert_eq!(node.slots[1].data_key.as_deref(), Some("assignee"));
        assert_eq!(node.slots[1].controls[0].key, "control.assignee");
        assert_eq!(
            node.slots[1].controls[0].data_key.as_deref(),
            Some("assignee")
        );
        assert!(
            node.slots
                .iter()
                .any(|slot| slot.anchor.as_deref() == Some("field.check.policy.check"))
        );
        assert_eq!(node.ports[0].anchor.as_deref(), Some("field.assignee"));
        assert_eq!(node.ports[1].side, PortViewSide::Right);
        assert_eq!(node.actions, vec!["action.review.approve"]);
        assert_eq!(node.menus, vec!["menu.review_card"]);
        assert_eq!(node.inspectors, vec!["inspector.check.policy"]);

        let rendered = render_proof_trace(&trace);
        assert!(rendered.contains("proof trace: graph="));
        assert!(rendered.contains("Review request"));
        assert!(rendered.contains("Maya"));
        assert!(rendered.contains("Waiting"));
        assert!(rendered.contains("MetricBadge"));
        assert!(rendered.contains("ConfigGroup"));
        assert!(rendered.contains("StatusBanner"));
        assert!(rendered.contains("slot field.assignee"));
        assert!(rendered.contains("control control.assignee"));
        assert!(rendered.contains("field.check.policy.check"));
        assert!(rendered.contains("action action.review.approve"));
        assert!(rendered.contains("menu menu.review_card"));
        assert!(rendered.contains("inspector inspector.check.policy"));
        assert!(rendered.contains("port source"));
        assert!(rendered.contains("visibility="));
    }

    #[test]
    fn dioxus_shaped_component_tree_consumes_builtin_kits_without_framework_types() {
        let proof = dioxus_shaped_component_tree_proof();

        assert_eq!(proof.framework_family, "dioxus-component-tree");
        assert_eq!(proof.nodes.len(), 1);
        let review = proof
            .nodes
            .iter()
            .find(|node| node.kind == "proof.review_card")
            .expect("component proof includes the review card node");

        assert_eq!(review.renderer_key, "review-card");
        assert!(review.component_key.contains("review-card"));
        assert!(
            review
                .children
                .iter()
                .any(|child| child.kind == NodeSurfaceSlotKind::ConfigGroup)
        );
        assert!(
            review
                .children
                .iter()
                .any(|child| child.kind == NodeSurfaceSlotKind::StatusBanner)
        );
        assert!(
            review
                .children
                .iter()
                .any(|child| child.anchor.as_deref() == Some("field.assignee"))
        );
        assert!(
            review
                .children
                .iter()
                .any(|child| child.key == "field.assignee" && child.control_count == 1)
        );
        assert!(
            review
                .children
                .iter()
                .any(|child| child.anchor.as_deref() == Some("field.check.policy.check"))
        );
        assert!(review.measurements.iter().all(|measurement| {
            measurement.rect.is_positive_finite()
                && review
                    .children
                    .iter()
                    .any(|child| child.key == measurement.key)
        }));
    }
}
