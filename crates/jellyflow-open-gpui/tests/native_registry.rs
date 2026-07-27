use jellyflow_open_gpui::authoring::OpenGpuiNodeBinding;
use jellyflow_open_gpui::registry::{
    OpenGpuiNodeRegistry, OpenGpuiNodeRegistryIssueKind, OpenGpuiRendererRegistrationStatus,
};
use open_gpui::{App, AppContext, Context, Entity, ParentElement, Render, Window, div};

struct WorkflowNode {
    _binding: Entity<OpenGpuiNodeBinding>,
}

impl Render for WorkflowNode {
    fn render(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        div().child("workflow")
    }
}

struct ShaderNode {
    _binding: Entity<OpenGpuiNodeBinding>,
}

impl Render for ShaderNode {
    fn render(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        div().child("shader")
    }
}

#[test]
fn sealed_registry_accepts_different_retained_render_types() {
    let mut builder = OpenGpuiNodeRegistry::builder();
    builder.register(
        "workflow.llm",
        |binding, _window: &mut Window, cx: &mut App| {
            cx.new(|_| WorkflowNode { _binding: binding })
        },
    );
    builder.register(
        "shader.multiply",
        |binding, _window: &mut Window, cx: &mut App| cx.new(|_| ShaderNode { _binding: binding }),
    );

    let registry = builder.seal().expect("valid renderer registrations");

    assert_eq!(registry.len(), 2);
    assert_eq!(
        registry.status("workflow.llm"),
        OpenGpuiRendererRegistrationStatus::Registered,
    );
    assert_eq!(
        registry.status("shader.multiply"),
        OpenGpuiRendererRegistrationStatus::Registered,
    );
}

#[test]
fn duplicate_renderer_key_is_isolated_while_other_registrations_seal() {
    let mut builder = OpenGpuiNodeRegistry::builder();
    builder.register(
        "workflow.condition",
        |binding, _window: &mut Window, cx: &mut App| {
            cx.new(|_| WorkflowNode { _binding: binding })
        },
    );
    builder.register(
        "workflow.condition",
        |binding, _window: &mut Window, cx: &mut App| cx.new(|_| ShaderNode { _binding: binding }),
    );
    builder.register(
        "shader.multiply",
        |binding, _window: &mut Window, cx: &mut App| cx.new(|_| ShaderNode { _binding: binding }),
    );

    let registry = builder
        .seal()
        .expect("a duplicate non-empty key is isolated inside a sealed registry");

    assert_eq!(registry.len(), 1, "only callable factories count");
    assert_eq!(
        registry.status("workflow.condition"),
        OpenGpuiRendererRegistrationStatus::Invalid(
            OpenGpuiNodeRegistryIssueKind::DuplicateRendererKey,
        ),
    );
    assert_eq!(
        registry.status("shader.multiply"),
        OpenGpuiRendererRegistrationStatus::Registered,
    );
}

#[test]
fn whitespace_only_renderer_key_fails_seal() {
    let mut builder = OpenGpuiNodeRegistry::builder();
    builder.register("  ", |binding, _window: &mut Window, cx: &mut App| {
        cx.new(|_| WorkflowNode { _binding: binding })
    });

    let error = builder
        .seal()
        .err()
        .expect("whitespace-only renderer key must fail sealing");

    assert_eq!(error.issues().len(), 1);
    assert_eq!(error.issues()[0].renderer_key(), "  ");
    assert_eq!(
        error.issues()[0].kind(),
        OpenGpuiNodeRegistryIssueKind::EmptyRendererKey,
    );
    assert!(
        error
            .to_string()
            .contains("renderer key \"  \" is empty or contains only whitespace")
    );
}

#[test]
fn seal_aggregates_configuration_issues_in_renderer_key_order() {
    let mut builder = OpenGpuiNodeRegistry::builder();
    for renderer_key in [
        "z.duplicate",
        "\t",
        "a.duplicate",
        "z.duplicate",
        "a.duplicate",
    ] {
        builder.register(
            renderer_key,
            |binding, _window: &mut Window, cx: &mut App| {
                cx.new(|_| WorkflowNode { _binding: binding })
            },
        );
    }

    let error = builder
        .seal()
        .err()
        .expect("all invalid renderer keys must be reported together");
    let issues = error.issues();

    assert_eq!(issues.len(), 3);
    assert_eq!(issues[0].renderer_key(), "\t");
    assert_eq!(
        issues[0].kind(),
        OpenGpuiNodeRegistryIssueKind::EmptyRendererKey
    );
    assert_eq!(issues[1].renderer_key(), "a.duplicate");
    assert_eq!(
        issues[1].kind(),
        OpenGpuiNodeRegistryIssueKind::DuplicateRendererKey,
    );
    assert_eq!(issues[2].renderer_key(), "z.duplicate");
    assert_eq!(
        issues[2].kind(),
        OpenGpuiNodeRegistryIssueKind::DuplicateRendererKey,
    );
}
