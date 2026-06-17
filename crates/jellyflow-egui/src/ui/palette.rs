use eframe::egui::{ScrollArea, TextEdit, Ui};
use jellyflow::runtime::schema::NodeKindViewDescriptor;

use crate::bridge::JellyflowEguiBridge;
use crate::state::{CanvasTool, JellyflowEguiState};

pub fn show_palette(ui: &mut Ui, bridge: &JellyflowEguiBridge, state: &mut JellyflowEguiState) {
    ui.heading("Nodes");
    ui.add(
        TextEdit::singleline(&mut state.palette_filter)
            .hint_text("Filter")
            .desired_width(f32::INFINITY),
    );
    ui.separator();

    let filter = state.palette_filter.trim().to_ascii_lowercase();
    ScrollArea::vertical().show(ui, |ui| {
        for descriptor in bridge.descriptors() {
            if !matches_filter(&descriptor, &filter) {
                continue;
            }
            let selected = state.pending_create_kind.as_ref() == Some(&descriptor.kind);
            if ui
                .selectable_label(selected, descriptor_label(&descriptor))
                .on_hover_text("Click a point on the canvas to create this node")
                .clicked()
            {
                state.pending_create_kind = Some(descriptor.kind.clone());
                state.canvas_tool = CanvasTool::CreateNode;
                state.set_status(format!("Creating {}", descriptor.title));
            }
        }
    });
}

fn matches_filter(descriptor: &NodeKindViewDescriptor, filter: &str) -> bool {
    if filter.is_empty() {
        return true;
    }
    descriptor.title.to_ascii_lowercase().contains(filter)
        || descriptor
            .renderer_key
            .to_ascii_lowercase()
            .contains(filter)
        || descriptor.kind.0.to_ascii_lowercase().contains(filter)
        || descriptor
            .category
            .iter()
            .any(|value| value.to_ascii_lowercase().contains(filter))
        || descriptor
            .keywords
            .iter()
            .any(|value| value.to_ascii_lowercase().contains(filter))
}

fn descriptor_label(descriptor: &NodeKindViewDescriptor) -> String {
    let category = descriptor.category.join(" / ");
    if category.is_empty() {
        descriptor.title.clone()
    } else {
        format!("{category} / {}", descriptor.title)
    }
}
