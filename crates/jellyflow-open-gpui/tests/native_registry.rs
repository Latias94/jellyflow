use jellyflow_core::NodeKindKey;
use jellyflow_open_gpui::authoring::OpenGpuiNodeBinding;
use jellyflow_open_gpui::registry::{
    OpenGpuiNodeAdapterPresentation, OpenGpuiNodeRegistrationPolicy, OpenGpuiNodeRegistry,
    OpenGpuiNodeRegistryIssueKind, OpenGpuiNodeRegistryRegistrationError,
    OpenGpuiNodeRendererDescriptor,
};
use jellyflow_open_gpui::scale_policy::OpenGpuiNodeScalePolicy;
use jellyflow_runtime::schema::{NodeCatalog, NodeKindSpec};
use jellyflow_runtime::view::RendererStamp;
use open_gpui::{App, AppContext, Context, Entity, ParentElement, Render, Window, div};

const WORKFLOW_KIND: &str = "workflow.llm";
const SHADER_KIND: &str = "shader.multiply";

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

fn catalog_with(entries: impl IntoIterator<Item = NodeKindSpec>) -> NodeCatalog {
    let mut catalog = NodeCatalog::new();
    for entry in entries {
        catalog.register(entry).expect("unique valid catalog kind");
    }
    catalog
}

fn spec(kind: &str, version: u32) -> NodeKindSpec {
    NodeKindSpec::builder(kind, kind, "legacy.catalog.renderer")
        .latest_kind_version(version)
        .build()
}

fn workflow_descriptor(
    kind: &str,
    version: u32,
    renderer: RendererStamp,
    accessible_name: &str,
) -> OpenGpuiNodeRendererDescriptor {
    OpenGpuiNodeRendererDescriptor::new(
        kind,
        version,
        renderer,
        OpenGpuiNodeAdapterPresentation::new(accessible_name),
        |binding, _window, cx: &mut App| Ok(cx.new(|_| WorkflowNode { _binding: binding })),
    )
}

fn shader_descriptor(
    kind: &str,
    version: u32,
    renderer: RendererStamp,
    accessible_name: &str,
) -> OpenGpuiNodeRendererDescriptor {
    OpenGpuiNodeRendererDescriptor::new(
        kind,
        version,
        renderer,
        OpenGpuiNodeAdapterPresentation::new(accessible_name),
        |binding, _window, cx: &mut App| Ok(cx.new(|_| ShaderNode { _binding: binding })),
    )
}

#[test]
fn descriptor_preserves_native_factory_identity_policy_presentation_and_overview() {
    let renderer = RendererStamp::new("open-gpui.workflow.llm", 7);
    let policy = OpenGpuiNodeRegistrationPolicy::default()
        .with_scale_policy(OpenGpuiNodeScalePolicy::ComponentManaged);
    let descriptor = workflow_descriptor(WORKFLOW_KIND, 3, renderer.clone(), "Language model")
        .with_policy(policy)
        .with_overview(|mount, _window, cx| {
            let binding = mount.binding();
            Ok(cx.new(move |_| ShaderNode { _binding: binding }))
        });

    assert_eq!(descriptor.kind(), &NodeKindKey::new(WORKFLOW_KIND));
    assert_eq!(descriptor.kind_version(), 3);
    assert_eq!(descriptor.renderer_stamp(), &renderer);
    assert_eq!(descriptor.policy(), policy);
    assert_eq!(
        descriptor.presentation().accessible_name(),
        "Language model"
    );
    assert!(descriptor.has_overview());

    let catalog = catalog_with([spec(WORKFLOW_KIND, 3), spec(SHADER_KIND, 1)]);
    let mut builder = OpenGpuiNodeRegistry::builder();
    builder.register(descriptor).expect("valid descriptor");
    builder
        .register(shader_descriptor(
            SHADER_KIND,
            1,
            RendererStamp::new("open-gpui.shader.multiply", 2),
            "Multiply",
        ))
        .expect("heterogeneous native entity type");

    let registry = builder.seal(&catalog).expect("catalog-compatible registry");
    assert_eq!(registry.len(), 2);
    assert_eq!(
        registry.renderer_stamp(&NodeKindKey::new(WORKFLOW_KIND), 3),
        Some(&renderer)
    );
    assert_eq!(
        registry
            .presentation(&NodeKindKey::new(WORKFLOW_KIND), 3)
            .map(|presentation| presentation.accessible_name()),
        Some("Language model")
    );
}

