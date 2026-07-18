use std::cell::Cell;
use std::error::Error;
use std::rc::Rc;
use std::sync::Arc;

use jellyflow::{GraphId, GraphRuntime, ViewSessionOptions, layout};
use jellyflow_core::{
    CanvasPoint, CanvasRect, CanvasSize, GraphBuilder, NodeId, NodeKindKey, PortCapacity,
    PortDirection, PortId, PortKey, PortKind,
};
use jellyflow_egui::{EguiNativeFrameHost, egui};
use jellyflow_open_gpui::chrome::{
    OpenGpuiEdgeLabelAppearance, OpenGpuiEdgeMarkerShape, OpenGpuiEdgePainterRecipe,
    OpenGpuiEdgePainterRegistry, OpenGpuiEdgeStrokePattern, OpenGpuiEdgeTone,
};
use jellyflow_open_gpui::{
    OpenGpuiContentHeightPolicy, OpenGpuiGraphTheme, OpenGpuiNodeBinding,
    OpenGpuiNodeRegistrationPolicy, OpenGpuiNodeRegistry, OpenGpuiNodeSizePolicy,
    OpenGpuiPortHandle, OpenGpuiRendererRegistrationStatus, OpenGpuiSceneHost,
};
use jellyflow_runtime::io::GraphFile;
use jellyflow_runtime::schema::{
    NodeCatalog, NodeKindMigrateError, NodeKindMigrator, NodeKindSpec, NodePortSpec,
};
use jellyflow_runtime::{
    FrameRequest, GraphCommand, HandlePosition, IncidentEdgePolicy, PublishedPortOutcome,
    RendererStamp, TopologyEdit, TopologyPort, ViewPoint, ViewRect, ViewSize,
};
use open_gpui::{
    AnyElement, AppContext, Context, Entity, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Render, Styled, Window, div, px, rgb,
};

struct ProductNodeState {
    title: String,
    mount_count: Cell<u64>,
    click_count: Cell<u64>,
}

struct ProductNode {
    binding: Entity<OpenGpuiNodeBinding>,
    state: Rc<ProductNodeState>,
}

struct PackageWorkflowMigrator;

impl NodeKindMigrator for PackageWorkflowMigrator {
    fn migrate(
        &self,
        from_version: u32,
        to_version: u32,
        data: &serde_json::Value,
    ) -> Result<serde_json::Value, NodeKindMigrateError> {
        if (from_version, to_version) != (1, 2) {
            return Err(NodeKindMigrateError::message(format!(
                "unexpected package fixture migration {from_version} -> {to_version}"
            )));
        }
        Ok(serde_json::json!({
            "migrated": true,
            "previous": data,
        }))
    }
}

impl Render for ProductNode {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        let binding = self.binding.read(cx);
        let base = binding
            .port_handle(PortId::from_u128(11))
            .expect("package fixture base port");
        let dynamic = binding.port_handle(PortId::from_u128(12));
        let drag_header = binding
            .drag_handle()
            .wrap(div().h(px(12.0)).child(self.state.title.clone()));
        let clicks = Rc::clone(&self.state);
        let mut content = div()
            .relative()
            .w_full()
            .h(px(if dynamic.is_some() { 144.0 } else { 108.0 }))
            .child(drag_header)
            .child(
                div()
                    .left(px(16.0))
                    .top(px(16.0))
                    .on_mouse_up(MouseButton::Left, move |_, _, _| {
                        clicks
                            .click_count
                            .set(clicks.click_count.get().saturating_add(1));
                    })
                    .child("Run"),
            )
            .child(
                div()
                    .absolute()
                    .right(px(0.0))
                    .top(px(72.0))
                    .child(package_port_marker(base)),
            );
        if let Some(dynamic) = dynamic {
            content = content.child(
                div()
                    .absolute()
                    .right(px(0.0))
                    .top(px(108.0))
                    .child(package_port_marker(dynamic)),
            );
        }
        binding.measure_content(content)
    }
}

fn package_port_marker(handle: OpenGpuiPortHandle) -> AnyElement {
    let presentation = handle.presentation();
    let color = if presentation.candidate().is_valid_target() {
        rgb(0x4dd6a2)
    } else if presentation.candidate().is_invalid_target() {
        rgb(0xff716a)
    } else if presentation.connected() {
        rgb(0x67a6ff)
    } else {
        rgb(0x7f8a98)
    };
    handle
        .with_minimum_hit_diameter(24.0)
        .interactive(
            div()
                .w(px(14.0))
                .h(px(14.0))
                .rounded(px(3.0))
                .border_2()
                .border_color(color)
                .bg(color),
            |_, _, _| {},
        )
        .into_any_element()
}

