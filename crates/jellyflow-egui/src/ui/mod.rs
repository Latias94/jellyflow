mod canvas;
mod inspector;
mod palette;
mod toolbar;

use eframe::egui::{CentralPanel, Panel, Ui};

use crate::bridge::JellyflowEguiBridge;
use crate::state::JellyflowEguiState;

pub fn show(ui: &mut Ui, bridge: &mut JellyflowEguiBridge, state: &mut JellyflowEguiState) {
    Panel::top("jellyflow_toolbar").show_inside(ui, |ui| {
        toolbar::show_toolbar(ui, bridge, state);
    });

    Panel::left("jellyflow_palette")
        .resizable(true)
        .default_size(220.0)
        .show_inside(ui, |ui| {
            palette::show_palette(ui, bridge, state);
        });

    Panel::right("jellyflow_inspector")
        .resizable(true)
        .default_size(260.0)
        .show_inside(ui, |ui| {
            inspector::show_inspector(ui, bridge, state);
        });

    CentralPanel::default().show_inside(ui, |ui| {
        canvas::show_canvas(ui, bridge, state);
    });
}
