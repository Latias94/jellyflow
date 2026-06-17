use jellyflow_egui::JellyflowEguiApp;
use jellyflow_egui::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Jellyflow egui demo")
            .with_inner_size([1280.0, 780.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Jellyflow egui demo",
        options,
        Box::new(|_cc| Ok(Box::new(JellyflowEguiApp::demo()?))),
    )
}
