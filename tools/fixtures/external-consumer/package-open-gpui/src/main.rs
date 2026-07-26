use jellyflow_open_gpui::registry::{OpenGpuiNodeRegistry, OpenGpuiRendererRegistrationStatus};
use jellyflow_open_gpui::theme::OpenGpuiGraphTheme;

fn main() {
    let registry = OpenGpuiNodeRegistry::builder()
        .seal()
        .expect("empty package registry is valid");
    assert_eq!(
        registry.status("package.missing"),
        OpenGpuiRendererRegistrationStatus::Missing
    );

    let theme = OpenGpuiGraphTheme::default();
    assert!(theme.metrics.port_hit_diameter.is_sign_positive());
    let _native_element = open_gpui::div();
}
