use eframe::egui::{ComboBox, Ui};

use crate::bridge::JellyflowEguiBridge;
use crate::state::{JellyflowEguiState, LayoutPresetChoice};

pub fn show_toolbar(ui: &mut Ui, bridge: &mut JellyflowEguiBridge, state: &mut JellyflowEguiState) {
    ui.horizontal(|ui| {
        if ui
            .button("Fit")
            .on_hover_text("Fit graph to view")
            .clicked()
            && bridge.fit_view(state.canvas.snapshot.viewport_size)
        {
            state.set_status("Fit view");
        }

        if ui
            .add_enabled(bridge.store().can_undo(), eframe::egui::Button::new("Undo"))
            .clicked()
        {
            match bridge.undo() {
                Ok(Some(_)) => state.set_status("Undo"),
                Ok(None) => state.set_status("Nothing to undo"),
                Err(err) => state.set_status(err.to_string()),
            }
        }

        if ui
            .add_enabled(bridge.store().can_redo(), eframe::egui::Button::new("Redo"))
            .clicked()
        {
            match bridge.redo() {
                Ok(Some(_)) => state.set_status("Redo"),
                Ok(None) => state.set_status("Nothing to redo"),
                Err(err) => state.set_status(err.to_string()),
            }
        }

        ui.separator();

        ComboBox::from_id_salt("jellyflow_layout_preset")
            .selected_text(state.selected_layout_preset.label())
            .show_ui(ui, |ui| {
                for preset in LayoutPresetChoice::ALL {
                    ui.selectable_value(&mut state.selected_layout_preset, preset, preset.label());
                }
            });

        if ui.button("Layout").clicked() {
            match bridge.apply_layout(state.selected_layout_preset) {
                Ok(Some(_)) => state.set_status("Layout applied"),
                Ok(None) => state.set_status("Layout unchanged"),
                Err(err) => state.set_status(err.to_string()),
            }
        }

        if let Some(message) = &state.status_message {
            ui.separator();
            ui.label(message);
        }
    });
}
