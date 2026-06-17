use eframe::egui::{ScrollArea, Ui};

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
                    });
                }
            }
        }
    });
}