#[test]
fn registration_rejects_invalid_descriptor_fields_before_mutating_builder() {
    let mut builder = OpenGpuiNodeRegistry::builder();

    assert_eq!(
        builder
            .register(workflow_descriptor(
                "  ",
                1,
                RendererStamp::new("renderer", 1),
                "Node",
            ))
            .unwrap_err(),
        OpenGpuiNodeRegistryRegistrationError::EmptyKind
    );
    assert!(matches!(
        builder
            .register(workflow_descriptor(
                WORKFLOW_KIND,
                0,
                RendererStamp::new("renderer", 1),
                "Node",
            ))
            .unwrap_err(),
        OpenGpuiNodeRegistryRegistrationError::InvalidKindVersion { .. }
    ));
    assert!(matches!(
        builder
            .register(workflow_descriptor(
                WORKFLOW_KIND,
                1,
                RendererStamp::new("\t", 1),
                "Node",
            ))
            .unwrap_err(),
        OpenGpuiNodeRegistryRegistrationError::EmptyRendererKey { .. }
    ));
    assert!(matches!(
        builder
            .register(workflow_descriptor(
                WORKFLOW_KIND,
                1,
                RendererStamp::new("renderer", 1),
                "  ",
            ))
            .unwrap_err(),
        OpenGpuiNodeRegistryRegistrationError::EmptyAccessibleName { .. }
    ));

    let empty = builder
        .seal(&catalog_with([spec(WORKFLOW_KIND, 1)]))
        .expect("rejected registrations leave the builder empty");
    assert!(empty.is_empty());
}

#[test]
fn duplicate_semantic_key_and_exact_stamp_are_atomic_while_incarnations_remain_distinct() {
    let catalog = catalog_with([spec(WORKFLOW_KIND, 1), spec(SHADER_KIND, 1)]);
    let first_stamp = RendererStamp::new("native.shared", 1);
    let second_stamp = RendererStamp::new("native.shared", 2);
    let mut builder = OpenGpuiNodeRegistry::builder();
    builder
        .register(workflow_descriptor(
            WORKFLOW_KIND,
            1,
            first_stamp.clone(),
            "Workflow",
        ))
        .expect("first descriptor");

    assert!(matches!(
        builder
            .register(shader_descriptor(
                WORKFLOW_KIND,
                1,
                RendererStamp::new("native.other", 1),
                "Duplicate kind",
            ))
            .unwrap_err(),
        OpenGpuiNodeRegistryRegistrationError::DuplicateKindVersion { .. }
    ));
    assert!(matches!(
        builder
            .register(shader_descriptor(
                SHADER_KIND,
                1,
                first_stamp.clone(),
                "Duplicate stamp",
            ))
            .unwrap_err(),
        OpenGpuiNodeRegistryRegistrationError::DuplicateRendererStamp { .. }
    ));
    builder
        .register(shader_descriptor(
            SHADER_KIND,
            1,
            second_stamp.clone(),
            "Shader",
        ))
        .expect("same stamp key with a new incarnation is a distinct renderer identity");

    let registry = builder.seal(&catalog).expect("valid surviving descriptors");
    assert_eq!(registry.len(), 2);
    assert_eq!(
        registry.renderer_stamp(&NodeKindKey::new(WORKFLOW_KIND), 1),
        Some(&first_stamp)
    );
    assert_eq!(
        registry.renderer_stamp(&NodeKindKey::new(SHADER_KIND), 1),
        Some(&second_stamp)
    );
}

#[test]
fn seal_reports_unknown_alias_and_version_mismatches_in_semantic_key_order() {
    let canonical = NodeKindSpec::builder("m.canonical", "Canonical", "legacy")
        .latest_kind_version(4)
        .alias("a.alias")
        .build();
    let catalog = catalog_with([canonical]);
    let mut builder = OpenGpuiNodeRegistry::builder();
    for (kind, version, stamp) in [
        ("z.unknown", 1, "native.unknown"),
        ("m.canonical", 3, "native.old"),
        ("a.alias", 4, "native.alias"),
    ] {
        builder
            .register(workflow_descriptor(
                kind,
                version,
                RendererStamp::new(stamp, 1),
                kind,
            ))
            .expect("locally valid descriptor");
    }

    let error = builder
        .seal(&catalog)
        .err()
        .expect("all catalog mismatches reject sealing");
    assert_eq!(
        error
            .issues()
            .iter()
            .map(|issue| issue.semantic_kind().0.as_str())
            .collect::<Vec<_>>(),
        ["a.alias", "m.canonical", "z.unknown"]
    );
    assert!(matches!(
        error.issues()[0].issue(),
        OpenGpuiNodeRegistryIssueKind::NonCanonicalSemanticKind { canonical }
            if canonical == &NodeKindKey::new("m.canonical")
    ));
    assert_eq!(error.issues()[0].semantic_version(), 4);
    assert_eq!(
        error.issues()[1].issue(),
        &OpenGpuiNodeRegistryIssueKind::SemanticVersionMismatch { catalog_version: 4 }
    );
    assert_eq!(
        error.issues()[2].issue(),
        &OpenGpuiNodeRegistryIssueKind::MissingSemanticKind
    );
}

#[test]
fn catalog_kinds_without_native_descriptors_are_valid_fallback_mappings() {
    let catalog = catalog_with([spec(WORKFLOW_KIND, 1), spec(SHADER_KIND, 1)]);
    let mut builder = OpenGpuiNodeRegistry::builder();
    builder
        .register(workflow_descriptor(
            WORKFLOW_KIND,
            1,
            RendererStamp::new("native.workflow", 1),
            "Workflow",
        ))
        .expect("known descriptor");

    let registry = builder
        .seal(&catalog)
        .expect("catalog kinds may intentionally use diagnostic fallback");
    assert!(
        registry
            .renderer_stamp(&NodeKindKey::new(SHADER_KIND), 1)
            .is_none()
    );
}
