use eframe::egui;

use crate::bridge::{DemoGraphError, JellyflowEguiBridge};
use crate::state::JellyflowEguiState;

/// Ready-to-run `eframe` application for Jellyflow node graphs.
pub struct JellyflowEguiApp {
    pub bridge: JellyflowEguiBridge,
    pub state: JellyflowEguiState,
}

impl JellyflowEguiApp {
    pub fn new(bridge: JellyflowEguiBridge) -> Self {
        Self {
            bridge,
            state: JellyflowEguiState::default(),
        }
    }

    pub fn demo() -> Result<Self, DemoGraphError> {
        JellyflowEguiBridge::demo().map(Self::new)
    }
}

impl eframe::App for JellyflowEguiApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        crate::input::handle_global_shortcuts(ui.ctx(), &mut self.bridge, &mut self.state);
        crate::ui::show(ui, &mut self.bridge, &mut self.state);
    }
}
