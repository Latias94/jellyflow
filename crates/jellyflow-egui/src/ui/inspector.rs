use eframe::egui::{Button, CollapsingHeader, ScrollArea, Ui};
use jellyflow::runtime::schema::{InspectorTarget, NodeKindViewDescriptor};

use crate::bridge::JellyflowEguiBridge;
use crate::state::{JellyflowEguiState, selected_edges, selected_nodes};

pub fn show_inspector(ui: &mut Ui, bridge: &JellyflowEguiBridge, state: &mut JellyflowEguiState) {
    ui.heading("Inspector");
    ui.separator();

    let nodes = selected_nodes(bridge.store());
    let edges = selected_edges(bridge.store());
    if nodes.is_empty() && edges.is_empty() {
        ui.label("No selection");
        return;
    }

    ScrollArea::vertical().show(ui, |ui| {
        if !nodes.is_empty() {
            ui.label(format!("Selected nodes: {}", nodes.len()));
            for node_id in nodes {
                if let Some(node) = bridge.store().graph().nodes().get(&node_id) {
                    ui.group(|ui| {
                        let title = bridge
                            .descriptor_for_node(node_id)
                            .map(|descriptor| descriptor.title)
                            .unwrap_or_else(|| node.kind.0.clone());
                        ui.strong(title);
                        ui.monospace(format!("{node_id:?}"));
                        ui.label(format!("kind: {}", node.kind.0));
                        ui.label(format!("pos: {:.1}, {:.1}", node.pos.x, node.pos.y));
                        if let Some(size) = node.size {
                            ui.label(format!("size: {:.1} x {:.1}", size.width, size.height));
                        }
                        if let Some(descriptor) = bridge.descriptor_for_node(node_id) {
                            show_authoring_descriptor(ui, &descriptor, &node.data);
                        }
                        match serde_json::to_string_pretty(&node.data) {
                            Ok(data) => {
                                state.inspector.data_buffer = data;
                                ui.monospace(&state.inspector.data_buffer);
                            }
                            Err(err) => {
                                ui.label(err.to_string());
                            }
                        }
                    });
                }
            }
        }

        if !edges.is_empty() {
            ui.separator();
            ui.label(format!("Selected edges: {}", edges.len()));
            for edge_id in edges {
                if let Some(edge) = bridge.store().graph().edges().get(&edge_id) {
                    ui.group(|ui| {
                        ui.strong(format!("{edge_id:?}"));
                        ui.label(format!("from: {:?}", edge.from));
                        ui.label(format!("to: {:?}", edge.to));
                        ui.label(format!("kind: {:?}", edge.kind));
                        if let Some(renderer_key) = &edge.view.renderer_key {
                            ui.label(format!("renderer: {renderer_key}"));
                        }
                        if let Some(label) = &edge.view.label {
                            ui.label(format!("label: {label}"));
                        }
                        if edge.data != serde_json::Value::Null {
                            match serde_json::to_string_pretty(&edge.data) {
                                Ok(data) => {
                                    ui.monospace(data);
                                }
                                Err(err) => {
                                    ui.label(err.to_string());
                                }
                            }
                        }
                    });
                }
            }
        }
    });
}

