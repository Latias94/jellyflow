use std::cell::RefCell;
use std::rc::Rc;

use jellyflow_core::{
    CanvasPoint, CanvasRect, CanvasSize, GraphBuilder, GraphId, Node, NodeId, NodeKindKey, Port,
    PortCapacity, PortDirection, PortId, PortKey, PortKind,
};
use jellyflow_open_gpui::accessibility::{
    OPEN_GPUI_DIAGNOSTIC_ROLE, OPEN_GPUI_EDGE_ROLE, OPEN_GPUI_INTERACTIVE_PORT_ROLE,
    OPEN_GPUI_NODE_ROLE, OPEN_GPUI_RECONNECT_HANDLE_ROLE, OPEN_GPUI_SCENE_ROLE,
    OPEN_GPUI_STATIC_PORT_ROLE,
};
use jellyflow_open_gpui::authoring::OpenGpuiNodeBinding;
use jellyflow_open_gpui::host::OpenGpuiSceneHost;
use jellyflow_open_gpui::registry::{
    OpenGpuiNodeAdapterPresentation, OpenGpuiNodeRegistry, OpenGpuiNodeRendererDescriptor,
};
use jellyflow_runtime::schema::{NodeCatalog, NodeKindSpec};
use jellyflow_runtime::{
    graph::GraphRuntime,
    view::{RendererStamp, ViewSessionOptions},
};
use open_gpui::{AppContext, Context, Entity, ParentElement, Render, Role, Styled, Window, div};

const KIND: &str = "test.accessible-node";
const RENDERER: &str = "test.native.accessible-node";

struct AccessibleProbeNode;

impl Render for AccessibleProbeNode {
    fn render(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        div().size_full().child("accessible probe")
    }
}

#[test]
fn semantic_roles_match_their_native_interaction_contracts() {
    assert_eq!(OPEN_GPUI_SCENE_ROLE, Role::Group);
    assert_eq!(OPEN_GPUI_NODE_ROLE, Role::Group);
    assert_eq!(OPEN_GPUI_STATIC_PORT_ROLE, Role::Image);
    assert_eq!(OPEN_GPUI_INTERACTIVE_PORT_ROLE, Role::Button);
    assert_eq!(OPEN_GPUI_EDGE_ROLE, Role::Button);
    assert_eq!(OPEN_GPUI_RECONNECT_HANDLE_ROLE, Role::Button);
    assert_eq!(OPEN_GPUI_DIAGNOSTIC_ROLE, Role::Alert);
}

#[open_gpui::test]
fn retained_binding_exposes_a_semantic_port_name_with_an_application_override(
    cx: &mut open_gpui::TestAppContext,
) {
    let runtime = GraphRuntime::builder(catalog()).build(graph()).unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let captured = Rc::new(RefCell::new(None::<Entity<OpenGpuiNodeBinding>>));
    let factory_capture = captured.clone();
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry
        .register(OpenGpuiNodeRendererDescriptor::new(
            KIND,
            1,
            RendererStamp::new(RENDERER, 1),
            OpenGpuiNodeAdapterPresentation::new("Accessible node"),
            move |binding, _window, cx| {
                *factory_capture.borrow_mut() = Some(binding);
                Ok(cx.new(|_| AccessibleProbeNode))
            },
        ))
        .expect("register accessible node renderer");

    let (_host, cx) = cx.add_window_view(move |_window, _cx| {
        OpenGpuiSceneHost::new(
            view,
            registry.seal(&catalog()).expect("valid test registry"),
            viewport(),
        )
        .expect("valid Open GPUI scene host")
    });
    draw(cx);

    let binding = captured.borrow().clone().expect("mounted node binding");
    cx.read_entity(&binding, |binding, _| {
        let authoring = binding.authoring().expect("live authoring view");
        let handle = authoring
            .port(port_id())
            .expect("real output port")
            .handle();
        assert_eq!(handle.accessible_name(), "value output port");
        assert!(handle.interaction_enabled());
        assert!(
            !handle
                .clone()
                .with_interaction_enabled(false)
                .interaction_enabled()
        );
        assert!(
            handle
                .clone()
                .with_interaction_enabled(false)
                .with_interaction_enabled(true)
                .interaction_enabled()
        );
        assert_eq!(
            handle
                .clone()
                .with_accessible_name("Shader result")
                .accessible_name(),
            "Shader result"
        );
    });
}

fn draw(cx: &mut open_gpui::VisualTestContext) {
    cx.update(|window, cx| {
        window.draw(cx).clear();
    });
    cx.run_until_parked();
}

fn graph() -> jellyflow_core::Graph {
    let mut graph = GraphBuilder::new(GraphId::from_u128(1));
    graph.insert_node(
        node_id(),
        Node {
            kind: NodeKindKey::new(KIND),
            kind_version: 1,
            pos: CanvasPoint::default(),
            origin: None,
            selectable: None,
            draggable: None,
            connectable: None,
            deletable: None,
            parent: None,
            extent: None,
            expand_parent: None,
            size: Some(CanvasSize {
                width: 180.0,
                height: 100.0,
            }),
            hidden: false,
            collapsed: false,
            ports: vec![port_id()],
            data: serde_json::Value::Null,
        },
    );
    graph.insert_port(
        port_id(),
        Port {
            node: node_id(),
            key: PortKey::new("value"),
            dir: PortDirection::Out,
            kind: PortKind::Data,
            capacity: PortCapacity::Single,
            connectable: None,
            connectable_start: None,
            connectable_end: None,
            ty: None,
            data: serde_json::Value::Null,
        },
    );
    graph.build().unwrap()
}

fn catalog() -> NodeCatalog {
    let mut catalog = NodeCatalog::new();
    catalog
        .register(NodeKindSpec::builder(KIND).build())
        .unwrap();
    catalog
}

fn viewport() -> CanvasRect {
    CanvasRect {
        origin: CanvasPoint::default(),
        size: CanvasSize {
            width: 800.0,
            height: 600.0,
        },
    }
}

fn node_id() -> NodeId {
    NodeId::from_u128(10)
}

fn port_id() -> PortId {
    PortId::from_u128(20)
}
