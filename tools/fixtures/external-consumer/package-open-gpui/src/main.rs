use jellyflow_open_gpui::authoring::OpenGpuiNodeAuthoring;
use jellyflow_open_gpui::registry::{OpenGpuiNodeRegistry, OpenGpuiRendererRegistrationStatus};
use jellyflow_open_gpui::theme::OpenGpuiGraphTheme;

fn readme_authoring_version_compiles(authoring: &OpenGpuiNodeAuthoring) {
    let _version = authoring.version();
}

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
    let _readme_authoring_version_compiles: fn(&OpenGpuiNodeAuthoring) =
        readme_authoring_version_compiles;
}
