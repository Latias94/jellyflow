use std::cell::{Cell, RefCell};
use std::rc::Rc;

use jellyflow_core::{
    CanvasPoint, CanvasRect, CanvasSize, GraphBuilder, GraphId, Node, NodeId, NodeKindKey, Port,
    PortCapacity, PortDirection, PortId, PortKey, PortKind,
};
use jellyflow_open_gpui::authoring::{
    MutationLayoutEffect, MutationRequest, OpenGpuiManagedMutation, OpenGpuiNodeBinding,
    OpenGpuiPortSide,
};
use jellyflow_open_gpui::host::OpenGpuiSceneHost;
use jellyflow_open_gpui::registry::{
    OpenGpuiNodeAdapterPresentation, OpenGpuiNodeRegistry, OpenGpuiNodeRendererDescriptor,
};
use jellyflow_runtime::schema::{NodeCatalog, NodeKindSpec};
use jellyflow_runtime::{
    commands::{GraphCommand, IncidentEdgePolicy, TopologyEdit, TopologyPort},
    graph::GraphRuntime,
    view::{RendererStamp, ViewSessionOptions, WeakViewSession},
};
use open_gpui::{
    AppContext, Context, Entity, InteractiveElement, ParentElement, Render, Styled, Window, div, px,
};
use open_gpui_ui_components::{
    Button, TextInput,
    gpui_adapter::{TextInputController, init_text_input},
};

const KIND: &str = "test.native-node-authoring";
const RENDERER: &str = "test.native.node-authoring";
const INPUT_SELECTOR: &str = "text-input:native-authoring-prompt:root";
const ADD_OUTPUT_SELECTOR: &str = "native-authoring-add-output";

struct NativeAuthoringNode {
    binding: Entity<OpenGpuiNodeBinding>,
    prompt: Entity<TextInputController>,
    topology: OpenGpuiManagedMutation<PortId>,
}

impl NativeAuthoringNode {
    fn add_output(&mut self, cx: &mut Context<Self>) {
        let authoring = self
            .binding
            .read(cx)
            .authoring()
            .expect("retained node authoring stays live");
        if authoring.port(dynamic_port_id()).is_some() {
            return;
        }
        let base = authoring
            .port(base_port_id())
            .expect("base port belongs to the retained node")
            .facts()
            .clone();
        let command = GraphCommand::topology(
            authoring.version(),
            TopologyEdit::new().replace_node_ports(
                node_id(),
                [
                    TopologyPort::new(base_port_id(), base),
                    TopologyPort::new(dynamic_port_id(), dynamic_port()),
                ],
                IncidentEdgePolicy::Reject,
            ),
        );
        self.topology
            .dispatch(
                authoring,
                command,
                MutationRequest::new(dynamic_port_id()),
                MutationLayoutEffect::InvalidateWhenSatisfied,
                cx,
            )
            .expect("native topology mutation enters the host journal");
        cx.notify();
    }
}

impl Render for NativeAuthoringNode {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl open_gpui::IntoElement {
        let authoring = self
            .binding
            .read(cx)
            .authoring()
            .expect("retained node authoring stays live");
        self.topology
            .observe(&authoring, |port| authoring.port(*port).is_some(), cx);

