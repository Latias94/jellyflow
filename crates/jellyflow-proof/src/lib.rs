#![deny(unsafe_code)]

use jellyflow::core::GraphBuilder;
use jellyflow::prelude::*;
use jellyflow::runtime::schema::{NodeRegistry, NodeSchema, NodeSurfaceSlotDescriptor, PortDecl};
use jellyflow::{NodeGraphEditorConfig, NodeGraphStore, NodeGraphViewState};
use serde_json::json;

pub fn proof_node_registry() -> NodeRegistry {
    let mut registry = NodeRegistry::new();
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
                    .with_order(0),
            )
            .surface_slot(
                NodeSurfaceSlotDescriptor::field_row("field.status")
                    .with_label("Status")
                    .with_slot("status")
                    .with_anchor("field.status")
                    .with_order(1),
            )
            .surface_slot(
                NodeSurfaceSlotDescriptor::badge("badge.priority")
                    .with_label("Priority")
                    .with_slot("meta.priority")
                    .with_anchor("meta.priority")
                    .with_order(2),
            )
            .surface_slot(
                NodeSurfaceSlotDescriptor::action_row("actions.primary")
                    .with_label("Actions")
                    .with_slot("actions.primary")
                    .with_anchor("actions.primary")
                    .with_order(3),
            )
            .default_data(json!({
                "title": "Review request",
                "summary": "Proof node for adapter boundaries",
                "assignee": "Maya",
                "status": "Waiting",
                "meta": { "priority": "High" },
                "actions": { "primary": ["Approve", "Reject"] }
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
        assert_eq!(descriptor.surface_slots.len(), 5);
        assert_eq!(descriptor.surface_slots[0].key, "header.main");
        assert_eq!(descriptor.surface_slots[4].key, "actions.primary");
    }

    #[test]
    fn proof_graph_builds_with_concrete_nodes_and_ports() {
        let graph = proof_graph();
        assert_eq!(graph.nodes().len(), 1);
        assert_eq!(graph.ports().len(), 2);
    }
}
