use eframe::egui;

use crate::bridge::JellyflowEguiBridge;
use crate::samples::{SampleGraphError, SampleGraphKind};
use crate::state::{JellyflowEguiState, LayoutPresetChoice};

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

    pub fn sample(kind: SampleGraphKind) -> Result<Self, SampleGraphError> {
        let (bridge, default_layout) = JellyflowEguiBridge::sample(kind)?;
        let mut state = JellyflowEguiState {
            selected_sample: kind,
            selected_layout_preset: default_layout,
            ..JellyflowEguiState::default()
        };
        if matches!(default_layout, LayoutPresetChoice::Freeform) {
            state.set_status("Knowledge board sample loaded");
        }
        Ok(Self { bridge, state })
    }

    pub fn demo() -> Result<Self, SampleGraphError> {
        Self::sample(SampleGraphKind::Workflow)
    }
}

impl eframe::App for JellyflowEguiApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        crate::input::handle_global_shortcuts(ui.ctx(), &mut self.bridge, &mut self.state);
        crate::ui::show(ui, &mut self.bridge, &mut self.state);
    }
}
