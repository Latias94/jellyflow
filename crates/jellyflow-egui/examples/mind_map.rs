use jellyflow_egui::egui;
use jellyflow_egui::{JellyflowEguiApp, SampleGraphKind};

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Jellyflow mind map sample")
            .with_inner_size([1280.0, 780.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Jellyflow mind map sample",
        options,
        Box::new(|_cc| {
            Ok(Box::new(JellyflowEguiApp::sample(
                SampleGraphKind::MindMap,
            )?))
        }),
    )
}