fn show_authoring_descriptor(
    ui: &mut Ui,
    descriptor: &NodeKindViewDescriptor,
    node_data: &serde_json::Value,
) {
    let summary = authoring_summary(descriptor, node_data);
    if summary.is_empty() {
        return;
    }

    ui.separator();
    CollapsingHeader::new("Authoring")
        .default_open(true)
        .show(ui, |ui| {
            if summary.surface_control_count > 0 {
                ui.label(format!("controls: {}", summary.surface_control_count));
                for slot in descriptor
                    .surface_slots
                    .iter()
                    .filter(|slot| !slot.controls.is_empty())
                {
                    ui.collapsing(slot.display_label().unwrap_or(slot.key.as_str()), |ui| {
                        for control in &slot.controls {
                            ui.label(format!(
                                "{} · {:?}",
                                control.display_label().unwrap_or(control.key.as_str()),
                                control.kind
                            ));
                        }
                    });
                }
            }

            if !descriptor.repeatable_collections.is_empty() {
                ui.label(format!(
                    "repeatables: {}",
                    descriptor.repeatable_collections.len()
                ));
                for collection in &descriptor.repeatable_collections {
                    let items = collection.item_projections(node_data);
                    ui.collapsing(
                        format!(
                            "{} ({})",
                            collection
                                .label
                                .as_deref()
                                .unwrap_or(collection.key.as_str()),
                            items.len()
                        ),
                        |ui| {
                            for item in items {
                                ui.label(format!("{} · {}", item.item_id, item.slot_key));
                            }
                        },
                    );
                }
            }

            if !descriptor.actions.is_empty() {
                ui.label(format!("actions: {}", descriptor.actions.len()));
                for action in &descriptor.actions {
                    ui.add_enabled(action.is_enabled(), Button::new(&action.label))
                        .on_hover_text(format!("{:?}", action.intent));
                }
            }

            if !descriptor.menus.is_empty() {
                ui.label(format!("menus: {}", descriptor.menus.len()));
                for menu in &descriptor.menus {
                    ui.label(format!(
                        "{} · {} actions",
                        menu.label.as_deref().unwrap_or(menu.key.as_str()),
                        menu.action_keys.len()
                    ));
                }
            }

            if !descriptor.inspectors.is_empty() {
                ui.label(format!("inspectors: {}", descriptor.inspectors.len()));
                for inspector in &descriptor.inspectors {
                    ui.label(format!(
                        "{} · {}",
                        inspector.label.as_deref().unwrap_or(inspector.key.as_str()),
                        inspector_target_label(&inspector.target)
                    ));
                }
            }

            if !descriptor.blackboards.is_empty() {
                ui.label(format!("blackboards: {}", descriptor.blackboards.len()));
                for blackboard in &descriptor.blackboards {
                    ui.label(format!(
                        "{} · {}",
                        blackboard.label, blackboard.collection.key
                    ));
                }
            }
        });
}

fn inspector_target_label(target: &InspectorTarget) -> String {
    match target {
        InspectorTarget::Graph => "graph".to_owned(),
        InspectorTarget::Node { node_kind } => format!("node:{node_kind}"),
        InspectorTarget::Edge => "edge".to_owned(),
        InspectorTarget::Port { port_key } => format!("port:{port_key}"),
        InspectorTarget::Slot { slot_key } => format!("slot:{slot_key}"),
        InspectorTarget::Control { control_key } => format!("control:{control_key}"),
        InspectorTarget::RepeatableItem {
            collection_key,
            item_id,
        } => {
            format!("repeatable:{collection_key}:{item_id}")
        }
        InspectorTarget::Diagnostic { diagnostic_key } => format!("diagnostic:{diagnostic_key}"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AuthoringSummary {
    surface_control_count: usize,
    repeatable_count: usize,
    action_count: usize,
    menu_count: usize,
    inspector_count: usize,
    blackboard_count: usize,
}

impl AuthoringSummary {
    fn is_empty(self) -> bool {
        self.surface_control_count == 0
            && self.repeatable_count == 0
            && self.action_count == 0
            && self.menu_count == 0
            && self.inspector_count == 0
            && self.blackboard_count == 0
    }
}

fn authoring_summary(
    descriptor: &NodeKindViewDescriptor,
    _node_data: &serde_json::Value,
) -> AuthoringSummary {
    AuthoringSummary {
        surface_control_count: descriptor
            .surface_slots
            .iter()
            .map(|slot| slot.controls.len())
            .sum(),
        repeatable_count: descriptor.repeatable_collections.len(),
        action_count: descriptor.actions.len(),
        menu_count: descriptor.menus.len(),
        inspector_count: descriptor.inspectors.len(),
        blackboard_count: descriptor.blackboards.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jellyflow::runtime::schema::NodeKitRegistry;

    #[test]
    fn authoring_summary_counts_descriptor_surfaces() {
        let registry = NodeKitRegistry::builtin().node_registry();
        let descriptor = registry
            .view_descriptors()
            .into_iter()
            .find(|descriptor| descriptor.kind.0 == "demo.table")
            .expect("table descriptor exists");

        let summary = authoring_summary(&descriptor, &descriptor.default_data);

        assert!(summary.surface_control_count >= 3);
        assert_eq!(summary.repeatable_count, 1);
        assert!(summary.action_count >= 3);
        assert!(summary.menu_count >= 1);
        assert!(summary.inspector_count >= 1);
    }
}
