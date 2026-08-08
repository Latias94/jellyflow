use jellyflow_core::{
    CanvasPoint, CanvasRect, CanvasSize, GraphBuilder, GraphId, NodeId, NodeKindKey, PortCapacity,
    PortDirection, PortId, PortKind,
};
use jellyflow_open_gpui::authoring::{OpenGpuiNodeBinding, OpenGpuiPortHandle};
use jellyflow_open_gpui::host::OpenGpuiSceneHost;
use jellyflow_open_gpui::registry::{
    OpenGpuiNodeAdapterPresentation, OpenGpuiNodeRegistry, OpenGpuiNodeRendererDescriptor,
};
use jellyflow_runtime::graph::GraphRuntime;
use jellyflow_runtime::schema::{NodeCatalog, NodeKindSpec, NodePortSpec};
use jellyflow_runtime::view::{RendererStamp, ViewSessionOptions};
use open_gpui::{
    AnyElement, AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, Window,
    div, px,
};
use open_gpui_ui_components::TextInput;

const KIND: &str = "external.node";
const RENDERER: &str = "external.open-gpui.node";

struct ProductNode {
    binding: Entity<OpenGpuiNodeBinding>,
}

impl Render for ProductNode {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        let authoring = self
            .binding
            .read(cx)
            .authoring()
            .expect("current authoring view");
        let output = authoring
            .port(output_id())
            .expect("catalog output port")
            .handle()
            .clone();
        let content = div()
            .relative()
            .w_full()
            .h(px(96.0))
            .child(
                self.binding
                    .read(cx)
                    .drag_handle()
                    .wrap(div().child("External node")),
            )
            .child(TextInput::new("external-label", "Label").value("Native control"))
            .child(port_marker(output));
        self.binding.read(cx).measure_content(content)
    }
}

fn port_marker(handle: OpenGpuiPortHandle) -> AnyElement {
    handle
        .interactive(div().w(px(12.0)).h(px(12.0)).rounded_full())
        .into_any_element()
}

fn output_id() -> PortId {
    PortId::from_u128(2)
}

fn catalog() -> NodeCatalog {
    let mut catalog = NodeCatalog::new();
    catalog
        .register(
            NodeKindSpec::builder(KIND)
                .port(NodePortSpec::new(
                    "output",
                    PortDirection::Out,
                    PortKind::Data,
                    PortCapacity::Multi,
                ))
                .build(),
        )
        .expect("register node kind");
    catalog
}

fn registry(catalog: &NodeCatalog) -> OpenGpuiNodeRegistry {
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry
        .register(OpenGpuiNodeRendererDescriptor::new(
            KIND,
            1,
            RendererStamp::new(RENDERER, 1),
            OpenGpuiNodeAdapterPresentation::new("External node"),
            |binding, _, cx| Ok(cx.new(|_| ProductNode { binding })),
        ))
        .expect("register renderer");
    registry.seal(catalog).expect("seal registry")
}

fn main() {
    let catalog = catalog();
    let registry = registry(&catalog);
    assert_eq!(
        registry.renderer_stamp(&NodeKindKey::new(KIND), 1),
        Some(&RendererStamp::new(RENDERER, 1)),
    );
    let creation = catalog
        .instantiate_node_with_ids(
            &NodeKindKey::new(KIND),
            NodeId::from_u128(1),
            CanvasRect {
                origin: CanvasPoint::default(),
                size: CanvasSize {
                    width: 240.0,
                    height: 120.0,
                },
            },
            [output_id()],
        )
        .expect("instantiate node");
    let mut graph = GraphBuilder::new(GraphId::from_u128(3));
    let (node, value, ports) = creation.into_parts();
    graph.insert_node(node, value);
    for (port, value) in ports {
        graph.insert_port(port, value);
    }
    let runtime = GraphRuntime::builder(catalog.clone())
        .build(graph.build().expect("valid graph"))
        .expect("build runtime");
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("attach view");
    let host = OpenGpuiSceneHost::new(
        view,
        registry,
        CanvasRect {
            origin: CanvasPoint::default(),
            size: CanvasSize {
                width: 800.0,
                height: 600.0,
            },
        },
    )
    .expect("build native host");
    assert_eq!(host.edge_painter_registry().len(), 0);
}