fn package_graph_theme() -> OpenGpuiGraphTheme {
    let mut theme = OpenGpuiGraphTheme::default();
    theme.palette.selection = rgb(0x8fc7ff).into();
    theme.metrics.port_hit_diameter = 24.0;
    theme
}

fn package_edge_painters() -> OpenGpuiEdgePainterRegistry {
    let mut painters = OpenGpuiEdgePainterRegistry::builder();
    painters
        .register(
            "package.edge",
            OpenGpuiEdgePainterRecipe::new(
                OpenGpuiEdgeStrokePattern::Dashed,
                OpenGpuiEdgeTone::Control,
                OpenGpuiEdgeMarkerShape::Circle,
                OpenGpuiEdgeMarkerShape::Arrow,
                OpenGpuiEdgeLabelAppearance::Badge,
            ),
        )
        .expect("package edge painter registration");
    painters.build()
}

fn product_registry(state: Rc<ProductNodeState>) -> OpenGpuiNodeRegistry {
    let mut builder = OpenGpuiNodeRegistry::builder();
    let policy = OpenGpuiNodeRegistrationPolicy::default().with_size_policy(
        OpenGpuiNodeSizePolicy::ContentHeight(
            OpenGpuiContentHeightPolicy::new(96.0, 180.0).expect("package content-height policy"),
        ),
    );
    builder.register_with_policy("package.native", policy, move |binding, _window, cx| {
        state
            .mount_count
            .set(state.mount_count.get().saturating_add(1));
        cx.new(|_| ProductNode {
            binding,
            state: Rc::clone(&state),
        })
    });
    builder
        .seal()
        .expect("package renderer registrations must be valid")
}

fn package_frame_request() -> FrameRequest {
    FrameRequest::new(
        CanvasRect {
            origin: CanvasPoint::default(),
            size: CanvasSize {
                width: 800.0,
                height: 600.0,
            },
        },
        RendererStamp::new("package.native", 1),
    )
}

