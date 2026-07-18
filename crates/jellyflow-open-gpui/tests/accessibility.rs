use std::cell::RefCell;
use std::rc::Rc;

use jellyflow::core::{
    CanvasPoint, CanvasRect, CanvasSize, EdgeId, GraphBuilder, GraphId, Node, NodeId, NodeKindKey,
    Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
};
use jellyflow::runtime::schema::{NodeCatalog, NodeKindSpec};
use jellyflow::runtime::{FrameRequest, GraphRuntime, RendererStamp, ViewSessionOptions};
use jellyflow_open_gpui::{
    OPEN_GPUI_DIAGNOSTIC_ROLE, OPEN_GPUI_EDGE_ROLE, OPEN_GPUI_INTERACTIVE_PORT_ROLE,
    OPEN_GPUI_NODE_ROLE, OPEN_GPUI_RECONNECT_HANDLE_ROLE, OPEN_GPUI_SCENE_ROLE,
    OPEN_GPUI_STATIC_PORT_ROLE, OpenGpuiNodeBinding, OpenGpuiNodeRegistry, OpenGpuiSceneHost,
    open_gpui_edge_element_id, open_gpui_node_element_id, open_gpui_port_element_id,
    open_gpui_reconnect_element_id, open_gpui_scene_element_id,
};
use open_gpui::{
    AppContext, Context, Entity, EntityId, ParentElement, Render, Role, Styled, Window, div,
};

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
fn semantic_roles_and_element_ids_are_stable_and_host_scoped() {
    assert_eq!(OPEN_GPUI_SCENE_ROLE, Role::Group);
    assert_eq!(OPEN_GPUI_NODE_ROLE, Role::Group);
    assert_eq!(OPEN_GPUI_STATIC_PORT_ROLE, Role::Image);
    assert_eq!(OPEN_GPUI_INTERACTIVE_PORT_ROLE, Role::Button);
    assert_eq!(OPEN_GPUI_EDGE_ROLE, Role::Button);
    assert_eq!(OPEN_GPUI_RECONNECT_HANDLE_ROLE, Role::Button);
    assert_eq!(OPEN_GPUI_DIAGNOSTIC_ROLE, Role::Alert);

    let runtime = GraphRuntime::builder(catalog()).build(graph()).unwrap();
    let first_view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let second_view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let first_frame = first_view
        .frame(FrameRequest::new(
            viewport(),
            RendererStamp::new(RENDERER, 1),
        ))
        .unwrap();
    let second_frame = second_view
        .frame(FrameRequest::new(
            viewport(),
            RendererStamp::new(RENDERER, 1),
        ))
        .unwrap();
    let first_instance = first_frame.nodes()[0].instance();
    let second_instance = second_frame.nodes()[0].instance();
    let first_host = EntityId::from(1);
    let second_host = EntityId::from(2);

    let scene = open_gpui_scene_element_id(first_host, first_view.id());
    assert_eq!(
        scene,
        open_gpui_scene_element_id(first_host, first_view.id())
    );
    assert_ne!(
        scene,
        open_gpui_scene_element_id(second_host, first_view.id())
    );
    assert_ne!(
        scene,
        open_gpui_scene_element_id(first_host, second_view.id())
    );

    let node = open_gpui_node_element_id(first_host, first_instance);
    assert_eq!(node, open_gpui_node_element_id(first_host, first_instance));
    assert_ne!(node, open_gpui_node_element_id(first_host, second_instance));

    let port = open_gpui_port_element_id(first_host, first_instance, port_id());
    assert_eq!(
        port,
        open_gpui_port_element_id(first_host, first_instance, port_id())
    );
    assert_ne!(
        port,
        open_gpui_port_element_id(second_host, first_instance, port_id())
    );

    let edge_id = EdgeId::from_u128(30);
    let edge = open_gpui_edge_element_id(first_host, first_view.id(), edge_id);
    assert_eq!(
        edge,
        open_gpui_edge_element_id(first_host, first_view.id(), edge_id)
    );
    assert_ne!(
        edge,
        open_gpui_edge_element_id(second_host, first_view.id(), edge_id)
    );
    assert_ne!(
        edge,
        open_gpui_edge_element_id(first_host, second_view.id(), edge_id)
    );

    let source_reconnect =
        open_gpui_reconnect_element_id(first_host, first_view.id(), edge_id, "source");
    assert_eq!(
        source_reconnect,
        open_gpui_reconnect_element_id(first_host, first_view.id(), edge_id, "source")
    );
    assert_ne!(
        source_reconnect,
        open_gpui_reconnect_element_id(first_host, first_view.id(), edge_id, "target")
    );
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
    registry.register(RENDERER, move |binding, _window, cx| {
        *factory_capture.borrow_mut() = Some(binding);
        cx.new(|_| AccessibleProbeNode)
    });

    let (_host, cx) = cx.add_window_view(move |_window, _cx| {
        OpenGpuiSceneHost::new(
            view,
            registry.seal().expect("valid test registry"),
            viewport(),
        )
    });
    draw(cx);

    let binding = captured.borrow().clone().expect("mounted node binding");
    cx.read_entity(&binding, |binding, _| {
        let handle = binding.port_handle(port_id()).expect("real output port");
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

fn graph() -> jellyflow::core::Graph {
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
        .register(
            NodeKindSpec::builder(KIND, "Accessible node", RENDERER)
                .default_size(CanvasSize {
                    width: 180.0,
                    height: 100.0,
                })
                .build(),
        )
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