        let node = cx.weak_entity();
        let add_output = Button::new("native-authoring-add-output-button", "Add output").on_click(
            move |_, _, cx| {
                let _ = node.update(cx, |node, cx| node.add_output(cx));
            },
        );
        let mut root = div()
            .relative()
            .size_full()
            .p(px(12.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                TextInput::new("native-authoring-prompt", "Prompt").controller(self.prompt.clone()),
            )
            .child(
                div()
                    .debug_selector(|| ADD_OUTPUT_SELECTOR.to_owned())
                    .self_start()
                    .flex_none()
                    .child(add_output),
            );

        for (index, port) in authoring.ports().enumerate() {
            let handle = port.handle().clone();
            let top = 48.0 + index as f32 * 30.0;
            let marker = handle.interactive(div().w(px(14.0)).h(px(14.0)));
            root = match handle.side() {
                OpenGpuiPortSide::Left => {
                    root.child(div().absolute().left(px(0.0)).top(px(top)).child(marker))
                }
                OpenGpuiPortSide::Right => {
                    root.child(div().absolute().right(px(0.0)).top(px(top)).child(marker))
                }
                OpenGpuiPortSide::Top | OpenGpuiPortSide::Bottom => {
                    unreachable!("default data ports use horizontal sides")
                }
            };
        }
        root
    }
}

#[open_gpui::test]
fn retained_native_node_keeps_component_state_while_mutating_real_ports(
    cx: &mut open_gpui::TestAppContext,
) {
    cx.update(init_text_input);
    let opaque_data = serde_json::json!({
        "prompt": "initial",
        "plugin_owned": { "sentinel": [1, 3, 5] }
    });
    let runtime = Rc::new(
        GraphRuntime::builder(catalog())
            .build(graph(opaque_data.clone()))
            .expect("native authoring runtime"),
    );
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("native authoring view");
    let weak_view = view.downgrade();
    let mount_count = Rc::new(Cell::new(0));
    let captured_node = Rc::new(RefCell::new(None));
    let factory_mount_count = Rc::clone(&mount_count);
    let factory_node = Rc::clone(&captured_node);
    let mut registry = OpenGpuiNodeRegistry::builder();
    registry
        .register(OpenGpuiNodeRendererDescriptor::new(
            KIND,
            1,
            RendererStamp::new(RENDERER, 1),
            OpenGpuiNodeAdapterPresentation::new("Native authoring node"),
            move |binding, _window, cx| {
                factory_mount_count.set(factory_mount_count.get() + 1);
                let prompt = cx.new(|cx| TextInputController::with_value(String::new(), cx));
                let topology = binding.read(cx).managed_mutation::<PortId>();
                let node = cx.new(|_| NativeAuthoringNode {
                    binding,
                    prompt,
                    topology,
                });
                *factory_node.borrow_mut() = Some(node.clone());
                Ok(node)
            },
        ))
        .expect("register native authoring renderer");
    let (_host, cx) = cx.add_window_view(move |_, _| {
        OpenGpuiSceneHost::new(
            view,
            registry.seal(&catalog()).expect("valid native registry"),
            viewport(),
        )
        .expect("valid Open GPUI scene host")
    });
    draw(cx);

    let node = captured_node
        .borrow()
        .clone()
        .expect("retained native node");
    let prompt = cx.read_entity(&node, |node, _| node.prompt.clone());
    assert_eq!(mount_count.get(), 1);
    assert_eq!(fresh_published_ports(&weak_view), vec![base_port_id()]);

    let transform_before = weak_view.viewport_transform().expect("initial viewport");
    let selection_before = weak_view.selection().expect("initial selection");
    let position_before = runtime
        .snapshot()
        .expect("initial graph")
        .graph()
        .node(&node_id())
        .expect("initial node")
        .pos;

    let input = cx.debug_bounds(INPUT_SELECTOR).expect("native text input");
    cx.simulate_click(input.center(), Default::default());
    cx.simulate_input("native text");
    let add_output = cx
        .debug_bounds(ADD_OUTPUT_SELECTOR)
        .expect("native topology button");
    cx.simulate_click(add_output.center(), Default::default());
    cx.run_until_parked();
    draw(cx);

    let snapshot = runtime.snapshot().expect("mutated graph");
    let graph_node = snapshot
        .graph()
        .node(&node_id())
        .expect("retained graph node");
    assert_eq!(graph_node.pos, position_before);
    assert_eq!(graph_node.data, opaque_data);
    assert_eq!(graph_node.ports, vec![base_port_id(), dynamic_port_id()]);
    assert!(snapshot.graph().port(&dynamic_port_id()).is_some());
    assert_eq!(
        mount_count.get(),
        1,
        "topology updates must retain the entity"
    );
    cx.read_entity(&prompt, |prompt, _| {
        assert_eq!(prompt.value(), "native text")
    });
    assert_eq!(
        fresh_published_ports(&weak_view),
        vec![base_port_id(), dynamic_port_id()],
    );
    assert_eq!(weak_view.viewport_transform().unwrap(), transform_before);
    assert_eq!(weak_view.selection().unwrap(), selection_before);
}

fn fresh_published_ports(view: &WeakViewSession) -> Vec<PortId> {
    let geometry = view.geometry_snapshot().expect("published geometry");
    let node = geometry.node(node_id()).expect("published node geometry");
    assert!(node.is_fresh());
    node.ports().keys().copied().collect()
}

fn graph(data: serde_json::Value) -> jellyflow_core::Graph {
    let mut builder = GraphBuilder::new(GraphId::from_u128(0xada0));
    builder.insert_node(
        node_id(),
        Node {
            kind: NodeKindKey::new(KIND),
            kind_version: 1,
            pos: CanvasPoint { x: 24.0, y: 20.0 },
            origin: None,
            selectable: None,
            draggable: None,
            connectable: None,
            deletable: None,
            parent: None,
            extent: None,
            expand_parent: None,
            size: Some(CanvasSize {
                width: 280.0,
                height: 150.0,
            }),
            hidden: false,
            collapsed: false,
            ports: vec![base_port_id()],
            data,
        },
    );
    builder.insert_port(base_port_id(), base_port());
    builder.build().expect("native authoring graph")
}

fn base_port() -> Port {
    port("input", PortDirection::In, PortCapacity::Single)
}

fn dynamic_port() -> Port {
    port("dynamic", PortDirection::Out, PortCapacity::Multi)
}

fn port(key: &str, direction: PortDirection, capacity: PortCapacity) -> Port {
    Port {
        node: node_id(),
        key: PortKey::new(key),
        dir: direction,
        kind: PortKind::Data,
        capacity,
        connectable: None,
        connectable_start: None,
        connectable_end: None,
        ty: None,
        data: serde_json::Value::Null,
    }
}

fn catalog() -> NodeCatalog {
    let mut catalog = NodeCatalog::new();
    catalog
        .register(NodeKindSpec::builder(KIND).build())
        .expect("native authoring catalog entry");
    catalog
}

fn node_id() -> NodeId {
    NodeId::from_u128(0xada1)
}

fn base_port_id() -> PortId {
    PortId::from_u128(0xada2)
}

fn dynamic_port_id() -> PortId {
    PortId::from_u128(0xada3)
}

fn viewport() -> CanvasRect {
    CanvasRect {
        origin: CanvasPoint::default(),
        size: CanvasSize {
            width: 640.0,
            height: 480.0,
        },
    }
}

fn draw(cx: &mut open_gpui::VisualTestContext) {
    cx.update(|window, cx| {
        window.draw(cx).clear();
    });
    cx.run_until_parked();
}
