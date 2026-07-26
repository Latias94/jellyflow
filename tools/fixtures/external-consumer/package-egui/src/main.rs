use jellyflow_egui::{EguiNativeFrameHost, egui};

fn main() {
    let context = egui::Context::default();
    assert!(context.pixels_per_point().is_finite());
    assert!(std::mem::size_of::<EguiNativeFrameHost>() > 0);
}