fn main() -> Result<(), Box<dyn Error>> {
    let kind = NodeKindKey::new("package.workflow");
    let node = NodeId::from_u128(10);
    let output = PortId::from_u128(11);
    let mut catalog = NodeCatalog::new();
    catalog.register(
        NodeKindSpec::builder(kind.clone(), "Package workflow", "package.native")
            .latest_kind_version(2)
            .alias("package.workflow.legacy")
            .default_size(CanvasSize {
                width: 220.0,
                height: 140.0,
            })
            .port(NodePortSpec::new(
                "result",
                PortDirection::Out,
                PortKind::Data,
                PortCapacity::Multi,
            ))
            .build(),
    )?;
    catalog.register_migrator(kind.clone(), Arc::new(PackageWorkflowMigrator))?;

    let persisted_node = NodeId::from_u128(13);
    let persisted_path = std::env::temp_dir().join(format!(
        "jellyflow-package-known-kind-{}.json",
        std::process::id()
    ));
    std::fs::write(
        &persisted_path,
        r#"{
          "graph_id": "00000000-0000-0000-0000-000000000002",
          "graph_version": 2,
          "graph": {
            "graph_id": "00000000-0000-0000-0000-000000000002",
            "graph_version": 1,
            "nodes": {
              "00000000-0000-0000-0000-00000000000d": {
                "kind": "package.workflow.legacy",
                "kind_version": 1,
                "pos": { "x": 24.0, "y": 32.0 },
                "data": { "title": "persisted legacy workflow" }
              }
            }
          }
        }"#,
    )?;
    let canonical = GraphFile::load_canonical_json(&persisted_path, &catalog)?;
    std::fs::remove_file(&persisted_path)?;
    assert_eq!(canonical.report().canonicalized().len(), 1);
    assert_eq!(canonical.report().upgraded().len(), 1);
    let migrated = canonical
        .graph()
        .node(&persisted_node)
        .expect("known-kind fixture node");
    assert_eq!(migrated.kind, kind);
    assert_eq!(migrated.kind_version, 2);
    assert_eq!(migrated.data["migrated"], true);
    let restored_runtime = GraphRuntime::from_canonical(canonical)?;
    assert_eq!(
        restored_runtime
            .snapshot()?
            .graph()
            .node(&persisted_node)
            .expect("canonical fixture published")
            .kind_version,
        2
    );

    let creation = catalog.instantiate_node_with_ids(
        &kind,
        node,
        CanvasPoint { x: 24.0, y: 32.0 },
        [output],
    )?;
    let (created_id, created_node, created_ports) = creation.into_parts();
    let mut graph = GraphBuilder::new(GraphId::from_u128(1));
    graph.insert_node(created_id, created_node);
    for (port_id, port) in created_ports {
        graph.insert_port(port_id, port);
    }
    let runtime = GraphRuntime::builder(catalog.clone()).build(
        graph
            .build()
            .expect("package consumer fixture graph must validate"),
    )?;
    let view = runtime.attach_view(ViewSessionOptions::default())?;

    let command_frame = view.frame(package_frame_request())?;
    let command_node = command_frame
        .nodes()
        .first()
        .expect("package node admitted to command frame");
    let base = command_node
        .graph_snapshot()
        .graph()
        .port(&output)
        .expect("catalog-created package port")
        .clone();
    let mut dynamic = base.clone();
    dynamic.key = PortKey::new("dynamic-result");
    let collector = command_frame.collector();
    collector.command(
        GraphCommand::topology(
            command_frame.graph_revision(),
            TopologyEdit::new().replace_node_ports(
                node,
                [
                    TopologyPort::new(output, base),
                    TopologyPort::new(PortId::from_u128(12), dynamic),
                ],
                IncidentEdgePolicy::Reject,
            ),
        )
        .with_label("add package dynamic port"),
    )?;
    let command_receipt = view.submit_feedback(collector.finish()?)?;
    assert!(
        command_receipt.operations()[0]
            .command_outcome()
            .expect("ordered command receipt")
            .as_ref()
            .is_ok_and(|outcome| outcome.applied().is_some())
    );

    let frame = view.frame(package_frame_request())?;
    assert_eq!(frame.nodes().len(), 1);
    assert_eq!(frame.nodes()[0].ports().len(), 2);

    // Exercise the publishable runtime API: adapters can only create complete layouts from an
    // owned frame, and every real port receives an explicit outcome.
    let frame_node = &frame.nodes()[0];
    let root = ViewRect::new(ViewPoint::new(24.0, 32.0), ViewSize::new(220.0, 140.0));
    let visual = ViewRect::new(ViewPoint::new(232.0, 88.0), ViewSize::new(12.0, 12.0));
    let hit = ViewRect::new(ViewPoint::new(228.0, 84.0), ViewSize::new(20.0, 20.0));
    let mut layout = frame_node.begin_layout(root);
    layout.port_rendered(
        frame_node
            .ports()
            .iter()
            .find(|port| port.id() == output)
            .expect("rendered package port")
            .layout_target(),
        visual,
        hit,
        ViewPoint::new(244.0, 94.0),
        HandlePosition::Right,
    )?;
    layout.port_unavailable(
        frame_node
            .ports()
            .iter()
            .find(|port| port.id() == PortId::from_u128(12))
            .expect("unmeasured package port")
            .layout_target(),
    )?;
    let collector = frame.collector();
    collector.layout(layout.finish()?)?;
    view.submit_feedback(collector.finish()?)?;
    let geometry = view.geometry_snapshot()?;
    assert!(matches!(
        geometry.port_outcome(output),
        Some(PublishedPortOutcome::Rendered(_))
    ));
    assert!(matches!(
        geometry.port_outcome(PortId::from_u128(12)),
        Some(PublishedPortOutcome::Unavailable)
    ));
    assert!(geometry.port(PortId::from_u128(12)).is_none());

    let egui_host = EguiNativeFrameHost::new(view);
    let _native_color = egui::Color32::WHITE;
    egui_host.close()?;

    // Production Open GPUI must compile without its test-support feature.
    let registry = OpenGpuiNodeRegistry::builder().seal()?;
    assert_eq!(
        registry.status("package.missing"),
        OpenGpuiRendererRegistrationStatus::Missing
    );
    let product_state = Rc::new(ProductNodeState {
        title: "Package-owned native node".to_owned(),
        mount_count: Cell::new(0),
        click_count: Cell::new(0),
    });
    let product_registry = product_registry(Rc::clone(&product_state));
    assert_eq!(
        product_registry.status("package.native"),
        OpenGpuiRendererRegistrationStatus::Registered
    );
    let open_gpui_view = runtime.attach_view(ViewSessionOptions::default())?;
    let weak_open_gpui_view = open_gpui_view.downgrade();
    let viewport = CanvasRect {
        origin: CanvasPoint::default(),
        size: CanvasSize {
            width: 1024.0,
            height: 768.0,
        },
    };
    let graph_theme = package_graph_theme();
    let host = OpenGpuiSceneHost::new(open_gpui_view, product_registry, viewport)
        .with_graph_theme(graph_theme)
        .with_edge_painter_registry(package_edge_painters());
    assert_eq!(host.graph_theme(), graph_theme);
    assert_eq!(host.edge_painter_registry().len(), 1);
    assert!(host.view_handle().is_open());
    drop(host);
    assert!(!weak_open_gpui_view.is_open());
    let _native_element = open_gpui::div();
    let _ = layout::builtin_layout_engine_registry();
    Ok(())
}
